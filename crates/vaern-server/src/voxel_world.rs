//! Server-side voxel world — authoritative ground state, edit pipeline,
//! and replication to clients.
//!
//! Design: every stage is a dedicated Bevy system with explicit inputs
//! and outputs. The pipeline reads top-to-bottom:
//!
//! ```text
//!   Added<ClientOf>       → queue_reconnect_snapshots
//!                              │
//!   MessageReceiver            │      ┌──────────────────────┐
//!    <ServerEditStroke> ──→ validate_edit_requests ──→ Messages<ValidatedEditStroke>
//!                                                              │
//!                              │                               ▼
//!                              │                      apply_validated_edits ──→ ChunkStore mutation
//!                              │                               │                     + DirtyChunks marks
//!                              │                               │                     + EditedChunks set
//!                              │                               ▼
//!                              │                      enqueue_dirty_chunks_for_broadcast
//!                              │                               │
//!                              ▼                               ▼
//!               drain_reconnect_snapshots            broadcast_voxel_deltas
//!                       │                                      │
//!                       ▼                                      ▼
//!            MessageSender<VoxelChunkDelta>        MessageSender<VoxelChunkDelta>
//!            (targeted at one new client)          (broadcast to every ClientOf)
//! ```
//!
//! Each stage is independently testable and swappable — e.g. the
//! validator can be replaced with a stricter one (cooldowns, ability
//! gating) without touching the apply or broadcast stages.

use bevy::prelude::*;
use lightyear::prelude::server::*;
use lightyear::prelude::*;

use vaern_protocol::{
    Channel1, PlayerTag, ServerBrushMode, ServerEditStroke, VoxelChunkDelta,
};
use vaern_voxel::chunk::{ChunkCoord, ChunkStore, DirtyChunks};
use vaern_voxel::edit::{BrushMode, EditStroke, SphereBrush};
use vaern_voxel::generator::{HeightfieldGenerator, WorldGenerator};
use vaern_voxel::persistence::{apply_into_store, load_from_disk};
use vaern_voxel::replication::ChunkDelta;
use vaern_voxel::VoxelChunk;

use bevy::math::IVec3;
use std::path::{Path, PathBuf};

use std::collections::{HashMap, HashSet, VecDeque};

// --- tuning -----------------------------------------------------------------

/// Chunks around each active player to keep seeded per axis. R=2 =
/// 5×5×3 = 75 chunks × 11 MB/player — covers the Y-snap descent
/// probe (±32u) plus a little hysteresis.
const SERVER_STREAM_RADIUS_XZ: i32 = 2;
const SERVER_STREAM_RADIUS_Y: i32 = 1;

/// Cap on edit-stroke radius (world units). Rejects malicious +inf.
const MAX_EDIT_RADIUS: f32 = 12.0;
/// Max distance from the requesting player's position to the edit
/// center. Prevents a stale / out-of-sight client from terraforming
/// across the map. Generous for now; tighten once ability gating is
/// in place.
const MAX_EDIT_RANGE_FROM_PLAYER: f32 = 40.0;

/// Chunks broadcast per fixed tick across the live-edit queue.
const LIVE_BROADCAST_PER_TICK: usize = 8;
/// Chunks sent per fixed tick to a reconnecting client as part of
/// catching them up to the current edited state. Kept lower than the
/// live-broadcast budget so a new login doesn't tank active-player
/// bandwidth.
const RECONNECT_BROADCAST_PER_TICK: usize = 4;

// --- plugin -----------------------------------------------------------------

pub struct VoxelServerPlugin;

impl Plugin for VoxelServerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EditedChunks>()
            .init_resource::<PendingDeltas>()
            .init_resource::<PendingReconnectSnapshots>()
            .add_message::<ValidatedEditStroke>()
            // Replay authored voxel edits at boot. Each chunk loaded
            // from disk is registered in `EditedChunks` so the existing
            // `queue_reconnect_snapshots` system ships it to every
            // connecting client — no separate broadcast path needed.
            .add_systems(Startup, load_authored_voxel_edits)
            .add_systems(
                FixedUpdate,
                (
                    stream_chunks_around_players,
                    validate_edit_requests,
                    apply_validated_edits,
                    enqueue_dirty_chunks_for_broadcast,
                    broadcast_voxel_deltas,
                    queue_reconnect_snapshots,
                    drain_reconnect_snapshots,
                )
                    .chain(),
            );
    }
}

