//! Client handshake and player-entity spawn. A netcode client connects, we
//! queue a `PendingSpawn`; `ClientHello` (or the timeout) finalizes it into
//! a replicated player entity with class-appropriate ability kit, Experience,
//! PlayerRace, and a zero-state QuestLog.

use core::time::Duration;
use std::collections::HashMap;

use bevy::prelude::*;
use lightyear::input::native::prelude::ActionState;
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use uuid::Uuid;
use vaern_character::{Experience, PlayerRace};
use vaern_combat::{
    AbilityCooldown, AnimState, Caster, DisplayName, Health, ManualCast, ResourcePool,
    Respawnable, Stamina,
};
use vaern_core::pillar::Pillar;
use vaern_equipment::Equipped;
use vaern_persistence::{PersistedCharacter, PersistedCosmetics, sanitize_loadout};
use vaern_protocol::{PlayerAppearance, PlayerWeapons};
use vaern_stats::{PILLAR_MAX, PillarCaps, PillarScores, PillarXp, derive_primaries};

use crate::persistence::{
    CharacterCosmetics, CharacterId, CharacterStore, persisted_to_quest_log,
};
use crate::resource_nodes::starter_profession_skills;
use vaern_protocol::{
    AbandonQuest, AcceptQuest, BindBeltSlotRequest, CastFired, CastIntent, ClearBeltSlotRequest,
    ClientHello, ConsumableBeltSnapshot, ConsumeBeltRequest, ConsumeItemRequest, HotbarSlotInfo,
    HotbarSnapshot, Inputs, PlayerStateSnapshot, PlayerTag, ProgressQuest, QuestLogSnapshot,
    StanceRequest,
};

use crate::class_kits;
use crate::data::GameData;
use crate::quests::QuestLog;
use crate::starter_gear;
use crate::util::prettify_ability_name;

pub const SEND_INTERVAL: Duration = Duration::from_millis(100);

/// Seconds to wait for a client's `ClientHello` before falling back to
/// `Pillar::Might` and spawning.
const HELLO_TIMEOUT_SECS: f32 = 2.0;

/// Starter PillarScores seed: the chosen pillar lands at `PILLAR_FOCUS_SEED`,
/// the other two at `PILLAR_DABBLE_SEED`. Clamped per-pillar by the race's
/// PillarCaps (a Hearthkin picking Arcana can still only reach 50 Arcana,
/// but starts with 25 of it anyway).
const PILLAR_FOCUS_SEED: u16 = 25;
const PILLAR_DABBLE_SEED: u16 = 5;

/// Server-side hotbar. One ability entity per slot; the client just sends
/// slot indices, server resolves via this component.
///
/// Slots 0..=5 are keyboard-bound (keys 1..=6 on client).
/// Slot 6 = light attack (LMB). Slot 7 = heavy attack (RMB).
#[derive(Component, Debug, Clone, Copy)]
pub struct ServerHotbar {
    pub slots: [Entity; class_kits::TOTAL_ABILITY_SLOTS],
}

/// One deferred spawn: a netcode client has connected but we haven't spawned
/// its player entity yet, because we're waiting for `ClientHello` (or the
/// timeout).
#[derive(Debug, Clone, Copy)]
struct PendingSpawn {
    link_entity: Entity,
    client_id: u64,
    waited_secs: f32,
}

#[derive(Resource, Default)]
pub struct PendingSpawns(Vec<PendingSpawn>);

/// Fallback pillar when `ClientHello` never arrives. Picks Might as the
/// neutral default — melee kit is the most forgiving placeholder for a
/// headless/buggy client that skipped the handshake.
fn fallback_pillar() -> Pillar {
    Pillar::Might
}

