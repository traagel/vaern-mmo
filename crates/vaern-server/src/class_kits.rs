//! Per-class-position hotbar composition.
//!
//! Translates a `ClassDef` (pillar, active tiers, label) into 3 concrete
//! `AbilitySpec`s via a hand-authored signature table. Stats come from a
//! baseline tier-scaling function until the combat sim validates real
//! numbers.

use vaern_combat::effects::{EffectKindSpec, EffectSpec};
use vaern_combat::{AbilityShape, AbilitySpec};
use vaern_core::Pillar;
use vaern_data::{AbilityIndex, ClassDef, FlavoredEffect, FlavoredEffectKind};

/// One hotbar slot's source spec: which (pillar, category) ability to draw
/// from. The concrete tier is chosen at runtime — by the class's max active
/// tier in the archetype path, or pinned to 25 in the pillar-starter path.
#[derive(Debug, Clone, Copy)]
pub struct KitSlot {
    pub pillar: Pillar,
    pub category: &'static str,
    /// If `Some`, the slot resolves at this tier instead of the class's max
    /// tier in the pillar. Lets a class's hotbar mix tiers so a single
    /// class (e.g. Fighter) can see single-target / cleave / self-AoE
    /// variety rather than six tier-100 finishers. Reserved for
    /// archetype-unlock path.
    #[allow(dead_code)]
    pub fixed_tier: Option<u8>,
}
#[allow(dead_code)]
const fn slot(pillar: Pillar, category: &'static str) -> KitSlot {
    KitSlot { pillar, category, fixed_tier: None }
}

/// Same as `slot` but pins the tier. Use to give a class hotbar shape
/// variety (cleave at tier 50, self-AoE at tier 75) instead of six
/// tier-100 finishers.
const fn slot_at(pillar: Pillar, category: &'static str, tier: u8) -> KitSlot {
    KitSlot { pillar, category, fixed_tier: Some(tier) }
}

/// Number of keyboard-bound hotbar slots per class. Keys 1..=6 on the client.
pub const HOTBAR_SLOTS: usize = 6;

/// Number of implicit auto-attack "slots" appended after the keyboard slots.
/// Slot 6 = light (LMB), slot 7 = heavy (RMB). Every class shares the same
/// auto-attack profile for now; later these can branch per weapon.
pub const AUTO_ATTACK_SLOTS: usize = 2;

/// Total ability-slot count including keyboard + auto-attacks.
pub const TOTAL_ABILITY_SLOTS: usize = HOTBAR_SLOTS + AUTO_ATTACK_SLOTS;