// --- startup: replay authored edits ----------------------------------------

/// Resolve the path to the authored voxel-edits file. Server can
/// override via `VAERN_VOXEL_EDITS` for staging environments; default
/// is the workspace canonical `src/generated/world/voxel_edits.bin`.
fn voxel_edits_path() -> PathBuf {
    if let Ok(env) = std::env::var("VAERN_VOXEL_EDITS") {
        return PathBuf::from(env);
    }
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../src/generated/world/voxel_edits.bin")
}

/// Read the on-disk delta file. For each delta:
/// 1. Seed the destination chunk from the heightfield (if not already
///    loaded — startup `ChunkStore` is empty).
/// 2. Apply the delta in-place, version-gated.
/// 3. Mark the chunk dirty (mesher would care if we ran one server-side
///    — currently a no-op since the server doesn't mesh).
/// 4. **Register in `EditedChunks`** so `queue_reconnect_snapshots`
///    sends the chunk to every newly-connecting client.
fn load_authored_voxel_edits(
    mut store: ResMut<ChunkStore>,
    mut dirty: ResMut<DirtyChunks>,
    mut edited: ResMut<EditedChunks>,
) {
    let path = voxel_edits_path();
    let deltas = match load_from_disk(&path) {
        Ok(d) => d,
        Err(e) => {
            warn!("server: failed to load voxel_edits at {path:?}: {e}");
            return;
        }
    };
    if deltas.is_empty() {
        info!("server: no authored voxel edits at {path:?} (file missing or empty)");
        return;
    }
    let generator = HeightfieldGenerator::new();
    let applied = apply_into_store(&deltas, &mut store, &mut dirty, &generator);
    for delta in &deltas {
        edited
            .coords
            .insert(ChunkCoord(IVec3::from_array(delta.coord)));
    }
    info!(
        "server: replayed {applied} authored chunk edits from {path:?} \
         ({} registered for reconnect-broadcast)",
        edited.coords.len()
    );
}

// --- resources --------------------------------------------------------------

/// Chunks that have diverged from the heightfield because an edit
/// has been applied to them. Used by `queue_reconnect_snapshots` to
/// hand newly-connected clients the full set of server-authoritative
/// deltas they missed.
#[derive(Resource, Default)]
struct EditedChunks {
    coords: HashSet<ChunkCoord>,
}

/// FIFO queue of chunks awaiting a broadcast delta (to every client).
/// Populated by `enqueue_dirty_chunks_for_broadcast` after every
/// edit; drained by `broadcast_voxel_deltas` at
/// `LIVE_BROADCAST_PER_TICK` per tick.
#[derive(Resource, Default)]
struct PendingDeltas {
    queue: VecDeque<ChunkCoord>,
}

/// Per-reconnecting-client queue of chunks to ship as catch-up
/// snapshots. Keyed on the `ClientOf` entity so the sender query
/// inside `drain_reconnect_snapshots` can target a single client.
#[derive(Resource, Default)]
struct PendingReconnectSnapshots {
    by_client: HashMap<Entity, VecDeque<ChunkCoord>>,
}

// --- internal messages ------------------------------------------------------

/// Passes a gate-accepted edit from the validator to the applier.
/// Carries the resolved `BrushMode` (not the wire-level
/// `ServerBrushMode`) and the applicant's client id for downstream
/// logging / attribution.
#[derive(Message, Clone, Copy, Debug)]
struct ValidatedEditStroke {
    center: Vec3,
    radius: f32,
    mode: BrushMode,
    applicant_client_id: u64,
}

// --- systems ----------------------------------------------------------------

