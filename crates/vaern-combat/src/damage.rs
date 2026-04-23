//! Stat-aware damage resolution.
//!
//! Every hit passes through `compute_damage`, which folds:
//!
//!   * **Caster scaling** — melee/spell damage multipliers from
//!     pillar-derived primaries; weapon min/max damage for physical
//!     attacks; crit roll against `total_crit_pct`.
//!   * **Target mitigation** — armor reduction (bludgeoning-agnostic
//!     for now, content-specific armor math per KCD layered model is
//!     deferred); per-channel resist using the school's DamageType
//!     index into `resist_total[12]`.
//!
//! No stats present (e.g. NPC without a `CombinedStats` component)
//! falls through to raw damage unmodified — keeps legacy NPC balance
//! intact until their stats get wired.

use bevy::prelude::*;
use rand::Rng;
use vaern_core::DamageType;
use vaern_stats::CombinedStats;

use crate::components::Stamina;
use crate::effects::{EffectKind, StanceKind, StatusEffects};

/// Armor-mitigation K constant. `reduction = armor / (armor + K)`.
/// Higher K = armor is less impactful. K=200 means 100 armor → 33%
/// reduction, 500 armor → 71%. Tune alongside content tier rollout.
const ARMOR_K: f32 = 200.0;

/// Each point of `resist_total[channel]` cuts 0.5% incoming damage
/// on that channel. Capped at 80% total reduction so a stacked resist
/// setup doesn't fully negate — you still take chip damage.
const RESIST_PER_POINT: f32 = 0.005;
const RESIST_CAP: f32 = 0.80;

/// Base crit multiplier. Classic D&D-inspired ×1.5 (not ×2) so crits
/// feel rewarding without trivializing fights. Future weapon modifiers
/// can bump via an affix (e.g. "of Deep Cuts +0.1 crit mult").
const CRIT_MULTIPLIER: f32 = 1.5;

/// Result of a damage computation. `was_crit` feeds VFX / combat log.
#[derive(Debug, Clone, Copy)]
pub struct DamageResult {
    pub final_damage: f32,
    pub was_crit: bool,
}

/// Fold caster + target stats into final damage for one hit.
///
/// * `base_damage` — the ability's spec damage (already resource/cast
///   consumed by the caller).
/// * `school` — ability's school id. Used for damage-type lookup.
/// * `caster` — optional; None means "no scaling, no crit."
/// * `caster_damage_bonus` — additive mult from active `StatMods` effects
///   on the caster (e.g. +0.2 from a damage potion). Call
///   `status_damage_bonus(&StatusEffects)` once per caster and pass the
///   scalar here. 0.0 means no buff contribution.
/// * `target` — optional; None means "no mitigation."
/// * `target_resist_bonus` — additive resist on the school's damage
///   type from active `StatMods` effects on the target (e.g. +40 fire
///   from a fire-resist potion). Call `status_resist_bonus(fx, dt)`
///   once per target and pass the scalar. Stacks onto
///   `target.resist_total[dt]` before the mitigation curve.
/// * `rng` — any `Rng`. Production systems pass `thread_rng()`;
///   tests pass a seeded `StdRng` for determinism.
pub fn compute_damage(
    base_damage: f32,
    school: &str,
    caster: Option<&CombinedStats>,
    caster_damage_bonus: f32,
    target: Option<&CombinedStats>,
    target_resist_bonus: f32,
    rng: &mut impl Rng,
) -> DamageResult {
    let mut damage = base_damage;
    let mut was_crit = false;

    // ---- Caster contribution ----
    if let Some(cs) = caster {
        // Physical schools add weapon damage on top of the ability's
        // base. Non-physical (magic) schools ignore weapon dmg.
        if is_physical(school) && cs.weapon_max_dmg > 0.0 {
            let lo = cs.weapon_min_dmg.max(0.0);
            let hi = cs.weapon_max_dmg.max(lo + 0.001);
            damage += rng.random_range(lo..=hi);
        }
        // Averaged mult — for a proper split, peek at school pillar
        // and pick melee_mult vs spell_mult. For now the average is
        // close enough and dodges a school-registry dep in combat.
        // StatMods buffs (e.g. haste/damage potions) sum additively
        // into the multiplier alongside pillar-derived melee/spell.
        let mult = (cs.melee_mult + cs.spell_mult) * 0.5 + caster_damage_bonus;
        damage *= mult;
        // Crit roll — total_crit_pct is 0..=100.
        if rng.random::<f32>() * 100.0 < cs.total_crit_pct {
            damage *= CRIT_MULTIPLIER;
            was_crit = true;
        }
    } else if caster_damage_bonus != 0.0 {
        // No CombinedStats, but caster may still have a buff from a
        // potion — apply the additive mult to raw damage.
        damage *= 1.0 + caster_damage_bonus;
    }

    // ---- Target mitigation ----
    if let Some(ts) = target {
        // Armor vs everything (approximation — KCD-phased per-channel
        // layered math is Phase B of the armor plan).
        let armor_reduction = ts.armor as f32 / (ts.armor as f32 + ARMOR_K);
        damage *= 1.0 - armor_reduction;

        // Per-channel resist lookup. Physical schools map to
        // slashing/piercing/bludgeoning; magical schools match their
        // own DamageType by name. Unknown schools skip resist. Active
        // `StatMods` buffs (e.g. fire-resist potion) contribute on top
        // of gear-baked resists; the 80% cap applies to the total.
        if let Some(dt) = school_to_damage_type(school) {
            let resist = ts.resist_total[dt as usize] + target_resist_bonus;
            let resist_pct = (resist * RESIST_PER_POINT).clamp(0.0, RESIST_CAP);
            damage *= 1.0 - resist_pct;
        }
    } else if target_resist_bonus > 0.0 {
        // No CombinedStats, but the target may still be buffed (NPC
        // with a resist potion from a rite, etc.). Apply the buff-only
        // mitigation against the matching damage type.
        if let Some(dt) = school_to_damage_type(school) {
            let resist_pct =
                (target_resist_bonus * RESIST_PER_POINT).clamp(0.0, RESIST_CAP);
            damage *= 1.0 - resist_pct;
            let _ = dt;
        }
    }

    DamageResult {
        final_damage: damage.max(1.0),
        was_crit,
    }
}