/// Per-class-position signature: 6 (pillar, category) slots picked to match
/// that position's role mix. Indexed by `class_id` (0..=14, see
/// `vaern-core::VALID_POSITIONS`). Pure-pillar classes (0 Fighter, 4 Wizard,
/// 8 Rogue) have fewer unique categories than slots — the extra slot repeats
/// the primary damage category and will resolve to a different flavored
/// variant once race-based school selection lands.
///
/// Reserved for the archetype-unlock path: starter characters commit to a
/// pillar and use `build_starter_hotbar_by_pillar`; this table feeds the
/// eventual class-unlock flow where a pillar journey graduates into one of
/// the 15 archetypes.
#[allow(dead_code)]
const KIT_SIGNATURES: [[KitSlot; HOTBAR_SLOTS]; 15] = [
    // 0 Fighter  (M100) — full-range offense (25/50/75) + pillar basics +
    //   the tier-100 finisher. Gives the hotbar visible shape variety
    //   (single strike / cone cleave / self-AoE / execute) instead of
    //   six tier-100 finishers.
    [
        slot_at(Pillar::Might, "offense", 25),  // 1 — target: single-strike
        slot_at(Pillar::Might, "offense", 50),  // 2 — cone: cleave
        slot_at(Pillar::Might, "offense", 75),  // 3 — aoe_on_self: whirlblade
        slot(Pillar::Might, "threat"),          // 4 — taunt
        slot(Pillar::Might, "defense"),         // 5 — defense
        slot(Pillar::Might, "offense"),         // 6 — tier-100 finisher (target)
    ],
    // 1 Paladin  (M75 A25)
    [
        slot(Pillar::Might, "offense"),
        slot(Pillar::Might, "defense"),
        slot(Pillar::Might, "threat"),
        slot(Pillar::Arcana, "protection"),
        slot(Pillar::Arcana, "healing"),
        slot(Pillar::Might, "sustain"),
    ],
    // 2 Cleric   (M50 A50)
    [
        slot(Pillar::Arcana, "healing"),
        slot(Pillar::Arcana, "protection"),
        slot(Pillar::Arcana, "damage"),
        slot(Pillar::Arcana, "control"),
        slot(Pillar::Might, "defense"),
        slot(Pillar::Might, "sustain"),
    ],
    // 3 Druid    (M25 A75)
    [
        slot(Pillar::Arcana, "damage"),
        slot(Pillar::Arcana, "healing"),
        slot(Pillar::Arcana, "control"),
        slot(Pillar::Arcana, "enhancement"),
        slot(Pillar::Arcana, "summoning"),
        slot(Pillar::Arcana, "protection"),
    ],
    // 4 Wizard   (A100) — all 6 arcana categories
    [
        slot(Pillar::Arcana, "damage"),
        slot(Pillar::Arcana, "control"),
        slot(Pillar::Arcana, "summoning"),
        slot(Pillar::Arcana, "enhancement"),
        slot(Pillar::Arcana, "protection"),
        slot(Pillar::Arcana, "healing"),
    ],
    // 5 Sorcerer (A75 F25)
    [
        slot(Pillar::Arcana, "damage"),
        slot(Pillar::Arcana, "enhancement"),
        slot(Pillar::Arcana, "control"),
        slot(Pillar::Arcana, "summoning"),
        slot(Pillar::Finesse, "mobility"),
        slot(Pillar::Finesse, "evasion"),
    ],
    // 6 Warlock  (A50 F50)
    [
        slot(Pillar::Arcana, "damage"),
        slot(Pillar::Arcana, "control"),
        slot(Pillar::Arcana, "summoning"),
        slot(Pillar::Finesse, "stealth"),
        slot(Pillar::Finesse, "evasion"),
        slot(Pillar::Arcana, "enhancement"),
    ],
    // 7 Bard     (A25 F75)
    [
        slot(Pillar::Arcana, "enhancement"),
        slot(Pillar::Finesse, "precision"),
        slot(Pillar::Finesse, "trickery"),
        slot(Pillar::Finesse, "utility"),
        slot(Pillar::Finesse, "evasion"),
        slot(Pillar::Finesse, "mobility"),
    ],
    // 8 Rogue    (F100) — all 6 finesse categories
    [
        slot(Pillar::Finesse, "precision"),
        slot(Pillar::Finesse, "stealth"),
        slot(Pillar::Finesse, "evasion"),
        slot(Pillar::Finesse, "mobility"),
        slot(Pillar::Finesse, "trickery"),
        slot(Pillar::Finesse, "utility"),
    ],
    // 9 Ranger   (M25 F75)
    [
        slot(Pillar::Finesse, "precision"),
        slot(Pillar::Might, "offense"),
        slot(Pillar::Finesse, "mobility"),
        slot(Pillar::Finesse, "utility"),
        slot(Pillar::Finesse, "stealth"),
        slot(Pillar::Finesse, "trickery"),
    ],
    // 10 Monk    (M50 F50)
    [
        slot(Pillar::Might, "offense"),
        slot(Pillar::Finesse, "evasion"),
        slot(Pillar::Finesse, "mobility"),
        slot(Pillar::Might, "defense"),
        slot(Pillar::Finesse, "precision"),
        slot(Pillar::Might, "sustain"),
    ],
    // 11 Barbarian (M75 F25)
    [
        slot(Pillar::Might, "offense"),
        slot(Pillar::Might, "sustain"),
        slot(Pillar::Finesse, "mobility"),
        slot(Pillar::Might, "defense"),
        slot(Pillar::Might, "threat"),
        slot(Pillar::Might, "control"),
    ],
    // 12 Duskblade (M50 A25 F25)
    [
        slot(Pillar::Might, "offense"),
        slot(Pillar::Arcana, "enhancement"),
        slot(Pillar::Finesse, "mobility"),
        slot(Pillar::Arcana, "damage"),
        slot(Pillar::Might, "defense"),
        slot(Pillar::Finesse, "evasion"),
    ],
    // 13 Mystic    (M25 A50 F25)
    [
        slot(Pillar::Arcana, "damage"),
        slot(Pillar::Arcana, "healing"),
        slot(Pillar::Finesse, "evasion"),
        slot(Pillar::Arcana, "control"),
        slot(Pillar::Arcana, "protection"),
        slot(Pillar::Finesse, "utility"),
    ],
    // 14 Warden    (M25 A25 F50)
    [
        slot(Pillar::Finesse, "precision"),
        slot(Pillar::Might, "defense"),
        slot(Pillar::Arcana, "protection"),
        slot(Pillar::Arcana, "healing"),
        slot(Pillar::Finesse, "evasion"),
        slot(Pillar::Finesse, "mobility"),
    ],
];

