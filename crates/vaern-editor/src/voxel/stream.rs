//! Chunk streamer + post-mesh material attach.
//!
//! Each frame, ensure chunks within the camera's `draw_distance_chunks`
//! radius are seeded into the `ChunkStore`. Post-mesh, attach the
//! shared `BiomeBlendMaterial` (one handle for every chunk) so all
//! chunks render through the same texture-array-backed PBR pipeline.
//! Per-vertex biome IDs + weights drive a 4-biome splat in the
//! fragment shader — biome paint never swaps materials, just marks
//! affected chunks dirty so their per-vertex weights re-compute.

use std::collections::HashSet;

use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use bevy::tasks::{block_on, futures_lite::future, AsyncComputeTaskPool, Task};
use vaern_voxel::chunk::{ChunkCoord, ChunkStore, DirtyChunks};
use vaern_voxel::generator::WorldGenerator;
use vaern_voxel::perf::{SystemFrameTimes, SystemTimer};
use vaern_voxel::plugin::ChunkRenderTag;
use vaern_voxel::VoxelChunk;

use super::biome_blend::{
    insert_blend_attributes, BiomeBlendAssets, BiomeBlendEnabled, BiomeBlendMaterial,
};
use super::biomes::BiomeKey;
use super::overrides::BiomeOverrideMap;
use super::store::EditorHeightfield;
use super::ChunkBiomeMap;
use crate::camera::FreeFlyCamera;
use crate::environment::EnvSettings;

/// Default biome for chunks not covered by a paint override. Voronoi
/// hub-based resolution was removed — every unpainted chunk uses this.
///
/// Was `Marsh` (Ground059, brown-dirt). Switched to `Grass` so the
/// pre-cartography-import editor view reads as generic green ground
/// rather than a global swamp. Marsh remains a legitimate paintable
/// destination biome for actual fens.
pub const DEFAULT_BIOME: BiomeKey = BiomeKey::Grass;

/// Default horizontal streaming radius (chunks). 16 chunks = ~512m
/// radius, comfortable scenery feel without a long cold-start. The
/// slider reaches 64 for users who want the full 2km draw distance —
/// the sparse `VoxelChunk` storage + async meshing + frustum culling
/// make 64 viable but warm-up takes ~15s and RAM climbs accordingly.
pub const DEFAULT_STREAM_RADIUS_XZ: i32 = 16;
/// Vertical radius — terrain amplitude is small, so 3 layers cover
/// surface + a slice of underground.
pub const STREAM_RADIUS_Y: i32 = 1;

/// Max in-flight async SDF-gen tasks. Each task runs
/// `generator.seed_chunk` (39,304 sample evaluations + try_compact) on
/// `AsyncComputeTaskPool`, in parallel with the main thread + each
/// other across worker threads. 32 saturates a typical 8-16 core box
/// without runaway queue growth.
///
/// Was `MAX_SEEDS_PER_FRAME = 256` (synchronous main-thread); with the
/// procedural heightfield each seed costs ~5 ms instead of the old
/// flat-marsh ~80 µs, so synchronous seeding pegged the main thread
/// at >1 s during fresh-region fill. Going async eliminates the stall
/// — the dispatcher's per-frame cost is now just task-queueing
/// overhead.
pub const SEED_TASK_BUDGET: usize = 32;

/// Per-frame cap on how many finished seed tasks the collector
/// processes. Each collection inserts the chunk into the store, runs
/// `sync_chunk_halos_for_one` against neighbors (~50 µs), tags the
/// biome map, and marks the chunk dirty for meshing. 8 keeps the
/// main-thread post-process under 1 ms even at peak.
pub const MAX_SEEDS_COLLECTED_PER_FRAME: usize = 8;

