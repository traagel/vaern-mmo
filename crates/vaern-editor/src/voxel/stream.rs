//! Chunk streamer + post-mesh material attach.
//!
//! Each frame, ensure chunks within the camera's `draw_distance_chunks`
//! radius are seeded into the `ChunkStore`. Post-mesh, attach a per-
//! biome PBR material via `BiomeMaterials` cache + `ChunkBiomeMap`
//! lookup, with biome resolved by override map → nearest-hub voronoi
//! fallback.

use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::math::Affine2;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use vaern_voxel::chunk::{ChunkCoord, ChunkStore, DirtyChunks};
use vaern_voxel::generator::WorldGenerator;
use vaern_voxel::plugin::ChunkRenderTag;
use vaern_voxel::VoxelChunk;

use super::biomes::BiomeKey;
use super::overrides::BiomeOverrideMap;
use super::store::EditorHeightfield;
use super::{BiomeMaterials, ChunkBiomeMap};
use crate::camera::FreeFlyCamera;
use crate::environment::EnvSettings;

/// Default biome for chunks not covered by a paint override. Voronoi
/// hub-based resolution was removed — every unpainted chunk uses this.
pub const DEFAULT_BIOME: BiomeKey = BiomeKey::Marsh;

/// World meters per biome-texture tile.
const TEXTURE_TILE_M: f32 = 24.0;

/// Default horizontal streaming radius (chunks).
pub const DEFAULT_STREAM_RADIUS_XZ: i32 = 5;
/// Vertical radius — terrain amplitude is small, so 3 layers cover
/// surface + a slice of underground.
pub const STREAM_RADIUS_Y: i32 = 1;

/// Each frame: seed any not-yet-loaded chunks around the camera's XZ.
pub fn stream_chunks_around_editor_camera(
    camera_q: Query<&Transform, With<FreeFlyCamera>>,
    mut store: ResMut<ChunkStore>,
    mut dirty: ResMut<DirtyChunks>,
    overrides: Res<BiomeOverrideMap>,
    env: Res<EnvSettings>,
    mut chunk_biomes: ResMut<ChunkBiomeMap>,
) {
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

    let generator = EditorHeightfield;
    let r_xz = env.draw_distance_chunks.max(1);
    let mut seeded = 0;
    for dz in -r_xz..=r_xz {
        for dy in -STREAM_RADIUS_Y..=STREAM_RADIUS_Y {
            for dx in -r_xz..=r_xz {
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

                // Biome resolution: paint override wins over the
                // global default. Voronoi hub-based fallback is gone.
                let biome = overrides
                    .get(coord.0.x, coord.0.z)
                    .unwrap_or(DEFAULT_BIOME);
                chunk_biomes.by_coord.insert(coord, biome);

                seeded += 1;
            }
        }
    }
    if seeded > 0 {
        debug!(
            "editor voxel streamer: seeded {seeded} chunks (surface_chunk_y={surface_chunk_y}, store={}, camera={:?})",
            store.len(),
            cam.translation
        );
    }
}

/// PostUpdate: attach the chunk's biome PBR material + world-XZ UVs.
pub fn attach_biome_material(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut biome_mats: ResMut<BiomeMaterials>,
    chunk_biomes: Res<ChunkBiomeMap>,
    new_chunks: Query<
        (Entity, &ChunkRenderTag, &Transform, &Mesh3d),
        (
            Added<ChunkRenderTag>,
            Without<MeshMaterial3d<StandardMaterial>>,
        ),
    >,
) {
    if new_chunks.is_empty() {
        return;
    }
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
        commands
            .entity(entity)
            .try_insert(MeshMaterial3d(handle));
    }
}

/// Build the per-biome `StandardMaterial`. World-XZ UVs tile the
/// texture every `TEXTURE_TILE_M` world units; sampler is Repeat with
/// max anisotropic filtering. Normal + AO maps loaded LINEAR.
pub fn build_biome_material(
    assets: &AssetServer,
    materials: &mut Assets<StandardMaterial>,
    biome: BiomeKey,
) -> Handle<StandardMaterial> {
    let color_settings = |s: &mut ImageLoaderSettings| {
        s.sampler = ground_sampler();
    };
    let linear_settings = |s: &mut ImageLoaderSettings| {
        s.sampler = ground_sampler();
        s.is_srgb = false;
    };
    let tex = biome.textures();
    let color = assets.load_with_settings(tex.color, color_settings);
    let normal = assets.load_with_settings(tex.normal, linear_settings);
    let ao = tex.ao.map(|p| assets.load_with_settings(p, linear_settings));

    materials.add(StandardMaterial {
        base_color_texture: Some(color),
        normal_map_texture: Some(normal),
        occlusion_texture: ao,
        perceptual_roughness: 1.0,
        metallic: 0.0,
        uv_transform: Affine2::from_scale(Vec2::splat(1.0 / TEXTURE_TILE_M)),
        ..default()
    })
}

fn ground_sampler() -> ImageSampler {
    ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        address_mode_w: ImageAddressMode::Repeat,
        anisotropy_clamp: 16,
        ..ImageSamplerDescriptor::linear()
    })
}

/// When the voxel crate re-meshes a chunk, the new mesh has positions
/// + normals only. Re-add world-XZ UVs + tangents so the material
/// samples correctly across re-meshes.
pub fn refresh_uvs_on_remesh(
    mut meshes: ResMut<Assets<Mesh>>,
    chunks: Query<(&ChunkRenderTag, &Transform, &Mesh3d), Changed<Mesh3d>>,
) {
    for (tag, xform, mesh3d) in &chunks {
        ensure_chunk_mesh_attributes(&mut meshes, &mesh3d.0, xform, tag.coord);
    }
}

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
    if let Err(e) = mesh.generate_tangents() {
        warn!("editor voxel chunk {coord:?}: generate_tangents failed: {e:?}");
    }
}