/// Sum `damage_mult_add` across all active `StatMods` on the caster.
/// Returns 0.0 when there are no effects or no StatMods. A haste/damage
/// potion that applies `damage_mult_add = 0.2` pushes +20% through the
/// mult layer alongside the pillar-derived base.
pub fn status_damage_bonus(effects: Option<&StatusEffects>) -> f32 {
    let Some(fx) = effects else { return 0.0 };
    let mut sum = 0.0;
    for e in &fx.0 {
        if let EffectKind::StatMods { damage_mult_add, .. } = &e.kind {
            sum += damage_mult_add;
        }
    }
    sum
}

/// Sum resist added on `dt` across all active `StatMods` on the target.
/// Folded into `resist_total[dt]` before the mitigation curve — the
/// 80% resist cap in `compute_damage` still applies. A fire-resist
/// potion (+40 fire) stacks onto gear resists and any material
/// bonus from runes.
pub fn status_resist_bonus(effects: Option<&StatusEffects>, dt: vaern_core::DamageType) -> f32 {
    let Some(fx) = effects else { return 0.0 };
    let idx = dt as usize;
    let mut sum = 0.0;
    for e in &fx.0 {
        if let EffectKind::StatMods { resist_adds, .. } = &e.kind {
            sum += resist_adds[idx];
        }
    }
    sum
}

/// Convenience: `status_resist_bonus` keyed by school id. Returns 0.0
/// when the school doesn't map to a `DamageType` (untyped physical
/// schools, unknown magical schools). Call sites work with school
/// strings from `AbilitySpec`, so this saves them the mapping
/// boilerplate.
pub fn status_resist_bonus_for_school(
    effects: Option<&StatusEffects>,
    school: &str,
) -> f32 {
    match school_to_damage_type(school) {
        Some(dt) => status_resist_bonus(effects, dt),
        None => 0.0,
    }
}

/// Cosine of the half-angle defining a target's "front cone" for block
/// reduction. 0.0 = 90° half-angle (180° arc — the entire front half).
/// A shield stance realistically protects ~150° — we're a bit generous
/// here to keep the feature forgiving to learn.
const BLOCK_FRONTAL_COS: f32 = 0.0;