/// Per-frame cap on `process_pending_blend_attaches`. Even with the
/// idempotent fast-path inside `ensure_chunk_mesh_attributes`, every
/// queued chunk still pays Commands overhead per frame:
/// `try_insert(MeshMaterial3d(...))` + `try_remove::<NeedsBlendAttach>()`.
/// At ~1173 chunks queued (Bug 2 — marker re-add loop, suspected to be
/// `Changed<Mesh3d>` firing from somewhere we haven't tracked down yet)
/// that's enough Commands churn to drop FPS from 165 → 20.
///
/// The cap creates a render race when edit bursts dirty more than 16
/// chunks at once: unprocessed chunks would render with the new mesh
/// asset (no UV) + the still-attached `BiomeBlendMaterial` → wgpu error
/// "Mesh is missing requested attribute: Vertex_Uv". Fix: in
/// `mark_chunks_needing_blend_refresh`, swap the material to the
/// plain-PBR fallback in the same tick `Changed<Mesh3d>` fires.
/// `process_pending_blend_attaches` re-inserts the blend material once
/// it catches up at 16/frame.
pub const MAX_ATTACHES_PER_FRAME: usize = 16;

/// In-flight async seed tasks. Each `(coord, task)` represents an
/// SDF-gen job running on the `AsyncComputeTaskPool`. Drained by
/// [`collect_completed_seeds`] on the main thread.
///
/// `in_flight_set` mirrors the dispatcher's set of currently-running
/// coords so [`dispatch_seed_tasks`] can dedupe in O(1) instead of
/// scanning the task list each candidate.
#[derive(Resource, Default)]
pub struct PendingSeeds {
    pub tasks: Vec<(ChunkCoord, Task<VoxelChunk>)>,
    pub in_flight_set: HashSet<ChunkCoord>,
}

/// Cache of the streamer's last steady-state arguments so we can skip
/// the (2*r+1)² × (2*Y+1) coord walk when nothing changed.
///
/// Without this cache, scrubbing a slider to draw_distance=64 makes the
/// streamer iterate ~150k coords every Update frame doing
/// `store.contains` checks even when no new chunks need seeding —
/// ~1.5 ms / frame burned on a no-op walk.
///
/// `fully_seeded` separately tracks whether the last walk completed
/// without hitting `MAX_SEEDS_PER_FRAME`. While the streamer is still
/// rate-limited mid-fill, the cache stays invalid so the next frame
/// continues seeding — but each frame's walk is bounded.
#[derive(Default)]
pub struct StreamCache {
    last_cam_chunk_xz: Option<(i32, i32)>,
    last_surface_chunk_y: Option<i32>,
    last_r_xz: i32,
    fully_seeded: bool,
}

