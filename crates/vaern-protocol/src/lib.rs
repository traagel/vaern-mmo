//! Shared networking protocol. Both server and client add `SharedPlugin` after
//! their respective `ClientPlugins` / `ServerPlugins` plugin groups. Registers
//! which components are replicated, which channels exist, and the input type
//! used for client-side prediction.

use core::net::{IpAddr, Ipv4Addr, SocketAddr};

use bevy::ecs::entity::MapEntities;
use bevy::prelude::*;
use lightyear::input::native::prelude::InputPlugin;
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};
use vaern_character::{Experience, PlayerRace};
use vaern_combat::{
    AnimOverride, AnimState, Casting, DisplayName, Health, NpcKind, ProjectileVisual, QuestGiverHub,
    ResourcePool,
};
use vaern_core::pillar::Pillar;
use vaern_persistence::PersistedCosmetics;

pub const FIXED_TIMESTEP_HZ: f64 = 60.0;
pub const SERVER_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 27015);
pub const CLIENT_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0);

/// 32-byte shared key for netcode. Zeroed for scaffold — swap to env-loaded
/// key before anything faces the real internet.
pub const SHARED_PRIVATE_KEY: [u8; 32] = [0; 32];
pub const SHARED_PROTOCOL_ID: u64 = 0x7661_6572_6E00_0001;

/// Player movement units per FixedUpdate tick. Both client (predicted) and
/// server (authoritative) integrate against this constant so the rollback
/// math stays simple.
pub const MOVE_PER_TICK: f32 = 6.0 / FIXED_TIMESTEP_HZ as f32;

pub struct Channel1;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Ping(pub u64);

/// Replicated cosmetic snapshot per player. The own player doesn't
/// consume this (lightyear 0.26 doesn't give the owning client a
/// non-`Predicted` copy of its own replicated components; the own
/// player reads from the `SelectedCharacter` client resource instead).
/// Remote players + a future "Remote player + NPC mesh" renderer will
/// build their Quaternius outfit from this component.
///
/// Mirrors `vaern_persistence::PersistedCosmetics` exactly — one
/// type on the wire and on disk, no converter churn.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct PlayerAppearance(pub PersistedCosmetics);

/// Replicated humanoid-NPC appearance hint. Ships only the archetype
/// key (`"peasant_male"`, `"knight_plate_male"`, …) + a scale; both
/// server and client load the same `humanoid_archetypes:` table from
/// `assets/npc_mesh_map.yaml` and resolve the key to the full
/// `PersistedCosmetics` locally.
///
/// Shipping the key instead of the expanded `PersistedCosmetics`
/// keeps the per-entity replication payload around 20 bytes instead
/// of ~200. With ~15 humanoid NPCs in a camp replicating on the
/// same tick, the larger form was exceeding UDP MTU and causing
/// specific spawn packets to drop — clients ended up with updates
/// for entities they'd never spawned.
///
/// Mutually exclusive with [`NpcMesh`] — a spawn either has one or
/// the other or neither (cuboid fallback).
#[derive(Component, Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct NpcAppearance {
    /// Archetype key in the shared `humanoid_archetypes:` table.
    pub archetype: String,
    /// Uniform scale factor; for named-boss upsizing. Stored as
    /// milli-scale so we can derive `Eq` for the component (wire size
    /// unchanged — still 4 bytes). `1000` = 1.0×, `1300` = 1.30×.
    pub scale_milli: u16,
}

impl NpcAppearance {
    pub fn new(archetype: impl Into<String>, scale: f32) -> Self {
        Self {
            archetype: archetype.into(),
            scale_milli: (scale * 1000.0).round().clamp(1.0, 65535.0) as u16,
        }
    }

    pub fn scale(&self) -> f32 {
        self.scale_milli as f32 / 1000.0
    }
}