/// Keep a cube of chunks around every active player seeded so
/// `ground_y` probes have data. Unchanged from phase 1.
fn stream_chunks_around_players(
    players: Query<&Transform, With<PlayerTag>>,
    mut store: ResMut<ChunkStore>,
) {
    if players.is_empty() {
        return;
    }
    let generator = HeightfieldGenerator::new();
    let mut newly_seeded = 0usize;
    for tf in &players {
        let center = ChunkCoord::containing(tf.translation);
        for dz in -SERVER_STREAM_RADIUS_XZ..=SERVER_STREAM_RADIUS_XZ {
            for dy in -SERVER_STREAM_RADIUS_Y..=SERVER_STREAM_RADIUS_Y {
                for dx in -SERVER_STREAM_RADIUS_XZ..=SERVER_STREAM_RADIUS_XZ {
                    let coord = ChunkCoord::new(
                        center.0.x + dx,
                        center.0.y + dy,
                        center.0.z + dz,
                    );
                    if store.contains(coord) {
                        continue;
                    }
                    let mut chunk = VoxelChunk::new_air();
                    generator.seed_chunk(coord, &mut chunk);
                    store.insert(coord, chunk);
                    newly_seeded += 1;
                }
            }
        }
    }
    if newly_seeded > 0 {
        debug!(
            "server voxel streamer: seeded {newly_seeded} chunks (store size: {})",
            store.len()
        );
    }
}

/// Read `ServerEditStroke` requests from clients and emit
/// [`ValidatedEditStroke`] for each request that passes every gate.
/// Current gates:
///   1. `radius` finite and in `(0, MAX_EDIT_RADIUS]`.
///   2. `center` finite.
///   3. Requester has a live `PlayerTag` entity.
///   4. `center` within `MAX_EDIT_RANGE_FROM_PLAYER` of that player.
///
/// Future: ability-gated (only specific abilities trigger edits),
/// per-player cooldown, zone-authority checks.
fn validate_edit_requests(
    mut links: Query<(&RemoteId, &mut MessageReceiver<ServerEditStroke>), With<ClientOf>>,
    players: Query<(&PlayerTag, &Transform)>,
    mut validated: MessageWriter<ValidatedEditStroke>,
) {
    for (remote, mut rx) in &mut links {
        let PeerId::Netcode(client_id) = remote.0 else {
            continue;
        };
        for stroke in rx.receive() {
            if !stroke.radius.is_finite() || stroke.radius <= 0.0 || stroke.radius > MAX_EDIT_RADIUS
            {
                warn!(
                    "voxel edit from {client_id}: rejected, radius {} out of (0, {MAX_EDIT_RADIUS}]",
                    stroke.radius
                );
                continue;
            }
            let center = Vec3::from_array(stroke.center);
            if !center.is_finite() {
                warn!("voxel edit from {client_id}: rejected, non-finite center");
                continue;
            }
            let Some((_, player_tf)) = players
                .iter()
                .find(|(tag, _)| tag.client_id == client_id)
            else {
                warn!("voxel edit from {client_id}: rejected, no live player entity");
                continue;
            };
            let dist = player_tf.translation.distance(center);
            if dist > MAX_EDIT_RANGE_FROM_PLAYER {
                warn!(
                    "voxel edit from {client_id}: rejected, {dist:.1}u > {MAX_EDIT_RANGE_FROM_PLAYER}u from player"
                );
                continue;
            }
            let mode = match stroke.mode {
                ServerBrushMode::Subtract => BrushMode::Subtract,
                ServerBrushMode::Union => BrushMode::Union,
            };
            validated.write(ValidatedEditStroke {
                center,
                radius: stroke.radius,
                mode,
                applicant_client_id: client_id,
            });
        }
    }
}

/// Consume [`ValidatedEditStroke`] events and apply each to the
/// authoritative `ChunkStore`. Marks touched chunks dirty (→
/// live broadcast queue) and records them in `EditedChunks` (→
/// reconnect snapshots for future clients).
fn apply_validated_edits(
    mut events: MessageReader<ValidatedEditStroke>,
    mut store: ResMut<ChunkStore>,
    mut dirty: ResMut<DirtyChunks>,
    mut edited: ResMut<EditedChunks>,
) {
    for ev in events.read() {
        // Pre-seed affected chunks so the brush carves terrain, not
        // pristine air. Mirrors the client path before phase 2.
        let generator = HeightfieldGenerator::new();
        let half = Vec3::splat(ev.radius + 1.0);
        let min = ev.center - half;
        let max = ev.center + half;
        let cmin = ChunkCoord::containing(min);
        let cmax = ChunkCoord::containing(max);
        for cz in cmin.0.z..=cmax.0.z {
            for cy in cmin.0.y..=cmax.0.y {
                for cx in cmin.0.x..=cmax.0.x {
                    let coord = ChunkCoord::new(cx, cy, cz);
                    if !store.contains(coord) {
                        let mut chunk = VoxelChunk::new_air();
                        generator.seed_chunk(coord, &mut chunk);
                        store.insert(coord, chunk);
                    }
                }
            }
        }

        let brush = SphereBrush {
            center: ev.center,
            radius: ev.radius,
            mode: ev.mode,
        };
        let touched = EditStroke::new(brush, &mut store, &mut dirty).apply();
        for coord in &touched {
            edited.coords.insert(*coord);
        }
        info!(
            "voxel edit applied from {}: center={:?} r={} mode={:?} touched={}",
            ev.applicant_client_id,
            ev.center,
            ev.radius,
            ev.mode,
            touched.len()
        );
    }
}