/// Build a pillar-weighted starter score table, clamped by race caps.
/// The chosen pillar starts at `PILLAR_FOCUS_SEED`; the other two at
/// `PILLAR_DABBLE_SEED`. Race `PillarCaps` still apply — a (50,100,75)
/// Sunward Elen picking Might can't exceed 50 Might.
pub(crate) fn starter_pillar_scores(pillar: Pillar, caps: PillarCaps) -> PillarScores {
    let mut scores = PillarScores {
        might: PILLAR_DABBLE_SEED,
        finesse: PILLAR_DABBLE_SEED,
        arcana: PILLAR_DABBLE_SEED,
    };
    match pillar {
        Pillar::Might => scores.might = PILLAR_FOCUS_SEED,
        Pillar::Finesse => scores.finesse = PILLAR_FOCUS_SEED,
        Pillar::Arcana => scores.arcana = PILLAR_FOCUS_SEED,
    }
    scores.might = scores.might.min(caps.might);
    scores.finesse = scores.finesse.min(caps.finesse);
    scores.arcana = scores.arcana.min(caps.arcana);
    scores
}

/// Pending server→client snapshot queued during `spawn_player`. Cleared once
/// `send_pending_hotbars` has fed it to the link's MessageSender.
#[derive(Component, Debug)]
pub struct PendingHotbarSnapshot(HotbarSnapshot);

pub fn handle_new_client(trigger: On<Add, LinkOf>, mut commands: Commands) {
    // Bevy tuple cap is 15 for `insert` — split belt messages into a
    // second insert to stay under the limit.
    commands.entity(trigger.entity).insert((
        ReplicationSender::new(SEND_INTERVAL, SendUpdatesMode::SinceLastAck, false),
        MessageReceiver::<CastIntent>::default(),
        MessageReceiver::<StanceRequest>::default(),
        MessageReceiver::<ClientHello>::default(),
        MessageReceiver::<AcceptQuest>::default(),
        MessageReceiver::<AbandonQuest>::default(),
        MessageReceiver::<ProgressQuest>::default(),
        MessageReceiver::<ConsumeItemRequest>::default(),
        MessageSender::<CastFired>::default(),
        MessageSender::<HotbarSnapshot>::default(),
        MessageSender::<QuestLogSnapshot>::default(),
        MessageSender::<PlayerStateSnapshot>::default(),
        Name::new("client-link"),
    ));
    commands.entity(trigger.entity).insert((
        MessageReceiver::<BindBeltSlotRequest>::default(),
        MessageReceiver::<ClearBeltSlotRequest>::default(),
        MessageReceiver::<ConsumeBeltRequest>::default(),
        MessageSender::<ConsumableBeltSnapshot>::default(),
    ));
    println!("new client link: {:?}", trigger.entity);
}

/// On connect, enqueue a pending spawn; don't spawn the player yet.
/// `process_pending_spawns` finalizes once `ClientHello` arrives or the
/// timeout fires.
pub fn handle_connected(
    trigger: On<Add, Connected>,
    query: Query<&RemoteId, With<ClientOf>>,
    mut pending: ResMut<PendingSpawns>,
) {
    let Ok(remote) = query.get(trigger.entity) else { return };
    let client_id = match remote.0 {
        PeerId::Netcode(id) => id,
        _ => return,
    };
    pending.0.push(PendingSpawn {
        link_entity: trigger.entity,
        client_id,
        waited_secs: 0.0,
    });
    println!("client {client_id} connected; waiting for ClientHello");
}

/// Hello payload captured from a `ClientHello` message. Kept as a
/// struct rather than a tuple so adding future fields (account id, etc.)
/// doesn't cascade through the deferred-spawn path.
#[derive(Debug, Clone)]
struct HelloData {
    pillar: Pillar,
    race_id: String,
    character_id: String,
    character_name: String,
    cosmetics: Option<PersistedCosmetics>,
}

