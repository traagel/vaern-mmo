//! On-disk character save format.
//!
//! One `PersistedCharacter` per file at
//! `~/.config/vaern/server/characters/<uuid>.json`. Flat aggregate — every
//! field is a directly-serialized component type or a string-tagged cosmetic
//! wrapper. Types re-derived from design data (pillar caps, HP max) are
//! NOT persisted — they're recomputed from race YAML on load so balance
//! patches apply retroactively.

use serde::{Deserialize, Serialize};
use vaern_character::Experience;
use vaern_core::pillar::Pillar;
use vaern_equipment::Equipped;
use vaern_inventory::{ConsumableBelt, PlayerInventory};
use vaern_professions::ProfessionSkills;
use vaern_stats::{PillarCaps, PillarScores, PillarXp};

use crate::cosmetic::PersistedCosmetics;

/// On-disk schema version. Bump on any backwards-incompatible layout change.
/// Load path quarantines files with a different version rather than
/// best-effort parsing them.
pub const SCHEMA_VERSION: u32 = 1;

/// Everything that must survive a logout / server restart for one character.
///
/// **Not persisted** (intentionally):
/// - Current HP / mana / stamina — restored to max on login.
/// - `StatusEffects` (DoTs, stances, buffs) — cleared on login.
/// - `PillarCaps` — redundant with race YAML; recomputed on load.
/// - Hotbar ability rows — deterministic from pillar, rebuilt on spawn.
///
/// **World position** IS persisted as `Option<[f32; 3]>` + yaw. `None`
/// (legacy saves without the field, or anonymous spawns) falls back to
/// the race's zone hub on load.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PersistedCharacter {
    pub schema_version: u32,

    // --- identity ---
    /// Stable UUID (dashed string form). Filename stem is `<character_id>.json`.
    pub character_id: String,
    pub name: String,
    pub race_id: String,
    pub core_pillar: Pillar,
    pub cosmetics: PersistedCosmetics,

    // --- progression ---
    pub experience: Experience,
    pub pillar_scores: PillarScores,
    /// Persisted verbatim for diagnostics, but the load path recomputes
    /// caps from race YAML and overrides this field. If you race-balance
    /// the affinity table, the new caps apply on next login.
    pub pillar_caps: PillarCaps,
    pub pillar_xp: PillarXp,

    // --- loadout ---
    pub inventory: PlayerInventory,
    pub equipped: Equipped,
    pub belt: ConsumableBelt,
    pub professions: ProfessionSkills,
    /// Wallet balance in copper. Defaulted to 0 for legacy saves predating
    /// the currency system.
    #[serde(default)]
    pub wallet_copper: u64,

    // --- quests ---
    pub quest_log: PersistedQuestLog,

    // --- world position ---
    /// Last known world position. `None` → respawn at race's zone
    /// hub (legacy saves + freshly created characters). Stored as a
    /// plain array to keep `vaern-persistence` free of a Bevy dep.
    #[serde(default)]
    pub position: Option<[f32; 3]>,
    /// Y-axis rotation in radians. `None` → face zone-default heading.
    /// Only yaw persists — players don't pitch or roll on disk.
    #[serde(default)]
    pub yaw_rad: Option<f32>,

    // --- bookkeeping ---
    pub created_at: i64,
    pub updated_at: i64,
}

/// Mirror of the server's in-memory `QuestLog` (`vaern-server/src/quests.rs`).
/// Kept as a sorted `Vec` so JSON output is deterministic (HashMap iteration
/// order is not) and diffs of save files stay readable.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct PersistedQuestLog {
    #[serde(default)]
    pub entries: Vec<PersistedQuestEntry>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PersistedQuestEntry {
    pub chain_id: String,
    pub current_step: u32,
    pub total_steps: u32,
    #[serde(default)]
    pub completed: bool,
}
