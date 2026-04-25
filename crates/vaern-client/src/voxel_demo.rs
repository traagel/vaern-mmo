//! Client-side voxel world — the game's ground.
//!
//! Replaces the retired tessellated grass plane AND the legacy
//! `scene/hub_regions.rs` overlay pipeline. Voxels are the only ground
//! layer now; each chunk picks its `StandardMaterial` from a per-biome
//! cache keyed on the Voronoi biome assignment for its footprint.
//!
//! Systems:
//!
//! 1. **Resolver init** — loads `BiomeResolver` from the world YAML on
//!    startup. Empty resolver on YAML failure (every chunk → Grass).
//! 2. **Streamer** — each frame, seed chunks around the camera; record
//!    each chunk's biome into `ChunkBiomeMap` at seed time so the
//!    material attach can look it up when the chunk gets meshed.
//! 3. **Material + UV attach** — when a chunk entity appears
//!    (`Added<ChunkRenderTag>`), add world-XZ UV coords to the mesh
//!    asset (voxel crate emits positions + normals only) and insert the
//!    biome's `MeshMaterial3d` handle (CC0 PBR set from ambientCG).
//! 4. **Debug stomp** — F10 carves a sphere crater at the camera focus.
//! 5. **Heartbeat** — periodic info log of chunk + material counts.
//!
//! Client-authoritative edits only. Server voxel authority + delta
//! replication is a separate slice.

use std::path::Path;

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::math::Affine2;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use lightyear::prelude::client::Client;
use lightyear::prelude::*;
use std::collections::HashMap;

use vaern_protocol::{Channel1, ServerBrushMode, ServerEditStroke, VoxelChunkDelta};
use vaern_voxel::chunk::{ChunkCoord, ChunkStore, DirtyChunks};
use vaern_voxel::config::CHUNK_WORLD_SIZE;
use vaern_voxel::generator::WorldGenerator;
use vaern_voxel::plugin::{ChunkEntityMap, ChunkRenderTag, VoxelCorePlugin, VoxelMeshPlugin};
use vaern_voxel::VoxelChunk;

use crate::menu::AppState;
use crate::voxel_biomes::{BiomeKey, BiomeResolver};

// --- tuning -----------------------------------------------------------------

/// `(2*R + 1)` chunks per axis streamed around the camera. With 32u
/// chunks, R=5 = 11-chunk-wide ~352u footprint. Empirically this is
/// the working-set size that stays rendering-stable; larger radii have
/// masked a render-pipeline regression I haven't isolated yet.
const STREAM_RADIUS_XZ: i32 = 5;
/// Vertical radius. Terrain amplitude ≤ ~2u, so 3 Y-chunks (-1, 0, 1)
/// cover the surface and a bit of underground for crater carving.
const STREAM_RADIUS_Y: i32 = 1;

/// F10 crater size.
const DEBUG_STOMP_RADIUS: f32 = 6.0;

/// Chunks the voxel mesher will re-extract per frame. Default is 4 —
/// too slow for the initial 363-chunk burst. 64 empties the queue in
/// ~6 frames without an allocation spike.
const MESHING_BUDGET: usize = 64;

/// Bias applied to the SDF surface in world units. Set to 0 so the
/// voxel surface coincides with `terrain::height`, which is what the
/// server uses to snap player + NPC Y. A -1u bias had been needed back
/// when chunk seams left gaps that characters fell through visually;
/// once the voxel crate's mesher started closing +side boundaries (see
/// `ChunkShape::MESH_MIN`), the gaps closed and the bias just lifted
/// entities off the ground. Small sub-voxel grid-snap variance (≤0.5u)
/// remains because Surface Nets centroid placement quantizes to
/// VOXEL_SIZE, but that's invisible in practice against 2u-tall
/// characters.
const GROUND_BIAS_Y: f32 = 0.0;

/// World meters per biome-texture tile. Matches the `hub_regions`
/// value so familiar biomes read at the same density.
const TEXTURE_TILE_M: f32 = 8.0;

// --- plugin -----------------------------------------------------------------

pub struct VoxelDemoPlugin;

impl Plugin for VoxelDemoPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
                VoxelCorePlugin,
                VoxelMeshPlugin { meshes_per_frame: MESHING_BUDGET },
            ))
            .init_resource::<BiomeMaterials>()
            .init_resource::<ChunkBiomeMap>()
            .init_resource::<BiomeResolver>()
            .add_systems(Startup, init_biome_resolver)
            .add_systems(
                Update,
                (
                    stream_chunks_around_camera.run_if(in_state(AppState::InGame)),
                    debug_stomp.run_if(in_state(AppState::InGame)),
                    apply_server_chunk_deltas.run_if(in_state(AppState::InGame)),
                    voxel_heartbeat.run_if(in_state(AppState::InGame)),
                ),
            )
            .add_systems(
                PostUpdate,
                (attach_biome_material_and_uvs, refresh_uvs_on_remesh),
            );
    }
}

