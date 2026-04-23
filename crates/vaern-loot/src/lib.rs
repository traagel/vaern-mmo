//! Loot rolling — mob death → `ItemInstance` (or nothing).
//!
//! Leaf crate (no Bevy plugin) so the sim can call `roll_drop` headlessly
//! for balance validation. Server wires the mob-death hook separately.
//!
//! ## Roll pipeline
//!
//! 1. Pick **rarity** from a heavy-tail distribution biased by `NpcKind`
//!    (Combat mostly commons, Named mostly rares+). Junk rolls short-
//!    circuit into `None` → no drop.
//! 2. Pick a **base** from the table's eligible base pool (filtered by
//!    `applies_to` + kind restrictions).
//! 3. Pick a **material** among `valid_for` for that base, weighted by
//!    closeness to the table's `material_tier`.
//! 4. Pick a **quality** — weighted around material tier (common mobs
//!    roll `regular` mostly, elites bias up).
//! 5. Pick **N affixes** where `N = rarity_to_max_slots(rarity) - 1` so
//!    at least one slot stays open for crafter polish (except Common →
//!    zero slots total). Affixes filtered by `applies_to` + tier range.
//!
//! Boss-shard affixes have `weight == 0` and are never rolled here —
//! they only land via deterministic shard imprints (future Phase E).

use rand::prelude::IndexedRandom;
use rand::{Rng, SeedableRng, rngs::StdRng};
use serde::{Deserialize, Serialize};
use vaern_combat::NpcKind;
use vaern_items::{
    AffixPosition, BaseKind, ContentRegistry, ItemInstance, Rarity, rarity_to_max_slots,
};

/// Per-encounter loot spec. Field semantics:
///
/// * `material_tier` — center of the material-roll bell curve. Mobs in
///   a T3 zone use material_tier 3 → mostly iron/bronze drops with
///   occasional steel, rarely silver.
/// * `base_kinds` — which item kinds can drop. Trash mobs may drop
///   armor+weapons+consumables; a specific blacksmith-themed elite
///   might drop weapons-only.
/// * `drop_chance` — probability any drop happens at all on kill. 0.3
///   = 30% of kills produce an item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DropTable {
    pub drop_chance: f32,
    pub material_tier: u8,
    #[serde(default = "default_base_kinds")]
    pub base_kinds: Vec<String>,
    /// Rarity distribution — a 5-tuple summing to ~1.0 (normalized
    /// anyway if not). Order: common, uncommon, rare, epic, legendary.
    /// Junk drops use a separate hidden probability to turn a drop
    /// into "nothing" (represented by rarity_curve below failing).
    pub rarity_curve: [f32; 5],
}

fn default_base_kinds() -> Vec<String> {
    vec![
        "weapon".into(),
        "armor".into(),
        "shield".into(),
    ]
}

impl DropTable {
    /// Default table for a regular combat mob. 30% drop chance, mostly
    /// commons, occasional uncommon, rare rare.
    pub fn combat(material_tier: u8) -> Self {
        Self {
            drop_chance: 0.30,
            material_tier,
            base_kinds: default_base_kinds(),
            rarity_curve: [0.80, 0.15, 0.04, 0.009, 0.001],
        }
    }

    /// Elite mob — 60% drop, skews uncommon with meaningful rare chance.
    pub fn elite(material_tier: u8) -> Self {
        Self {
            drop_chance: 0.60,
            material_tier,
            base_kinds: default_base_kinds(),
            rarity_curve: [0.30, 0.45, 0.20, 0.04, 0.01],
        }
    }

    /// Named / mini-boss — guaranteed drop, skews rare+.
    pub fn named(material_tier: u8) -> Self {
        Self {
            drop_chance: 1.00,
            material_tier,
            base_kinds: default_base_kinds(),
            rarity_curve: [0.05, 0.20, 0.45, 0.25, 0.05],
        }
    }