impl HelloData {
    fn fallback() -> Self {
        // Used on the hello-timeout path. `Pillar` has no `Default` on
        // purpose (we want explicit choices in gameplay code); we pick
        // the same fallback as the pre-persistence code — Might.
        Self {
            pillar: fallback_pillar(),
            race_id: String::new(),
            character_id: String::new(),
            character_name: String::new(),
            cosmetics: None,
        }
    }
}

/// Resolved spawn path chosen for a pending client. Drives whether we
/// hit disk, mint a fresh UUID-anchored character, or fall through to
/// today's anonymous starter-gear spawn.
enum SpawnSource {
    /// UUID parsed + file exists + loaded cleanly.
    LoadExisting(Box<PersistedCharacter>),
    /// UUID parsed + no file on disk (or file was corrupt and
    /// quarantined). Create a fresh starter character keyed by this
    /// UUID so the next flush writes the initial file.
    CreateNew {
        uuid: Uuid,
        name: String,
        pillar: Pillar,
        race_id: String,
        cosmetics: PersistedCosmetics,
    },
    /// No UUID in the hello (or malformed). Today's pre-persistence
    /// spawn — pillar only, no save. Kept so stale clients and
    /// headless/test harnesses still get a playable character.
    Anonymous { pillar: Pillar, race_id: String },
}

fn resolve_spawn_source(hello: HelloData, store: &CharacterStore) -> SpawnSource {
    if hello.character_id.is_empty() {
        return SpawnSource::Anonymous {
            pillar: hello.pillar,
            race_id: hello.race_id,
        };
    }
    let Ok(uuid) = Uuid::parse_str(&hello.character_id) else {
        warn!(
            "[persist] malformed character_id {:?} — anonymous fallback",
            hello.character_id
        );
        return SpawnSource::Anonymous {
            pillar: hello.pillar,
            race_id: hello.race_id,
        };
    };
    if store.exists(uuid) {
        match store.load(uuid) {
            Ok(ch) => return SpawnSource::LoadExisting(Box::new(ch)),
            Err(e) => {
                warn!("[persist] load {uuid} failed: {e}; recreating");
                // Fall through — the bad file was already quarantined.
            }
        }
    }
    SpawnSource::CreateNew {
        uuid,
        name: hello.character_name,
        pillar: hello.pillar,
        race_id: hello.race_id,
        cosmetics: hello.cosmetics.unwrap_or_default(),
    }
}

/// Drain `ClientHello` messages from each link and resolve pending spawns.
/// Spawns via the requested UUID-anchored character if available, falls
/// back to CreateNew for first-time UUIDs, or anonymous on timeout.
pub fn process_pending_spawns(
    time: Res<Time>,
    data: Res<GameData>,
    store: Res<CharacterStore>,
    mut pending: ResMut<PendingSpawns>,
    mut hello_rx: Query<(&RemoteId, &mut MessageReceiver<ClientHello>), With<ClientOf>>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();

    let mut hellos: HashMap<u64, HelloData> = HashMap::new();
    for (remote, mut rx) in &mut hello_rx {
        let PeerId::Netcode(id) = remote.0 else { continue };
        for msg in rx.receive() {
            hellos.insert(
                id,
                HelloData {
                    pillar: msg.core_pillar,
                    race_id: msg.race_id.clone(),
                    character_id: msg.character_id.clone(),
                    character_name: msg.character_name.clone(),
                    cosmetics: msg.cosmetics.clone(),
                },
            );
        }
    }

    let mut i = 0;
    while i < pending.0.len() {
        let entry = &mut pending.0[i];
        entry.waited_secs += dt;

        let chosen: Option<HelloData> = if let Some(h) = hellos.remove(&entry.client_id) {
            Some(h)
        } else if entry.waited_secs >= HELLO_TIMEOUT_SECS {
            Some(HelloData::fallback())
        } else {
            None
        };

        if let Some(hello) = chosen {
            let entry = pending.0.swap_remove(i);
            let source = resolve_spawn_source(hello, &store);
            spawn_player(
                &mut commands,
                &data,
                entry.link_entity,
                entry.client_id,
                source,
            );
            continue;
        }
        i += 1;
    }
}

