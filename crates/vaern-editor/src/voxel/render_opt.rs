//! Editor-side render optimizations for voxel chunks.
//!
//! 1. **`tag_new_chunks_no_shadow`** — every new chunk render entity
//!    gains [`bevy::light::NotShadowCaster`]. Voxel ground self-
//!    shadowing isn't aesthetically meaningful and the cascade pass
//!    dominates frame time.
//!
//! 2. **`evict_chunks_outside_draw_distance`** — per-frame, **despawn**
//!    chunk render entities whose chunk-XZ chebyshev distance from the
//!    camera exceeds `EnvSettings.draw_distance_chunks`. Keeps
//!    `ChunkStore` data + `ChunkBiomeMap` entries so widening the
//!    slider re-shows them via re-mesh (instant; SDF data is in RAM).
//!    Cleans `ChunkEntityMap.by_coord` so the streamer doesn't think
//!    a stale entity still exists.
//!
//!    Why despawn beats visibility-toggle: hidden entities still get
//!    iterated by Bevy's transform / visibility-propagation systems
//!    each frame. Despawned entities skip every iteration. Reversible
//!    because chunk SDF data lives in `ChunkStore`, not the entity.
//!
//! 3. **`log_chunk_setup_once`** — one-shot diagnostic 5s after
//!    launch that confirms how many chunks have a `Visibility`
//!    component. Useful for proving the ECS hooks actually fire.

use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use std::collections::HashSet;
use vaern_voxel::chunk::{ChunkCoord, DirtyChunks};
use vaern_voxel::plugin::{ChunkEntityMap, ChunkRenderTag};

use crate::camera::FreeFlyCamera;
use crate::environment::EnvSettings;
use crate::state::EditorAppState;
use crate::ui::console::ConsoleLog;

pub struct ChunkRenderOptPlugin;

impl Plugin for ChunkRenderOptPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                tag_new_chunks_no_shadow,
                evict_chunks_outside_draw_distance,
                log_chunk_setup_once,
            )
                .run_if(in_state(EditorAppState::Editing)),
        );
    }
}

/// On every newly-spawned chunk render entity, attach
/// [`NotShadowCaster`] so the directional sun's cascade pass skips it.
///
/// Uses `try_insert` (not `insert`) because the eviction system can
/// despawn a chunk in the same frame it's spawned — when the user
/// shrinks the slider while warm-up is meshing distant chunks. Plain
/// `insert` would panic on the despawned entity; `try_insert`
/// silently no-ops.
pub fn tag_new_chunks_no_shadow(
    mut commands: Commands,
    new_chunks: Query<Entity, (Added<ChunkRenderTag>, Without<NotShadowCaster>)>,
) {
    for entity in &new_chunks {
        commands.entity(entity).try_insert(NotShadowCaster);
    }
}

/// Tracks chunks the eviction system *itself* despawned so we know to
/// re-spawn them when they come back into range. Critical that this
/// only contains coords WE despawned — coords the mesher despawned
/// because their mesh was empty (fully-solid or fully-air chunks)
/// must NOT end up in here, or my re-mesh pass would mark them dirty
/// every frame, the mesher would re-process them, find empty buffers,
/// despawn nothing, leave them out of `entity_map`, and the next
/// frame's pass would mark them dirty again — busy-loop saturating
/// the per-frame meshing budget so freshly-streamed surface chunks
/// (or brush-edited ones) wait in queue indefinitely.
#[derive(Default)]
pub struct EvictionState {
    /// Chunks I despawned that should be re-meshed when their XZ
    /// returns to within draw distance.
    pending_respawn: HashSet<ChunkCoord>,
}

/// Despawn chunk render entities outside the configured draw distance.
/// Re-spawn (via mesher) chunks that are back in range — but only the
/// ones THIS system despawned. Chunks the mesher itself dropped
/// because they had no surface (always-empty: fully-solid or fully-
/// air chunks) are intentionally not re-marked.
#[allow(clippy::too_many_arguments)]
pub fn evict_chunks_outside_draw_distance(
    camera_q: Query<&Transform, With<FreeFlyCamera>>,
    env: Res<EnvSettings>,
    chunk_q: Query<(Entity, &ChunkRenderTag)>,
    mut entity_map: ResMut<ChunkEntityMap>,
    mut dirty: ResMut<DirtyChunks>,
    mut commands: Commands,
    mut log: ResMut<ConsoleLog>,
    mut last_summary: Local<f32>,
    mut state: Local<EvictionState>,
    time: Res<Time>,
) {
    let Ok(cam) = camera_q.single() else {
        return;
    };
    let cam_chunk = ChunkCoord::containing(cam.translation);
    let r = env.draw_distance_chunks.max(1);
    let cx = cam_chunk.0.x;
    let cz = cam_chunk.0.z;

    // Despawn pass: track despawned coords in `pending_respawn` so we
    // know to re-mesh them when they come back into range.
    let mut despawned = 0usize;
    for (entity, tag) in chunk_q.iter() {
        let dx = (tag.coord.0.x - cx).abs();
        let dz = (tag.coord.0.z - cz).abs();
        if dx > r || dz > r {
            commands.entity(entity).despawn();
            entity_map.by_coord.remove(&tag.coord);
            state.pending_respawn.insert(tag.coord);
            despawned += 1;
        }
    }

    // Re-spawn pass: walk only the `pending_respawn` set (NOT all of
    // `store.coords()`). For coords back in range without a live
    // entity, mark dirty. For coords whose entity already exists
    // (mesher caught up), drop from the set.
    let mut remarked = 0usize;
    let mut to_remove = Vec::new();
    for &coord in state.pending_respawn.iter() {
        let dx = (coord.0.x - cx).abs();
        let dz = (coord.0.z - cz).abs();
        if dx > r || dz > r {
            // Still out of range, keep waiting.
            continue;
        }
        if entity_map.by_coord.contains_key(&coord) {
            // Mesher re-spawned it — done tracking.
            to_remove.push(coord);
        } else {
            dirty.mark(coord);
            remarked += 1;
        }
    }
    for coord in to_remove {
        state.pending_respawn.remove(&coord);
    }

    // Only log on real despawn activity. Warm-up streaming is silent
    // because we no longer touch chunks the streamer freshly seeds.
    if despawned > 0 && time.elapsed_secs() - *last_summary > 1.0 {
        *last_summary = time.elapsed_secs();
        log.push(format!(
            "chunk evict: -{despawned} despawned, +{remarked} remeshing, {} pending",
            state.pending_respawn.len(),
        ));
    }
}

/// One-shot diagnostic ~5 seconds after launch: count chunks that
/// have / lack a `Visibility` component. Confirms whether Bevy's
/// `register_required_components::<Mesh3d, Visibility>` actually
/// applied to vaern-voxel's chunk spawns.
pub fn log_chunk_setup_once(
    chunks_with_vis: Query<Entity, (With<ChunkRenderTag>, With<Visibility>)>,
    chunks_without_vis: Query<Entity, (With<ChunkRenderTag>, Without<Visibility>)>,
    mut log: ResMut<ConsoleLog>,
    mut done: Local<bool>,
    time: Res<Time>,
) {
    if *done || time.elapsed_secs() < 5.0 {
        return;
    }
    *done = true;
    let with_v = chunks_with_vis.iter().count();
    let without_v = chunks_without_vis.iter().count();
    log.push(format!(
        "chunk diagnostic: {with_v} chunks have Visibility, {without_v} lack it"
    ));
}