/// Replicated NPC mesh hint — which EverythingLibrary species the
/// client should spawn for this NPC, plus a world-space scale. Server
/// decides the species at spawn (via keyword match on the mob id) and
/// replicates once; the client spawns the GLB as a child of the NPC
/// entity. NPCs whose mob id doesn't map get no `NpcMesh` and fall
/// through to the legacy cuboid renderer.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct NpcMesh {
    /// Species basename, keyed against `AnimalCatalog` on the client
    /// (e.g. `"GrayWolf"`, `"KomodoDragon"`).
    pub species: String,
    /// Uniform scale factor applied to the spawned mesh root. Pack
    /// meshes come in wildly different sizes (ant vs mammoth), so
    /// per-species hand-tuned scales seat each mob at a plausible
    /// gameplay size without editing the source GLBs.
    pub scale: f32,
}

/// Replicated weapon loadout per player. Server folds `Equipped` →
/// MEGAKIT prop basenames (via `vaern_assets::weapon_props_from_equipped`)
/// on every `Changed<Equipped>`. Remote clients consume this to spawn
/// `QuaterniusWeaponOverlay` entities on the right hand bones. Own
/// player reads its own `OwnEquipped` resource directly, per the
/// lightyear-0.26-Predicted-copy limitation.
#[derive(Component, Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct PlayerWeapons {
    /// MEGAKIT prop basename for `hand_r`, or `None` for empty hand.
    pub mainhand: Option<String>,
    /// MEGAKIT prop basename for `hand_l`, or `None` for empty hand.
    pub offhand: Option<String>,
}

/// Client → Server: sent once after connect to lock in the character's
/// core pillar (Might / Finesse / Arcana). Server defers spawn until it
/// arrives (with a short timeout fallback so stuck clients don't block
/// forever). Archetype / Order / Spec are NOT picked at creation — those
/// evolve through play, so the commitment at this handshake is pillar only.
///
/// **Persistence fields** (`character_id`, `cosmetics`, `character_name`)
/// are all `#[serde(default)]` so older clients still deserialize. PR1
/// wires them through the protocol; the server load/save path that
/// actually reads them lands in PR2.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ClientHello {
    pub core_pillar: Pillar,
    /// Race id from the character-select screen (e.g. "mannin", "skarn").
    /// Server uses this to pick the starter zone spawn point. Empty string
    /// = fall back to mannin/dalewatch_marches.
    #[serde(default)]
    pub race_id: String,
    /// Stable per-character UUID (dashed string). Generated client-side
    /// at char-create; persists across logins. Empty = "no character"
    /// (PR1 placeholder — server falls back to today's fresh-spawn path).
    #[serde(default)]
    pub character_id: String,
    /// Display name from char-create. Server uses it only for logs +
    /// bookkeeping in `PersistedCharacter.name`; display name on the
    /// player entity is still driven by `DisplayName`.
    #[serde(default)]
    pub character_name: String,
    /// Cosmetic picks from char-create. Server persists verbatim on
    /// first login. On re-login the server's on-disk copy wins and this
    /// is ignored.
    #[serde(default)]
    pub cosmetics: Option<PersistedCosmetics>,
}

/// Client → Server: the player pressed a hotbar key, targeting `target`.
/// Target is a client-local entity id; lightyear remaps it to the server's id.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CastIntent {
    pub slot: u8,
    pub target: Entity,
}

impl bevy::ecs::entity::MapEntities for CastIntent {
    fn map_entities<M: bevy::ecs::entity::EntityMapper>(&mut self, mapper: &mut M) {
        self.target = mapper.get_mapped(self.target);
    }
}

/// Client → Server: player requested a stance change. Block is a
/// toggle (held-key semantics — client sends `SetBlock(true)` on press
/// and `SetBlock(false)` on release). Parry is a one-shot tap that
/// opens a short negate-window on the server.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum StanceRequest {
    SetBlock(bool),
    ParryTap,
}

