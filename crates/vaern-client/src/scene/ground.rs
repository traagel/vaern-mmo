//! Visible ground: a tessellated plane with gentle procedural
//! displacement, textured with the ambientCG Grass002 PBR set.
//!
//! The plane is `GROUND_SIZE` wide and subdivided into
//! `GROUND_RESOLUTION × GROUND_RESOLUTION` cells. Per-vertex Y is
//! displaced by a small multi-octave sine/cosine field — no external
//! noise dependency — to break the flat-plate look. Max amplitude is
//! kept under ~2u so the server-authoritative player position (which
//! lives on Y=0) doesn't visibly detach from the terrain.
//!
//! UVs are set to world-space `(x, z)` so a single
//! `uv_transform` scale of `1/8` produces one texture tile per 8m
//! uniformly, regardless of mesh subdivision.

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::math::Affine2;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use vaern_core::terrain;

use crate::menu::AppState;
use crate::shared::GameWorld;

// --- tuning -----------------------------------------------------------------

/// Side length of the ground plane. Zones sit on a 2800u ring with
/// another ~400u of content extending outward — outermost mob can
/// land around world coord ±3200u. 8000u ground (±4000u) covers every
/// zone with ~800u of skirt past the furthest spawn.
const GROUND_SIZE: f32 = 8000.0;

/// Cells per axis. 320 → 321×321 ≈ 103k verts / 205k tris at 25u cell
/// size — the highest-frequency terrain octave (period ~70u) still
/// samples at ~2.8 cells per cycle, fine with smoothed normals. Keeps
/// the mesh well under a million triangles.
const GROUND_RESOLUTION: u32 = 320;

/// World units per texture tile. One tile of grass = 8m so the
/// player isn't staring at a single smeared mega-pixel.
const TEXTURE_TILE_M: f32 = 8.0;

// --- plugin -----------------------------------------------------------------

pub struct GroundPlugin;

impl Plugin for GroundPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), setup_ground);
    }
}

// --- systems ----------------------------------------------------------------

fn setup_ground(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    assets: Res<AssetServer>,
) {
    // ambientCG Grass002 PBR set. Scalar roughness on purpose —
    // `metallic_roughness_texture` reads metallic from the B channel,
    // and a grayscale roughness JPG would inject bogus metallic.
    //
    // Bevy's default image sampler uses ClampToEdge, which — combined
    // with our large UVs (world-meters pre-uv_transform) — stretches
    // one edge pixel across the whole plane and reads as flat green.
    // Force Repeat on both axes so the texture actually tiles.
    let repeat = |s: &mut ImageLoaderSettings| {
        s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::Repeat,
            address_mode_v: ImageAddressMode::Repeat,
            address_mode_w: ImageAddressMode::Repeat,
            ..ImageSamplerDescriptor::linear()
        });
    };
    let color = assets.load_with_settings(
        "extracted/terrain/grass002/Grass002_2K-JPG_Color.jpg",
        repeat,
    );
    let normal = assets.load_with_settings(
        "extracted/terrain/grass002/Grass002_2K-JPG_NormalGL.jpg",
        repeat,
    );
    let ao = assets.load_with_settings(
        "extracted/terrain/grass002/Grass002_2K-JPG_AmbientOcclusion.jpg",
        repeat,
    );

    commands.spawn((
        Mesh3d(meshes.add(build_terrain_mesh(GROUND_SIZE, GROUND_RESOLUTION))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color_texture: Some(color),
            normal_map_texture: Some(normal),
            occlusion_texture: Some(ao),
            perceptual_roughness: 0.95,
            metallic: 0.0,
            // UVs are stored in world-space meters. Scaling by 1/8
            // gives one texture tile every 8m in any direction.
            uv_transform: Affine2::from_scale(Vec2::splat(1.0 / TEXTURE_TILE_M)),
            ..default()
        })),
        Transform::IDENTITY,
        GameWorld,
    ));
}

fn build_terrain_mesh(size: f32, resolution: u32) -> Mesh {
    let verts_per_axis = resolution + 1;
    let step = size / resolution as f32;
    let half = size * 0.5;

    let mut positions: Vec<[f32; 3]> =
        Vec::with_capacity((verts_per_axis * verts_per_axis) as usize);
    let mut uvs: Vec<[f32; 2]> =
        Vec::with_capacity((verts_per_axis * verts_per_axis) as usize);

    for z in 0..verts_per_axis {
        for x in 0..verts_per_axis {
            let xw = x as f32 * step - half;
            let zw = z as f32 * step - half;
            let yw = terrain::height(xw, zw);
            positions.push([xw, yw, zw]);
            // UVs in world meters; material's uv_transform scales
            // them into tile-space.
            uvs.push([xw, zw]);
        }
    }

    let mut indices: Vec<u32> =
        Vec::with_capacity((resolution * resolution * 6) as usize);
    for z in 0..resolution {
        for x in 0..resolution {
            let tl = z * verts_per_axis + x;
            let tr = tl + 1;
            let bl = tl + verts_per_axis;
            let br = bl + 1;
            indices.extend_from_slice(&[tl, bl, br, tl, br, tr]);
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    // Smooth normals from the (now displaced) positions — averages at
    // shared vertices so the hills read rounded rather than faceted.
    mesh.compute_normals();
    mesh
}