/// Each frame: seed any not-yet-loaded chunks around the camera's XZ.
///
/// Skips the full coord walk when (camera-chunk-XZ, surface-chunk-Y,
/// draw distance) all match the previous tick — the bbox is identical
/// so no new chunks could need seeding. The cache is invalidated on
/// any of these changing.
pub fn stream_chunks_around_editor_camera(
    camera_q: Query<&Transform, With<FreeFlyCamera>>,
    store: Res<ChunkStore>,
    mut pending: ResMut<PendingSeeds>,
    env: Res<EnvSettings>,
    mut cache: Local<StreamCache>,
    mut first_frame_logged: Local<bool>,
    mut perf: ResMut<SystemFrameTimes>,
    mut log: ResMut<crate::ui::console::ConsoleLog>,
) {
    let _timer = SystemTimer::new(&mut perf, "editor::stream_chunks");
    let Ok(cam) = camera_q.single() else {
        return;
    };

    // Flat ground: every chunk's surface lands at world Y =
    // GROUND_BIAS_Y, which `ChunkCoord::containing` resolves to chunk
    // y = 0. Anchor vertical streaming there directly — the previous
    // `terrain::height(cam.xz)` lookup was meaningless once the noise
    // heightfield was removed.
    let surface_world =
        Vec3::new(cam.translation.x, super::store::GROUND_BIAS_Y, cam.translation.z);
    let surface_chunk_y = ChunkCoord::containing(surface_world).0.y;
    let cam_chunk_xz = ChunkCoord::containing(cam.translation);
    let r_xz = env.draw_distance_chunks.max(1);

    // Steady-state skip: when (camera-chunk-XZ, surface-chunk-Y,
    // draw distance) all match the previous tick AND we've already
    // finished seeding the bbox, return immediately. The
    // `fully_seeded` flag is what makes the rate-limit work: while
    // mid-fill we keep coming back next frame to seed more, but once
    // done we stay quiet.
    let cam_xz_key = (cam_chunk_xz.0.x, cam_chunk_xz.0.z);
    let bbox_unchanged = cache.last_cam_chunk_xz == Some(cam_xz_key)
        && cache.last_surface_chunk_y == Some(surface_chunk_y)
        && cache.last_r_xz == r_xz;
    if bbox_unchanged && cache.fully_seeded {
        return;
    }
    if !bbox_unchanged {
        // Bbox changed → start a fresh fill.
        cache.fully_seeded = false;
    }

    // Build the candidate offset list once, sorted so the chunk under
    // the camera lands first and far chunks last. Surface (dy=0) gets
    // priority over padding (dy=±1) so the visible Y layer fills in
    // before the padding layers compete for the per-frame seed budget.
    let mut candidates: Vec<(i32, i32, i32)> =
        Vec::with_capacity(((2 * r_xz + 1).pow(2) * (2 * STREAM_RADIUS_Y + 1)) as usize);
    for dy in -STREAM_RADIUS_Y..=STREAM_RADIUS_Y {
        for dz in -r_xz..=r_xz {
            for dx in -r_xz..=r_xz {
                candidates.push((dx, dy, dz));
            }
        }
    }
    candidates.sort_by_key(|(dx, dy, dz)| {
        // Tuple sort: surface first, then chebyshev distance.
        (dy.abs(), dx.abs().max(dz.abs()))
    });

    let pool = AsyncComputeTaskPool::get();
    let mut dispatched = 0usize;
    let mut already_in_store = 0usize;
    let mut already_in_flight = 0usize;
    let mut hit_cap = false;
    let total_candidates = candidates.len();
    for (dx, dy, dz) in candidates {
        let coord = ChunkCoord::new(
            cam_chunk_xz.0.x + dx,
            surface_chunk_y + dy,
            cam_chunk_xz.0.z + dz,
        );
        if store.contains(coord) {
            already_in_store += 1;
            continue;
        }
        if pending.in_flight_set.contains(&coord) {
            already_in_flight += 1;
            continue;
        }
        if pending.tasks.len() >= SEED_TASK_BUDGET {
            // Worker pool is saturated for this frame. Don't queue more
            // — the next frame's dispatcher picks up where we left off
            // because the cache stays invalid (`fully_seeded = false`).
            hit_cap = true;
            break;
        }
        // Spawn the SDF gen on a worker thread. `EditorHeightfield` is
        // a unit struct (Copy) and `procedural::sample` reads from a
        // process-global `OnceLock`, so the closure is trivially Send.
        let task = pool.spawn(async move {
            let mut chunk = VoxelChunk::new_air();
            let generator = EditorHeightfield;
            generator.seed_chunk(coord, &mut chunk);
            chunk
        });
        pending.tasks.push((coord, task));
        pending.in_flight_set.insert(coord);
        dispatched += 1;
    }

    cache.last_cam_chunk_xz = Some(cam_xz_key);
    cache.last_surface_chunk_y = Some(surface_chunk_y);
    cache.last_r_xz = r_xz;
    // Fully seeded only when the walk completed AND no tasks are still
    // in flight — otherwise next frame must keep dispatching as worker
    // threads finish and the budget opens up.
    cache.fully_seeded = !hit_cap && pending.tasks.is_empty();

    // First-frame diagnostic.
    if !*first_frame_logged {
        *first_frame_logged = true;
        let line = format!(
            "streamer first-frame: cam=({:.0}, {:.0}, {:.0}) chunk=({}, {}, {}) r_xz={r_xz} candidates={total_candidates} in_store={already_in_store} in_flight={already_in_flight} dispatched={dispatched} cap_hit={hit_cap} store={}",
            cam.translation.x, cam.translation.y, cam.translation.z,
            cam_chunk_xz.0.x, surface_chunk_y, cam_chunk_xz.0.z,
            store.len(),
        );
        info!("editor: {line}");
        log.push(line);
    }

    if dispatched > 0 {
        debug!(
            "editor voxel streamer: dispatched {dispatched} seed tasks (cap_hit={hit_cap}, in_flight={}, surface_chunk_y={surface_chunk_y}, store={}, camera={:?})",
            pending.tasks.len(),
            store.len(),
            cam.translation
        );
    }
}

