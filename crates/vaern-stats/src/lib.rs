//! Pillar-based character identity + three-tier gear stat pool.
//!
//! Design locked 2026-04-20. See `memory/project_stat_armor_system.md`.
//!
//! ## Three-tier pool
//!
//! 1. **Primary (pillar identity, not gear-rolled):** HP, mana, damage
//!    mults, base crit/dodge/haste/resist, **base parry** (Might × 0.01),
//!    carry. All derived from pillar scores via pure fns.
//! 2. **Secondary (core gear rolls, 2-4 per piece):** armor, weapon
//!    min/max dmg, crit rating, haste rating, Fortune, MP5, block
//!    chance/value, **12-channel resists** (hardcore prep layer — stays
//!    here, not tertiary; see `feedback_hardcore_prep.md`).
//! 3. **Tertiary (rare, ~10-15% of drops, small magnitude):** Luck,
//!    Leech, Move Speed, Avoidance. Bonus rolls; not planning-critical.
//!
//! ## Block vs Parry — both ACTIVE, mutually exclusive
//!
//! New World-style action combat. Shared input binding; weapon loadout
//! determines which stance is available.
//!
//! * **Active Block** (shield equipped) — hold key, wide front cone,
//!   stamina drains, `block_value` absorbs flat damage, `block_chance_pct`
//!   rolls on each hit → Perfect Block (full negate + attacker stagger
//!   + stamina refund).
//! * **Active Parry** (any melee weapon, no shield) — tap key during
//!   incoming-attack timing window, narrow front cone, on success:
//!   full negate + longer stagger + counterattack opening. `base_parry_pct`
//!   (Might-derived) widens the timing window.
//!
//! You cannot have both at once — loadout choice commits a build
//! identity: shield-tank (Block Value + Chance) vs duelist (Might parry).
//! Do not introduce passive parry on incoming hits; combat must consume
//! input for defensive actions.
//!
//! Leaf crate: core-only deps. vaern-combat will consume `CombinedStats`
//! for damage resolution; vaern-items rolls into `SecondaryStats` +
//! `TertiaryStats`.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use vaern_core::{DAMAGE_TYPE_COUNT, pillar::Pillar};

pub const PILLAR_MAX: u16 = 100;

/// Runtime per-character pillar scores. Grows slowly through play;
/// clamped by `PillarCaps` from race affinity. This is identity, not
/// a roll — never mutate it from gear.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PillarScores {
    pub might: u16,
    pub finesse: u16,
    pub arcana: u16,
}

impl Default for PillarScores {
    /// New character: 5 in every pillar so base stats aren't zero.
    fn default() -> Self {
        Self {
            might: 5,
            finesse: 5,
            arcana: 5,
        }
    }
}

impl PillarScores {
    pub fn get(&self, pillar: Pillar) -> u16 {
        match pillar {
            Pillar::Might => self.might,
            Pillar::Finesse => self.finesse,
            Pillar::Arcana => self.arcana,
        }
    }

    pub fn set(&mut self, pillar: Pillar, value: u16) {
        match pillar {
            Pillar::Might => self.might = value,
            Pillar::Finesse => self.finesse = value,
            Pillar::Arcana => self.arcana = value,
        }
    }
}

/// Per-character pillar caps derived from race affinity. A Hearthkin
/// (100, 50, 50) can't train Finesse or Arcana past 50. Enforces
/// racial identity.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PillarCaps {
    pub might: u16,
    pub finesse: u16,
    pub arcana: u16,
}

impl Default for PillarCaps {
    fn default() -> Self {
        Self {
            might: PILLAR_MAX,
            finesse: PILLAR_MAX,
            arcana: PILLAR_MAX,
        }
    }
}

impl PillarCaps {
    pub fn get(&self, pillar: Pillar) -> u16 {
        match pillar {
            Pillar::Might => self.might,
            Pillar::Finesse => self.finesse,
            Pillar::Arcana => self.arcana,
        }
    }
}

/// XP banked toward the next pillar point, per pillar.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PillarXp {
    pub might: u32,
    pub finesse: u32,
    pub arcana: u32,
}