/// Neutral-morality default school per pillar. Faction/Order flavor layer
/// will override later.
const fn default_school(pillar: Pillar) -> &'static str {
    match pillar {
        Pillar::Arcana => "fire",
        Pillar::Might => "blade",
        Pillar::Finesse => "dagger",
    }
}

/// Scaffold tier-baseline: (damage, cooldown_secs, cast_secs, resource_cost).
/// Might and finesse abilities are always instant — melee swings and weapon
/// techniques don't have a channel bar. Only arcana spells have cast times,
/// scaling up with tier (the signature "slow big spell" archetype).
fn tier_stats(pillar: Pillar, tier: u8) -> (f32, f32, f32, f32) {
    let (damage, cooldown, base_cast, cost) = match tier {
        25 => (8.0, 0.8, 0.0, 6.0),
        50 => (18.0, 1.5, 0.8, 12.0),
        75 => (32.0, 3.5, 1.4, 22.0),
        100 => (52.0, 7.0, 2.0, 34.0),
        _ => (0.0, 1.0, 0.0, 0.0),
    };
    let cast_secs = match pillar {
        Pillar::Might | Pillar::Finesse => 0.0,
        Pillar::Arcana => base_cast,
    };
    (damage, cooldown, cast_secs, cost)
}

/// How much threat a category generates per point of damage. Tank-signature
/// categories outscale DPS so a well-kitted tank can hold a target without
/// outdamaging the group.
fn category_threat_multiplier(pillar: Pillar, category: &str) -> f32 {
    match (pillar, category) {
        (Pillar::Might, "threat") => 4.0,
        (Pillar::Arcana, "protection") => 2.5,
        (Pillar::Might, "defense") => 1.5,
        (Pillar::Arcana, "control") => 1.5,
        _ => 1.0,
    }
}

/// One slot's full details — combat spec plus display-facing name/category/pillar.
/// Used to build both the gameplay ability entities AND the client-facing
/// `HotbarSnapshot`.
#[derive(Debug, Clone)]
pub struct HotbarSlotDetail {
    pub spec: AbilitySpec,
    pub variant_name: String,
    pub category: String,
    pub pillar: Pillar,
    pub tier: u8,
}

