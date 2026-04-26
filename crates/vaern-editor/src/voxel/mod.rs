//! Editor voxel runtime — the local `ChunkStore` + meshing + camera-
//! relative streaming pipeline.

pub mod biomes;
pub mod overrides;
pub mod render_opt;
pub mod store;
pub mod stream;
pub mod undo;

use bevy::prelude::*;
use std::collections::HashMap;
use vaern_voxel::chunk::ChunkCoord;
use vaern_voxel::plugin::{VoxelCorePlugin, VoxelMeshPlugin};

use crate::state::EditorAppState;
use biomes::BiomeKey;

/// Cache of per-biome `StandardMaterial` handles. Built lazily so a
/// zone with only one biome doesn't allocate the rest.
#[derive(Resource, Default)]
pub struct BiomeMaterials(pub HashMap<BiomeKey, Handle<StandardMaterial>>);

/// Each chunk's biome assignment, captured at seed time so the
/// PostUpdate material attach doesn't have to re-query the resolver.
#[derive(Resource, Default)]
pub struct ChunkBiomeMap {
    pub by_coord: HashMap<ChunkCoord, BiomeKey>,
}

/// Per-frame chunk meshing budget. Higher = faster initial fill at the
/// cost of one-frame allocation spikes.
pub const MESHING_BUDGET: usize = 64;

pub struct EditorVoxelPlugin;

impl Plugin for EditorVoxelPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            VoxelCorePlugin,
            VoxelMeshPlugin {
                meshes_per_frame: MESHING_BUDGET,
            },
        ))
        .add_plugins(undo::VoxelUndoPlugin)
        .add_plugins(render_opt::ChunkRenderOptPlugin)
        .init_resource::<BiomeMaterials>()
        .init_resource::<ChunkBiomeMap>()
        .init_resource::<overrides::BiomeOverrideMap>()
        .add_systems(Startup, overrides::load_biome_overrides_into_resource)
        .add_systems(
            Update,
            stream::stream_chunks_around_editor_camera
                .run_if(in_state(EditorAppState::Editing)),
        )
        .add_systems(
            PostUpdate,
            (stream::attach_biome_material, stream::refresh_uvs_on_remesh),
        );
    }
}