impl PillarXp {
    pub fn get(&self, pillar: Pillar) -> u32 {
        match pillar {
            Pillar::Might => self.might,
            Pillar::Finesse => self.finesse,
            Pillar::Arcana => self.arcana,
        }
    }

    pub fn get_mut(&mut self, pillar: Pillar) -> &mut u32 {
        match pillar {
            Pillar::Might => &mut self.might,
            Pillar::Finesse => &mut self.finesse,
            Pillar::Arcana => &mut self.arcana,
        }
    }
}

/// XP required to advance `current` → `current + 1`. Quadratic:
/// `50 + 4·L²`. Levels 1→5 ≈ 50–150 XP, 50→51 ≈ 10k XP, 90→91 ≈ 32k.
pub fn xp_to_next_point(current: u16) -> u32 {
    let l = current as u32;
    50 + 4 * l * l
}

pub const XP_PER_ABILITY_CAST: u32 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PillarGain {
    pub pillar: Pillar,
    pub points_gained: u16,
    pub new_score: u16,
}

/// Award XP to a pillar. Clamped by caps; overflow is discarded.
/// Big lumps can roll across multiple thresholds in one call.
pub fn award_pillar_xp(
    scores: &mut PillarScores,
    xp: &mut PillarXp,
    caps: &PillarCaps,
    pillar: Pillar,
    amount: u32,
) -> PillarGain {
    let cap = caps.get(pillar);
    let mut current = scores.get(pillar);
    if current >= cap {
        *xp.get_mut(pillar) = 0;
        return PillarGain {
            pillar,
            points_gained: 0,
            new_score: current,
        };
    }
    let bank = xp.get_mut(pillar);
    *bank = bank.saturating_add(amount);
    let mut gained = 0u16;
    while current < cap {
        let need = xp_to_next_point(current);
        if *bank < need {
            break;
        }
        *bank -= need;
        current += 1;
        gained += 1;
    }
    if current >= cap {
        *bank = 0;
    }
    scores.set(pillar, current);
    PillarGain {
        pillar,
        points_gained: gained,
        new_score: current,
    }
}

/// Derived primaries — pure function of pillar scores. Combat + UI
/// read these; gear layers on top via `SecondaryStats` + `TertiaryStats`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DerivedPrimaries {
    pub hp_max: u32,
    pub mana_max: u32,
    pub melee_mult: f32,
    pub spell_mult: f32,
    pub base_crit_pct: f32,
    pub base_dodge_pct: f32,
    /// Timing-window scalar for active parry. Multiplied by combat's
    /// base parry window (e.g. 0.3s) to give total window; 100 Might
    /// doubles the window. Combat owns the actual parry attempt logic.
    pub base_parry_pct: f32,
    /// Generic resist baseline — applied to every damage-type channel
    /// as a floor before per-school resists stack on top.
    pub base_resist: f32,
    pub carry_kg: f32,
}

pub mod formula {
    pub const HP_BASE: u32 = 80;
    pub const HP_PER_MIGHT: f32 = 8.0;
    pub const HP_PER_FINESSE: f32 = 3.0;
    pub const HP_PER_ARCANA: f32 = 3.0;

    pub const MANA_BASE: u32 = 40;
    pub const MANA_PER_ARCANA: f32 = 12.0;
    pub const MANA_PER_FINESSE: f32 = 2.0;

    pub const MELEE_MULT_PER_MIGHT: f32 = 0.005;
    pub const SPELL_MULT_PER_ARCANA: f32 = 0.005;

    pub const BASE_CRIT_PCT: f32 = 2.0;
    pub const CRIT_PCT_PER_FINESSE: f32 = 0.02;

    pub const BASE_DODGE_PCT: f32 = 1.0;
    pub const DODGE_PCT_PER_FINESSE: f32 = 0.01;

    /// 100 Might = +1.0 parry window scalar (double-wide window when
    /// combat's base window is e.g. 0.3s). Pillar identity for duelists.
    pub const PARRY_PCT_PER_MIGHT: f32 = 0.01;