/// Build a HOTBAR_SLOTS-ability kit for a class position. Each slot resolves
/// to the highest tier the class has active in its (pillar, category); any
/// slot that can't resolve (e.g. class lacks the pillar) degenerates to a
/// weak filler at tier 25 of the slot's pillar. Returns per-slot spec plus
/// display-facing name / category / pillar for the client snapshot.
///
/// Reserved for the archetype-unlock path — fresh characters use
/// `build_starter_hotbar_by_pillar`.
#[allow(dead_code)]
pub fn build_hotbar_detailed(
    class: &ClassDef,
    abilities: &AbilityIndex,
) -> [HotbarSlotDetail; HOTBAR_SLOTS] {
    let signature = &KIT_SIGNATURES[class.class_id as usize];
    std::array::from_fn(|i| resolve_slot(signature[i], class, abilities))
}

/// Starter hotbar signature keyed by core pillar. Characters commit to a
/// pillar at creation (not a class); this fills the 6 keyboard slots with
/// the pillar's 6 natural categories, all at tier 25. As the character
/// grows pillar scores through play, archetype / Order unlocks will
/// reshape the hotbar — that's a later slice.
const fn starter_signature(pillar: Pillar) -> [KitSlot; HOTBAR_SLOTS] {
    match pillar {
        Pillar::Might => [
            slot_at(Pillar::Might, "offense", 25),
            slot_at(Pillar::Might, "defense", 25),
            slot_at(Pillar::Might, "threat", 25),
            slot_at(Pillar::Might, "sustain", 25),
            slot_at(Pillar::Might, "control", 25),
            slot_at(Pillar::Might, "offense", 25),
        ],
        Pillar::Arcana => [
            slot_at(Pillar::Arcana, "damage", 25),
            slot_at(Pillar::Arcana, "healing", 25),
            slot_at(Pillar::Arcana, "control", 25),
            slot_at(Pillar::Arcana, "protection", 25),
            slot_at(Pillar::Arcana, "enhancement", 25),
            slot_at(Pillar::Arcana, "summoning", 25),
        ],
        Pillar::Finesse => [
            slot_at(Pillar::Finesse, "precision", 25),
            slot_at(Pillar::Finesse, "evasion", 25),
            slot_at(Pillar::Finesse, "mobility", 25),
            slot_at(Pillar::Finesse, "stealth", 25),
            slot_at(Pillar::Finesse, "trickery", 25),
            slot_at(Pillar::Finesse, "utility", 25),
        ],
    }
}

/// Build a 6-slot starter hotbar for a character whose only commitment is
/// their core pillar. Every slot is tier 25, sourcing a variant name from
/// `abilities/<pillar>/<category>.yaml` when one exists (falls back to
/// `"<pillar>_<category>"` otherwise). Use in place of the 15-class
/// `build_hotbar_detailed` for fresh characters.
pub fn build_starter_hotbar_by_pillar(
    pillar: Pillar,
    abilities: &AbilityIndex,
) -> [HotbarSlotDetail; HOTBAR_SLOTS] {
    let signature = starter_signature(pillar);
    std::array::from_fn(|i| resolve_starter_slot(signature[i], abilities))
}

fn resolve_starter_slot(slot: KitSlot, abilities: &AbilityIndex) -> HotbarSlotDetail {
    let school = default_school(slot.pillar).to_owned();
    // Starter kits pin tier 25 always — the archetype-unlock path hasn't
    // landed, so higher tiers are unreachable today.
    let tier = 25;
    let variant_name = abilities
        .get(slot.pillar, slot.category)
        .and_then(|def| def.variant_at(tier))
        .map(|v| v.name.clone())
        .unwrap_or_else(|| format!("{}_{}", pillar_str(slot.pillar), slot.category));
    let (damage, cooldown_secs, cast_secs, resource_cost) = tier_stats(slot.pillar, tier);
    let range = default_range_for_school(&school);
    HotbarSlotDetail {
        spec: AbilitySpec {
            damage,
            cooldown_secs,
            cast_secs,
            resource_cost,
            school,
            threat_multiplier: category_threat_multiplier(slot.pillar, slot.category),
            range,
            shape: AbilityShape::Target,
            aoe_radius: 0.0,
            ..AbilitySpec::default()
        },
        variant_name,
        category: slot.category.to_string(),
        pillar: slot.pillar,
        tier,
    }
}