/// Spawn a player entity.
///
/// Three sources collapse into one function:
///
/// * **LoadExisting** — rehydrate every persisted component from the
///   on-disk snapshot. Race caps are re-derived from race YAML (not
///   trusted from the file) so balance patches apply retroactively.
///   Vitals restore to max; the player respawns at their race's zone
///   hub (no last-position fidelity). Unresolvable items are dropped.
/// * **CreateNew** — fresh starter character tagged with a server-
///   supplied UUID. `Changed<>` on the first tick flags the entity
///   dirty so the next flush writes the initial file.
/// * **Anonymous** — today's pre-persistence behavior. No UUID, no
///   save path. Pillar-only identity; name + cosmetics remain default.
///
/// Replicated to everyone, predicted for the owner, interpolated for
/// spectators. Archetype / Order / Spec are NOT committed at spawn —
/// characters start at a pillar and evolve into a class through play.
fn spawn_player(
    commands: &mut Commands,
    data: &GameData,
    link_entity: Entity,
    client_id: u64,
    source: SpawnSource,
) {
    // Peel the `source` into a consistent set of locals. This keeps the
    // spawn body below single-path; the only branches that remain are
    // "did we get persisted gear or do we build starter gear".
    struct Seed {
        core_pillar: Pillar,
        race_id: String,
        persisted: Option<PersistedCharacter>,
        uuid: Option<Uuid>,
        name: String,
        cosmetics: PersistedCosmetics,
    }
    let seed = match source {
        SpawnSource::LoadExisting(ch) => Seed {
            core_pillar: ch.core_pillar,
            race_id: ch.race_id.clone(),
            uuid: Some(Uuid::parse_str(&ch.character_id).expect("validated at load")),
            name: ch.name.clone(),
            cosmetics: ch.cosmetics.clone(),
            persisted: Some(*ch),
        },
        SpawnSource::CreateNew { uuid, name, pillar, race_id, cosmetics } => Seed {
            core_pillar: pillar,
            race_id,
            persisted: None,
            uuid: Some(uuid),
            name,
            cosmetics,
        },
        SpawnSource::Anonymous { pillar, race_id } => Seed {
            core_pillar: pillar,
            race_id,
            persisted: None,
            uuid: None,
            name: String::new(),
            cosmetics: PersistedCosmetics::default(),
        },
    };
    let core_pillar = seed.core_pillar;
    let race_id = seed.race_id.as_str();

    let peer = PeerId::Netcode(client_id);
    let mut hotbar_kit = class_kits::build_starter_hotbar_by_pillar(core_pillar, &data.abilities);
    // Overlay per-ability YAML overrides from `flavored/*.yaml` onto each
    // slot's spec before the spec is cloned onto the ability entity.
    for slot in hotbar_kit.iter_mut() {
        if let Some(flavored) =
            data.flavored.get(slot.pillar, &slot.category, slot.tier, &slot.spec.school)
        {
            class_kits::apply_flavored_overrides(&mut slot.spec, flavored);
        }
    }
    // Auto-attacks (light + heavy). Not driven by YAML yet — hardcoded
    // defaults in `build_auto_attacks`; future pass can let weapons tune
    // them.
    let auto_kit = class_kits::build_auto_attacks();
    // Combine into a single list. Slots 0..=5 = keyboard kit, 6 = light,
    // 7 = heavy.
    let mut kit: [class_kits::HotbarSlotDetail; class_kits::TOTAL_ABILITY_SLOTS] =
        std::array::from_fn(|i| {
            if i < class_kits::HOTBAR_SLOTS {
                hotbar_kit[i].clone()
            } else {
                auto_kit[i - class_kits::HOTBAR_SLOTS].clone()
            }
        });
    let _ = &mut kit; // kit is consumed below
    let spawn_zone = data.zone_for_race(race_id).to_string();
    let zone_hub = data.zone_origin(&spawn_zone);
    // Honor the saved position when rehydrating an existing character;
    // fall through to the zone hub for CreateNew / Anonymous / legacy
    // saves that predate position persistence. Respawnable.home stays
    // pinned to the zone hub so "return-to-hub" on death is predictable.
    let (spawn_pos, spawn_yaw) = match seed.persisted.as_ref() {
        Some(ch) => {
            let pos = ch
                .position
                .map(|[x, y, z]| Vec3::new(x, y, z))
                .unwrap_or(zone_hub);
            let yaw = ch.yaw_rad.unwrap_or(0.0);
            (pos, yaw)
        }
        None => (zone_hub, 0.0),
    };

    // Race affinity → PillarCaps. Default (universal 100/100/100) when
    // race_id is empty or missing from the registry — keeps fallback
    // spawn behavior working for clients that skip ClientHello.
    let caps = data
        .races
        .iter()
        .find(|r| r.id == race_id)
        .map(|r| PillarCaps {
            might: (r.affinity.might as u16).min(PILLAR_MAX),
            finesse: (r.affinity.finesse as u16).min(PILLAR_MAX),
            arcana: (r.affinity.arcana as u16).min(PILLAR_MAX),
        })
        .unwrap_or_default();
    println!(
        "[spawn] client {client_id} race='{race_id}' → zone '{spawn_zone}' at ({:.0},{:.0},{:.0})",
        spawn_pos.x, spawn_pos.y, spawn_pos.z
    );

    // Bevy tuple bundles cap at 15; group by concern so the whole
    // player bundle stays under the limit even as we add more state.
    let identity = (
        Name::new(format!("player-{client_id}-{core_pillar}")),
        Transform::from_translation(spawn_pos).with_rotation(Quat::from_rotation_y(spawn_yaw)),
        PlayerRace(if race_id.is_empty() {
            "mannin".into()
        } else {
            race_id.to_string()
        }),
        PlayerTag { client_id, core_pillar },
        DisplayName(seed.name.clone()),
        CharacterCosmetics(seed.cosmetics.clone()),
        // Replicated copy of the cosmetics so remote clients can build
        // the Quaternius mesh for this player (consumer lands with the
        // "Remote player + NPC mesh" renderer).
        PlayerAppearance(seed.cosmetics.clone()),
        // Replicated weapon loadout. Seeded empty — filled by
        // `sync_player_weapons_from_gear` once `Equipped` arrives
        // (and on every subsequent `Changed<Equipped>`).
        PlayerWeapons::default(),
    );
    // Progression + loadout: fresh vs rehydrated-from-disk. Vitals
    // always restore to max (we don't persist current HP/mana/stamina
    // per the design call — no logout-safe-zone concept); caps always
    // re-derive from race YAML so a race-balance patch applies to
    // existing characters on next login.
    let (pillar_scores, pillar_xp, experience, quest_log, professions, inventory, equipped, belt) =
        if let Some(ch) = seed.persisted.as_ref() {
            let mut inv = ch.inventory.clone();
            let mut eq = ch.equipped.clone();
            let mut b = ch.belt.clone();
            let dropped = sanitize_loadout(&mut inv, &mut eq, &mut b, &data.content);
            for d in &dropped {
                warn!(
                    "[persist] dropped unresolvable item on load: source={} base_id={}",
                    d.source, d.base_id
                );
            }
            (
                ch.pillar_scores,
                ch.pillar_xp,
                ch.experience,
                persisted_to_quest_log(&ch.quest_log),
                ch.professions.clone(),
                inv,
                eq,
                b,
            )
        } else {
            (
                starter_pillar_scores(core_pillar, caps),
                PillarXp::default(),
                Experience::default(),
                QuestLog::default(),
                starter_profession_skills(),
                starter_gear::build_starter_inventory_for_pillar(core_pillar, &data.content),
                Equipped::default(),
                vaern_inventory::ConsumableBelt::default(),
            )
        };

    let derived = derive_primaries(&pillar_scores);
    let combat = (
        Health::full(derived.hp_max as f32),
        ResourcePool::full(derived.mana_max as f32, 10.0),
        // Stamina: 100/100 baseline, 12/sec regen. Block drains at
        // 15/sec (6.7s full-pool sustain) and Parry taps cost 20 per
        // negate. Tuned alongside stance constants in combat_io.
        Stamina::full(100.0, 12.0),
        AnimState::default(),
        ManualCast,
        Respawnable { home: zone_hub },
    );
    let progression = (quest_log, experience, pillar_scores, caps, pillar_xp);
    let gear = (inventory, equipped, belt, professions);
    let net = (
        ActionState::<Inputs>::default(),
        Replicate::to_clients(NetworkTarget::All),
        PredictionTarget::to_clients(NetworkTarget::Single(peer)),
        InterpolationTarget::to_clients(NetworkTarget::AllExceptSingle(peer)),
        ControlledBy {
            owner: link_entity,
            lifetime: Default::default(),
        },
    );

    let entity = commands
        .spawn((identity, combat, progression, gear, net))
        .id();

    // Persistence marker. Anonymous fallback spawns don't get one, so
    // the dirty-mark + save path ignores them entirely.
    if let Some(uuid) = seed.uuid {
        commands.entity(entity).insert(CharacterId(uuid));
    }

    let slots: [Entity; class_kits::TOTAL_ABILITY_SLOTS] = std::array::from_fn(|i| {
        commands
            .spawn((
                kit[i].spec.clone(),
                AbilityCooldown::ready(),
                Caster(entity),
            ))
            .id()
    });
    commands.entity(entity).insert(ServerHotbar { slots });

    // Build + queue the HotbarSnapshot on the link entity; a follow-up system
    // drains it into the link's MessageSender<HotbarSnapshot>. Needs to be a
    // pending component because MessageSenders aren't directly accessible via
    // Commands at this point.
    //
    // Per-slot content is pulled from flavored/<pillar>/<category>.yaml when
    // a matching (tier, school) variant exists — that's the source of
    // canonical id (used as the client-side icon filename), description,
    // display name, and damage_type. Falls back to the generic
    // abilities/<pillar>/<category>.yaml variant name if no flavored entry.
    let snapshot = HotbarSnapshot {
        slots: (0..class_kits::TOTAL_ABILITY_SLOTS)
            .map(|i| {
                let d = &kit[i];
                // Auto-attack slots (6/7) don't have flavored YAML entries —
                // they're hardcoded in `build_auto_attacks`. Only consult
                // flavored for the keyboard slots.
                let flavored = if i < class_kits::HOTBAR_SLOTS {
                    data.flavored.get(d.pillar, &d.category, d.tier, &d.spec.school)
                } else {
                    None
                };
                let pillar_str = class_kits::pillar_str(d.pillar).to_string();
                let ability_id = flavored.map(|f| f.id.clone()).unwrap_or_else(|| {
                    format!(
                        "{}.{}.{}.{}.{}",
                        pillar_str, d.category, d.tier, d.spec.school, d.variant_name
                    )
                });
                let name = flavored
                    .map(|f| prettify_ability_name(&f.name))
                    .unwrap_or_else(|| prettify_ability_name(&d.variant_name));
                let description = flavored.map(|f| f.description.clone()).unwrap_or_default();
                let damage_type = flavored.map(|f| f.damage_type.clone()).unwrap_or_default();
                let shape_str = match d.spec.shape {
                    vaern_combat::AbilityShape::Target => "target",
                    vaern_combat::AbilityShape::AoeOnTarget => "aoe_on_target",
                    vaern_combat::AbilityShape::AoeOnSelf => "aoe_on_self",
                    vaern_combat::AbilityShape::Cone => "cone",
                    vaern_combat::AbilityShape::Line => "line",
                    vaern_combat::AbilityShape::Projectile => "projectile",
                }
                .to_string();
                HotbarSlotInfo {
                    slot: i as u8,
                    ability_id,
                    name,
                    description,
                    school: d.spec.school.clone(),
                    category: d.category.clone(),
                    pillar: pillar_str,
                    damage_type,
                    tier: d.tier,
                    damage: d.spec.damage,
                    cooldown_secs: d.spec.cooldown_secs,
                    cast_secs: d.spec.cast_secs,
                    resource_cost: d.spec.resource_cost,
                    range: d.spec.range,
                    shape: shape_str,
                    aoe_radius: d.spec.aoe_radius,
                    cone_half_angle_deg: d.spec.cone_half_angle_deg,
                    line_width: d.spec.line_width,
                    projectile_speed: d.spec.projectile_speed,
                }
            })
            .collect(),
    };
    let slot_count = snapshot.slots.len();
    commands
        .entity(link_entity)
        .insert(PendingHotbarSnapshot(snapshot));

    println!(
        "spawned player {entity:?} for client {client_id} on pillar {core_pillar}; queued {slot_count}-slot hotbar snapshot on link {link_entity:?}",
    );
}

