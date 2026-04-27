//! Chunk streamer + post-mesh material attach.
//!
//! Each frame, ensure chunks within the camera's `draw_distance_chunks`
//! radius are seeded into the `ChunkStore`. Post-mesh, attach the
//! shared `BiomeBlendMaterial` (one handle for every chunk) so all
//! chunks render through the same texture-array-backed PBR pipeline.
//! Per-vertex biome IDs + weights drive a 4-biome splat in the
//! fragment shader — biome paint never swaps materials, just marks
//! affected chunks dirty so their per-vertex weights re-compute.

use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
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
pub const DEFAULT_BIOME: BiomeKey = BiomeKey::Marsh;

/// Default horizontal streaming radius (chunks). 16 chunks = ~512m
/// radius, comfortable scenery feel without a long cold-start. The
/// slider reaches 64 for users who want the full 2km draw distance —
/// the sparse `VoxelChunk` storage + async meshing + frustum culling
/// make 64 viable but warm-up takes ~15s and RAM climbs accordingly.
pub const DEFAULT_STREAM_RADIUS_XZ: i32 = 16;
/// Vertical radius — terrain amplitude is small, so 3 layers cover
/// surface + a slice of underground.
pub const STREAM_RADIUS_Y: i32 = 1;

/// Maximum chunks the streamer will seed in a single frame. Each seed
/// runs `generator.seed_chunk` (39,304 sample evaluations + try_compact)
/// — at the trivial flat-marsh generator that's ~80µs per chunk, so
/// 256 = ~20ms / frame ceiling. Spreads the initial-fill cost over
/// ~13 frames at draw=16 (3267 chunks total) instead of dumping the
/// whole 260ms in one frame.
pub const MAX_SEEDS_PER_FRAME: usize = 256;

// (MAX_ATTACHES_PER_FRAME is gone — tangent generation was the
// expensive part, and dropping it leaves only ~150µs of work per
// chunk. With MeshingBudget=16 the natural inflow is bounded; an
// uncapped pass keeps mesh attributes + material in sync with the
// pipeline's expectations on the same tick the chunk's mesh changes.)

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
    mut store: ResMut<ChunkStore>,
    mut dirty: ResMut<DirtyChunks>,
    overrides: Res<BiomeOverrideMap>,
    env: Res<EnvSettings>,
    mut chunk_biomes: ResMut<ChunkBiomeMap>,
    mut cache: Local<StreamCache>,
    mut perf: ResMut<SystemFrameTimes>,
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

    let generator = EditorHeightfield;
    let mut seeded = 0usize;
    let mut hit_cap = false;
    for (dx, dy, dz) in candidates {
        let coord = ChunkCoord::new(
            cam_chunk_xz.0.x + dx,
            surface_chunk_y + dy,
            cam_chunk_xz.0.z + dz,
        );
        if store.contains(coord) {
            continue;
        }
        let mut chunk = VoxelChunk::new_air();
        generator.seed_chunk(coord, &mut chunk);
        store.insert(coord, chunk);
        dirty.mark(coord);

        let biome = overrides
            .get(coord.0.x, coord.0.z)
            .unwrap_or(DEFAULT_BIOME);
        chunk_biomes.by_coord.insert(coord, biome);

        seeded += 1;
        if seeded >= MAX_SEEDS_PER_FRAME {
            hit_cap = true;
            break;
        }
    }

    cache.last_cam_chunk_xz = Some(cam_xz_key);
    cache.last_surface_chunk_y = Some(surface_chunk_y);
    cache.last_r_xz = r_xz;
    // If we walked the whole candidate list without hitting the cap,
    // the bbox is fully seeded — flip the flag so subsequent frames
    // skip the walk entirely.
    cache.fully_seeded = !hit_cap;

    if seeded > 0 {
        debug!(
            "editor voxel streamer: seeded {seeded} chunks (cap_hit={hit_cap}, surface_chunk_y={surface_chunk_y}, store={}, camera={:?})",
            store.len(),
            cam.translation
        );
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
/// meshed chunk with `NeedsBlendAttach` if it isn't already tagged.
/// Cheap — just iterates the small `Added` / `Changed` filter results
/// and inserts a unit component.
pub fn mark_chunks_needing_blend_refresh(
    mut commands: Commands,
    new_chunks: Query<Entity, (Added<ChunkRenderTag>, Without<NeedsBlendAttach>)>,
    changed_meshes: Query<
        Entity,
        (Changed<Mesh3d>, With<ChunkRenderTag>, Without<NeedsBlendAttach>),
    >,
) {
    for e in &new_chunks {
        commands.entity(e).try_insert(NeedsBlendAttach);
    }
    for e in &changed_meshes {
        commands.entity(e).try_insert(NeedsBlendAttach);
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

    // Single uncapped pass. All four required attributes (UVs +
    // BiomeIds + BiomeWeights — tangents are intentionally absent)
    // need to be present on the mesh AND the material attached, all
    // before render extraction. With MeshingBudget=16, the natural
    // per-frame inflow is ~16 chunks → ~2-3ms in this pass. During
    // burst events (initial fill, biome paint marking neighbors) it
    // can spike higher, but partial completion would render incomplete
    // chunks — `Mesh is missing requested attribute: Vertex_*` spam
    // and gaps in the world. Better to spend a one-frame spike than
    // ship visual holes.
    for (entity, tag, xform, mesh3d) in pending.iter() {
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
        e.try_remove::<NeedsBlendAttach>();
    }
}

/// Single-pass attribute setup: UVs + per-vertex biome blend
/// (Vertex_BiomeIds + Vertex_BiomeWeights). All three are required by
/// the BiomeBlendMaterial pipeline; tangents are intentionally
/// skipped (the shader doesn't sample normal maps).
///
/// Idempotent: re-running on a chunk that already has all three
/// attributes is a few hashmap lookups and overwrite — cheap. Called
/// every frame for chunks tagged `NeedsBlendAttach`.
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

    // Per-vertex biome IDs + weights — recomputed every re-mesh so a
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