/// Apply the target's active stances to a freshly-computed damage
/// result. Mutates `StatusEffects` when a parry consumes itself, and
/// debits `Stamina` for the parry cost. Returns the final damage that
/// should be applied to HP.
///
/// * **Parry** — if the target has an active parry window, fully
///   negates the hit (damage → 0) and removes the parry effect.
///   Subsequent hits in the same frame pass through untouched.
/// * **Block** — if the target has an active Block stance, reduces
///   damage by `frontal_reduction` when the caster is in the target's
///   front cone, or `flank_reduction` otherwise. Back hits (caster
///   behind target's facing) get no reduction at all.
///
/// `caster_pos` and `target_tf` let us compute hit angle; pass
/// `caster_pos = target_tf.translation` for shapes with no real
/// direction (e.g. AoeOnSelf) to degrade gracefully — the dot
/// product ends up 1.0 so the hit is treated as frontal.
pub fn apply_stances(
    raw_damage: f32,
    caster_pos: Vec3,
    target_tf: &Transform,
    effects: Option<&mut StatusEffects>,
    stamina: Option<&mut Stamina>,
) -> f32 {
    let Some(effects) = effects else { return raw_damage };

    // Parry wins over Block — the player actively timed this.
    if effects.active_parry().is_some() {
        if let Some(cost) = effects.consume_parry() {
            if let Some(s) = stamina {
                s.current = (s.current - cost).max(0.0);
            }
        }
        return 0.0;
    }

    if let Some(StanceKind::Block {
        frontal_reduction,
        flank_reduction,
        ..
    }) = effects.active_block()
    {
        let facing = target_tf.rotation * Vec3::NEG_Z;
        let mut to_caster = caster_pos - target_tf.translation;
        to_caster.y = 0.0;
        let d2 = to_caster.length_squared();
        // Self-hit / zero-distance: treat as frontal.
        let reduction = if d2 < 1e-4 {
            frontal_reduction
        } else {
            let to_caster = to_caster / d2.sqrt();
            let mut facing_flat = Vec3::new(facing.x, 0.0, facing.z);
            if facing_flat.length_squared() > 1e-4 {
                facing_flat = facing_flat.normalize();
            } else {
                facing_flat = Vec3::NEG_Z;
            }
            let dot = facing_flat.dot(to_caster);
            if dot >= BLOCK_FRONTAL_COS {
                // Interpolate: dot=1 (dead-front) → full frontal; dot=0
                // (sideways) → flank reduction. Back hits (dot<0) fall
                // through to no reduction.
                let t = dot.clamp(0.0, 1.0);
                flank_reduction + (frontal_reduction - flank_reduction) * t
            } else {
                0.0
            }
        };
        return (raw_damage * (1.0 - reduction)).max(1.0);
    }

    raw_damage
}

fn is_physical(school: &str) -> bool {
    matches!(
        school,
        "blade" | "blunt" | "spear" | "unarmed" | "dagger" | "bow" | "crossbow" | "thrown"
    )
}

