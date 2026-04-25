//! Bevy plugin wiring + per-frame systems.
//!
//! Two plugins, used together on the client, only the core on the
//! server:
//!
//! * [`VoxelCorePlugin`] — inserts [`crate::ChunkStore`] and
//!   [`crate::chunk::DirtyChunks`] resources, plus the extractor config
//!   resource. Nothing else. The server uses only this.
//! * [`VoxelMeshPlugin`] — runs iso-surface extraction against dirty
//!   chunks each frame (budgeted) and keeps a 1:1 map between chunk
//!   coords and Bevy entities holding the resulting meshes. Pulls in
//!   rendering types; the server does not need it.
//!
//! The combined convenience [`VaernVoxelPlugin`] adds both.

use bevy::prelude::*;
use std::collections::HashMap;

use crate::chunk::{ChunkCoord, ChunkStore, DirtyChunks};
use crate::config::{PADDING, VOXEL_SIZE};
use crate::mesh::{BufferedMesh, DefaultExtractor, IsoSurfaceExtractor, MeshSink, build_bevy_mesh};

/// Inserts voxel-world resources without touching rendering.
/// Server-safe.
pub struct VoxelCorePlugin;

impl Plugin for VoxelCorePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChunkStore>()
            .init_resource::<DirtyChunks>()
            .insert_resource(ExtractorConfig::default());
    }
}

/// Rendering half of the voxel pipeline — meshes dirty chunks and
/// maintains a chunk-to-entity map so new edits update the right
/// mesh instance in place instead of leaking entities.
///
/// Field `meshes_per_frame` caps how many chunks get re-meshed in one
/// Update tick; anything over budget waits for next frame. 4 is the
/// sane default — tune per project.
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
            .add_systems(Update, rebuild_dirty_chunks);
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

/// The extractor used by [`rebuild_dirty_chunks`]. Stored as a resource
/// so a caller can swap in a customized extractor (different
/// placement / normal / splitter) at startup.
#[derive(Resource)]
pub struct ExtractorConfig {
    pub extractor: Box<dyn IsoSurfaceExtractor>,
}

impl Default for ExtractorConfig {
    fn default() -> Self {
        Self {
            extractor: Box::new(DefaultExtractor::default_config()),
        }
    }
}

/// Per-frame remesh budget (how many dirty chunks to drain).
#[derive(Resource, Copy, Clone)]
pub struct MeshingBudget(pub usize);

/// Chunk → entity map. The rebuild system looks up by coord so edits
/// update an existing entity's `Mesh3d` handle rather than spawning a
/// duplicate.
#[derive(Resource, Default)]
pub struct ChunkEntityMap {
    pub by_coord: HashMap<ChunkCoord, Entity>,
}

/// Per-entity marker — which chunk does this entity render?
#[derive(Component, Copy, Clone)]
pub struct ChunkRenderTag {
    pub coord: ChunkCoord,
}

// --- Systems -----------------------------------------------------------

/// Pull up to [`MeshingBudget`] chunks off the dirty queue, re-extract
/// their meshes, and update / spawn the corresponding Bevy entities.
///
/// Intentionally synchronous for v1 — extraction is CPU-bound and well
/// under a millisecond per chunk at 32³. Async / rayon dispatch can
/// be bolted on later without changing the plugin surface.
fn rebuild_dirty_chunks(
    mut dirty: ResMut<DirtyChunks>,
    store: Res<ChunkStore>,
    budget: Res<MeshingBudget>,
    extractor: Res<ExtractorConfig>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut entity_map: ResMut<ChunkEntityMap>,
    mut commands: Commands,
    mut mesh_q: Query<(&ChunkRenderTag, &mut Mesh3d)>,
) {
    if dirty.is_empty() {
        return;
    }

    let coords = dirty.drain_budget(budget.0);
    let mut buffer = BufferedMesh::new();

    for coord in coords {
        let Some(chunk) = store.get(coord) else {
            // Chunk removed between dirty-mark and meshing; drop the
            // entity if we had one.
            if let Some(entity) = entity_map.by_coord.remove(&coord) {
                commands.entity(entity).despawn();
            }
            continue;
        };

        buffer.reset();
        extractor.extractor.extract(chunk, &mut buffer);

        if buffer.is_empty() {
            // Chunk has no surface — despawn a stale render entity.
            if let Some(entity) = entity_map.by_coord.remove(&coord) {
                commands.entity(entity).despawn();
            }
            continue;
        }

        // Build a new Bevy Mesh asset and either update the existing
        // entity or spawn a fresh one positioned at the chunk origin.
        let offset = -Vec3::splat(PADDING as f32 * VOXEL_SIZE);
        let mesh = build_bevy_mesh(&buffer, VOXEL_SIZE, offset);
        let mesh_handle = meshes.add(mesh);

        if let Some(&entity) = entity_map.by_coord.get(&coord) {
            if let Ok((_, mut m)) = mesh_q.get_mut(entity) {
                m.0 = mesh_handle;
                continue;
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
    }
}