    /// Pick a default table for an NpcKind + tier. Quest-givers don't
    /// drop loot (zero drop_chance). Callers can override by building
    /// their own table.
    pub fn for_npc(kind: NpcKind, material_tier: u8) -> Option<Self> {
        match kind {
            NpcKind::Combat => Some(Self::combat(material_tier)),
            NpcKind::Elite => Some(Self::elite(material_tier)),
            NpcKind::Named => Some(Self::named(material_tier)),
            NpcKind::QuestGiver => None,
        }
    }
}

/// Roll a single drop against the table. Returns `None` if no item
/// dropped (drop_chance gate failed). Caller wraps with its own RNG
/// — pass a seeded `StdRng` for deterministic tests.
///
/// Rarity in this model is *derived* from (material.base_rarity + quality
/// offset), not set independently. The table's `rarity_curve` biases
/// the quality roll: a Named table prefers higher-rarity-offset quality
/// bands (fine/superior/masterful) so drops resolve high; a Combat
/// table's curve peaks at offset 0 → regular quality → resolved rarity
/// tracks material base. This keeps one rarity view (the resolver's)
/// instead of a rolled-vs-resolved schism.
pub fn roll_drop(
    table: &DropTable,
    registry: &ContentRegistry,
    rng: &mut impl Rng,
) -> Option<ItemInstance> {
    if rng.random::<f32>() >= table.drop_chance {
        return None;
    }

    let base = pick_base(registry, &table.base_kinds, rng)?;
    let base_kind_tag = base_kind_tag(&base_kind(registry, &base)?);

    let material_id = if kind_needs_material(base_kind_tag) {
        pick_material(registry, &base, table.material_tier, rng)?
    } else {
        None
    };

    // Quality roll — biased by the table's rarity_curve so Named tables
    // skew upward while Combat tables cluster at regular.
    let quality = pick_quality_biased(registry, &table.rarity_curve, rng)?;

    // Compute resolved rarity to size the affix budget. This keeps
    // the invariant "affix count ≤ rarity_to_max_slots(resolved)".
    let mat_rarity = material_id
        .as_ref()
        .and_then(|mid| registry.get_material(mid))
        .map(|m| m.base_rarity)
        .unwrap_or(Rarity::Common);
    let q_offset = registry
        .get_quality(&quality)
        .map(|q| q.rarity_offset)
        .unwrap_or(0);
    let resolved_rarity = apply_rarity_offset(mat_rarity, q_offset);

    // Pre-rolled drops leave 1 slot open for crafter polish. Zero-slot
    // rarities (Junk/Common) roll no affixes.
    let slots = rarity_to_max_slots(resolved_rarity) as usize;
    let affix_count = slots.saturating_sub(1);

    let affixes = if affix_count > 0 {
        pick_affixes(
            registry,
            base_kind_tag,
            table.material_tier,
            affix_count,
            rng,
        )
    } else {
        Vec::new()
    };

    Some(ItemInstance {
        base_id: base,
        material_id,
        quality_id: quality,
        affixes,
    })
}

fn apply_rarity_offset(base: Rarity, offset: i8) -> Rarity {
    let ord = match base {
        Rarity::Junk => 0i16,
        Rarity::Common => 1,
        Rarity::Uncommon => 2,
        Rarity::Rare => 3,
        Rarity::Epic => 4,
        Rarity::Legendary => 5,
    };
    let shifted = (ord + offset as i16).clamp(0, 5);
    match shifted {
        0 => Rarity::Junk,
        1 => Rarity::Common,
        2 => Rarity::Uncommon,
        3 => Rarity::Rare,
        4 => Rarity::Epic,
        _ => Rarity::Legendary,
    }
}

/// Deterministic version of `roll_drop` — takes a seed. Useful for
/// log-reproducible drops and balance tests.
pub fn roll_drop_seeded(
    table: &DropTable,
    registry: &ContentRegistry,
    seed: u64,
) -> Option<ItemInstance> {
    let mut rng = StdRng::seed_from_u64(seed);
    roll_drop(table, registry, &mut rng)
}