/// Drain the `DirtyChunks` set into the live-broadcast queue.
/// Repeated enqueues of the same chunk during a multi-edit frame
/// are fine — the broadcaster always reads the latest store state
/// at send time.
fn enqueue_dirty_chunks_for_broadcast(
    mut dirty: ResMut<DirtyChunks>,
    mut pending: ResMut<PendingDeltas>,
) {
    if dirty.is_empty() {
        return;
    }
    pending.queue.extend(dirty.drain_all());
}

/// Send up to `LIVE_BROADCAST_PER_TICK` queued deltas to every
/// connected client. Zone-scoped routing is a future refinement.
fn broadcast_voxel_deltas(
    mut pending: ResMut<PendingDeltas>,
    store: Res<ChunkStore>,
    mut senders: Query<&mut MessageSender<VoxelChunkDelta>, With<ClientOf>>,
) {
    if pending.queue.is_empty() || senders.is_empty() {
        return;
    }
    for _ in 0..LIVE_BROADCAST_PER_TICK {
        let Some(coord) = pending.queue.pop_front() else {
            break;
        };
        let Some(chunk) = store.get(coord) else {
            continue;
        };
        let delta = ChunkDelta::full_snapshot(coord.0, chunk);
        let wire = VoxelChunkDelta(delta);
        for mut sender in &mut senders {
            let _ = sender.send::<Channel1>(wire.clone());
        }
    }
}

/// When a new `ClientOf` link appears (fresh connection), seed a
/// per-client queue with every `EditedChunks` coord — everything the
/// server has diverged from the heightfield on. That client needs to
/// catch up before its world is consistent.
fn queue_reconnect_snapshots(
    new_links: Query<Entity, Added<ClientOf>>,
    edited: Res<EditedChunks>,
    mut pending: ResMut<PendingReconnectSnapshots>,
) {
    for link in &new_links {
        if edited.coords.is_empty() {
            continue;
        }
        let queue: VecDeque<ChunkCoord> = edited.coords.iter().copied().collect();
        info!(
            "reconnect snapshot: queuing {} edited chunks for client link {:?}",
            queue.len(),
            link
        );
        pending.by_client.insert(link, queue);
    }
}

/// Each tick, pop a few chunks off each pending client's queue and
/// send them as targeted `VoxelChunkDelta` messages. Cleans up empty
/// entries as queues drain so the resource stays small.
fn drain_reconnect_snapshots(
    mut pending: ResMut<PendingReconnectSnapshots>,
    store: Res<ChunkStore>,
    mut senders: Query<&mut MessageSender<VoxelChunkDelta>, With<ClientOf>>,
) {
    if pending.by_client.is_empty() {
        return;
    }
    let mut drained_clients: Vec<Entity> = Vec::new();
    for (&link, queue) in pending.by_client.iter_mut() {
        let Ok(mut sender) = senders.get_mut(link) else {
            // Client disconnected before catch-up finished.
            drained_clients.push(link);
            continue;
        };
        for _ in 0..RECONNECT_BROADCAST_PER_TICK {
            let Some(coord) = queue.pop_front() else {
                break;
            };
            let Some(chunk) = store.get(coord) else {
                continue;
            };
            let delta = ChunkDelta::full_snapshot(coord.0, chunk);
            let _ = sender.send::<Channel1>(VoxelChunkDelta(delta));
        }
        if queue.is_empty() {
            drained_clients.push(link);
        }
    }
    for link in drained_clients {
        pending.by_client.remove(&link);
    }
}