#[allow(dead_code)]
fn resolve_slot(slot: KitSlot, class: &ClassDef, abilities: &AbilityIndex) -> HotbarSlotDetail {
    let school = default_school(slot.pillar).to_owned();
    let class_cap = class.max_tier(slot.pillar).unwrap_or(25);
    // Fixed-tier slots pin to the requested tier, but never above the class's
    // pillar cap (can't use a tier 75 ability on a class that only has 25).
    let cap = match slot.fixed_tier {
        Some(fixed) => fixed.min(class_cap),
        None => class_cap,
    };
    let tier = abilities
        .get(slot.pillar, slot.category)
        .and_then(|def| def.highest_tier_at_or_below(cap))
        .unwrap_or(25);
    let variant_name = abilities
        .get(slot.pillar, slot.category)
        .and_then(|def| def.variant_at(tier))
        .map(|v| v.name.clone())
        .unwrap_or_else(|| format!("{}_{}", pillar_str(slot.pillar), slot.category));
    let (damage, cooldown_secs, cast_secs, resource_cost) = tier_stats(slot.pillar, tier);
    let range = default_range_for_school(&school);
    HotbarSlotDetail {
        spec: AbilitySpec {
            damage,
            cooldown_secs,
            cast_secs,
            resource_cost,
            school,
            threat_multiplier: category_threat_multiplier(slot.pillar, slot.category),
            range,
            shape: AbilityShape::Target,
            aoe_radius: 0.0,
            ..AbilitySpec::default()
        },
        variant_name,
        category: slot.category.to_string(),
        pillar: slot.pillar,
        tier,
    }
}

/// School-based range default, used when a flavored variant doesn't set
/// `range` explicitly. Only clear-cut cases (weapons with obvious reach) are
/// defaulted here — schools like `honor` / `fury` / `acrobat` / `stealth`
/// can drive either melee strikes or personal auras depending on the
/// category, so those fall back to spell range and rely on explicit YAML
/// override per ability.
/// Build the class's light + heavy auto-attack details. Stateless for now —
/// every class gets the same blade-cone profile. A future weapon system can
/// return per-weapon variations (bow = line-projectile heavy, etc.).
pub fn build_auto_attacks() -> [HotbarSlotDetail; AUTO_ATTACK_SLOTS] {
    // Light (LMB): fast, short cooldown, narrow cone. Instant resolution.
    let light = HotbarSlotDetail {
        spec: AbilitySpec {
            damage: 6.0,
            cooldown_secs: 0.5,
            cast_secs: 0.0,
            resource_cost: 0.0,
            school: "blade".into(),
            threat_multiplier: 1.0,
            range: 3.5,
            shape: AbilityShape::Cone,
            aoe_radius: 0.0,
            cone_half_angle_deg: 20.0, // 40° quick slash
            ..AbilitySpec::default()
        },
        variant_name: "light_attack".into(),
        category: "auto".into(),
        pillar: Pillar::Might,
        tier: 0,
    };
    // Heavy (RMB): windup → big cone, longer cooldown. Shows the cast bar
    // during its 0.4s windup so the player gets a commitment signal.
    let heavy = HotbarSlotDetail {
        spec: AbilitySpec {
            damage: 20.0,
            cooldown_secs: 1.5,
            cast_secs: 0.4,
            resource_cost: 0.0,
            school: "blade".into(),
            threat_multiplier: 1.0,
            range: 3.5,
            shape: AbilityShape::Cone,
            aoe_radius: 0.0,
            cone_half_angle_deg: 30.0, // 60° heavy sweep
            ..AbilitySpec::default()
        },
        variant_name: "heavy_attack".into(),
        category: "auto".into(),
        pillar: Pillar::Might,
        tier: 0,
    };
    [light, heavy]
}