/// Server → Client: an ability just resolved and dealt damage at `target`.
/// Used to fire client-side impact visuals and floating damage numbers even
/// for instant casts (which have no long-lived Casting component to
/// replicate). `caster` is included so the client can drive own-player
/// Attacking / Hit transient animations locally — more robust than
/// relying on lightyear 0.26's predicted dynamic-component insertion
/// of `AnimOverride`, which races derive_anim_state on the own-player
/// Predicted copy.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CastFired {
    pub caster: Entity,
    pub target: Entity,
    pub school: String,
    pub damage: f32,
}

impl bevy::ecs::entity::MapEntities for CastFired {
    fn map_entities<M: bevy::ecs::entity::EntityMapper>(&mut self, mapper: &mut M) {
        self.caster = mapper.get_mapped(self.caster);
        self.target = mapper.get_mapped(self.target);
    }
}

/// Server → Client: one entry per hotbar slot, sent once after the server
/// builds a player's kit. Client renders the hotbar UI from this — cooldowns
/// are tracked locally (optimistic) on keypress.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HotbarSlotInfo {
    pub slot: u8,
    /// Dot-separated canonical id — `{pillar}.{category}.{tier}.{school}.{name}`.
    /// Also the icon filename under `icons/` (append `.png`).
    pub ability_id: String,
    pub name: String,
    pub description: String,
    pub school: String,
    pub category: String,
    pub pillar: String,
    pub damage_type: String,
    pub tier: u8,
    pub damage: f32,
    pub cooldown_secs: f32,
    pub cast_secs: f32,
    pub resource_cost: f32,
    /// Max target distance. Self-AoE = 0 (range ignored).
    #[serde(default)]
    pub range: f32,
    /// `"target" | "aoe_on_target" | "aoe_on_self" | "cone" | "line" | "projectile"`.
    /// Plain string so protocol stays decoupled from `vaern-combat::AbilityShape`.
    #[serde(default = "default_shape_str")]
    pub shape: String,
    #[serde(default)]
    pub aoe_radius: f32,
    #[serde(default)]
    pub cone_half_angle_deg: f32,
    #[serde(default)]
    pub line_width: f32,
    #[serde(default)]
    pub projectile_speed: f32,
}

fn default_shape_str() -> String {
    "target".into()
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct HotbarSnapshot {
    pub slots: Vec<HotbarSlotInfo>,
}

// ─── quest protocol ────────────────────────────────────────────────────────

/// Client → Server: "open this chain for me." Server validates the chain id
/// exists and the player isn't already on it, then marks quest Accepted.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AcceptQuest {
    pub chain_id: String,
}

/// Client → Server: drop an active quest. Server removes the log entry,
/// does NOT reset rewards already granted.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AbandonQuest {
    pub chain_id: String,
}

/// Client → Server: manual step advance. Scaffold affordance until real
/// objective detection (kill / talk / deliver) lands — lets us verify the
/// full accept → progress → complete round-trip end-to-end.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ProgressQuest {
    pub chain_id: String,
}

/// One line in the player's quest log.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct QuestLogEntry {
    pub chain_id: String,
    /// 0 = accepted, not yet progressed. Equals chain.total_steps when the
    /// whole chain is complete.
    pub current_step: u32,
    pub total_steps: u32,
    pub completed: bool,
}

/// Server → Client: full snapshot of the owning player's quest log. Resent
/// on every change (accept / progress / abandon / complete).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct QuestLogSnapshot {
    pub entries: Vec<QuestLogEntry>,
}

