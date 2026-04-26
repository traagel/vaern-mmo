//! Editor voxel runtime — the local `ChunkStore` + meshing + camera-
//! relative streaming pipeline.
//!
//! Mirrors the client's `voxel_demo.rs` flow but client-local: no
//! network deltas, no replication. Edits applied via this module touch
//! local resources only and are persisted (in V2) by the
//! `persistence` module writing back to YAML.
//!
//! # Submodules
//!
//! | module    | role                                            |
//! |-----------|-------------------------------------------------|
//! | [`store`] | resource init + biased heightfield generator    |
//! | [`stream`]| stream chunks around the editor camera          |
//! | [`undo`]  | ring buffer of applied edit strokes (stubbed)   |

pub mod biomes;
pub mod store;
pub mod stream;
pub mod undo;

use bevy::prelude::*;
use std::collections::HashMap;
use vaern_voxel::chunk::ChunkCoord;
use vaern_voxel::plugin::{VoxelCorePlugin, VoxelMeshPlugin};

use crate::state::EditorAppState;
use biomes::{BiomeKey, BiomeResolver};

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

/// Plugin: registers `VoxelCorePlugin` + `VoxelMeshPlugin` from
/// `vaern-voxel`, plus the editor-specific streaming + UV systems.
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
        .init_resource::<BiomeMaterials>()
        .init_resource::<ChunkBiomeMap>()
        .init_resource::<BiomeResolver>()
        .add_systems(Startup, init_biome_resolver)
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

/// Build the BiomeResolver from disk on Startup. Mirrors how
/// `vaern-client/src/voxel_demo.rs::init_biome_resolver` works — load
/// the world YAML once, populate the hub-anchor table.
fn init_biome_resolver(mut commands: Commands) {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let world_root = manifest.join("../../src/generated/world");
    commands.insert_resource(BiomeResolver::load(&world_root));
}
