//! Bevy plugin wiring + per-frame systems.
//!
//! Two plugins, used together on the client, only the core on the
//! server:
//!
//! * [`VoxelCorePlugin`] — inserts [`crate::ChunkStore`] and
//!   [`crate::chunk::DirtyChunks`] resources, plus the extractor config
//!   resource. Nothing else. The server uses only this.
//! * [`VoxelMeshPlugin`] — runs iso-surface extraction against dirty
//!   chunks each frame (budgeted, async) and keeps a 1:1 map between
//!   chunk coords and Bevy entities holding the resulting meshes.
//!   Pulls in rendering types; the server does not need it.
//!
//! The combined convenience [`VaernVoxelPlugin`] adds both.
//!
//! ## Async meshing
//!
//! Extraction runs on `AsyncComputeTaskPool` rather than the main
//! thread. Each frame `dispatch_mesh_tasks` drains up to
//! `MeshingBudget - in_flight` chunks from the dirty queue and spawns
//! a task per chunk. `collect_completed_meshes` polls the task list,
//! consumes finished `BufferedMesh`es, and spawns/updates entities.
//! In-flight tasks survive across frames (chunks won't be re-dispatched
//! while their previous mesh job is still running).

use bevy::prelude::*;
use bevy::tasks::{block_on, futures_lite::future, AsyncComputeTaskPool, Task};
use std::collections::HashMap;
use std::sync::Arc;

use crate::chunk::{ChunkCoord, ChunkStore, DirtyChunks};
use crate::config::{PADDING, VOXEL_SIZE};
use crate::mesh::{BufferedMesh, DefaultExtractor, IsoSurfaceExtractor, build_bevy_mesh};
use crate::perf::{SystemFrameTimes, SystemTimer};

/// Inserts voxel-world resources without touching rendering.
/// Server-safe.
pub struct VoxelCorePlugin;

impl Plugin for VoxelCorePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChunkStore>()
            .init_resource::<DirtyChunks>()
            .init_resource::<SystemFrameTimes>()
            .insert_resource(ExtractorConfig::default());
    }
}

/// Rendering half of the voxel pipeline — meshes dirty chunks and
/// maintains a chunk-to-entity map so new edits update the right
/// mesh instance in place instead of leaking entities.
///
/// Field `meshes_per_frame` caps total in-flight async mesh tasks.
/// Higher = warmer task pool but more peak RAM (each in-flight task
/// holds a chunk clone).
#[derive(Clone, Copy, Debug)]
pub struct VoxelMeshPlugin {
    pub meshes_per_frame: usize,
}

impl Default for VoxelMeshPlugin {
    fn default() -> Self {
        Self { meshes_per_frame: 4 }
    }
}

impl Plugin for VoxelMeshPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(MeshingBudget(self.meshes_per_frame))
            .init_resource::<ChunkEntityMap>()
            .init_resource::<PendingMeshes>()
            .add_systems(Update, (dispatch_mesh_tasks, collect_completed_meshes).chain());
    }
}

/// Convenience: core + mesh plugins in one call. Client uses this.
pub struct VaernVoxelPlugin;

impl Plugin for VaernVoxelPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((VoxelCorePlugin, VoxelMeshPlugin::default()));
    }
}

// --- Resources ---------------------------------------------------------

/// The extractor used by the meshing systems. Stored as an `Arc` so
/// `dispatch_mesh_tasks` can clone a cheap reference into each spawned
/// async task without forcing the underlying extractor to implement
/// `Clone`. Swap by re-inserting the resource at startup.
#[derive(Resource, Clone)]
pub struct ExtractorConfig {
    pub extractor: Arc<dyn IsoSurfaceExtractor>,
}

impl Default for ExtractorConfig {
    fn default() -> Self {
        Self {
            extractor: Arc::new(DefaultExtractor::default_config()),
        }
    }
}

/// Cap on in-flight async mesh tasks (NOT a strict per-frame mesh
/// completion cap). Tasks that finish in one frame free their slot
/// immediately so steady-state throughput can exceed this number.
#[derive(Resource, Copy, Clone)]
pub struct MeshingBudget(pub usize);

/// Chunk → entity map. The rebuild system looks up by coord so edits
/// update an existing entity's `Mesh3d` handle rather than spawning a
/// duplicate.
#[derive(Resource, Default)]
pub struct ChunkEntityMap {
    pub by_coord: HashMap<ChunkCoord, Entity>,
}