/// Server → Client: owner's combat + progression state. Sent on change to the
/// owning link — bypasses the Replicated/Predicted split that otherwise leaves
/// the client reading stale `Health`/`Experience` on the Predicted copy. UI
/// reads this resource directly.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct PlayerStateSnapshot {
    pub hp_current: f32,
    pub hp_max: f32,
    pub pool_current: f32,
    pub pool_max: f32,
    pub xp_current: u32,
    pub xp_level: u32,
    pub xp_to_next: u32,
    /// In-flight cast state. `is_casting == false` means no bar to show;
    /// remaining fields are zero/empty in that case. Sent every tick so the
    /// bar ticks down live on the client.
    #[serde(default)]
    pub is_casting: bool,
    #[serde(default)]
    pub cast_total: f32,
    #[serde(default)]
    pub cast_remaining: f32,
    #[serde(default)]
    pub cast_school: String,
    #[serde(default)]
    pub cast_ability_name: String,
    /// Pillar scores + caps + banked XP. Sent every tick for the
    /// owning client — unit frame's pillar bars + "XP to next pillar
    /// point" readout come from here. Secondary/tertiary gear stats
    /// stream via a separate `EquippedStatsSnapshot` (not shipped yet).
    #[serde(default)]
    pub might: u16,
    #[serde(default)]
    pub finesse: u16,
    #[serde(default)]
    pub arcana: u16,
    #[serde(default)]
    pub might_cap: u16,
    #[serde(default)]
    pub finesse_cap: u16,
    #[serde(default)]
    pub arcana_cap: u16,
    #[serde(default)]
    pub might_xp: u32,
    #[serde(default)]
    pub finesse_xp: u32,
    #[serde(default)]
    pub arcana_xp: u32,
    #[serde(default)]
    pub might_xp_to_next: u32,
    #[serde(default)]
    pub finesse_xp_to_next: u32,
    #[serde(default)]
    pub arcana_xp_to_next: u32,
    /// Stamina pool — powers Block (continuous drain) and Parry
    /// (per-hit cost). UI renders a bar below mana.
    #[serde(default)]
    pub stamina_current: f32,
    #[serde(default)]
    pub stamina_max: f32,
    /// True while the player holds an active Block stance. UI shows a
    /// "BLOCKING" tag and tints the stamina bar.
    #[serde(default)]
    pub is_blocking: bool,
    /// True during the brief Parry window after a tap. Short-lived
    /// (~0.35s) but useful for client-side tell animation.
    #[serde(default)]
    pub is_parrying: bool,
}

// ─── inventory + equipment protocol ────────────────────────────────────────
//
// Wire model: server owns PlayerInventory + Equipped components; clients
// receive snapshots and send Equip/Unequip requests. Snapshots ship the
// raw `ItemInstance` tuple (base_id + material_id + quality_id + affixes)
// — client resolves locally against its own `ContentRegistry`. Cheap on
// the wire; display name + stats composed on demand.

/// One line in an inventory snapshot: an item instance plus its stack count.
/// `None` in an `InventorySnapshot.slots` position means that slot is empty.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct InventorySlotEntry {
    pub instance: vaern_items::ItemInstance,
    pub count: u32,
}

/// Server → Client: the owning player's inventory. Resent on any change
/// (add / take / equip displaces / unequip inserts). `slots.len() ==
/// capacity`; empty slots are `None` so the indices the client clicks
/// map 1:1 to server-side slot indices.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct InventorySnapshot {
    pub capacity: u32,
    pub slots: Vec<Option<InventorySlotEntry>>,
}

/// One entry in an equipped snapshot — which slot holds which instance.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct EquippedSlotEntry {
    pub slot: vaern_equipment::EquipSlot,
    pub instance: vaern_items::ItemInstance,
}

/// Server → Client: the owning player's currently-equipped items.
/// Resent on any equip/unequip. Slots not present in the list are empty.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct EquippedSnapshot {
    pub entries: Vec<EquippedSlotEntry>,
}

/// Client → Server: "take the item at `inventory_idx` and put it in
/// `slot`." Server validates kind→slot pairing and (for weapons)
/// mainhand/offhand grip rules. Any displaced item returns to the
/// inventory; next tick's snapshot reflects the new state.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct EquipRequest {
    pub slot: vaern_equipment::EquipSlot,
    pub inventory_idx: u32,
}