// ---------------------------------------------------------------------------
// Roll sub-steps
// ---------------------------------------------------------------------------

/// Map the table's rarity_curve (per-rarity weights) onto the available
/// quality entries, biasing higher-curve-weight rarities toward
/// higher-offset qualities. Elite/Named tables therefore roll
/// fine/superior/masterful more often; Combat rolls cluster at regular.
///
/// We don't require the quality pool to have exactly 5 entries or any
/// particular naming — just score each quality by how close its
/// rarity_offset is to a curve-weighted "target offset."
fn pick_quality_biased(
    registry: &ContentRegistry,
    curve: &[f32; 5],
    rng: &mut impl Rng,
) -> Option<String> {
    // Linear ramp from common (0) to legendary (2.0) — sharper slope
    // than the raw rarity ordinals so curve skew translates to a
    // meaningful offset shift. Named tables land near target 1.0
    // (favor fine/superior); Combat near 0.1 (favor regular).
    let offsets = [0.0_f32, 0.5, 1.0, 1.5, 2.0];
    let total: f32 = curve.iter().sum();
    let target_offset: f32 = if total > 0.0 {
        curve
            .iter()
            .zip(offsets.iter())
            .map(|(w, o)| w * o)
            .sum::<f32>()
            / total
    } else {
        0.0
    };

    let pool: Vec<&vaern_items::Quality> = registry.qualities().collect();
    if pool.is_empty() {
        return None;
    }
    // Tight bell — 0.05 denominator keeps the peak narrow so named
    // tables don't spill much weight to regular quality.
    let weights: Vec<f32> = pool
        .iter()
        .map(|q| {
            let d = (q.rarity_offset as f32 - target_offset).abs();
            1.0 / (0.05 + d)
        })
        .collect();
    let idx = weighted_index(&weights, rng)?;
    Some(pool[idx].id.clone())
}

fn pick_base(
    registry: &ContentRegistry,
    wanted_kinds: &[String],
    rng: &mut impl Rng,
) -> Option<String> {
    let eligible: Vec<&vaern_items::ItemBase> = registry
        .bases()
        .filter(|b| wanted_kinds.iter().any(|k| k == base_kind_tag(&b.kind)))
        .collect();
    eligible.choose(rng).map(|b| b.id.clone())
}

fn pick_material(
    registry: &ContentRegistry,
    base_id: &str,
    target_tier: u8,
    rng: &mut impl Rng,
) -> Option<Option<String>> {
    let base = registry.get_base(base_id)?;
    // Filter materials by valid_for + weapon/shield eligibility matching
    // the base kind.
    let eligible: Vec<&vaern_items::Material> = registry
        .materials()
        .filter(|m| material_fits_base(m, &base.kind))
        .collect();
    if eligible.is_empty() {
        return Some(None);
    }
    // Weight inversely to tier distance — closer-to-target materials
    // win more often but any is possible.
    let weights: Vec<f32> = eligible
        .iter()
        .map(|m| {
            let d = (m.tier as i16 - target_tier as i16).abs() as f32;
            1.0 / (1.0 + d * d)
        })
        .collect();
    let idx = weighted_index(&weights, rng)?;
    Some(Some(eligible[idx].id.clone()))
}