/// Async mesh tasks currently in flight. Survives across frames; the
/// next dispatch tick won't re-queue a coord whose previous mesh job
/// is still pending.
#[derive(Resource, Default)]
pub struct PendingMeshes {
    pub tasks: Vec<(ChunkCoord, Task<BufferedMesh>)>,
}

/// Per-entity marker — which chunk does this entity render?
#[derive(Component, Copy, Clone)]
pub struct ChunkRenderTag {
    pub coord: ChunkCoord,
}

// --- Systems -----------------------------------------------------------

/// Drain up to `budget - in_flight` coords off the dirty queue and
/// spawn one async extraction task per coord on the
/// `AsyncComputeTaskPool`. Each task gets its own clone of the chunk's
/// `VoxelChunk` (cheap for `Uniform`, one alloc for `Dense`) and an
/// `Arc` of the extractor.
///
/// Uses a `Local<HashSet<ChunkCoord>>` to track in-flight coords so a
/// chunk that's marked dirty again while its mesh job is still running
/// doesn't get a redundant second task — the dirty mark stays on the
/// queue and re-dispatches once the prior job finishes.
pub fn dispatch_mesh_tasks(
    mut dirty: ResMut<DirtyChunks>,
    store: Res<ChunkStore>,
    budget: Res<MeshingBudget>,
    extractor: Res<ExtractorConfig>,
    mut pending: ResMut<PendingMeshes>,
    mut commands: Commands,
    mut entity_map: ResMut<ChunkEntityMap>,
    mut perf: ResMut<SystemFrameTimes>,
) {
    let _timer = SystemTimer::new(&mut perf, "voxel::dispatch_mesh_tasks");

    if dirty.is_empty() {
        return;
    }
    let in_flight = pending.tasks.len();
    if in_flight >= budget.0 {
        return;
    }
    let want = budget.0 - in_flight;

    let pool = AsyncComputeTaskPool::get();
    let in_flight_set: std::collections::HashSet<ChunkCoord> =
        pending.tasks.iter().map(|(c, _)| *c).collect();

    let coords = dirty.drain_budget(want);
    for coord in coords {
        if in_flight_set.contains(&coord) {
            // Already meshing; re-mark dirty so the next tick picks it
            // up after the in-flight job finishes.
            dirty.mark(coord);
            continue;
        }
        let Some(chunk) = store.get(coord) else {
            // Chunk removed between dirty-mark and dispatch; drop the
            // entity if we had one.
            if let Some(entity) = entity_map.by_coord.remove(&coord) {
                commands.entity(entity).despawn();
            }
            continue;
        };
        let chunk_owned = chunk.clone();
        let extractor = extractor.extractor.clone();
        let task = pool.spawn(async move {
            let mut buf = BufferedMesh::new();
            extractor.extract(&chunk_owned, &mut buf);
            buf
        });
        pending.tasks.push((coord, task));
    }
}

/// Poll every in-flight task; for finished ones, build the Bevy mesh
/// asset and spawn / update the chunk render entity. Empty meshes
/// (chunk had no surface) drop any stale entity for the coord.
pub fn collect_completed_meshes(
    mut pending: ResMut<PendingMeshes>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut entity_map: ResMut<ChunkEntityMap>,
    mut commands: Commands,
    mut mesh_q: Query<(&ChunkRenderTag, &mut Mesh3d)>,
    mut perf: ResMut<SystemFrameTimes>,
) {
    let _timer = SystemTimer::new(&mut perf, "voxel::collect_completed_meshes");
    pending.tasks.retain_mut(|(coord, task)| {
        let Some(buffer) = block_on(future::poll_once(task)) else {
            return true; // still in flight
        };
        let coord = *coord;

        if buffer.is_empty() {
            if let Some(entity) = entity_map.by_coord.remove(&coord) {
                commands.entity(entity).despawn();
            }
            return false;
        }

        let offset = -Vec3::splat(PADDING as f32 * VOXEL_SIZE);
        let mesh = build_bevy_mesh(&buffer, VOXEL_SIZE, offset);
        let mesh_handle = meshes.add(mesh);

        if let Some(&entity) = entity_map.by_coord.get(&coord) {
            if let Ok((_, mut m)) = mesh_q.get_mut(entity) {
                m.0 = mesh_handle;
                return false;
            }
        }

        let origin = coord.world_origin();
        let entity = commands
            .spawn((
                Mesh3d(mesh_handle),
                Transform::from_translation(origin),
                ChunkRenderTag { coord },
                Name::new(format!("VoxelChunk({},{},{})", coord.0.x, coord.0.y, coord.0.z)),
            ))
            .id();
        entity_map.by_coord.insert(coord, entity);
        false
    });
}