/// Client → Server: "clear `slot`, push its contents back to inventory."
/// No-op if the slot is already empty.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct UnequipRequest {
    pub slot: vaern_equipment::EquipSlot,
}

/// Client → Server: "consume one charge of the item at `inventory_idx`."
/// Server resolves the item's `ConsumeEffect`, applies it to the player
/// (heal / buff), and decrements the stack. Ignored if the slot is empty,
/// the item isn't a `Consumable`, or the effect is `None`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ConsumeItemRequest {
    pub inventory_idx: u32,
}

/// Client → Server: "bind this item template to belt slot `slot_idx`."
/// `slot_idx` is 0..BELT_SLOTS. Server validates the instance is present
/// somewhere in the player's inventory and is a Consumable before
/// storing the binding.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct BindBeltSlotRequest {
    pub slot_idx: u8,
    pub instance: vaern_items::ItemInstance,
}

/// Client → Server: "unbind belt slot `slot_idx`." No-op if empty.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ClearBeltSlotRequest {
    pub slot_idx: u8,
}

/// Client → Server: "fire belt slot `slot_idx`." Server searches the
/// player's inventory for a matching stack; if found, applies the
/// ConsumeEffect + decrements. Binding persists on empty-inventory.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ConsumeBeltRequest {
    pub slot_idx: u8,
}

/// Server → Client: per-tick (on change) snapshot of the 4 belt slot
/// bindings. Empty slots are `None`. Client renders the belt strip from
/// this + looks up matching counts from its OwnInventory locally.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ConsumableBeltSnapshot {
    pub slots: Vec<Option<vaern_items::ItemInstance>>,
}

// ─── loot container protocol ────────────────────────────────────────────────
//
// Mob dies → server spawns a server-only entity with a `LootContainer`
// component (contents + owner + world position). Owner-only: containers
// are visible only to the top-threat player who killed the mob.
//
// Clients track nearby loot via `PendingLootsSnapshot` — a per-tick
// summary keyed by `loot_id` (stable u64). When the player presses `G`
// near one, the client sends `LootOpenRequest`; server responds with
// `LootWindowSnapshot` carrying the full contents. Take-individual and
// take-all requests transfer items into the player's inventory. When
// the container empties the server sends `LootClosedNotice` so the
// client window auto-closes.

/// Server-assigned stable id for a loot container. Decoupled from
/// Bevy `Entity` so we don't lock the wire onto generation counters.
pub type LootId = u64;

/// Summary entry in `PendingLootsSnapshot` — just enough info for the
/// client to place a world marker and show item count. Full contents
/// come over `LootWindowSnapshot` only when the owner opens it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LootContainerSummary {
    pub loot_id: LootId,
    pub pos_x: f32,
    pub pos_y: f32,
    pub pos_z: f32,
    pub item_count: u32,
}

/// Server → Client: every container currently owned by the viewing
/// client. Resent on change (spawn / take / despawn). Empty when the
/// player has no unclaimed loot.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct PendingLootsSnapshot {
    pub containers: Vec<LootContainerSummary>,
}

/// One entry in the loot window — an instance plus its stack count.
/// Structurally the same as `InventorySlotEntry`; kept separate so
/// future divergence (e.g. looted-by-whom metadata) doesn't leak.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct LootWindowEntry {
    pub instance: vaern_items::ItemInstance,
    pub count: u32,
}

/// Server → Client: the full contents of one loot container, sent in
/// response to `LootOpenRequest` and then resent whenever the
/// container's contents change while the window is open. Indexed by
/// `loot_id`; client correlates with its active loot window.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct LootWindowSnapshot {
    pub loot_id: LootId,
    pub slots: Vec<LootWindowEntry>,
}

/// Client → Server: "I pressed G near this container, send me its
/// contents." Server validates proximity + ownership.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct LootOpenRequest {
    pub loot_id: LootId,
}