fn pick_affixes(
    registry: &ContentRegistry,
    base_kind_tag: &str,
    target_tier: u8,
    count: usize,
    rng: &mut impl Rng,
) -> Vec<String> {
    if count == 0 {
        return Vec::new();
    }
    // Random-rollable pool only: weight > 0, applies_to contains the
    // base kind, tier range covers target_tier.
    let mut eligible: Vec<&vaern_items::Affix> = registry
        .affixes()
        .filter(|a| {
            a.weight > 0
                && a.applies_to.iter().any(|t| t == base_kind_tag)
                && a.min_tier <= target_tier
                && a.max_tier >= target_tier
        })
        .collect();

    // Cap the pick against what's available + split by position so we
    // don't stack 3 prefixes on a common (looks weird in display).
    // Simple rule: alternate prefer prefix/suffix via weighted weights.
    let mut picked = Vec::with_capacity(count);
    let mut used_prefix_ids: Vec<String> = Vec::new();
    let mut used_suffix_ids: Vec<String> = Vec::new();

    for _ in 0..count {
        if eligible.is_empty() {
            break;
        }
        let weights: Vec<f32> = eligible.iter().map(|a| a.weight as f32).collect();
        let Some(idx) = weighted_index(&weights, rng) else {
            break;
        };
        let a = eligible[idx];
        match a.position {
            AffixPosition::Prefix => used_prefix_ids.push(a.id.clone()),
            AffixPosition::Suffix => used_suffix_ids.push(a.id.clone()),
        }
        picked.push(a.id.clone());
        // Remove this affix id from the pool so we don't roll a dupe.
        eligible.remove(idx);
    }
    picked
}

/// Seeded weighted-index sampler. Returns None if the weight slice is
/// empty or sums to zero.
fn weighted_index(weights: &[f32], rng: &mut impl Rng) -> Option<usize> {
    let total: f32 = weights.iter().filter(|w| **w > 0.0).sum();
    if total <= 0.0 {
        return None;
    }
    let roll = rng.random::<f32>() * total;
    let mut acc = 0.0;
    for (i, w) in weights.iter().enumerate() {
        if *w <= 0.0 {
            continue;
        }
        acc += *w;
        if roll < acc {
            return Some(i);
        }
    }
    // Fallback on float rounding — last positive index.
    weights.iter().rposition(|w| *w > 0.0)
}

// ---------------------------------------------------------------------------
// Kind-string helpers (mirror composition::base_kind_tag, which is crate-private)
// ---------------------------------------------------------------------------

fn base_kind(registry: &ContentRegistry, base_id: &str) -> Option<BaseKind> {
    registry.get_base(base_id).map(|b| b.kind.clone())
}

fn base_kind_tag(kind: &BaseKind) -> &'static str {
    match kind {
        BaseKind::Weapon { .. } => "weapon",
        BaseKind::Armor { .. } => "armor",
        BaseKind::Shield { .. } => "shield",
        BaseKind::Rune { .. } => "rune",
        BaseKind::Consumable { .. } => "consumable",
        BaseKind::Reagent => "reagent",
        BaseKind::Trinket => "trinket",
        BaseKind::Quest => "quest",
        BaseKind::Material => "material",
        BaseKind::Currency => "currency",
        BaseKind::Misc => "misc",
    }
}

fn kind_needs_material(tag: &str) -> bool {
    matches!(tag, "weapon" | "armor" | "shield")
}

fn material_fits_base(m: &vaern_items::Material, kind: &BaseKind) -> bool {
    match kind {
        BaseKind::Armor { armor_type, .. } => m.valid_for.contains(armor_type),
        BaseKind::Weapon { .. } => m.weapon_eligible,
        BaseKind::Shield { .. } => m.shield_eligible,
        _ => true,
    }
}

