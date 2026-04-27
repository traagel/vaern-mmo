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

use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::ViewVisibility;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use std::collections::HashSet;
use vaern_voxel::chunk::{ChunkCoord, DirtyChunks};
use vaern_voxel::config::CHUNK_WORLD_SIZE;
use vaern_voxel::perf::{SystemFrameTimes, SystemTimer};
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
                ensure_chunk_aabb,
                evict_chunks_outside_draw_distance
                    .run_if(|t: Res<crate::voxel::PerfToggles>| !t.skip_eviction),
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
    mut perf: ResMut<SystemFrameTimes>,
) {
    let _timer = SystemTimer::new(&mut perf, "editor::evict_chunks");
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

/// Insert a chunk-sized [`Aabb`] on every newly-spawned chunk render
/// entity that lacks one. Bevy's [`bevy::render::view::calculate_bounds`]
/// will compute an AABB from the mesh asset *eventually*, but only after
/// the mesh asset is loaded into the renderer — for procedurally-built
/// meshes inserted via `Assets<Mesh>::add` this can lag a frame, and
/// without an AABB the chunk is treated as unbounded and never frustum-
/// culled. Hand-attaching a known chunk-sized box gives the renderer
/// something to cull against immediately.
///
/// The AABB is in entity-local space. Chunk meshes are translated to
/// `coord.world_origin()` and the meshing pass writes vertices in the
/// `[-PADDING * VOXEL_SIZE, (CHUNK_DIM + PADDING) * VOXEL_SIZE]` range
/// relative to that origin (see `build_bevy_mesh`'s `offset`). Round
/// generously: `[-1.0, CHUNK_WORLD_SIZE + 1.0]` covers any vertex.
pub fn ensure_chunk_aabb(
    mut commands: Commands,
    new_chunks: Query<Entity, (Added<ChunkRenderTag>, Without<Aabb>)>,
) {
    let half = (CHUNK_WORLD_SIZE + 2.0) * 0.5;
    let center = Vec3::splat(CHUNK_WORLD_SIZE * 0.5 - 1.0);
    let aabb = Aabb {
        center: center.into(),
        half_extents: Vec3::splat(half).into(),
    };
    for entity in &new_chunks {
        commands.entity(entity).try_insert(aabb);
    }
}

/// One-shot diagnostic ~5 seconds after launch: count chunks that
/// have / lack a `Visibility` component, plus how many are currently
/// frustum-culled (`ViewVisibility::get() == false` despite being in
/// draw range). The frustum-cull count proves the renderer is doing
/// its job: at draw_distance=64 the streamer fills ~16k XZ chunks but
/// a typical 60° camera FOV sees less than 1/4 of them — anything else
/// would mean our AABBs aren't wired up correctly.
pub fn log_chunk_setup_once(
    chunks_with_vis: Query<Entity, (With<ChunkRenderTag>, With<Visibility>)>,
    chunks_without_vis: Query<Entity, (With<ChunkRenderTag>, Without<Visibility>)>,
    chunks_with_aabb: Query<Entity, (With<ChunkRenderTag>, With<Aabb>)>,
    chunks_view_vis: Query<&ViewVisibility, With<ChunkRenderTag>>,
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
    let with_aabb = chunks_with_aabb.iter().count();
    let total_view = chunks_view_vis.iter().count();
    let visible = chunks_view_vis.iter().filter(|v| v.get()).count();
    let culled = total_view.saturating_sub(visible);
    log.push(format!(
        "chunk diagnostic: {with_v} have Visibility, {without_v} lack it, {with_aabb} have Aabb, {visible} visible after frustum cull (+{culled} culled)"
    ));
}
