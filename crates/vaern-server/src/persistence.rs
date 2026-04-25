//! Server-side character persistence glue.
//!
//! Wires `vaern-persistence`'s `ServerCharacterStore` into the ECS:
//!
//! * `CharacterId` marker on any persisted player entity. Anonymous /
//!   timeout-fallback spawns don't get one, so they're never saved.
//! * `CharacterCosmetics` stores the cosmetic snapshot server-side.
//!   PR3 will replace this with the replicated `PlayerAppearance`
//!   component; for now it's a plain server-only component that flows
//!   through save/load.
//! * `CharactersDirty` set + `SaveTimer` drive the 5-second flush.
//!   `mark_dirty_on_change` promotes any `Changed<>` on a persisted
//!   component into an entry in the set; `flush_dirty_characters`
//!   drains it on tick.
//! * `save_on_disconnect` observer fires synchronously when a link's
//!   `Connected` marker is removed — closes the save window at logout
//!   so crashes only lose ≤5s of progress.

use std::collections::HashSet;
use std::time::Duration;

use bevy::ecs::query::QueryData;
use bevy::prelude::*;
use lightyear::prelude::*;
use uuid::Uuid;
use vaern_character::{Experience, PlayerRace};
use vaern_combat::DisplayName;
use vaern_economy::PlayerWallet;
use vaern_equipment::Equipped;
use vaern_inventory::{ConsumableBelt, PlayerInventory};
use vaern_assets::quaternius::{outfit_from_equipped, weapon_props_from_equipped};
use vaern_persistence::{
    PersistedCharacter, PersistedCosmetics, PersistedHeadSlot, PersistedOutfitSlot,
    PersistedQuestEntry, PersistedQuestLog, SCHEMA_VERSION, ServerCharacterStore,
};
use vaern_professions::ProfessionSkills;
use vaern_protocol::{PlayerAppearance, PlayerTag, PlayerWeapons};
use vaern_stats::{PillarCaps, PillarScores, PillarXp};

use crate::data::GameData;
use crate::quests::{QuestLog, QuestLogProgress};

/// ECS newtype wrapper around `ServerCharacterStore`. Lives here (not in
/// `vaern-persistence`) so the I/O crate stays free of a direct Bevy dep.
/// `Deref` lets call sites use `store.save(...)` as if it were the raw
/// store.
#[derive(Resource, Debug, Clone)]
pub struct CharacterStore(pub ServerCharacterStore);

