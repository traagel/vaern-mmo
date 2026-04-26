//! Chunk streamer + post-mesh material attach.
//!
//! Each frame, ensure chunks within `STREAM_RADIUS_*` of the editor
//! camera are seeded into the `ChunkStore`. Post-mesh, attach a
//! placeholder PBR material (the editor doesn't read biome YAML in V1
//! — that's deferred to a follow-up that promotes the client's
//! `BiomeResolver` into a shared crate).
//!
//! Approach mirrors `vaern-client/src/voxel_demo.rs::stream_chunks_around_camera`.

use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::math::Affine2;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use vaern_core::terrain;
use vaern_voxel::chunk::{ChunkCoord, ChunkStore, DirtyChunks};
use vaern_voxel::config::CHUNK_WORLD_SIZE;
use vaern_voxel::generator::WorldGenerator;
use vaern_voxel::plugin::ChunkRenderTag;
use vaern_voxel::VoxelChunk;

use super::biomes::{BiomeKey, BiomeResolver};
use super::store::EditorHeightfield;
use super::{BiomeMaterials, ChunkBiomeMap};
use crate::camera::FreeFlyCamera;

/// World meters per biome-texture tile. Mirrors the client's value so
/// textures read at the same density.
const TEXTURE_TILE_M: f32 = 8.0;

/// 11-chunk-wide horizontal streaming radius (R=5).
pub const STREAM_RADIUS_XZ: i32 = 5;
/// Vertical radius — terrain amplitude is small, so 3 layers cover
/// surface + a slice of underground for cave editing. The vertical
/// band is centered on the **terrain surface** at the camera's XZ
/// (not on the camera's chunk-Y) so flying high doesn't make the
/// ground stop streaming.
pub const STREAM_RADIUS_Y: i32 = 1;

/// Each frame: seed any not-yet-loaded chunks around the camera's XZ.
///
/// The vertical band is anchored to the terrain surface, not the
/// camera's height. Without this, flying the free-fly camera up to 80u
/// puts the seeded Y range at world [32, 128] while the ground sits at
/// world Y ≈ 0–5 — the surface-nets extractor finds no zero-crossing
/// in any seeded chunk and the viewport renders empty.
pub fn stream_chunks_around_editor_camera(
    camera_q: Query<&Transform, With<FreeFlyCamera>>,
    mut store: ResMut<ChunkStore>,
    mut dirty: ResMut<DirtyChunks>,
    resolver: Res<BiomeResolver>,
    mut chunk_biomes: ResMut<ChunkBiomeMap>,
) {
    let Ok(cam) = camera_q.single() else {
        return;
    };

    // Anchor vertical streaming on the terrain surface at the camera's
    // XZ, not the camera's Y. The analytical heightmap is cheap enough
    // to sample every frame (~1 µs); voxel-store ground lookups would
    // also work but bootstrap to the same answer on the first frame
    // anyway because the chunks aren't loaded yet.
    let terrain_y = terrain::height(cam.translation.x, cam.translation.z);
    let surface_world = Vec3::new(cam.translation.x, terrain_y, cam.translation.z);
    let surface_chunk_y = ChunkCoord::containing(surface_world).0.y;
    let cam_chunk_xz = ChunkCoord::containing(cam.translation);

    let generator = EditorHeightfield;
    let mut seeded = 0;
    for dz in -STREAM_RADIUS_XZ..=STREAM_RADIUS_XZ {
        for dy in -STREAM_RADIUS_Y..=STREAM_RADIUS_Y {
            for dx in -STREAM_RADIUS_XZ..=STREAM_RADIUS_XZ {
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

                // Sample biome at the chunk footprint center so the
                // material attach (PostUpdate) doesn't have to re-query
                // the resolver per chunk.
                let origin = coord.world_origin();
                let cx = origin.x + CHUNK_WORLD_SIZE * 0.5;
                let cz = origin.z + CHUNK_WORLD_SIZE * 0.5;
                chunk_biomes.by_coord.insert(coord, resolver.biome_at(cx, cz));

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
///
/// Mirrors `vaern-client/src/voxel_demo.rs::attach_biome_material_and_uvs`:
/// look up the chunk in `ChunkBiomeMap` (populated by the streamer at
/// seed time), fetch-or-build the per-biome material from
/// `BiomeMaterials`, insert it on the entity.
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
            .insert(MeshMaterial3d(handle));
    }
}

/// Build the per-biome `StandardMaterial`. World-XZ UVs tile the
/// texture every `TEXTURE_TILE_M` world units; sampler set to Repeat
/// so neighboring chunks read from the same tile-grid.
fn build_biome_material(
    assets: &AssetServer,
    materials: &mut Assets<StandardMaterial>,
    biome: BiomeKey,
) -> Handle<StandardMaterial> {
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

/// When the voxel crate re-meshes a chunk (e.g. after an edit stroke
/// carves a crater), the new mesh has positions + normals only. Re-add
/// world-XZ UVs + tangents so the placeholder material samples
/// correctly across edits.
pub fn refresh_uvs_on_remesh(
    mut meshes: ResMut<Assets<Mesh>>,
    chunks: Query<(&ChunkRenderTag, &Transform, &Mesh3d), Changed<Mesh3d>>,
) {
    for (tag, xform, mesh3d) in &chunks {
        ensure_chunk_mesh_attributes(&mut meshes, &mesh3d.0, xform, tag.coord);
    }
}

/// Add world-XZ UVs + tangents to the chunk mesh in place. No-op if
/// the asset has been despawned mid-frame or already has UVs.
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
