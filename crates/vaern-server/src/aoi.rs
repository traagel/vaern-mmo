//! Server-side area-of-interest replication.
//!
//! Without AoI, the server broadcasts every NPC's Transform to every
//! connected client every tick. With 603 mobs × N clients at 60Hz the
//! server bursts enough UDP packets per tick to overflow the kernel
//! receive buffer even on localhost — initial spawn packets get dropped
//! for some entities, and clients see the survivors rubber-band because
//! legitimate Transform updates compete with ~600 entities' worth of
//! discarded-update noise in the same buffer.
//!
//! AoI here is zone-scoped: one lightyear [`Room`] per starter zone.
//! Every replicated NPC / resource node joins its zone's room at spawn;
//! every connected client's link joins the room of the zone containing
//! their player. Players transition rooms at zone boundaries. Entities
//! gated by `NetworkVisibility` are only replicated to clients that
//! share a room with them; everything else (players, projectiles) has
//! no `NetworkVisibility` and replicates unconditionally.

use std::collections::HashMap;

use bevy::prelude::*;
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use vaern_combat::NpcKind;
use vaern_professions::NodeKind;
use vaern_protocol::PlayerTag;

use crate::data::GameData;

/// One room per starter zone, keyed by zone id. Built at startup from
/// `data.zone_offsets`; never mutated afterward.
#[derive(Resource, Default)]
pub struct ZoneRooms(pub HashMap<String, Entity>);

/// The zone each connected client's link is currently subscribed to.
/// Only updated when the per-tick system detects a transition, so idle
/// ticks cost one HashMap lookup per player.
#[derive(Resource, Default)]
pub struct ClientZone(pub HashMap<u64, String>);

/// Startup: spawn one `Room` per starter zone.
pub fn init_zone_rooms(
    data: Res<GameData>,
    mut rooms: ResMut<ZoneRooms>,
    mut commands: Commands,
) {
    for zone_id in data.zone_offsets.keys() {
        let room = commands
            .spawn((Room::default(), Name::new(format!("room-{zone_id}"))))
            .id();
        rooms.0.insert(zone_id.clone(), room);
    }
    info!("[aoi] created {} zone rooms", rooms.0.len());
}

/// On Added, attach `NetworkVisibility` and add the entity to the room
/// of its nearest zone. Covers both NPCs (`NpcKind`) and resource nodes
/// (`NodeKind`). Loot containers aren't replicated (see `loot_io`), so
/// they don't need a room. Projectiles deliberately stay room-less:
/// their lifetimes are <a few seconds and they're visually tied to a
/// caster who's probably on screen anyway.
pub fn assign_added_entities_to_rooms(
    data: Res<GameData>,
    rooms: Res<ZoneRooms>,
    new_npcs: Query<(Entity, &Transform), Added<NpcKind>>,
    new_nodes: Query<(Entity, &Transform), Added<NodeKind>>,
    mut commands: Commands,
) {
    for (entity, tf) in &new_npcs {
        let zone = nearest_zone(&data, tf.translation);
        info!("[aoi:assign] npc {:?} → zone={}", entity, zone);
        assign_to_zone_room(&data, &rooms, entity, tf.translation, &mut commands);
    }
    for (entity, tf) in &new_nodes {
        assign_to_zone_room(&data, &rooms, entity, tf.translation, &mut commands);
    }
}

fn assign_to_zone_room(
    data: &GameData,
    rooms: &ZoneRooms,
    entity: Entity,
    pos: Vec3,
    commands: &mut Commands,
) {
    let zone_id = nearest_zone(data, pos);
    let Some(&room) = rooms.0.get(&zone_id) else { return };
    commands.entity(entity).try_insert(NetworkVisibility);
    commands.trigger(RoomEvent {
        room,
        target: RoomTarget::AddEntity(entity),
    });
}

/// Per-tick: detect zone transitions for each connected player and
/// migrate its link sender between rooms. No-op when nothing changed.
pub fn sync_player_zone_subscriptions(
    data: Res<GameData>,
    rooms: Res<ZoneRooms>,
    mut client_zone: ResMut<ClientZone>,
    players: Query<(&PlayerTag, &Transform, &ControlledBy)>,
    mut commands: Commands,
) {
    for (tag, tf, cb) in &players {
        let new_zone = nearest_zone(data.as_ref(), tf.translation);
        let current = client_zone.0.get(&tag.client_id);
        if current.map(String::as_str) == Some(new_zone.as_str()) {
            continue;
        }
        if let Some(old_zone) = current {
            if let Some(&old_room) = rooms.0.get(old_zone) {
                commands.trigger(RoomEvent {
                    room: old_room,
                    target: RoomTarget::RemoveSender(cb.owner),
                });
            }
        }
        if let Some(&new_room) = rooms.0.get(&new_zone) {
            commands.trigger(RoomEvent {
                room: new_room,
                target: RoomTarget::AddSender(cb.owner),
            });
            info!(
                "[aoi] client {} zone: {:?} -> {}",
                tag.client_id, current, new_zone
            );
            client_zone.0.insert(tag.client_id, new_zone);
        }
    }
}

/// Forget a client's remembered zone when it disconnects. Lightyear's
/// RoomPlugin already pops the sender from every `Room`; this keeps
/// the parallel `ClientZone` bookkeeping in sync so a reconnecting
/// client with the same id rebinds cleanly.
pub fn handle_client_disconnect(
    trigger: On<Remove, Connected>,
    links: Query<&RemoteId, With<ClientOf>>,
    mut client_zone: ResMut<ClientZone>,
) {
    let Ok(remote) = links.get(trigger.entity) else { return };
    if let PeerId::Netcode(id) = remote.0 {
        client_zone.0.remove(&id);
    }
}

fn nearest_zone(data: &GameData, pos: Vec3) -> String {
    data.zone_offsets
        .iter()
        .min_by(|a, b| {
            a.1.distance_squared(pos)
                .total_cmp(&b.1.distance_squared(pos))
        })
        .map(|(id, _)| id.clone())
        .unwrap_or_default()
}