/// Drain finished seed tasks. For each completed `(coord, chunk)`:
///   1. Insert into the chunk store.
///   2. Run `sync_chunk_halos_for_one` so neighbor padding agrees with
///      what we just generated (otherwise visible mesh seams).
///   3. Stamp the biome map from cartography overrides.
///   4. Mark the chunk dirty so the meshing pipeline picks it up.
///
/// Capped at `MAX_SEEDS_COLLECTED_PER_FRAME` so the halo-sync cost
/// stays bounded even when many tasks finish in the same frame.
pub fn collect_completed_seeds(
    mut store: ResMut<ChunkStore>,
    mut dirty: ResMut<DirtyChunks>,
    overrides: Res<BiomeOverrideMap>,
    mut pending: ResMut<PendingSeeds>,
    mut chunk_biomes: ResMut<ChunkBiomeMap>,
    mut perf: ResMut<SystemFrameTimes>,
) {
    let _timer = SystemTimer::new(&mut perf, "editor::collect_seeds");
    if pending.tasks.is_empty() {
        return;
    }
    let mut collected = 0usize;
    let mut i = 0;
    while i < pending.tasks.len() {
        if collected >= MAX_SEEDS_COLLECTED_PER_FRAME {
            break;
        }
        // Poll exactly once and capture the result. `poll_once` returns
        // `Some(value)` and consumes the future on completion — polling
        // again panics with "Task polled after completion".
        let chunk_opt = {
            let (_, task) = &mut pending.tasks[i];
            block_on(future::poll_once(task))
        };
        let Some(chunk) = chunk_opt else {
            i += 1;
            continue;
        };
        let (coord, _finished_task) = pending.tasks.swap_remove(i);
        pending.in_flight_set.remove(&coord);

        // Late camera move could have walked away from this chunk, but
        // there's no harm in inserting it — eviction is the streamer's
        // job. Skip if a concurrent dirty path already filled this slot
        // (shouldn't happen with the in_flight_set dedup, but cheap).
        if store.contains(coord) {
            continue;
        }
        store.insert(coord, chunk);
        // Inherit halo padding from already-loaded neighbors so the
        // mesh seam stays clean.
        vaern_voxel::persistence::sync_chunk_halos_for_one(&mut store, coord);
        dirty.mark(coord);

        let biome = overrides.get(coord.0.x, coord.0.z).unwrap_or(DEFAULT_BIOME);
        chunk_biomes.by_coord.insert(coord, biome);
        collected += 1;
    }
}

/// Marker on chunk entities whose mesh attributes (UVs, tangents,
/// biome blend weights) and material attach are pending. Inserted by
/// `mark_chunks_needing_blend_refresh` on either:
///   * `Added<ChunkRenderTag>` — fresh chunk, never had material attached
///   * `Changed<Mesh3d>` — re-mesh, attributes are now stale
///
/// Persists across frames (unlike `Added<T>` / `Changed<T>` filters
/// which only match for one tick) so the rate-limited
/// `process_pending_blend_attaches` system can defer work without
/// losing track of which chunks still need it.
#[derive(Component)]
pub struct NeedsBlendAttach;