// --- resources --------------------------------------------------------------

/// Cache of per-biome `StandardMaterial` handles, built lazily the
/// first time each biome is needed. Shared across every chunk of the
/// same biome so we don't allocate redundant materials.
#[derive(Resource, Default)]
struct BiomeMaterials(HashMap<BiomeKey, Handle<StandardMaterial>>);

/// Which biome each seeded chunk belongs to. Populated by the streamer
/// at seed time so the material attach system (PostUpdate, runs on a
/// different frame than seeding in general) doesn't have to re-query
/// the resolver per chunk.
#[derive(Resource, Default)]
struct ChunkBiomeMap {
    by_coord: HashMap<ChunkCoord, BiomeKey>,
}

// --- generator --------------------------------------------------------------

/// `HeightfieldGenerator` with a constant Y bias. Identical to the
/// crate's `HeightfieldGenerator` except the voxel surface sits at
/// `terrain::height(x, z) + GROUND_BIAS_Y`.
#[derive(Clone, Copy, Debug)]
struct BiasedHeightfield;

impl WorldGenerator for BiasedHeightfield {
    #[inline]
    fn sample(&self, p: Vec3) -> f32 {
        let h = vaern_core::terrain::height(p.x, p.z) + GROUND_BIAS_Y;
        p.y - h
    }
}

// --- startup ----------------------------------------------------------------

fn init_biome_resolver(mut commands: Commands) {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let world_root = manifest.join("../../src/generated/world");
    commands.insert_resource(BiomeResolver::load(&world_root));
}

// --- streamer ---------------------------------------------------------------

fn stream_chunks_around_camera(
    camera_q: Query<&Transform, With<Camera3d>>,
    mut store: ResMut<ChunkStore>,
    mut dirty: ResMut<DirtyChunks>,
    resolver: Res<BiomeResolver>,
    mut chunk_biomes: ResMut<ChunkBiomeMap>,
) {
    let Ok(cam_xform) = camera_q.single() else {
        return;
    };
    let center = ChunkCoord::containing(cam_xform.translation);

    let generator = BiasedHeightfield;
    let mut newly_seeded = 0usize;
    for dz in -STREAM_RADIUS_XZ..=STREAM_RADIUS_XZ {
        for dy in -STREAM_RADIUS_Y..=STREAM_RADIUS_Y {
            for dx in -STREAM_RADIUS_XZ..=STREAM_RADIUS_XZ {
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
                dirty.mark(coord);

                // Sample biome at chunk footprint center.
                let origin = coord.world_origin();
                let cx = origin.x + CHUNK_WORLD_SIZE * 0.5;
                let cz = origin.z + CHUNK_WORLD_SIZE * 0.5;
                let biome = resolver.biome_at(cx, cz);
                chunk_biomes.by_coord.insert(coord, biome);

                newly_seeded += 1;
            }
        }
    }
    if newly_seeded > 0 {
        info!(
            "voxel streamer: seeded {newly_seeded} chunks around cam={:?} (store size: {})",
            cam_xform.translation,
            store.len()
        );
    }
}

// --- material + UV attach ---------------------------------------------------

/// For each freshly spawned chunk render entity:
///   1. Add world-XZ UV_0 coords to its mesh asset — voxel crate emits
///      positions + normals only, so tiled biome textures would sample
///      nothing without this.
///   2. Look up the chunk's biome in `ChunkBiomeMap`, fetch or build
///      that biome's shared material, and insert it on the entity.
///
/// Runs in PostUpdate so `Added<ChunkRenderTag>` only resolves to
/// entities the voxel crate's Update-scheduled `rebuild_dirty_chunks`
/// has already spawned + flushed.
fn attach_biome_material_and_uvs(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut biome_mats: ResMut<BiomeMaterials>,
    chunk_biomes: Res<ChunkBiomeMap>,
    new_chunks: Query<
        (Entity, &ChunkRenderTag, &Transform, &Mesh3d),
        (Added<ChunkRenderTag>, Without<MeshMaterial3d<StandardMaterial>>),
    >,
) {
    if new_chunks.is_empty() {
        return;
    }
    let mut attached = 0usize;
    for (entity, tag, xform, mesh3d) in &new_chunks {
        ensure_chunk_mesh_attributes(&mut meshes, &mesh3d.0, xform, tag.coord);

        let biome = chunk_biomes
            .by_coord
            .get(&tag.coord)
            .copied()
            .unwrap_or(BiomeKey::Grass);
        let handle = biome_mats
            .0
            .entry(biome)
            .or_insert_with(|| build_biome_material(&asset_server, &mut materials, biome))
            .clone();

        // Direct insert — voxel crate's rebuild_dirty_chunks and the
        // evictor (currently disabled) are the only things that despawn
        // chunk entities, and both run in schedules that can't race
        // with this PostUpdate system. `commands.queue(move |world|...)`
        // was used here defensively but silently swallowed the insert
        // for some `Added` entities, so carved-from-underground chunks
        // rendered without materials — the shallow-vs-deep hole split
        // the user reported.
        commands.entity(entity).insert(MeshMaterial3d(handle));
        attached += 1;
    }
    if attached > 0 {
        debug!("attached biome material to {attached} new chunk render entities");
    }
}