/// Client → Server: take one stack out of the container at `slot_idx`
/// and push it into the player's inventory. Silent no-op if the slot
/// is empty or inventory full (server logs; future UI can surface it).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct LootTakeRequest {
    pub loot_id: LootId,
    pub slot_idx: u32,
}

/// Client → Server: take everything the container holds into the
/// player's inventory. Partial success allowed (some fit, some
/// don't); server responds with updated snapshot.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct LootTakeAllRequest {
    pub loot_id: LootId,
}

/// Server → Client: the container is gone (emptied or despawned via
/// timer). Client closes the loot window if it was showing this
/// container.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct LootClosedNotice {
    pub loot_id: LootId,
}

// ─── harvest protocol ──────────────────────────────────────────────────────
//
// Resource nodes live in the world as replicated entities carrying
// `NodeKind` + `NodeState` from vaern-professions. Client proximity-
// detects and sends `HarvestRequest` on `H`. Server validates range +
// profession skill, yields the node's material, and flips state to
// Harvested. Respawn is server-tick timer; client sees the state
// change through component replication.

/// Client → Server: "harvest this node." Server locates the entity
/// by its replicated id, validates proximity + Available state +
/// the player's profession skill.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HarvestRequest {
    /// Network-replicated target entity (the node). Deserialized the
    /// same way `CastIntent.target` is — via lightyear entity mapping.
    pub node: Entity,
}

impl bevy::ecs::entity::MapEntities for HarvestRequest {
    fn map_entities<M: bevy::ecs::entity::EntityMapper>(&mut self, mapper: &mut M) {
        self.node = mapper.get_mapped(self.node);
    }
}

/// Marker on a player entity. Lets server+client distinguish players from
/// NPCs, and lets a client find its own replicated player by matching its
/// own client id. `core_pillar` is the pillar locked in at char-create;
/// class / archetype / order emerge later through play and are NOT on this
/// tag.
#[derive(Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PlayerTag {
    pub client_id: u64,
    pub core_pillar: Pillar,
}

/// Cardinal-direction WASD inputs from the player. Ships every tick via
/// `lightyear_inputs_native` so the server replays them deterministically and
/// the client predicts ahead. `camera_yaw_mrad` is the orbit-camera's yaw
/// (in milliradians, 1000 = 1 radian) at the moment the input was sampled;
/// server uses it to rotate the stick into world-space so W always means
/// "forward relative to camera" regardless of which way the player faces.
#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Eq, Clone, Reflect)]
pub struct WasdInput {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    #[serde(default)]
    pub camera_yaw_mrad: i32,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Reflect)]
pub enum Inputs {
    Move(WasdInput),
}

impl Default for Inputs {
    fn default() -> Self {
        Self::Move(WasdInput::default())
    }
}

impl MapEntities for Inputs {
    fn map_entities<M: EntityMapper>(&mut self, _mapper: &mut M) {}
}

/// Translate WASD inputs into a unit-length world-space movement direction.
/// Movement is camera-relative: pressing W moves the player along the
/// horizontal direction the camera is looking, A/D strafes perpendicular
/// to that. The camera yaw is bundled into `WasdInput.camera_yaw_mrad`
/// (sampled on the client each tick), so both client prediction and
/// server authoritative movement compute the same delta deterministically.
pub fn input_to_direction(input: &Inputs) -> Vec3 {
    let Inputs::Move(d) = input;
    let forward_input = (d.up as i32 - d.down as i32) as f32;
    let side_input = (d.right as i32 - d.left as i32) as f32;
    if forward_input == 0.0 && side_input == 0.0 {
        return Vec3::ZERO;
    }
    let yaw = d.camera_yaw_mrad as f32 * 0.001;
    // Camera at yaw=0 sits at +Z of player looking -Z → "forward" is -Z.
    let forward = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
    let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
    let dir = forward * forward_input + right * side_input;
    if dir.length_squared() > 0.0 {
        dir.normalize()
    } else {
        Vec3::ZERO
    }
}

