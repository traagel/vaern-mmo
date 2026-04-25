//! Derive `CombinedStats` for mob entities from bestiary data.
//!
//! Input is the mob's `creature_type` + `armor_class` + `NpcKind` +
//! effective level. Output is a `CombinedStats` component attached at
//! spawn so `vaern-combat::compute_damage` mitigates hits against
//! real armor and per-channel resists.
//!
//! Mapping rules:
//!
//!  * **armor (u32):** inverse of the armor mitigation formula
//!    (`armor = K × r / (1 - r)`) where r = `armor_class.physical_reduction`.
//!    Produces an armor value that, fed back through
//!    `damage × (1 - armor/(armor+K))`, reproduces the authored
//!    fractional reduction.
//!
//!  * **Magical resist (9 channels):** `armor_class.magic_reduction`
//!    distributes evenly across the 9 magical damage channels as a
//!    baseline. Creature-type resistances bump individual channels.
//!
//!  * **Creature-type resistances:** `creature_type.resistances` map
//!    uses school ids (fire/frost/light/blade/...) with values in
//!    [-1.0, +1.0] where negative = resistant (takes less damage),
//!    positive = vulnerable. Converted to resist_total points via
//!    `resist_points = -200 × modifier` so:
//!      - `-0.5` (50% less damage) → +100 resist → 50% reduction
//!      - `+0.25` (25% more damage) → -50 resist → amplifies 25%
//!
//!  * **NpcKind multiplier:** Combat 1.0×, Elite 1.25×, Named 1.5×
//!    on both armor and resist. Gives a readable power-tier bump
//!    without rewriting per-mob tuning.

use vaern_combat::NpcKind;
use vaern_core::{DAMAGE_TYPE_COUNT, DamageType};
use vaern_data::{ArmorClass, CreatureType};
use vaern_stats::CombinedStats;

/// Must match `vaern_combat::damage::ARMOR_K`. Extracted as a local
/// constant here because that symbol is module-private. If you retune
/// the armor formula in vaern-combat, sync this too.
const ARMOR_K: f32 = 200.0;

/// Must match `vaern_combat::damage::RESIST_PER_POINT`.
const RESIST_PER_POINT: f32 = 0.005;

/// Multipliers on armor + resist by rarity. Elite/Named mobs present
/// a meaningfully tougher mitigation profile without changing their
/// bestiary data.
fn rarity_mult(kind: NpcKind) -> f32 {
    match kind {
        NpcKind::Combat => 1.0,
        NpcKind::Elite => 1.25,
        NpcKind::Named => 1.5,
        NpcKind::QuestGiver | NpcKind::Vendor => 1.0, // unused — non-combat
    }
}

/// School id → DamageType. Matches the authored `damage_type` field in
/// `src/generated/schools/<pillar>/<school>.yaml` exactly.
fn school_to_damage_type(school: &str) -> Option<DamageType> {
    if let Some(dt) = DamageType::from_str(school) {
        return Some(dt);
    }
    match school {
        // might/*
        "blade" => Some(DamageType::Slashing),
        "blunt" | "unarmed" | "earth" => Some(DamageType::Bludgeoning),
        "spear" | "dagger" | "bow" | "crossbow" | "thrown" | "nature" => Some(DamageType::Piercing),
        // finesse/* (most physical are handled above; these are magical)
        "alchemy" => Some(DamageType::Acid),
        "acrobat" | "trickster" => Some(DamageType::Bludgeoning),
        "silent" => Some(DamageType::Piercing),
        // arcana/*
        "arcane" => Some(DamageType::Force),
        "frost" => Some(DamageType::Cold),
        "light" | "devotion" => Some(DamageType::Radiant),
        "shadow" => Some(DamageType::Necrotic),
        // fire, cold, lightning, blood, poison already matched via from_str
        _ => None,
    }
}

/// Inverse of `reduction = armor / (armor + K)`.
fn reduction_to_armor(reduction: f32) -> u32 {
    let r = reduction.clamp(0.0, 0.95);
    (ARMOR_K * r / (1.0 - r)).round() as u32
}

/// Convert a bestiary damage modifier (negative = resistant, positive =
/// vulnerable) to resist_total points. Inverse of the resist formula
/// used in vaern-combat.
fn modifier_to_resist_points(modifier: f32) -> f32 {
    -modifier / RESIST_PER_POINT
}