/// Map school id → DamageType for resist-channel lookups. Uses
/// `DamageType::from_str` first (catches fire/cold/lightning/etc.
/// schools that ARE damage types) then falls back to a weapon-school
/// hardcoded map for the physical family.
fn school_to_damage_type(school: &str) -> Option<DamageType> {
    if let Some(dt) = DamageType::from_str(school) {
        return Some(dt);
    }
    match school {
        "blade" => Some(DamageType::Slashing),
        "blunt" | "unarmed" | "earth" => Some(DamageType::Bludgeoning),
        "spear" | "dagger" | "bow" | "crossbow" | "thrown" | "nature" => Some(DamageType::Piercing),
        "alchemy" => Some(DamageType::Acid),
        "acrobat" | "trickster" => Some(DamageType::Bludgeoning),
        "silent" => Some(DamageType::Piercing),
        "arcane" => Some(DamageType::Force),
        "frost" => Some(DamageType::Cold),
        "light" | "devotion" => Some(DamageType::Radiant),
        "shadow" => Some(DamageType::Necrotic),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{SeedableRng, rngs::StdRng};
    use vaern_core::DAMAGE_TYPE_COUNT;
    use vaern_stats::CombinedStats;

    fn blank_stats() -> CombinedStats {
        CombinedStats {
            hp_max: 100,
            mana_max: 50,
            melee_mult: 1.0,
            spell_mult: 1.0,
            total_crit_pct: 0.0,
            total_dodge_pct: 0.0,
            total_haste_pct: 0.0,
            total_parry_pct: 0.0,
            carry_kg: 50.0,
            armor: 0,
            fortune_pct: 0.0,
            mp5: 0.0,
            weapon_min_dmg: 0.0,
            weapon_max_dmg: 0.0,
            block_chance_pct: 0.0,
            block_value: 0,
            resist_total: [0.0; DAMAGE_TYPE_COUNT],
            luck: 0,
            leech_pct: 0.0,
            move_speed_pct: 0.0,
            avoidance_pct: 0.0,
        }
    }

    #[test]
    fn no_stats_passes_through() {
        let mut rng = StdRng::seed_from_u64(1);
        let r = compute_damage(50.0, "blade", None, 0.0, None, 0.0, &mut rng);
        assert_eq!(r.final_damage, 50.0);
        assert!(!r.was_crit);
    }

    #[test]
    fn high_might_caster_hits_harder() {
        let mut rng = StdRng::seed_from_u64(2);
        let mut c = blank_stats();
        c.melee_mult = 1.5; // +50% melee
        c.weapon_min_dmg = 4.0;
        c.weapon_max_dmg = 8.0;
        let r = compute_damage(20.0, "blade", Some(&c), 0.0, None, 0.0, &mut rng);
        // Base 20 + weapon 4..8 = 24..28, × mean_mult 1.25 = 30..35.
        assert!(r.final_damage > 25.0, "expected boosted damage, got {}", r.final_damage);
    }

    #[test]
    fn armored_target_takes_less() {
        let mut rng = StdRng::seed_from_u64(3);
        let unarmored =
            compute_damage(100.0, "blade", None, 0.0, None, 0.0, &mut rng).final_damage;
        let mut t = blank_stats();
        t.armor = 200; // → 50% reduction
        let armored =
            compute_damage(100.0, "blade", None, 0.0, Some(&t), 0.0, &mut rng).final_damage;
        assert!(
            armored < unarmored * 0.6,
            "armored should take significantly less: {} vs {}",
            armored, unarmored
        );
    }

    #[test]
    fn fire_resist_reduces_fire_damage_only() {
        let mut rng = StdRng::seed_from_u64(4);
        let mut t = blank_stats();
        t.resist_total[DamageType::Fire.index()] = 100.0; // 50% fire reduction
        let fire =
            compute_damage(100.0, "fire", None, 0.0, Some(&t), 0.0, &mut rng).final_damage;
        let cold =
            compute_damage(100.0, "cold", None, 0.0, Some(&t), 0.0, &mut rng).final_damage;
        assert!(fire < cold, "fire should be resisted, cold not: fire={fire} cold={cold}");
    }

    #[test]
    fn guaranteed_crit_produces_multiplier() {
        let mut rng = StdRng::seed_from_u64(5);
        let mut c = blank_stats();
        c.total_crit_pct = 100.0; // always crits
        let r = compute_damage(50.0, "blade", Some(&c), 0.0, None, 0.0, &mut rng);
        assert!(r.was_crit);
        // Base 50 × melee/spell avg 1.0 × crit 1.5 = 75 (weapon roll 0..0).
        assert!(r.final_damage >= 74.0 && r.final_damage <= 76.0);
    }

    #[test]
    fn damage_floors_at_one() {
        let mut rng = StdRng::seed_from_u64(6);
        let mut t = blank_stats();
        t.armor = 9999;
        let r = compute_damage(10.0, "blade", None, 0.0, Some(&t), 0.0, &mut rng);
        assert!(r.final_damage >= 1.0);
    }

    #[test]
    fn stat_mods_bonus_boosts_damage_with_stats() {
        let mut rng = StdRng::seed_from_u64(7);
        let c = blank_stats(); // melee_mult = spell_mult = 1.0, no crit
        let unbuffed =
            compute_damage(100.0, "blade", Some(&c), 0.0, None, 0.0, &mut rng).final_damage;
        // +0.2 stat-mods bonus pushes mean mult from 1.0 to 1.2.
        let mut rng2 = StdRng::seed_from_u64(7);
        let buffed =
            compute_damage(100.0, "blade", Some(&c), 0.2, None, 0.0, &mut rng2).final_damage;
        assert!(
            (buffed - unbuffed * 1.2).abs() < 0.01,
            "buffed {buffed} should be 1.2× unbuffed {unbuffed}"
        );
    }

    #[test]
    fn stat_mods_bonus_without_stats_still_applies() {
        let mut rng = StdRng::seed_from_u64(8);
        // No caster stats — pillar math skipped — but the buff still multiplies
        // raw damage. Proves NPCs (which today lack CombinedStats) can still
        // benefit from a buff effect.
        let r = compute_damage(100.0, "blade", None, 0.5, None, 0.0, &mut rng);
        assert!((r.final_damage - 150.0).abs() < 0.01);
    }

    #[test]
    fn status_damage_bonus_sums_stat_mods() {
        use crate::effects::{EffectKind, StatusEffect, StatusEffects};
        use bevy::ecs::entity::Entity;

        // Placeholder source — value is irrelevant for this test.
        let src = Entity::PLACEHOLDER;
        let zero_resists = [0.0f32; vaern_core::DAMAGE_TYPE_COUNT];
        let mut fx = StatusEffects::default();
        fx.0.push(StatusEffect {
            id: "bless".into(),
            source: src,
            remaining_secs: 10.0,
            tick_interval: 0.0,
            tick_remaining: 0.0,
            kind: EffectKind::StatMods {
                damage_mult_add: 0.15,
                resist_adds: zero_resists,
            },
        });
        fx.0.push(StatusEffect {
            id: "rage".into(),
            source: src,
            remaining_secs: 5.0,
            tick_interval: 0.0,
            tick_remaining: 0.0,
            kind: EffectKind::StatMods {
                damage_mult_add: 0.10,
                resist_adds: zero_resists,
            },
        });
        // Non-StatMods effects don't contribute.
        fx.0.push(StatusEffect {
            id: "burning".into(),
            source: src,
            remaining_secs: 3.0,
            tick_interval: 1.0,
            tick_remaining: 1.0,
            kind: EffectKind::Dot {
                damage_per_tick: 3.0,
                school: "fire".into(),
                threat_multiplier: 1.0,
            },
        });
        assert!((status_damage_bonus(Some(&fx)) - 0.25).abs() < 1e-6);
        assert_eq!(status_damage_bonus(None), 0.0);
    }

    #[test]
    fn status_resist_bonus_sums_stat_mods_for_matching_channel() {
        use crate::effects::{EffectKind, StatusEffect, StatusEffects};
        use vaern_core::DamageType;

        let src = Entity::PLACEHOLDER;
        let mut fx = StatusEffects::default();
        // Two overlapping fire-resist buffs stack.
        let mut adds_a = [0.0f32; vaern_core::DAMAGE_TYPE_COUNT];
        adds_a[DamageType::Fire.index()] = 30.0;
        let mut adds_b = [0.0f32; vaern_core::DAMAGE_TYPE_COUNT];
        adds_b[DamageType::Fire.index()] = 15.0;
        fx.0.push(StatusEffect {
            id: "fire_resist_lesser".into(),
            source: src,
            remaining_secs: 10.0,
            tick_interval: 0.0,
            tick_remaining: 0.0,
            kind: EffectKind::StatMods {
                damage_mult_add: 0.0,
                resist_adds: adds_a,
            },
        });
        fx.0.push(StatusEffect {
            id: "fire_resist_greater".into(),
            source: src,
            remaining_secs: 10.0,
            tick_interval: 0.0,
            tick_remaining: 0.0,
            kind: EffectKind::StatMods {
                damage_mult_add: 0.0,
                resist_adds: adds_b,
            },
        });
        assert_eq!(status_resist_bonus(Some(&fx), DamageType::Fire), 45.0);
        // Unrelated channel is untouched.
        assert_eq!(status_resist_bonus(Some(&fx), DamageType::Cold), 0.0);
        assert_eq!(status_resist_bonus(None, DamageType::Fire), 0.0);
    }

    #[test]
    fn resist_buff_reduces_incoming_damage_of_matching_school() {
        // Compare raw damage vs damage against a target whose StatMods
        // adds fire resist. Both cases use None for CombinedStats so
        // only the buff-side math is exercised.
        let mut rng = StdRng::seed_from_u64(10);
        let baseline =
            compute_damage(100.0, "fire", None, 0.0, None, 0.0, &mut rng).final_damage;
        let mut rng2 = StdRng::seed_from_u64(10);
        // +100 fire resist → 50% mitigation at RESIST_PER_POINT = 0.005.
        let buffed =
            compute_damage(100.0, "fire", None, 0.0, None, 100.0, &mut rng2).final_damage;
        assert!((baseline - 100.0).abs() < 0.01, "baseline no-op gives raw damage");
        assert!((buffed - 50.0).abs() < 0.01, "100 resist → 50% reduction, got {buffed}");
    }

    #[test]
    fn resist_buff_is_capped_at_eighty_percent() {
        // Extremely large resist bonus should cap at RESIST_CAP = 0.80.
        let mut rng = StdRng::seed_from_u64(11);
        let r = compute_damage(100.0, "fire", None, 0.0, None, 10_000.0, &mut rng);
        // 80% reduction → 20 damage, floored at >=1.0.
        assert!((r.final_damage - 20.0).abs() < 0.01, "got {}", r.final_damage);
    }
}