/// Lightweight Update system: tag every newly-spawned or freshly-re-
/// meshed chunk with `NeedsBlendAttach`, but ONLY when the underlying
/// mesh asset actually lacks the required vertex attributes.
///
/// Why the asset check is critical: in this codebase `Changed<Mesh3d>`
/// fires every frame for every chunk (suspected spurious — see Bug 2
/// in `MAX_ATTACHES_PER_FRAME` comment). Without filtering by actual
/// asset state, every chunk would get re-marked and re-hidden every
/// frame. Capped to 16/frame on the un-hide side, that means ~1157 of
/// 1173 chunks are hidden each frame → catastrophic flickering, large
/// gaps in the world, back faces visible through holes. The fix:
/// only mark+hide when `mesh.attribute(ATTRIBUTE_BIOME_WEIGHTS_LO)`
/// returns None (a genuine fresh asset from `collect_completed_meshes`
/// after a sculpt or fresh seed).
///
/// For re-meshed chunks that pass the filter, also flip them to
/// `Visibility::Hidden` so the renderer skips them entirely until
/// `process_pending_blend_attaches` catches up at its 16/frame cap.
/// The cap-induced render race we're avoiding:
/// `collect_completed_meshes` drains every completed async-meshing
/// task in one tick, reassigning new `Mesh3d` handles to all dirtied
/// chunks at once. Each new mesh asset arrives without UV + biome-
/// weight attributes. If a chunk still has the `BiomeBlendMaterial`
/// (which requires UV) when render extraction runs that frame, wgpu
/// errors out with "Mesh is missing requested attribute: Vertex_Uv".
///
/// Why hide instead of swapping the material? Switching
/// `MeshMaterial3d<A>` ↔ `MeshMaterial3d<B>` on the same entity is
/// fragile in Bevy 0.18 — the renderer can end up with the new
/// material's pipeline trying to bind the old material's bind group.
/// Hiding skips render extraction entirely, so no pipeline / bind
/// group selection happens during the bridge state. Visual cost:
/// genuinely re-meshed chunks blink invisible for 1-N frames during
/// a sculpt burst, then snap back. Acceptable.
pub fn mark_chunks_needing_blend_refresh(
    mut commands: Commands,
    meshes: Res<Assets<Mesh>>,
    new_chunks: Query<
        (Entity, &Mesh3d),
        (Added<ChunkRenderTag>, Without<NeedsBlendAttach>),
    >,
    changed_meshes: Query<
        (Entity, &Mesh3d),
        (Changed<Mesh3d>, With<ChunkRenderTag>, Without<NeedsBlendAttach>),
    >,
) {
    let mesh_lacks_attrs = |mesh3d: &Mesh3d| -> bool {
        match meshes.get(&mesh3d.0) {
            Some(mesh) => mesh
                .attribute(super::biome_blend::ATTRIBUTE_BIOME_WEIGHTS_LO)
                .is_none(),
            // Asset not yet loaded; definitely needs attach when it lands.
            None => true,
        }
    };

    for (e, mesh3d) in &new_chunks {
        if mesh_lacks_attrs(mesh3d) {
            commands.entity(e).try_insert(NeedsBlendAttach);
            // No Visibility::Hidden on fresh chunks — they have no
            // material yet, so they don't render and there's no race.
        }
    }
    for (e, mesh3d) in &changed_meshes {
        if mesh_lacks_attrs(mesh3d) {
            commands
                .entity(e)
                .try_insert(NeedsBlendAttach)
                .try_insert(Visibility::Hidden);
        }
        // Otherwise: spurious `Changed<Mesh3d>` (same handle reassigned
        // by `collect_completed_meshes` or some other system mutating
        // Mesh3d without changing the asset). Skip — the mesh already
        // has the attributes and the chunk is rendering correctly.
    }
}

/// PostUpdate: process up to `MAX_ATTACHES_PER_FRAME` chunks tagged
/// with `NeedsBlendAttach`. For each: build UVs + MikkTSpace tangents
/// + per-vertex biome blend weights, then attach the shared
/// `BiomeBlendMaterial` (or the fallback `StandardMaterial` if the
/// perf toggle is off), then remove the marker.
///
/// Excess pending chunks stay tagged and get picked up next frame.
/// This bounds PostUpdate to ~8ms / frame even when 64+ chunks finish
/// meshing simultaneously — without the cap, the unbounded for-loop
/// here was the dominant per-frame cost during initial fill.
pub fn process_pending_blend_attaches(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    blend_assets: Option<Res<BiomeBlendAssets>>,
    blend_enabled: Res<BiomeBlendEnabled>,
    overrides: Res<BiomeOverrideMap>,
    pending: Query<(Entity, &ChunkRenderTag, &Transform, &Mesh3d), With<NeedsBlendAttach>>,
    mut perf: ResMut<SystemFrameTimes>,
) {
    let _timer = SystemTimer::new(&mut perf, "editor::process_pending_blend_attaches");
    let Some(blend_assets) = blend_assets else {
        // Startup hasn't initialized the shared material yet; markers
        // stay on the entities and we pick them up next frame.
        return;
    };

    // Capped pass with idempotent fast-path inside
    // `ensure_chunk_mesh_attributes`. The cap protects FPS from the
    // marker re-add loop (Bug 2 — see comment on `MAX_ATTACHES_PER_FRAME`).
    // Edge case: when an edit burst marks more than 16 chunks via
    // `Changed<Mesh3d>`, the unprocessed chunks were flipped to
    // `Visibility::Hidden` by `mark_chunks_needing_blend_refresh`
    // and skip render extraction; we restore visibility here after
    // the attributes are guaranteed attached.
    let mut processed = 0usize;
    for (entity, tag, xform, mesh3d) in pending.iter() {
        if processed >= MAX_ATTACHES_PER_FRAME {
            break;
        }
        ensure_chunk_mesh_attributes(
            &mut meshes,
            &mesh3d.0,
            xform,
            tag.coord,
            &overrides,
        );
        let mut e = commands.entity(entity);
        if blend_enabled.0 {
            e.try_insert(MeshMaterial3d(blend_assets.material.clone()));
        } else {
            e.try_insert(MeshMaterial3d(blend_assets.fallback_material.clone()));
        }
        e.try_insert(Visibility::Inherited);
        e.try_remove::<NeedsBlendAttach>();
        processed += 1;
    }
}

