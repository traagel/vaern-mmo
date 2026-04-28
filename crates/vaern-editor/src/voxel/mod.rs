//! Editor voxel runtime — the local `ChunkStore` + meshing + camera-
//! relative streaming pipeline.

pub mod biome_blend;
pub mod biomes;
pub mod overrides;
pub mod procedural;
pub mod render_opt;
pub mod store;
pub mod stream;
pub mod undo;

use bevy::pbr::MaterialPlugin;
use bevy::prelude::*;
use std::collections::HashMap;
use vaern_voxel::chunk::ChunkCoord;
use vaern_voxel::plugin::{VoxelCorePlugin, VoxelMeshPlugin};

use crate::state::EditorAppState;
use biome_blend::{BiomeBlendAssets, BiomeBlendEnabled, BiomeBlendMaterial, BlendDebugMode};
use biomes::BiomeKey;

/// Phase 2 isolation toggles — each gates one of the suspect systems
/// behind a runtime flag so the user can A/B without code changes
/// while looking at the per-system timing table.
#[derive(Resource)]
pub struct PerfToggles {
    /// When true, hide all chunk render entities (sets Visibility::Hidden).
    /// Tests render-side cost.
    pub hide_chunks: bool,
    /// When true, `evict_chunks_outside_draw_distance` becomes a no-op.
    pub skip_eviction: bool,
    /// When true, `stream_chunks_around_editor_camera` becomes a no-op.
    pub skip_streamer: bool,
}

impl Default for PerfToggles {
    fn default() -> Self {
        Self {
            hide_chunks: false,
            skip_eviction: false,
            skip_streamer: false,
        }
    }
}

/// Each chunk's biome assignment, captured at seed time so the
/// PostUpdate material attach doesn't have to re-query the resolver.
/// Still useful for diagnostics + paint logic even after the move to
/// per-vertex blending — the dominant biome at a chunk is what shows
/// up in the inspector palette and what gets persisted to
/// `biome_overrides.bin`.
#[derive(Resource, Default)]
pub struct ChunkBiomeMap {
    pub by_coord: HashMap<ChunkCoord, BiomeKey>,
}

/// Per-frame chunk meshing budget — number of in-flight async mesh
/// extraction tasks the dispatcher will spawn per Update tick.
///
/// Lowered from 64 to 16 so the downstream `process_pending_blend_attaches`
/// (which runs MikkTSpace tangent generation + per-vertex biome blend
/// weight compute on every newly-meshed chunk) doesn't get flooded.
/// Combined with `MAX_ATTACHES_PER_FRAME=16` this keeps per-frame
/// PostUpdate cost bounded; initial fill is ~3× longer in wall time
/// but the editor stays interactive throughout (~30 FPS) instead of
/// stalling at 1-2 FPS.
pub const MESHING_BUDGET: usize = 16;

pub struct EditorVoxelPlugin;

impl Plugin for EditorVoxelPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            VoxelCorePlugin,
            VoxelMeshPlugin {
                meshes_per_frame: MESHING_BUDGET,
            },
        ))
        .add_plugins(MaterialPlugin::<BiomeBlendMaterial>::default())
        .add_plugins(undo::VoxelUndoPlugin)
        .add_plugins(render_opt::ChunkRenderOptPlugin)
        .init_resource::<ChunkBiomeMap>()
        .init_resource::<overrides::BiomeOverrideMap>()
        .init_resource::<BiomeBlendEnabled>()
        .init_resource::<BlendDebugMode>()
        .init_resource::<PerfToggles>()
        .init_resource::<stream::PendingSeeds>()
        .add_systems(Startup, biome_blend::init_biome_blend_assets)
        // Procedural heightfield + biome overrides both need
        // `EditorContext.active_zone` populated, which happens during
        // `seed_context_from_boot` at Startup. Running on
        // `OnEnter(Editing)` (entered the same frame by
        // `advance_to_editing`) guarantees the context is ready and
        // the chunk streamer has not yet sampled the generator.
        .add_systems(
            OnEnter(EditorAppState::Editing),
            (
                procedural::load_procedural_heightfield,
                overrides::load_biome_overrides_into_resource,
            ),
        )
        .add_systems(
            Update,
            (
                stream::stream_chunks_around_editor_camera
                    .run_if(|t: Res<PerfToggles>| !t.skip_streamer),
                // Collector runs AFTER the dispatcher so a freshly-
                // dispatched task can't be polled in the same frame
                // (it won't be ready anyway, but ordering is cleaner).
                stream::collect_completed_seeds
                    .after(stream::stream_chunks_around_editor_camera),
                stream::apply_biome_blend_toggle,
                // Must run AFTER `collect_completed_meshes` so the
                // `Changed<Mesh3d>` filter sees the mesh swap. Without
                // this ordering, ~half of re-meshes land in the next
                // tick and Bevy briefly renders chunks with the new
                // mesh + old material (no UVs) → "Mesh is missing
                // requested attribute: Vertex_Uv" log spam.
                stream::mark_chunks_needing_blend_refresh
                    .after(vaern_voxel::plugin::collect_completed_meshes),
                apply_hide_chunks_toggle,
                push_blend_debug_mode_to_material,
            )
                .run_if(in_state(EditorAppState::Editing)),
        )
        .add_systems(PostUpdate, stream::process_pending_blend_attaches);
    }
}

/// On `BlendDebugMode` change, copy the new mode value into the live
/// `BiomeBlendMaterial`'s uniform so the shader's debug branch picks
/// it up. Cheap — runs only when the inspector dropdown changes.
fn push_blend_debug_mode_to_material(
    mode: Res<BlendDebugMode>,
    blend_assets: Option<Res<BiomeBlendAssets>>,
    mut materials: ResMut<Assets<BiomeBlendMaterial>>,
) {
    if !mode.is_changed() {
        return;
    }
    let Some(blend_assets) = blend_assets else {
        return;
    };
    let Some(mat) = materials.get_mut(&blend_assets.material) else {
        return;
    };
    mat.extension.params.debug_mode = *mode as u32;
}

/// Phase 2 isolation toggle: when `PerfToggles.hide_chunks` flips,
/// flip every chunk's `Visibility` to `Hidden` (or `Visible`) so the
/// user can measure render-side cost. Only runs on toggle change.
fn apply_hide_chunks_toggle(
    toggles: Res<PerfToggles>,
    mut chunks: Query<&mut Visibility, With<vaern_voxel::plugin::ChunkRenderTag>>,
) {
    if !toggles.is_changed() {
        return;
    }
    let target = if toggles.hide_chunks {
        Visibility::Hidden
    } else {
        Visibility::Inherited
    };
    for mut vis in &mut chunks {
        if *vis != target {
            *vis = target;
        }
    }
}