pub fn default_range_for_school(school: &str) -> f32 {
    match school {
        // Melee weapon schools.
        "blade" | "blunt" | "shield" | "dagger" | "spear" | "claw" | "fang" | "unarmed" => 3.5,
        // Ranged weapon schools.
        "bow" | "crossbow" => 30.0,
        // Everything else defaults to spell range; set explicit range in YAML
        // for self-only / non-default ranges.
        _ => 25.0,
    }
}

/// Overlay optional YAML overrides from a `FlavoredAbility` onto an already-
/// built `AbilitySpec`. Each field replaces the tier/pillar/school default
/// only when set.
pub fn apply_flavored_overrides(spec: &mut AbilitySpec, flavored: &vaern_data::FlavoredAbility) {
    if let Some(v) = flavored.damage {
        spec.damage = v;
    }
    if let Some(v) = flavored.cast_secs {
        spec.cast_secs = v;
    }
    if let Some(v) = flavored.cooldown_secs {
        spec.cooldown_secs = v;
    }
    if let Some(v) = flavored.resource_cost {
        spec.resource_cost = v;
    }
    if let Some(v) = flavored.range {
        spec.range = v;
    }
    if let Some(shape) = flavored.shape {
        spec.shape = match shape {
            vaern_data::FlavoredShape::Target => AbilityShape::Target,
            vaern_data::FlavoredShape::AoeOnTarget => AbilityShape::AoeOnTarget,
            vaern_data::FlavoredShape::AoeOnSelf => AbilityShape::AoeOnSelf,
            vaern_data::FlavoredShape::Cone => AbilityShape::Cone,
            vaern_data::FlavoredShape::Line => AbilityShape::Line,
            vaern_data::FlavoredShape::Projectile => AbilityShape::Projectile,
        };
    }
    if let Some(v) = flavored.aoe_radius {
        spec.aoe_radius = v;
    }
    if let Some(v) = flavored.cone_half_angle_deg {
        spec.cone_half_angle_deg = v;
    }
    if let Some(v) = flavored.line_width {
        spec.line_width = v;
    }
    if let Some(v) = flavored.projectile_speed {
        spec.projectile_speed = v;
    }
    if let Some(v) = flavored.projectile_radius {
        spec.projectile_radius = v;
    }
    if let Some(effect) = &flavored.applies_effect {
        spec.applies_effect = Some(flavored_effect_to_spec(effect));
    }
}

/// Convert data-layer `FlavoredEffect` to the runtime `EffectSpec`.
/// Identity mapping — the two types are kept separate only to avoid
/// pulling bevy into the vaern-data crate.
fn flavored_effect_to_spec(f: &FlavoredEffect) -> EffectSpec {
    let kind = match &f.kind {
        FlavoredEffectKind::Dot {
            dps,
            tick_interval,
            school,
        } => EffectKindSpec::Dot {
            dps: *dps,
            tick_interval: *tick_interval,
            school: school.clone(),
        },
        FlavoredEffectKind::Slow { speed_mult } => EffectKindSpec::Slow {
            speed_mult: *speed_mult,
        },
    };
    EffectSpec {
        id: f.id.clone(),
        duration_secs: f.duration_secs,
        kind,
    }
}