/// Single-pass attribute setup: UVs + per-vertex biome blend
/// (3 weight vec4s in `biome_blend.rs`). All four are required by the
/// BiomeBlendMaterial pipeline; tangents are intentionally skipped
/// (the shader doesn't sample normal maps).
///
/// Genuinely idempotent: early-outs when the four attributes are
/// already attached to the mesh. The earlier comment claimed
/// idempotence but the body unconditionally cloned positions
/// (~48KB) + rebuilt UVs (~32KB) + recomputed all 3 biome-weight
/// vec4s every frame, costing ~74µs per chunk × 1173 chunks = 87ms
/// per frame at draw=16. Now: the fast path is one HashMap lookup.
///
/// Re-meshing (sculpt/biome-paint dirty) replaces the mesh handle's
/// underlying asset, which arrives without the four attributes — so
/// the early-out correctly falls through and the rebuild runs.
fn ensure_chunk_mesh_attributes(
    meshes: &mut Assets<Mesh>,
    mesh_handle: &Handle<Mesh>,
    xform: &Transform,
    _coord: ChunkCoord,
    overrides: &BiomeOverrideMap,
) {
    let Some(mesh) = meshes.get_mut(mesh_handle) else {
        return;
    };
    // Idempotent fast path. The four attributes (UV_0 + 3 biome-weight
    // vec4s) are inserted as a unit, so checking just one of the biome
    // attributes is sufficient — partial state shouldn't exist.
    if mesh
        .attribute(super::biome_blend::ATTRIBUTE_BIOME_WEIGHTS_LO)
        .is_some()
    {
        return;
    }
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION).cloned()
    else {
        return;
    };

    // UVs in world-XZ space; the fragment shader scales by tile_size_m.
    let ox = xform.translation.x;
    let oz = xform.translation.z;
    let uvs: Vec<[f32; 2]> = positions.iter().map(|p| [p[0] + ox, p[2] + oz]).collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);

    // Per-vertex biome weights — recomputed every re-mesh so a
    // biome-paint stroke that marks neighbors dirty automatically
    // refreshes blend attributes on the next remesh tick.
    insert_blend_attributes(mesh, xform.translation, overrides);
}

/// Swap chunk materials whenever the perf-isolation toggle flips.
/// Runs on `Changed<BiomeBlendEnabled>` semantics — but since the
/// resource isn't a Component, we manually `is_changed` it.
pub fn apply_biome_blend_toggle(
    mut commands: Commands,
    enabled: Res<BiomeBlendEnabled>,
    blend_assets: Option<Res<BiomeBlendAssets>>,
    chunks_q: Query<Entity, With<vaern_voxel::plugin::ChunkRenderTag>>,
) {
    if !enabled.is_changed() {
        return;
    }
    let Some(assets) = blend_assets else {
        return;
    };
    for entity in &chunks_q {
        let mut e = commands.entity(entity);
        if enabled.0 {
            // Switch to biome blend material; remove the fallback.
            e.try_remove::<MeshMaterial3d<StandardMaterial>>();
            e.try_insert(MeshMaterial3d(assets.material.clone()));
        } else {
            // Switch to fallback StandardMaterial; remove the blend.
            e.try_remove::<MeshMaterial3d<BiomeBlendMaterial>>();
            e.try_insert(MeshMaterial3d(assets.fallback_material.clone()));
        }
    }
}
