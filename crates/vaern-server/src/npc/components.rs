//! Component shapes for NPCs + the global spawn-slot resource. Pure data;
//! behavior lives in `spawn` and `ai`.

use std::collections::HashMap;
use std::collections::HashSet;

use bevy::prelude::*;
use vaern_combat::NpcKind;

/// Server-side marker for "this is an NPC" (replication-opaque — not shared
/// with the client, which discovers NPCs via `Health` + absent `PlayerTag`).
#[derive(Component)]
pub struct Npc;

/// The NPC's home position. Set at spawn; used by `npc_leash_home` to decide
/// when the NPC has strayed too far and should reset.
#[derive(Component, Debug, Clone, Copy)]
pub struct NpcHome(pub Vec3);

/// Server-only marker: `npc_select_targets` and `npc_chase_target` skip
/// entities with this component. Quest-giver NPCs get it.
#[derive(Component, Debug)]
pub struct NonCombat;

/// Canonical slot id of a combat mob, e.g.
/// `mob_dalewatch_marches_named_drifter_mage_named`. Used by quest
/// kill-objective observers to match a dying mob against quest steps.
#[derive(Component, Debug, Clone)]
pub struct MobSourceId(pub String);

/// Mob level (1..). Carried so the XP-on-kill observer can scale rewards
/// against the killer's level (red mobs pay more, grey mobs nothing).
#[derive(Component, Debug, Clone, Copy)]
pub struct MobLevel(pub u32);

/// Per-NPC aggro radius. Different creature tiers have different detection
/// profiles — common mobs are tight-aggro so you don't pull the whole zone
/// by breathing near them.
#[derive(Component, Debug, Clone, Copy)]
pub struct AggroRange(pub f32);

/// Per-NPC leash radius. NPCs reset when they travel this far from home.
#[derive(Component, Debug, Clone, Copy)]
pub struct LeashRange(pub f32);

/// Idle-wandering state. While the NPC has no Target, it walks between
/// random waypoints within `NPC_ROAM_RADIUS` of its home.
#[derive(Component, Debug, Clone, Copy)]
pub struct RoamState {
    pub waypoint: Vec3,
    /// Seconds to idle at waypoint before picking the next.
    pub wait_secs: f32,
}

/// Per-NPC threat list: accumulated threat per player entity. Threat decays
/// lazily when an attacker dies; a full threat-decay-over-time system can
/// come later.
#[derive(Component, Debug, Default, Clone)]
pub struct ThreatTable(pub HashMap<Entity, f32>);

impl ThreatTable {
    pub fn add(&mut self, player: Entity, amount: f32) {
        *self.0.entry(player).or_insert(0.0) += amount;
    }

    /// Highest-threat player still alive (present in `alive`). Returns None if
    /// every threatening player has despawned.
    pub fn top(&self, alive: &HashSet<Entity>) -> Option<Entity> {
        self.0
            .iter()
            .filter(|(e, t)| **t > 0.0 && alive.contains(e))
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(e, _)| *e)
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

/// One slot in the NPC spawn table. Owns its spot on the map and is
/// responsible for respawning a fresh NPC after its current occupant dies.
#[derive(Debug)]
pub struct NpcSpawnSlot {
    pub position: Vec3,
    pub max_hp: f32,
    /// Damage the slot's auto-cast blade deals; scales with level for mobs.
    pub attack_damage: f32,
    /// Player-facing name shown on the client's nameplate.
    pub display_name: String,
    /// Hint for the client on how to render / what interaction to offer.
    pub kind: NpcKind,
    /// Quest givers don't aggro / cast / chase.
    pub non_combat: bool,
    /// Quest-giver hub metadata (None for mobs). Client uses this to pick
    /// which chain(s) this giver offers.
    pub hub_info: Option<(String, String, String)>, // (hub_id, hub_role, zone_id)
    /// Chain this giver is bound to: (chain_id, step_index). Step 0 =
    /// main giver, higher = mid-chain contact. None = ambient no-quest NPC.
    pub chain_info: Option<(String, u32)>,
    /// Canonical mob slot id for combat mobs. `None` for quest-givers.
    pub mob_slot_id: Option<String>,
    /// Mob level (drives XP scaling on kill). 0 for non-combat NPCs.
    pub level: u32,
    pub current: Option<Entity>,
    /// Seconds until next respawn. Ticks while `current` is None.
    pub countdown: f32,
    /// Pre-derived CombinedStats from bestiary data. Attached to the
    /// entity at spawn time so combat mitigation reads real armor +
    /// resists. `None` for quest-givers (they never take hits).
    pub combined_stats: Option<vaern_stats::CombinedStats>,
    /// Pre-resolved render spec from `assets/npc_mesh_map.yaml`,
    /// keyed on `display_name`. Either a beast-mesh hint or a
    /// humanoid-cosmetics bundle; `None` → cuboid fallback.
    /// At spawn time the right replicated component is inserted
    /// (`NpcMesh` for `Beast`, `NpcAppearance` for `Humanoid`).
    pub visual_from_map: Option<crate::npc_mesh::NpcVisual>,
    /// Vendor listings (for `NpcKind::Vendor`). Attached as a
    /// `VendorStock` component at spawn time. `None` → not a vendor.
    pub vendor_stock: Option<vaern_economy::VendorStock>,
}

#[derive(Resource, Default)]
pub struct NpcSpawns {
    pub slots: Vec<NpcSpawnSlot>,
}