pub fn npc_combined_stats(
    creature_type: &CreatureType,
    armor_class: &ArmorClass,
    kind: NpcKind,
) -> CombinedStats {
    let mult = rarity_mult(kind);

    let armor = (reduction_to_armor(armor_class.physical_reduction) as f32 * mult) as u32;

    // Base magical resist from armor_class.magic_reduction, spread
    // across every magical channel. Physical channels get their
    // reduction via `armor` (not via resist_total) so they're left
    // at 0 here (creature-type resistances can still bump them).
    let magical_base_points = modifier_to_resist_points(-armor_class.magic_reduction) * mult;
    let mut resist_total = [0.0f32; DAMAGE_TYPE_COUNT];
    for dt in [
        DamageType::Fire,
        DamageType::Cold,
        DamageType::Lightning,
        DamageType::Force,
        DamageType::Radiant,
        DamageType::Necrotic,
        DamageType::Blood,
        DamageType::Poison,
        DamageType::Acid,
    ] {
        resist_total[dt.index()] = magical_base_points;
    }

    // Creature-type-specific resistances apply on top. School names
    // that don't map to a DamageType are silently skipped (authors
    // sometimes use "arcane" as a family label — no-op here).
    for (school, modifier) in &creature_type.resistances {
        if let Some(dt) = school_to_damage_type(school) {
            resist_total[dt.index()] += modifier_to_resist_points(*modifier) * mult;
        }
    }

    CombinedStats {
        hp_max: 0,
        mana_max: 0,
        melee_mult: 1.0,
        spell_mult: 1.0,
        total_crit_pct: 0.0,
        total_dodge_pct: 0.0,
        total_haste_pct: 0.0,
        total_parry_pct: 0.0,
        carry_kg: 0.0,
        armor,
        fortune_pct: 0.0,
        mp5: 0.0,
        weapon_min_dmg: 0.0,
        weapon_max_dmg: 0.0,
        block_chance_pct: 0.0,
        block_value: 0,
        resist_total,
        luck: 0,
        leech_pct: 0.0,
        move_speed_pct: 0.0,
        avoidance_pct: 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vaern_data::bestiary::{Affinities, BehaviorDefaults, HpScaling};

    fn dummy_creature(resistances: Vec<(&str, f32)>) -> CreatureType {
        CreatureType {
            id: "dummy".into(),
            name: "Dummy".into(),
            category: "test".into(),
            description: String::new(),
            hp_scaling: HpScaling {
                base_at_level_1: 30,
                per_level_multiplier: 1.1,
                formula: String::new(),
            },
            default_armor_class: "none".into(),
            resistances: resistances
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
            affinities: Affinities {
                preferred: vec![],
                allowed: vec![],
                forbidden: vec![],
            },
            behavior_defaults: BehaviorDefaults {
                intelligence: String::new(),
                social: String::new(),
                flee_threshold: 0.0,
                aggro_range: String::new(),
            },
            tags: vec![],
        }
    }

    fn dummy_armor(phys: f32, mag: f32) -> ArmorClass {
        ArmorClass {
            id: "dummy".into(),
            name: "Dummy".into(),
            tier: "light".into(),
            physical_reduction: phys,
            magic_reduction: mag,
            weak_against: vec![],
            strong_against: vec![],
            mobility_penalty: 0.0,
            notes: String::new(),
        }
    }

    #[test]
    fn plate_armor_produces_high_armor_value() {
        let ct = dummy_creature(vec![]);
        let ac = dummy_armor(0.4, 0.12);
        let s = npc_combined_stats(&ct, &ac, NpcKind::Combat);
        // 0.4 reduction with K=200 → armor ≈ 133. Allow slop for rounding.
        assert!(s.armor > 125 && s.armor < 140, "got {}", s.armor);
    }

    #[test]
    fn dragonkin_resists_fire() {
        // dragonkin: fire -0.5 → 50% less fire damage
        let ct = dummy_creature(vec![("fire", -0.5)]);
        let ac = dummy_armor(0.1, 0.1);
        let s = npc_combined_stats(&ct, &ac, NpcKind::Combat);
        let fire = s.resist_total[DamageType::Fire.index()];
        // -0.5 modifier → +100 resist points (before the baseline
        // magic_reduction also adds 20). Total ~120 on Fire.
        assert!(fire > 100.0, "expected strong fire resist, got {}", fire);
    }

    #[test]
    fn undead_vulnerable_to_radiant_has_negative_resist() {
        let ct = dummy_creature(vec![("light", 0.25)]);
        let ac = dummy_armor(0.0, 0.0);
        let s = npc_combined_stats(&ct, &ac, NpcKind::Combat);
        // +0.25 modifier → -50 resist points → amplifies radiant damage.
        let light_res = s.resist_total[DamageType::Radiant.index()];
        assert!(light_res < 0.0, "expected negative resist, got {}", light_res);
    }

    #[test]
    fn named_rarity_bumps_stats() {
        let ct = dummy_creature(vec![]);
        let ac = dummy_armor(0.4, 0.1);
        let combat = npc_combined_stats(&ct, &ac, NpcKind::Combat);
        let named = npc_combined_stats(&ct, &ac, NpcKind::Named);
        assert!(named.armor > combat.armor);
    }
}