pub fn pillar_str(pillar: Pillar) -> &'static str {
    match pillar {
        Pillar::Might => "might",
        Pillar::Arcana => "arcana",
        Pillar::Finesse => "finesse",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn generated_root() -> std::path::PathBuf {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        manifest.join("../../src/generated").canonicalize().unwrap()
    }

    /// Test-only shim preserving the old API: returns just the `AbilitySpec`
    /// per slot. Production code uses `build_hotbar_detailed` directly.
    fn build_hotbar(class: &ClassDef, abilities: &AbilityIndex) -> [AbilitySpec; HOTBAR_SLOTS] {
        build_hotbar_detailed(class, abilities).map(|d| d.spec)
    }

    #[test]
    fn every_class_produces_three_specs() {
        let classes = vaern_data::load_classes(generated_root().join("archetypes")).unwrap();
        let abilities = vaern_data::load_abilities(generated_root().join("abilities")).unwrap();
        for class in &classes {
            let kit = build_hotbar(class, &abilities);
            for (i, spec) in kit.iter().enumerate() {
                assert!(
                    spec.damage > 0.0,
                    "class {} slot {} has zero damage",
                    class.internal_label,
                    i
                );
                assert!(!spec.school.is_empty());
            }
        }
    }

    #[test]
    fn wizard_kit_is_all_arcana_high_tier() {
        let classes = vaern_data::load_classes(generated_root().join("archetypes")).unwrap();
        let abilities = vaern_data::load_abilities(generated_root().join("abilities")).unwrap();
        let wizard = classes.iter().find(|c| c.class_id == 4).unwrap();
        let kit = build_hotbar(wizard, &abilities);
        for spec in &kit {
            assert_eq!(spec.school, "fire");
            assert!(spec.damage >= 52.0, "wizard at A100 should land tier-100 stats");
        }
    }

    #[test]
    fn fighter_kit_is_might_based() {
        let classes = vaern_data::load_classes(generated_root().join("archetypes")).unwrap();
        let abilities = vaern_data::load_abilities(generated_root().join("abilities")).unwrap();
        let fighter = classes.iter().find(|c| c.class_id == 0).unwrap();
        let kit = build_hotbar(fighter, &abilities);
        for spec in &kit {
            assert_eq!(spec.school, "blade");
        }
        // Signature spreads offense across tiers 25/50/75 + a tier-100 finisher
        // so the hotbar shows shape variety rather than six identical finishers.
        // Slot 0 = tier-25 target, slot 5 = tier-100 finisher.
        assert!(kit[0].damage <= 10.0, "slot 0 should be tier-25 (8 dmg), got {}", kit[0].damage);
        assert!(kit[5].damage >= 52.0, "slot 5 should be tier-100 (52 dmg), got {}", kit[5].damage);
    }

    #[test]
    fn fighter_threat_slot_has_high_multiplier() {
        let classes = vaern_data::load_classes(generated_root().join("archetypes")).unwrap();
        let abilities = vaern_data::load_abilities(generated_root().join("abilities")).unwrap();
        let fighter = classes.iter().find(|c| c.class_id == 0).unwrap();
        let kit = build_hotbar(fighter, &abilities);
        // Fighter signature (new): offense@25, offense@50, offense@75, threat,
        // defense, offense@100. Threat mult table: offense=1.0, threat=4.0,
        // defense=1.5.
        assert_eq!(kit[0].threat_multiplier, 1.0); // offense
        assert_eq!(kit[3].threat_multiplier, 4.0); // threat
        assert_eq!(kit[4].threat_multiplier, 1.5); // defense
    }

    #[test]
    fn might_and_finesse_kits_are_instant() {
        let classes = vaern_data::load_classes(generated_root().join("archetypes")).unwrap();
        let abilities = vaern_data::load_abilities(generated_root().join("abilities")).unwrap();
        // Fighter (all Might), Rogue (all Finesse), Barbarian (M75/F25) — every
        // slot in these kits should have cast_secs == 0.
        for class_id in [0u8, 8, 11] {
            let class = classes.iter().find(|c| c.class_id == class_id).unwrap();
            let kit = build_hotbar(class, &abilities);
            for (i, spec) in kit.iter().enumerate() {
                assert_eq!(
                    spec.cast_secs, 0.0,
                    "{} slot {} should be instant, got cast_secs = {}",
                    class.internal_label, i, spec.cast_secs
                );
            }
        }
    }

    #[test]
    fn wizard_arcana_spells_have_cast_times() {
        let classes = vaern_data::load_classes(generated_root().join("archetypes")).unwrap();
        let abilities = vaern_data::load_abilities(generated_root().join("abilities")).unwrap();
        let wizard = classes.iter().find(|c| c.class_id == 4).unwrap();
        let kit = build_hotbar(wizard, &abilities);
        // Wizard is A100, so every slot resolves to tier 100 → 2s cast.
        for (i, spec) in kit.iter().enumerate() {
            assert_eq!(spec.school, "fire");
            assert_eq!(
                spec.cast_secs, 2.0,
                "Wizard slot {i} should have a 2s arcana cast, got {}",
                spec.cast_secs
            );
        }
    }

    #[test]
    fn wizard_has_baseline_threat() {
        let classes = vaern_data::load_classes(generated_root().join("archetypes")).unwrap();
        let abilities = vaern_data::load_abilities(generated_root().join("abilities")).unwrap();
        let wizard = classes.iter().find(|c| c.class_id == 4).unwrap();
        let kit = build_hotbar(wizard, &abilities);
        // Wizard signature: damage, control, summoning — 1.0, 1.5, 1.0.
        assert_eq!(kit[0].threat_multiplier, 1.0);
        assert_eq!(kit[1].threat_multiplier, 1.5);
        assert_eq!(kit[2].threat_multiplier, 1.0);
    }

    #[test]
    fn starter_kit_might_is_all_melee_instant() {
        let abilities = vaern_data::load_abilities(generated_root().join("abilities")).unwrap();
        let kit = build_starter_hotbar_by_pillar(Pillar::Might, &abilities);
        for (i, s) in kit.iter().enumerate() {
            assert_eq!(s.pillar, Pillar::Might, "slot {i}");
            assert_eq!(s.tier, 25, "starter slots are tier 25, slot {i}");
            assert_eq!(s.spec.school, "blade", "slot {i}");
            assert_eq!(s.spec.cast_secs, 0.0, "Might kit is all instant, slot {i}");
            assert!(s.spec.damage > 0.0, "slot {i} zero damage");
        }
        // Threat slot (index 2) has the tank-signature multiplier.
        assert_eq!(kit[2].spec.threat_multiplier, 4.0);
    }

    #[test]
    fn starter_kit_arcana_has_cast_times_and_mana_cost() {
        let abilities = vaern_data::load_abilities(generated_root().join("abilities")).unwrap();
        let kit = build_starter_hotbar_by_pillar(Pillar::Arcana, &abilities);
        for (i, s) in kit.iter().enumerate() {
            assert_eq!(s.pillar, Pillar::Arcana, "slot {i}");
            assert_eq!(s.tier, 25, "slot {i}");
            assert_eq!(s.spec.school, "fire");
            // At tier 25, arcana cast_secs is still 0 (tier_stats table).
            // Verify the resource cost though — arcana abilities cost mana.
            assert!(s.spec.resource_cost > 0.0, "arcana slot {i} should cost mana");
        }
    }

    #[test]
    fn starter_kit_finesse_is_dagger_school() {
        let abilities = vaern_data::load_abilities(generated_root().join("abilities")).unwrap();
        let kit = build_starter_hotbar_by_pillar(Pillar::Finesse, &abilities);
        for s in &kit {
            assert_eq!(s.pillar, Pillar::Finesse);
            assert_eq!(s.spec.school, "dagger");
            assert_eq!(s.spec.cast_secs, 0.0);
        }
        // Category names match the signature: precision/evasion/mobility/...
        let cats: Vec<&str> = kit.iter().map(|s| s.category.as_str()).collect();
        assert_eq!(
            cats,
            vec!["precision", "evasion", "mobility", "stealth", "trickery", "utility"]
        );
    }
}