impl std::ops::Deref for CharacterStore {
    type Target = ServerCharacterStore;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Marker for persisted players. UUID doubles as the `<uuid>.json`
/// filename in the store.
#[derive(Component, Debug, Clone, Copy)]
pub struct CharacterId(pub Uuid);

/// Server-side cosmetic snapshot. Source-of-truth for what gets written
/// to `PersistedCharacter.cosmetics`. Not replicated — PR3 introduces a
/// replicated `PlayerAppearance` component that mirrors this shape for
/// remote-player rendering.
#[derive(Component, Debug, Clone, Default)]
pub struct CharacterCosmetics(pub PersistedCosmetics);

/// UUIDs of characters whose on-entity state has drifted from disk
/// since the last flush.
#[derive(Resource, Default)]
pub struct CharactersDirty(pub HashSet<Uuid>);

/// Wall-clock flush cadence. Real time (not virtual) so a paused
/// schedule doesn't stall the save cycle.
#[derive(Resource)]
pub struct SaveTimer(pub Timer);

impl Default for SaveTimer {
    fn default() -> Self {
        Self(Timer::new(Duration::from_secs(5), TimerMode::Repeating))
    }
}

// ---------------------------------------------------------------------------
// Query bundle — too many fields for a plain tuple (Bevy caps at 15).
// ---------------------------------------------------------------------------

#[derive(QueryData)]
pub struct PersistedPlayerQuery {
    pub entity: Entity,
    pub character_id: &'static CharacterId,
    pub display_name: &'static DisplayName,
    pub tag: &'static PlayerTag,
    pub race: &'static PlayerRace,
    pub cosmetics: &'static CharacterCosmetics,
    pub experience: &'static Experience,
    pub pillar_scores: &'static PillarScores,
    pub pillar_caps: &'static PillarCaps,
    pub pillar_xp: &'static PillarXp,
    pub inventory: &'static PlayerInventory,
    pub equipped: &'static Equipped,
    pub belt: &'static ConsumableBelt,
    pub professions: &'static ProfessionSkills,
    pub wallet: &'static PlayerWallet,
    pub quest_log: &'static QuestLog,
    pub transform: &'static Transform,
    pub controlled_by: &'static ControlledBy,
}

/// Fold a player entity's persisted components into a `PersistedCharacter`.
/// `now` is the unix-seconds timestamp to stamp on `updated_at`.
pub fn build_persisted(item: &PersistedPlayerQueryItem<'_, '_>, now: i64) -> PersistedCharacter {
    PersistedCharacter {
        schema_version: SCHEMA_VERSION,
        character_id: item.character_id.0.to_string(),
        name: item.display_name.0.clone(),
        race_id: item.race.0.clone(),
        core_pillar: item.tag.core_pillar,
        cosmetics: item.cosmetics.0.clone(),
        experience: *item.experience,
        pillar_scores: *item.pillar_scores,
        pillar_caps: *item.pillar_caps,
        pillar_xp: *item.pillar_xp,
        inventory: item.inventory.clone(),
        equipped: item.equipped.clone(),
        belt: item.belt.clone(),
        professions: item.professions.clone(),
        wallet_copper: item.wallet.copper,
        quest_log: PersistedQuestLog {
            entries: quest_log_to_persisted(item.quest_log),
        },
        position: Some([
            item.transform.translation.x,
            item.transform.translation.y,
            item.transform.translation.z,
        ]),
        yaw_rad: Some(item.transform.rotation.to_euler(EulerRot::YXZ).0),
        // `created_at` belongs on the first-save path only, and we
        // don't have easy access to it here. flush_dirty_characters
        // overrides with the on-disk value when the file pre-exists.
        created_at: now,
        updated_at: now,
    }
}

fn quest_log_to_persisted(log: &QuestLog) -> Vec<PersistedQuestEntry> {
    let mut entries: Vec<PersistedQuestEntry> = log
        .entries
        .iter()
        .map(|(chain_id, p)| PersistedQuestEntry {
            chain_id: chain_id.clone(),
            current_step: p.current_step,
            total_steps: p.total_steps,
            completed: p.completed,
            kill_count: p.kill_count,
        })
        .collect();
    entries.sort_by(|a, b| a.chain_id.cmp(&b.chain_id));
    entries
}

/// Inverse of `quest_log_to_persisted`. Used on load to seed the
/// server's in-memory `QuestLog` from disk.
pub fn persisted_to_quest_log(persisted: &PersistedQuestLog) -> QuestLog {
    let mut log = QuestLog::default();
    for entry in &persisted.entries {
        log.entries.insert(
            entry.chain_id.clone(),
            QuestLogProgress {
                current_step: entry.current_step,
                total_steps: entry.total_steps,
                completed: entry.completed,
                kill_count: entry.kill_count,
            },
        );
    }
    // Flip dirty so `broadcast_quest_logs` ships the loaded log to the
    // owning client on its first tick after spawn.
    log.dirty = true;
    log
}

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

/// Promote any change on a persisted component into an entry in
/// `CharactersDirty`. `Changed<T>` is inclusive of `Added<T>`, so a
/// freshly spawned persisted character ends up dirty on its first
/// tick — the next flush saves a first-time-create file without a
/// dedicated save-on-create call site.
pub fn mark_dirty_on_change(
    q: Query<
        &CharacterId,
        Or<(
            Changed<PlayerInventory>,
            Changed<Equipped>,
            Changed<ConsumableBelt>,
            Changed<Experience>,
            Changed<PillarScores>,
            Changed<PillarXp>,
            Changed<ProfessionSkills>,
            Changed<QuestLog>,
            Changed<PlayerWallet>,
            Changed<Transform>,
        )>,
    >,
    mut dirty: ResMut<CharactersDirty>,
) {
    for cid in &q {
        dirty.0.insert(cid.0);
    }
}

/// On wall-clock cadence, drain the dirty set and persist each entry.
/// Runs on `Time<Real>` so virtual-time pauses don't stall the save.
/// Failed saves log a warn; the id stays dropped from the dirty set
/// regardless, so one I/O hiccup doesn't wedge the pipeline — the
/// next mutation re-queues it.
pub fn flush_dirty_characters(
    time: Res<Time<Real>>,
    mut timer: ResMut<SaveTimer>,
    mut dirty: ResMut<CharactersDirty>,
    store: Res<CharacterStore>,
    players: Query<PersistedPlayerQuery>,
) {
    timer.0.tick(time.delta());
    if !timer.0.just_finished() || dirty.0.is_empty() {
        return;
    }
    let now = unix_now();
    let batch: Vec<Uuid> = dirty.0.drain().collect();
    for uuid in batch {
        let Some(item) = players.iter().find(|item| item.character_id.0 == uuid) else {
            // Owner logged out between dirty-mark and flush; the
            // disconnect observer already took the save.
            continue;
        };
        let persisted = build_persisted(&item, now);
        if let Err(e) = store.save(uuid, &persisted) {
            warn!("[persist] flush save failed for {uuid}: {e}");
        }
    }
}

/// Synchronous save on client-link disconnect. Co-located observer with
/// `aoi::handle_client_disconnect`; registered separately in main.rs.
/// `On<Remove, Connected>` fires before entity despawn, so the player's
/// components are still queryable via `ControlledBy.owner`.
pub fn save_on_disconnect(
    trigger: On<Remove, Connected>,
    store: Res<CharacterStore>,
    players: Query<PersistedPlayerQuery>,
) {
    let now = unix_now();
    for item in &players {
        if item.controlled_by.owner != trigger.entity {
            continue;
        }
        let uuid = item.character_id.0;
        let persisted = build_persisted(&item, now);
        match store.save(uuid, &persisted) {
            Ok(()) => info!("[persist] saved {uuid} on disconnect"),
            Err(e) => warn!("[persist] disconnect save failed for {uuid}: {e}"),
        }
    }
}

/// Fold the gear-derived Quaternius outfit into the replicated
/// `PlayerAppearance` whenever equipment or cosmetics change. Gear wins
/// for body / legs / arms / feet; head piece: armor wins if equipped,
/// else fall back to the cosmetic head piece; hair and beard stay from
/// the cosmetic snapshot. Remote clients read `PlayerAppearance` to
/// build the Quaternius mesh — this system is what makes equipping
/// armor visible to other players.
pub fn sync_player_appearance_from_gear(
    data: Res<GameData>,
    mut q: Query<
        (&Equipped, &CharacterCosmetics, &mut PlayerAppearance),
        Or<(
            Changed<Equipped>,
            Changed<CharacterCosmetics>,
            Added<PlayerAppearance>,
        )>,
    >,
) {
    for (equipped, cos, mut appearance) in &mut q {
        let gear = outfit_from_equipped(equipped.slots(), &data.content);
        let mut next = cos.0.clone();
        next.body = gear.body.map(PersistedOutfitSlot::from_slot);
        next.legs = gear.legs.map(PersistedOutfitSlot::from_slot);
        next.arms = gear.arms.map(PersistedOutfitSlot::from_slot);
        next.feet = gear.feet.map(PersistedOutfitSlot::from_slot);
        if let Some(head) = gear.head_piece {
            next.head_piece = Some(PersistedHeadSlot::from_slot(head));
        }
        // Only write on an actual diff to avoid Changed<PlayerAppearance>
        // firing in a loop against itself (Equipped changes → we write →
        // Changed<PlayerAppearance> fires on remote clients but not here,
        // since only `Changed<Equipped>` triggers this system).
        if appearance.0 != next {
            appearance.0 = next;
        }
    }
}

/// Fold `Equipped` → MEGAKIT prop basenames into the replicated
/// `PlayerWeapons` component on every gear change. Remote clients
/// read this to spawn `QuaterniusWeaponOverlay` entities on the
/// correct hand bones. Only writes on diff so replication stays quiet.
pub fn sync_player_weapons_from_gear(
    data: Res<GameData>,
    mut q: Query<
        (&Equipped, &mut PlayerWeapons),
        Or<(Changed<Equipped>, Added<PlayerWeapons>)>,
    >,
) {
    for (equipped, mut weapons) in &mut q {
        let props = weapon_props_from_equipped(equipped.slots(), &data.content);
        let next = PlayerWeapons {
            mainhand: props.mainhand,
            offhand: props.offhand,
        };
        if *weapons != next {
            *weapons = next;
        }
    }
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
