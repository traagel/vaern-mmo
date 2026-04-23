//! Server-side NPC state, spawning, and AI. Everything unique to non-player
//! entities — markers, per-mob aggro/leash radii, roam state, threat tables,
//! spawn-slot bookkeeping, and target acquisition.
//!
//! Player-spawn logic lives in `crate::connect`; quest-giver metadata rides
//! on the shared `QuestGiverHub` component in `vaern-combat`.
//!
//! Submodules:
//!
//! - `components` — component structs + the `NpcSpawns` resource
//! - `spawn`      — zone-wide seeding + per-slot (re)spawn
//! - `ai`         — threat crediting, target selection, chase/leash/roam

mod ai;
mod components;
mod spawn;
mod stats;

use vaern_combat::NpcKind;

pub use ai::{
    credit_threat_from_casts, npc_chase_target, npc_leash_home, npc_roam, npc_select_targets,
    snap_npcs_to_terrain,
};
pub use components::{MobSourceId, Npc, NpcHome, NpcSpawns, ThreatTable};
pub use spawn::{manage_npc_respawn, seed_npc_spawns};

// ─── constants ─────────────────────────────────────────────────────────────

pub(crate) const NPC_RESPAWN_SECS: f32 = 30.0;
/// NPC movement speed in world units per second (player is 6 u/s).
pub(crate) const NPC_MOVE_SPEED: f32 = 4.5;
/// Stop chasing when within this distance of the target (melee range).
pub(crate) const NPC_MELEE_RANGE: f32 = 1.8;
/// Wander speed when roaming (idle). Half of chase speed so it reads as
/// relaxed movement vs purposeful chase.
pub(crate) const NPC_ROAM_SPEED: f32 = 2.2;
/// Radius around `NpcHome` within which a mob picks roaming waypoints.
pub(crate) const NPC_ROAM_RADIUS: f32 = 4.5;
/// Base aggro range for common mobs. Elite + named mobs get bumped in
/// `aggro_for_kind`.
pub(crate) const NPC_DEFAULT_AGGRO: f32 = 8.0;

// ─── kind → radius tables ──────────────────────────────────────────────────

pub(crate) fn aggro_for_kind(kind: NpcKind) -> f32 {
    match kind {
        NpcKind::QuestGiver => 0.0, // never aggros
        NpcKind::Named => 14.0,
        NpcKind::Elite => 11.0,
        NpcKind::Combat => NPC_DEFAULT_AGGRO,
    }
}

pub(crate) fn leash_for_kind(kind: NpcKind) -> f32 {
    aggro_for_kind(kind).max(NPC_DEFAULT_AGGRO) + 16.0
}