/// When the voxel crate re-meshes a chunk (e.g. after an F10 stomp
/// carves a crater), it replaces the `Mesh3d` handle with a freshly
/// built mesh that has positions + normals only. Without re-adding
/// world-XZ UVs + MikkTSpace tangents on the new mesh, the material's
/// biome texture samples at UV (0,0) and the chunk renders white.
/// Watches `Changed<Mesh3d>` so this only fires when the handle
/// actually swaps, not every frame.
fn refresh_uvs_on_remesh(
    mut meshes: ResMut<Assets<Mesh>>,
    chunks: Query<(&ChunkRenderTag, &Transform, &Mesh3d), Changed<Mesh3d>>,
) {
    for (tag, xform, mesh3d) in &chunks {
        ensure_chunk_mesh_attributes(&mut meshes, &mesh3d.0, xform, tag.coord);
    }
}

/// Add world-XZ UVs + tangents to the mesh asset if missing. No-op if
/// the asset isn't resolvable (just-despawned race) or the attributes
/// are already present.
fn ensure_chunk_mesh_attributes(
    meshes: &mut Assets<Mesh>,
    mesh_handle: &Handle<Mesh>,
    xform: &Transform,
    coord: ChunkCoord,
) {
    let Some(mesh) = meshes.get_mut(mesh_handle) else {
        return;
    };
    if mesh.attribute(Mesh::ATTRIBUTE_UV_0).is_some() {
        return;
    }
    // Positions are in LOCAL chunk space; add the chunk entity's
    // world translation so UVs are world-XZ and the texture tiles
    // seamlessly across chunks.
    if let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION).cloned()
    {
        let ox = xform.translation.x;
        let oz = xform.translation.z;
        let uvs: Vec<[f32; 2]> = positions
            .iter()
            .map(|p| [p[0] + ox, p[2] + oz])
            .collect();
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    }
    // MikkTSpace tangents are deterministic given (position, normal,
    // UV) — neighbor chunks compute identical values at shared world
    // positions, so seams read as one continuous surface.
    if let Err(e) = mesh.generate_tangents() {
        warn!("voxel chunk {coord:?}: generate_tangents failed: {e:?}");
    }
}

fn build_biome_material(
    assets: &AssetServer,
    materials: &mut Assets<StandardMaterial>,
    biome: BiomeKey,
) -> Handle<StandardMaterial> {
    // Biome textures tile via world-XZ UVs; default sampler clamps to
    // edge, which would read a single pixel across the whole world.
    // Force Repeat so textures tile.
    let repeat = |s: &mut ImageLoaderSettings| {
        s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::Repeat,
            address_mode_v: ImageAddressMode::Repeat,
            address_mode_w: ImageAddressMode::Repeat,
            ..ImageSamplerDescriptor::linear()
        });
    };
    let tex = biome.textures();
    let color = assets.load_with_settings(tex.color, repeat);
    let normal = assets.load_with_settings(tex.normal, repeat);
    let ao = tex.ao.map(|p| assets.load_with_settings(p, repeat));

    let _ = RenderAssetUsages::default(); // keep the import live
    materials.add(StandardMaterial {
        base_color_texture: Some(color),
        normal_map_texture: Some(normal),
        occlusion_texture: ao,
        perceptual_roughness: 0.95,
        metallic: 0.0,
        uv_transform: Affine2::from_scale(Vec2::splat(1.0 / TEXTURE_TILE_M)),
        ..default()
    })
}

// --- diagnostics ------------------------------------------------------------