    pub const RESIST_PER_ARCANA: f32 = 0.1;

    pub const CARRY_BASE_KG: f32 = 20.0;
    pub const CARRY_PER_MIGHT: f32 = 1.5;

    /// Multiplier applied to cast time and cooldown. `1.0 / (1 + h/100)` —
    /// asymptotic to zero, so stacking haste has diminishing absolute gains
    /// but never collapses to an insta-cast. `h = 0 → 1.0`, `h = 50 → 0.667`,
    /// `h = 100 → 0.5`. Snapshotted at cast start; mid-cast stat changes
    /// don't retroactively speed up the current cast.
    pub fn cast_speed_scale(haste_pct: f32) -> f32 {
        1.0 / (1.0 + haste_pct.max(0.0) / 100.0)
    }
}

#[cfg(test)]
mod haste_tests {
    use super::formula::cast_speed_scale;

    #[test]
    fn zero_haste_is_identity() {
        assert!((cast_speed_scale(0.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn fifty_haste_is_two_thirds() {
        assert!((cast_speed_scale(50.0) - (2.0 / 3.0)).abs() < 1e-6);
    }

    #[test]
    fn hundred_haste_is_half() {
        assert!((cast_speed_scale(100.0) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn negative_haste_clamps_to_identity() {
        assert!((cast_speed_scale(-25.0) - 1.0).abs() < 1e-6);
    }
}

pub fn derive_primaries(p: &PillarScores) -> DerivedPrimaries {
    let m = p.might as f32;
    let f = p.finesse as f32;
    let a = p.arcana as f32;
    DerivedPrimaries {
        hp_max: (formula::HP_BASE as f32
            + m * formula::HP_PER_MIGHT
            + f * formula::HP_PER_FINESSE
            + a * formula::HP_PER_ARCANA) as u32,
        mana_max: (formula::MANA_BASE as f32
            + a * formula::MANA_PER_ARCANA
            + f * formula::MANA_PER_FINESSE) as u32,
        melee_mult: 1.0 + m * formula::MELEE_MULT_PER_MIGHT,
        spell_mult: 1.0 + a * formula::SPELL_MULT_PER_ARCANA,
        base_crit_pct: formula::BASE_CRIT_PCT + f * formula::CRIT_PCT_PER_FINESSE,
        base_dodge_pct: formula::BASE_DODGE_PCT + f * formula::DODGE_PCT_PER_FINESSE,
        base_parry_pct: m * formula::PARRY_PCT_PER_MIGHT,
        base_resist: a * formula::RESIST_PER_ARCANA,
        carry_kg: formula::CARRY_BASE_KG + m * formula::CARRY_PER_MIGHT,
    }
}

/// Flat stat pool gear can roll into — the *core* gear layer.
/// Indexed resist array by `DamageType as usize` (12 channels). Resists
/// live here (not tertiary) because preparation-driven gear swaps are
/// the hardcore reward loop; see `feedback_hardcore_prep.md`.
///
/// Item-side convention:
/// * Armor pieces roll `armor` + per-damage-type `resists`
/// * Weapons roll `weapon_min_dmg` + `weapon_max_dmg`
/// * Shields roll `block_chance_pct` + `block_value`
/// * Any slot can roll `crit_rating_pct`, `haste_rating_pct`,
///   `fortune_pct`, `mp5`
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct SecondaryStats {
    pub armor: u32,
    pub weapon_min_dmg: f32,
    pub weapon_max_dmg: f32,
    pub crit_rating_pct: f32,
    pub haste_rating_pct: f32,
    pub fortune_pct: f32,
    pub mp5: f32,
    pub block_chance_pct: f32,
    pub block_value: u32,
    /// Per-damage-type resist, indexed by `DamageType as usize`.
    /// Stacks additively on top of `DerivedPrimaries.base_resist`.
    pub resists: [f32; DAMAGE_TYPE_COUNT],
}

impl SecondaryStats {
    pub fn add(&mut self, other: &SecondaryStats) {
        self.armor += other.armor;
        self.weapon_min_dmg += other.weapon_min_dmg;
        self.weapon_max_dmg += other.weapon_max_dmg;
        self.crit_rating_pct += other.crit_rating_pct;
        self.haste_rating_pct += other.haste_rating_pct;
        self.fortune_pct += other.fortune_pct;
        self.mp5 += other.mp5;
        self.block_chance_pct += other.block_chance_pct;
        self.block_value += other.block_value;
        for i in 0..DAMAGE_TYPE_COUNT {
            self.resists[i] += other.resists[i];
        }
    }
}

/// Rare, small-magnitude bonus stats. ~10-15% of drops get one roll
/// here; Legendary rolls always get one. Not build-planning-critical —
/// they're the "lucky roll" tier, distinct from the hardcore-prep
/// planning stats in `SecondaryStats`.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct TertiaryStats {
    /// Generic RNG shifter — drop quality, gold find, rare-mob chance,
    /// non-combat success rolls. Rating, scaled by loot/reward systems.
    pub luck: u32,
    /// Heal for this % of outgoing damage dealt.
    pub leech_pct: f32,
    /// Flat % movement speed bonus over baseline.
    pub move_speed_pct: f32,
    /// % AoE damage reduction (Turtle WoW pattern). Small rolls on any
    /// slot; content-side gating by rarity, not slot (per design review).
    pub avoidance_pct: f32,
}

impl TertiaryStats {
    pub fn add(&mut self, other: &TertiaryStats) {
        self.luck += other.luck;
        self.leech_pct += other.leech_pct;
        self.move_speed_pct += other.move_speed_pct;
        self.avoidance_pct += other.avoidance_pct;
    }
}

/// Fully-resolved character stats after folding pillar-derived
/// primaries + gear secondaries + tertiary bonus rolls.
/// Combat reads this — it's a `Component` so a dedicated server
/// system denormalizes it onto each player/NPC on state change,
/// and `vaern-combat`'s damage pipeline can query it directly.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct CombinedStats {
    // derived
    pub hp_max: u32,
    pub mana_max: u32,
    pub melee_mult: f32,
    pub spell_mult: f32,
    pub total_crit_pct: f32,
    pub total_dodge_pct: f32,
    pub total_haste_pct: f32,
    pub total_parry_pct: f32,
    pub carry_kg: f32,
    // secondary
    pub armor: u32,
    pub fortune_pct: f32,
    pub mp5: f32,
    pub weapon_min_dmg: f32,
    pub weapon_max_dmg: f32,
    pub block_chance_pct: f32,
    pub block_value: u32,
    /// Total resist per damage type: `base_resist` + secondary flat.
    /// Not a percentage — combat decides the mitigation curve.
    pub resist_total: [f32; DAMAGE_TYPE_COUNT],
    // tertiary
    pub luck: u32,
    pub leech_pct: f32,
    pub move_speed_pct: f32,
    pub avoidance_pct: f32,
}

/// Pure fn: fold all three stat layers into combat-ready numbers.
/// Single fold point — if a stat isn't here, combat can't see it.
pub fn combine(
    derived: &DerivedPrimaries,
    secondary: &SecondaryStats,
    tertiary: &TertiaryStats,
) -> CombinedStats {
    let mut resist_total = [0.0; DAMAGE_TYPE_COUNT];
    for i in 0..DAMAGE_TYPE_COUNT {
        resist_total[i] = derived.base_resist + secondary.resists[i];
    }
    CombinedStats {
        hp_max: derived.hp_max,
        mana_max: derived.mana_max,
        melee_mult: derived.melee_mult,
        spell_mult: derived.spell_mult,
        total_crit_pct: derived.base_crit_pct + secondary.crit_rating_pct,
        total_dodge_pct: derived.base_dodge_pct,
        total_haste_pct: secondary.haste_rating_pct,
        total_parry_pct: derived.base_parry_pct,
        carry_kg: derived.carry_kg,
        armor: secondary.armor,
        fortune_pct: secondary.fortune_pct,
        mp5: secondary.mp5,
        weapon_min_dmg: secondary.weapon_min_dmg,
        weapon_max_dmg: secondary.weapon_max_dmg,
        block_chance_pct: secondary.block_chance_pct,
        block_value: secondary.block_value,
        resist_total,
        luck: tertiary.luck,
        leech_pct: tertiary.leech_pct,
        move_speed_pct: tertiary.move_speed_pct,
        avoidance_pct: tertiary.avoidance_pct,
    }
}

/// Per-(slot × tier × rarity) pool of stat points item generators
/// spend. Designer-authored table; referenced by the seeder to keep
/// drops balanced against the derived baseline. Minimal now — flesh
/// out in the item-gen slice.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StatBudget {
    pub total: u32,
    pub armor_share: f32,
    pub weapon_dmg_share: f32,
}

impl Default for StatBudget {
    fn default() -> Self {
        Self {
            total: 30,
            armor_share: 0.5,
            weapon_dmg_share: 0.7,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vaern_core::DamageType;

    #[test]
    fn default_scores_derive_nonzero_primaries() {
        let d = derive_primaries(&PillarScores::default());
        assert!(d.hp_max > 0);
        assert!(d.mana_max > 0);
        assert!(d.melee_mult > 1.0);
        assert!(d.spell_mult > 1.0);
    }

    #[test]
    fn hp_scales_with_might_more_than_with_arcana() {
        let might = PillarScores {
            might: 50,
            finesse: 0,
            arcana: 0,
        };
        let arcana = PillarScores {
            might: 0,
            finesse: 0,
            arcana: 50,
        };
        assert!(derive_primaries(&might).hp_max > derive_primaries(&arcana).hp_max);
    }

    #[test]
    fn parry_scales_with_might_only() {
        let high_m = PillarScores {
            might: 95,
            finesse: 5,
            arcana: 5,
        };
        let high_f = PillarScores {
            might: 5,
            finesse: 95,
            arcana: 5,
        };
        // high_f has Might 5 → 5 × 0.01 = 0.05 window scalar
        assert!((derive_primaries(&high_f).base_parry_pct - 0.05).abs() < 1e-5);
        // high_m has Might 95 → 95 × 0.01 = 0.95
        assert!((derive_primaries(&high_m).base_parry_pct - 0.95).abs() < 1e-5);
        // Only Might drives it — high_m > high_f confirms.
        assert!(derive_primaries(&high_m).base_parry_pct > derive_primaries(&high_f).base_parry_pct);
    }

    #[test]
    fn xp_curve_is_monotonic_quadratic() {
        let a = xp_to_next_point(10);
        let b = xp_to_next_point(20);
        let c = xp_to_next_point(50);
        assert!(a < b);
        assert!(b < c);
        assert_eq!(c, 10_050);
    }

    #[test]
    fn xp_award_grants_point_when_bank_crosses_threshold() {
        let mut scores = PillarScores::default();
        let mut xp = PillarXp::default();
        let caps = PillarCaps::default();
        let need = xp_to_next_point(scores.might);
        let gain = award_pillar_xp(&mut scores, &mut xp, &caps, Pillar::Might, need);
        assert_eq!(gain.points_gained, 1);
        assert_eq!(scores.might, 6);
        assert_eq!(xp.might, 0);
    }

    #[test]
    fn xp_award_rolls_multiple_points_from_big_lump() {
        let mut scores = PillarScores {
            might: 10,
            finesse: 5,
            arcana: 5,
        };
        let mut xp = PillarXp::default();
        let caps = PillarCaps::default();
        let big = xp_to_next_point(10) + xp_to_next_point(11) + 10;
        let gain = award_pillar_xp(&mut scores, &mut xp, &caps, Pillar::Might, big);
        assert_eq!(gain.points_gained, 2);
        assert_eq!(scores.might, 12);
        assert_eq!(xp.might, 10);
    }

    #[test]
    fn xp_clamped_at_race_cap_drops_overflow() {
        let mut scores = PillarScores {
            might: 5,
            finesse: 50,
            arcana: 5,
        };
        let mut xp = PillarXp::default();
        let caps = PillarCaps {
            might: 100,
            finesse: 50,
            arcana: 50,
        };
        let gain = award_pillar_xp(&mut scores, &mut xp, &caps, Pillar::Finesse, 999_999);
        assert_eq!(gain.points_gained, 0);
        assert_eq!(scores.finesse, 50);
        assert_eq!(xp.finesse, 0);
    }

    #[test]
    fn secondary_add_accumulates_all_fields_including_12_resists() {
        let mut a = SecondaryStats::default();
        a.armor = 10;
        a.crit_rating_pct = 1.0;
        a.block_value = 5;
        for i in 0..DAMAGE_TYPE_COUNT {
            a.resists[i] = 1.0;
        }
        let mut b = SecondaryStats::default();
        b.armor = 20;
        b.block_value = 3;
        for i in 0..DAMAGE_TYPE_COUNT {
            b.resists[i] = 2.0;
        }
        a.add(&b);
        assert_eq!(a.armor, 30);
        assert_eq!(a.block_value, 8);
        for r in a.resists.iter() {
            assert!((*r - 3.0).abs() < 1e-5);
        }
    }

    #[test]
    fn tertiary_add_accumulates_all_fields() {
        let mut a = TertiaryStats {
            luck: 100,
            leech_pct: 1.0,
            move_speed_pct: 2.0,
            avoidance_pct: 0.5,
        };
        let b = TertiaryStats {
            luck: 50,
            leech_pct: 0.5,
            move_speed_pct: 1.0,
            avoidance_pct: 1.0,
        };
        a.add(&b);
        assert_eq!(a.luck, 150);
        assert!((a.leech_pct - 1.5).abs() < 1e-5);
        assert!((a.move_speed_pct - 3.0).abs() < 1e-5);
        assert!((a.avoidance_pct - 1.5).abs() < 1e-5);
    }

    #[test]
    fn combine_folds_all_three_layers() {
        let scores = PillarScores {
            might: 50,
            finesse: 40,
            arcana: 10,
        };
        let derived = derive_primaries(&scores);
        let mut secondary = SecondaryStats::default();
        secondary.armor = 100;
        secondary.crit_rating_pct = 3.0;
        secondary.block_value = 20;
        secondary.resists[DamageType::Fire as usize] = 15.0;
        let tertiary = TertiaryStats {
            luck: 42,
            leech_pct: 1.5,
            move_speed_pct: 5.0,
            avoidance_pct: 2.0,
        };
        let c = combine(&derived, &secondary, &tertiary);

        // Primary passes through
        assert_eq!(c.hp_max, derived.hp_max);
        assert!((c.total_parry_pct - derived.base_parry_pct).abs() < 1e-5);
        // Secondary surfaces
        assert_eq!(c.armor, 100);
        assert_eq!(c.block_value, 20);
        assert!((c.total_crit_pct - (derived.base_crit_pct + 3.0)).abs() < 1e-5);
        assert!(
            (c.resist_total[DamageType::Fire as usize] - (derived.base_resist + 15.0)).abs() < 1e-5
        );
        // Tertiary surfaces
        assert_eq!(c.luck, 42);
        assert!((c.leech_pct - 1.5).abs() < 1e-5);
        assert!((c.move_speed_pct - 5.0).abs() < 1e-5);
        assert!((c.avoidance_pct - 2.0).abs() < 1e-5);
    }

    #[test]
    fn combine_leaves_untouched_resist_channels_at_base_only() {
        let scores = PillarScores {
            might: 5,
            finesse: 5,
            arcana: 30,
        };
        let derived = derive_primaries(&scores);
        let mut secondary = SecondaryStats::default();
        secondary.resists[DamageType::Fire as usize] = 10.0;
        let c = combine(&derived, &secondary, &TertiaryStats::default());
        // Fire had a flat add.
        assert!(
            (c.resist_total[DamageType::Fire as usize] - (derived.base_resist + 10.0)).abs() < 1e-5
        );
        // Cold had no flat — just base.
        assert!((c.resist_total[DamageType::Cold as usize] - derived.base_resist).abs() < 1e-5);
    }
}