// ---------------------------------------------------------------------------
// Tests — deterministic with seeded RNG
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn load_content() -> Option<ContentRegistry> {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let items_root = manifest.join("../../src/generated/items");
        if !items_root.exists() {
            return None;
        }
        let mut r = ContentRegistry::new();
        r.load_tree(&items_root).ok()?;
        Some(r)
    }

    #[test]
    fn combat_table_drops_at_expected_rate() {
        let Some(reg) = load_content() else { return };
        let table = DropTable::combat(3);
        let mut rng = StdRng::seed_from_u64(42);
        let mut drops = 0;
        for _ in 0..10_000 {
            if roll_drop(&table, &reg, &mut rng).is_some() {
                drops += 1;
            }
        }
        // drop_chance ≈ 0.30 → ~3000 drops expected.
        assert!(drops > 2500 && drops < 3500, "drop count off: {drops}");
    }

    #[test]
    fn named_table_drops_higher_rarity_than_combat() {
        let Some(reg) = load_content() else { return };
        let combat_tbl = DropTable::combat(4);
        let named_tbl = DropTable::named(4);

        let mut combat_rarities = [0; 6];
        let mut named_rarities = [0; 6];
        let mut rng = StdRng::seed_from_u64(7);
        for _ in 0..3_000 {
            if let Some(inst) = roll_drop(&combat_tbl, &reg, &mut rng) {
                let r = reg.resolve(&inst).unwrap();
                combat_rarities[r.rarity as usize] += 1;
            }
            if let Some(inst) = roll_drop(&named_tbl, &reg, &mut rng) {
                let r = reg.resolve(&inst).unwrap();
                named_rarities[r.rarity as usize] += 1;
            }
        }
        // Named mean rarity should exceed combat mean rarity.
        let mean = |counts: [i32; 6]| {
            let total: i32 = counts.iter().sum();
            if total == 0 {
                0.0
            } else {
                counts
                    .iter()
                    .enumerate()
                    .map(|(i, c)| (i as f32) * (*c as f32))
                    .sum::<f32>()
                    / total as f32
            }
        };
        // Named should clearly outpace combat even though both sit in
        // the uncommon/rare band at tier 4. A ~0.5 mean-rarity gap is
        // plenty to feel distinct in play.
        assert!(
            mean(named_rarities) > mean(combat_rarities) + 0.4,
            "named tier should clearly beat combat (named {}, combat {})",
            mean(named_rarities),
            mean(combat_rarities)
        );
    }

    #[test]
    fn rolled_drops_always_resolve_cleanly() {
        let Some(reg) = load_content() else { return };
        let table = DropTable::elite(4);
        let mut rng = StdRng::seed_from_u64(999);
        let mut tested = 0;
        for _ in 0..2_000 {
            if let Some(inst) = roll_drop(&table, &reg, &mut rng) {
                let r = reg.resolve(&inst);
                assert!(r.is_ok(), "drop failed to resolve: {inst:?} → {r:?}");
                tested += 1;
            }
        }
        assert!(tested > 100, "not enough drops to exercise resolver");
    }

    #[test]
    fn shard_only_affixes_never_rolled_randomly() {
        let Some(reg) = load_content() else { return };
        let table = DropTable::named(6);
        let mut rng = StdRng::seed_from_u64(1337);
        for _ in 0..5_000 {
            if let Some(inst) = roll_drop(&table, &reg, &mut rng) {
                for affix_id in &inst.affixes {
                    let a = reg.get_affix(affix_id).unwrap();
                    assert!(
                        a.weight > 0,
                        "shard-only affix {affix_id} appeared on a random drop"
                    );
                    assert!(!a.soulbinds, "soulbound affix appeared on random drop");
                }
            }
        }
    }

    #[test]
    fn affix_count_respects_rarity_slot_budget() {
        let Some(reg) = load_content() else { return };
        let table = DropTable::named(4);
        let mut rng = StdRng::seed_from_u64(2024);
        for _ in 0..2_000 {
            if let Some(inst) = roll_drop(&table, &reg, &mut rng) {
                let r = reg.resolve(&inst).unwrap();
                let slots = rarity_to_max_slots(r.rarity) as usize;
                // Pre-rolled drops leave 1 open slot (or 0 if rarity has 0 slots).
                let max_pre_rolled = slots.saturating_sub(1);
                assert!(
                    inst.affixes.len() <= max_pre_rolled,
                    "rarity {:?} (slots={}) had {} affixes, expected ≤ {}",
                    r.rarity,
                    slots,
                    inst.affixes.len(),
                    max_pre_rolled
                );
            }
        }
    }

    #[test]
    fn seeded_roll_is_deterministic() {
        let Some(reg) = load_content() else { return };
        let table = DropTable::combat(3);
        let a = roll_drop_seeded(&table, &reg, 42);
        let b = roll_drop_seeded(&table, &reg, 42);
        assert_eq!(a, b, "same seed must produce same drop");
    }
}