pub fn send_pending_hotbars(
    mut q: Query<
        (
            Entity,
            &PendingHotbarSnapshot,
            &mut MessageSender<HotbarSnapshot>,
        ),
        With<ClientOf>,
    >,
    mut commands: Commands,
) {
    for (entity, pending, mut sender) in &mut q {
        let n = pending.0.slots.len();
        let _ = sender.send::<vaern_protocol::Channel1>(pending.0.clone());
        commands.entity(entity).remove::<PendingHotbarSnapshot>();
        println!("[hotbar:send] shipped {n}-slot snapshot to link {entity:?}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps(m: u16, f: u16, a: u16) -> PillarCaps {
        PillarCaps { might: m, finesse: f, arcana: a }
    }

    #[test]
    fn might_seed_focuses_might() {
        let s = starter_pillar_scores(Pillar::Might, caps(100, 100, 100));
        assert_eq!(s.might, PILLAR_FOCUS_SEED);
        assert_eq!(s.finesse, PILLAR_DABBLE_SEED);
        assert_eq!(s.arcana, PILLAR_DABBLE_SEED);
    }

    #[test]
    fn arcana_seed_focuses_arcana() {
        let s = starter_pillar_scores(Pillar::Arcana, caps(100, 100, 100));
        assert_eq!(s.might, PILLAR_DABBLE_SEED);
        assert_eq!(s.finesse, PILLAR_DABBLE_SEED);
        assert_eq!(s.arcana, PILLAR_FOCUS_SEED);
    }

    #[test]
    fn race_caps_clamp_focus_pillar() {
        // Hearthkin (100/50/50) picking Arcana: focus wants 25, cap allows 25, no clamp.
        let s = starter_pillar_scores(Pillar::Arcana, caps(100, 50, 50));
        assert_eq!(s.arcana, PILLAR_FOCUS_SEED);
        // A (low-cap) synthetic race with 10 Arcana would clamp the seed.
        let s = starter_pillar_scores(Pillar::Arcana, caps(100, 50, 10));
        assert_eq!(s.arcana, 10);
    }

    #[test]
    fn race_caps_clamp_dabble_pillars() {
        // Synthetic (100/1/1): dabble clamps to 1 in each off-pillar.
        let s = starter_pillar_scores(Pillar::Might, caps(100, 1, 1));
        assert_eq!(s.might, PILLAR_FOCUS_SEED);
        assert_eq!(s.finesse, 1);
        assert_eq!(s.arcana, 1);
    }
}