fn voxel_heartbeat(
    time: Res<Time>,
    mut last_log: Local<f32>,
    store: Res<ChunkStore>,
    entity_map: Res<ChunkEntityMap>,
    biome_mats: Res<BiomeMaterials>,
    with_mat: Query<(), (With<ChunkRenderTag>, With<MeshMaterial3d<StandardMaterial>>)>,
    without_mat: Query<(), (With<ChunkRenderTag>, Without<MeshMaterial3d<StandardMaterial>>)>,
) {
    let now = time.elapsed_secs();
    if now - *last_log < 1.0 {
        return;
    }
    *last_log = now;
    info!(
        "voxel heartbeat: store={} entities={} with_mat={} no_mat={} biome_mats_cached={}",
        store.len(),
        entity_map.by_coord.len(),
        with_mat.iter().count(),
        without_mat.iter().count(),
        biome_mats.0.len(),
    );
}

// --- debug stomp ------------------------------------------------------------

/// F10: ask the server to carve a crater at the camera's forward
/// focus. Server is authoritative — it applies the brush against its
/// `ChunkStore`, broadcasts the changed chunks as `VoxelChunkDelta`
/// messages, and every connected client (including us) sees the edit
/// via [`apply_server_chunk_deltas`]. No client-side edit happens
/// here, so the crater only shows once the server round-trip lands.
///
/// Crater center Y is anchored **1u below the analytical terrain
/// surface** at the XZ focus, not to an arbitrary world Y. That way
/// the sphere always carves a consistent `radius + 1` depth into the
/// ground regardless of camera pitch — otherwise a steep-down look
/// put the sphere center *above* terrain surface and you got a
/// shallow dimple where only the bottom half of the sphere overlapped
/// solid ground.
fn debug_stomp(
    keys: Res<ButtonInput<KeyCode>>,
    camera_q: Query<&Transform, With<Camera3d>>,
    mut sender: Query<&mut MessageSender<ServerEditStroke>, With<Client>>,
) {
    if !keys.just_pressed(KeyCode::F10) {
        return;
    }
    let Ok(cam_xform) = camera_q.single() else {
        return;
    };
    let Ok(mut tx) = sender.single_mut() else {
        return;
    };
    let forward = cam_xform.forward();
    let focus = cam_xform.translation + forward * 15.0;
    let ground = vaern_core::terrain::height(focus.x, focus.z);
    let center = Vec3::new(focus.x, ground - 1.0, focus.z);
    let stroke = ServerEditStroke {
        center: center.to_array(),
        radius: DEBUG_STOMP_RADIUS,
        mode: ServerBrushMode::Subtract,
    };
    let _ = tx.send::<Channel1>(stroke);
    info!(
        "💥 F10: sent edit-stroke request to server at {center:?} (r={DEBUG_STOMP_RADIUS}u, ground_y={ground:.2})"
    );
}

/// Drain `VoxelChunkDelta` messages from the server. For each delta:
///   - Ensure the local `ChunkStore` has a chunk at that coord (seed
///     from the heightfield if missing — rare, usually the streamer
///     has already loaded it).
///   - Apply the delta to the local chunk (version-gated — the crate
///     drops out-of-order deltas so replay is safe).
///   - Mark the chunk dirty so the mesher re-extracts.
///   - Seed `ChunkBiomeMap` so the re-meshed chunk gets the right
///     biome material on re-attach.
fn apply_server_chunk_deltas(
    mut rx: Query<&mut MessageReceiver<VoxelChunkDelta>, With<Client>>,
    mut store: ResMut<ChunkStore>,
    mut dirty: ResMut<DirtyChunks>,
    resolver: Res<BiomeResolver>,
    mut chunk_biomes: ResMut<ChunkBiomeMap>,
) {
    let Ok(mut rx) = rx.single_mut() else {
        return;
    };
    for wire in rx.receive() {
        let delta = wire.0;
        let coord = ChunkCoord::new(delta.coord[0], delta.coord[1], delta.coord[2]);
        // Seed if missing so apply_to has a valid chunk to mutate.
        // Version on the fresh chunk is 0, so the incoming delta
        // (version >= 1 after a server edit) always applies.
        if !store.contains(coord) {
            let generator = BiasedHeightfield;
            let mut chunk = VoxelChunk::new_air();
            generator.seed_chunk(coord, &mut chunk);
            store.insert(coord, chunk);
        }
        if let Some(chunk) = store.get_mut(coord) {
            delta.apply_to(chunk);
        }
        dirty.mark(coord);

        // Assign biome if not already known (first time we've seen
        // this chunk). Matches the streamer's seeding path.
        chunk_biomes.by_coord.entry(coord).or_insert_with(|| {
            let origin = coord.world_origin();
            let cx = origin.x + CHUNK_WORLD_SIZE * 0.5;
            let cz = origin.z + CHUNK_WORLD_SIZE * 0.5;
            resolver.biome_at(cx, cz)
        });
    }
}