/// Transform lerp: translation / scale linear, rotation slerp.
fn lerp_transform(start: Transform, end: Transform, t: f32) -> Transform {
    Transform {
        translation: start.translation.lerp(end.translation, t),
        rotation: start.rotation.slerp(end.rotation, t),
        scale: start.scale.lerp(end.scale, t),
    }
}

pub struct SharedPlugin;

impl Plugin for SharedPlugin {
    fn build(&self, app: &mut App) {
        app.register_component::<Transform>()
            .add_interpolation_with(lerp_transform)
            .add_prediction();
        app.register_component::<Health>().add_prediction();
        app.register_component::<PlayerTag>().add_prediction();
        app.register_component::<ResourcePool>().add_prediction();
        app.register_component::<Casting>()
            .add_map_entities()
            .add_prediction();
        app.register_component::<DisplayName>().add_prediction();
        app.register_component::<NpcKind>().add_prediction();
        app.register_component::<QuestGiverHub>().add_prediction();
        app.register_component::<Experience>().add_prediction();
        app.register_component::<PlayerRace>().add_prediction();
        app.register_component::<ProjectileVisual>().add_prediction();
        app.register_component::<AnimState>().add_prediction();
        // Replicate AnimOverride so the client-side derive_anim_state's
        // `Without<AnimOverride>` filter actually skips entities that
        // the server has flashed into Attacking / Hit — otherwise the
        // transient state gets clobbered within one tick.
        app.register_component::<AnimOverride>().add_prediction();
        // Cosmetic snapshot — drives remote-player Quaternius assembly
        // once that renderer lands. No entity fields → no MapEntities.
        app.register_component::<PlayerAppearance>().add_prediction();
        // Weapon loadout — server folds Equipped → MEGAKIT prop basenames
        // on Changed<Equipped>; remote clients spawn QuaterniusWeaponOverlay
        // entities on the right hand bones from this.
        app.register_component::<PlayerWeapons>().add_prediction();
        // NPC species mesh hint — set once at spawn, never mutates.
        app.register_component::<NpcMesh>().add_prediction();
        // Humanoid-NPC cosmetics — same shape as PlayerAppearance.
        app.register_component::<NpcAppearance>().add_prediction();

        app.register_message::<Ping>()
            .add_direction(NetworkDirection::Bidirectional);
        app.register_message::<ClientHello>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<CastIntent>()
            .add_map_entities()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<StanceRequest>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<CastFired>()
            .add_map_entities()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<HotbarSnapshot>()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<AcceptQuest>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<AbandonQuest>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<ProgressQuest>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<QuestLogSnapshot>()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<PlayerStateSnapshot>()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<InventorySnapshot>()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<EquippedSnapshot>()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<EquipRequest>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<UnequipRequest>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<ConsumeItemRequest>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<BindBeltSlotRequest>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<ClearBeltSlotRequest>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<ConsumeBeltRequest>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<ConsumableBeltSnapshot>()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<PendingLootsSnapshot>()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<LootWindowSnapshot>()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<LootClosedNotice>()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<LootOpenRequest>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<LootTakeRequest>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<LootTakeAllRequest>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<HarvestRequest>()
            .add_map_entities()
            .add_direction(NetworkDirection::ClientToServer);
        // Resource nodes replicate normally like NPCs so all clients
        // see them in the world. State changes (harvested ↔ available)
        // flow via component replication, no explicit message needed.
        app.register_component::<vaern_professions::NodeKind>()
            .add_prediction();
        app.register_component::<vaern_professions::NodeState>()
            .add_prediction();

        app.add_channel::<Channel1>(ChannelSettings {
            mode: ChannelMode::OrderedReliable(ReliableSettings::default()),
            ..default()
        })
        .add_direction(NetworkDirection::Bidirectional);

        app.add_plugins(InputPlugin::<Inputs>::default());
    }
}
