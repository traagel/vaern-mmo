//! Voronoi-partitioned hub regions + wiggly roads between hubs.
//!
//! At zone entry the client loads the world YAML, computes the same
//! zone-ring layout the server uses, and for each zone with explicit
//! hub offsets:
//!
//! 1. Runs a grid-based nearest-hub partition
//!    ([`vaern_core::voronoi::partition`]) around the hub cluster.
//! 2. Spawns a textured floor-patch mesh per hub from its owned cells,
//!    Y-snapped to the shared terrain height field with a small +0.08u
//!    lift so it doesn't z-fight the base grass plane.
//! 3. For each pair of Voronoi-adjacent hubs, builds a Catmull-Rom road
//!    through the hubs with a jittered midpoint for a wiggly shape,
//!    and spawns a textured ribbon mesh along it (+0.15u lift so it
//!    reads above the hub patches).
//!
//! Biome → texture set is fixed in this file; hub YAMLs declare a
//! `biome: <key>` field. Unknown biomes fall through to `grass`.
//! Missing texture files are logged and rendered without a color
//! texture (tinted fallback) — the TODO is to wire a real asset
//! downloader (ambientCG zip extraction) when we add biomes whose
//! textures aren't already extracted.

use std::collections::HashMap;
use std::path::Path;

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::math::Affine2;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use vaern_core::terrain::ground_surface_y;
use vaern_core::voronoi::{
    catmull_rom, neighbor_pairs, partition, wiggle_midpoints, Bounds2, GridCell, Hub2, Point2,
};
use vaern_data::{load_world, Hub};

use crate::menu::AppState;
use crate::shared::GameWorld;

// ─── tuning ─────────────────────────────────────────────────────────────────

/// Must match `vaern-server::data::load_game_data`'s ring radius.
/// Duplicated here intentionally — client doesn't link the server crate.
const ZONE_RING_RADIUS: f32 = 2800.0;

/// Padding around the hub bounding box, so each hub's Voronoi cell
/// extends well past the hub itself and the biome reads on the ground
/// you walk over, not just at the hub center. Generous — covers
/// Dalewatch's 1200×1200u playable box even though hubs span ~800u.
const BOUNDS_PADDING: f32 = 450.0;

/// Partition grid resolution per zone. Dalewatch is 1200×1200u with
/// hubs spanning ~500u; 32×32 gives ~15u cells which read smooth enough
/// at walking distance without making each hub mesh explode in tri
/// count (~256 quads per hub worst case).
const CELLS_PER_AXIS: u32 = 32;

/// Y lifts — overlay meshes need SOME offset above the base ground
/// to avoid z-fighting, but every centimeter they sit above the
/// terrain is a centimeter that occludes the player's feet (server-
/// snapped to Y = terrain::height). Zones sit ~2800u from world
/// origin, so float-depth precision is tight — 1 cm isn't enough at
/// that distance. 5 cm + a negative `depth_bias` on the overlay
/// materials gives a robust depth-test win without noticeable foot
/// occlusion on a top-down MMO camera.
const HUB_PATCH_LIFT: f32 = 0.05;
const ROAD_LIFT: f32 = 0.08;

/// `StandardMaterial::depth_bias` — pulls the overlay's depth-buffer
/// value toward the camera so it wins the test against the base
/// ground even when the two surfaces are near-coplanar. Value is in
/// depth-buffer units; -1.0 is ~one depth-step closer which is plenty
/// to beat float-precision ties at distance. Road depth_bias is
/// pushed further so it renders over hub patches at entrances.
const HUB_PATCH_DEPTH_BIAS: f32 = -1.0;
const ROAD_DEPTH_BIAS: f32 = -2.0;

/// Width of the road ribbon in world units. Wide enough to read on
/// screen, narrow enough to feel like a path rather than a highway.
const ROAD_WIDTH: f32 = 4.5;

/// Wiggle amplitude for the spline midpoints. ~30u breaks the line
/// without turning the road into a drunken zigzag.
const ROAD_WIGGLE_AMP: f32 = 32.0;

/// Samples per spline segment for the road path. 12 is plenty smooth
/// for a 3-control-point spline; raises the tri count per road to
/// ~24 × 2 = 48 triangles, negligible.
const ROAD_SAMPLES_PER_SEGMENT: usize = 12;

/// World meters per texture tile on the hub floor-patch and road.
/// Matches `ground.rs`'s `TEXTURE_TILE_M` so adjacent biomes don't
/// read at wildly different scales.
const TEXTURE_TILE_M: f32 = 8.0;
const ROAD_TEX_TILE_M: f32 = 4.0;

// ─── plugin ────────────────────────────────────────────────────────────────

pub struct HubRegionsPlugin;

impl Plugin for HubRegionsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_hub_regions);
    }
}

// ─── biome → texture table ─────────────────────────────────────────────────

struct BiomeTextureSet {
    color: &'static str,
    normal: &'static str,
    /// Ambient-occlusion texture. Some ambientCG sets (e.g. Snow004,
    /// PavingStones004) ship without AO — leave `None` and the
    /// StandardMaterial will render without an occlusion map.
    ao: Option<&'static str>,
    /// ambientCG set id. If any of `color` / `normal` / `ao` is missing
    /// from disk, `log_missing_textures` warns the dev to re-run
    /// `scripts/download_biome_textures.sh` (which knows this id).
    #[allow(dead_code)]
    ambientcg_id: &'static str,
}

/// Registry of known biomes. Add a new entry when you add a biome
/// string to a hub YAML; if the texture files aren't extracted yet,
/// run `scripts/download_biome_textures.sh` after adding the biome's
/// ambientCG id to that script's BIOMES table.
fn biome_table(biome: &str) -> &'static BiomeTextureSet {
    match biome {
        "grass_lush" => &BiomeTextureSet {
            color: "extracted/terrain/grass004/Grass004_2K-JPG_Color.jpg",
            normal: "extracted/terrain/grass004/Grass004_2K-JPG_NormalGL.jpg",
            ao: Some("extracted/terrain/grass004/Grass004_2K-JPG_AmbientOcclusion.jpg"),
            ambientcg_id: "Grass004",
        },
        "mossy" => &BiomeTextureSet {
            color: "extracted/terrain/ground037/Ground037_2K-JPG_Color.jpg",
            normal: "extracted/terrain/ground037/Ground037_2K-JPG_NormalGL.jpg",
            ao: Some("extracted/terrain/ground037/Ground037_2K-JPG_AmbientOcclusion.jpg"),
            ambientcg_id: "Ground037",
        },
        "dirt" => &BiomeTextureSet {
            color: "extracted/terrain/ground048/Ground048_2K-JPG_Color.jpg",
            normal: "extracted/terrain/ground048/Ground048_2K-JPG_NormalGL.jpg",
            ao: Some("extracted/terrain/ground048/Ground048_2K-JPG_AmbientOcclusion.jpg"),
            ambientcg_id: "Ground048",
        },
        "snow" => &BiomeTextureSet {
            color: "extracted/terrain/snow/Snow004_2K-JPG_Color.jpg",
            normal: "extracted/terrain/snow/Snow004_2K-JPG_NormalGL.jpg",
            // Snow004 ships Color + Normal only — no AO.
            ao: None,
            ambientcg_id: "Snow004",
        },
        "stone" => &BiomeTextureSet {
            color: "extracted/terrain/stone/PavingStones004_2K-JPG_Color.jpg",
            normal: "extracted/terrain/stone/PavingStones004_2K-JPG_NormalGL.jpg",
            ao: None,
            ambientcg_id: "PavingStones004",
        },
        "scorched" => &BiomeTextureSet {
            color: "extracted/terrain/scorched/Ground063_2K-JPG_Color.jpg",
            normal: "extracted/terrain/scorched/Ground063_2K-JPG_NormalGL.jpg",
            ao: Some("extracted/terrain/scorched/Ground063_2K-JPG_AmbientOcclusion.jpg"),
            ambientcg_id: "Ground063",
        },
        "marsh" => &BiomeTextureSet {
            color: "extracted/terrain/marsh/Ground059_2K-JPG_Color.jpg",
            normal: "extracted/terrain/marsh/Ground059_2K-JPG_NormalGL.jpg",
            ao: Some("extracted/terrain/marsh/Ground059_2K-JPG_AmbientOcclusion.jpg"),
            ambientcg_id: "Ground059",
        },
        "rocky" => &BiomeTextureSet {
            color: "extracted/terrain/rocky/Rocks023_2K-JPG_Color.jpg",
            normal: "extracted/terrain/rocky/Rocks023_2K-JPG_NormalGL.jpg",
            ao: Some("extracted/terrain/rocky/Rocks023_2K-JPG_AmbientOcclusion.jpg"),
            ambientcg_id: "Rocks023",
        },
        _ => &BiomeTextureSet {
            // "grass" + fallback for unknown keys.
            color: "extracted/terrain/grass002/Grass002_2K-JPG_Color.jpg",
            normal: "extracted/terrain/grass002/Grass002_2K-JPG_NormalGL.jpg",
            ao: Some("extracted/terrain/grass002/Grass002_2K-JPG_AmbientOcclusion.jpg"),
            ambientcg_id: "Grass002",
        },
    }
}

/// Road uses the same `Ground048` "dirt path variation" set the `dirt`
/// biome uses, but at a tighter tile scale so the path reads as a
/// narrow strip of dirt rather than a wide field.
const ROAD_TEXTURE: &BiomeTextureSet = &BiomeTextureSet {
    color: "extracted/terrain/ground048/Ground048_2K-JPG_Color.jpg",
    normal: "extracted/terrain/ground048/Ground048_2K-JPG_NormalGL.jpg",
    ao: Some("extracted/terrain/ground048/Ground048_2K-JPG_AmbientOcclusion.jpg"),
    ambientcg_id: "Ground048",
};

/// Log-and-continue check: if any of the biome's files are missing
/// from `assets/`, warn the dev to re-run the download script.
fn log_missing_textures(set: &BiomeTextureSet, assets_root: &Path) {
    let mut missing = Vec::new();
    for p in [Some(set.color), Some(set.normal), set.ao].into_iter().flatten() {
        let full = assets_root.join(p);
        if !full.exists() {
            missing.push(full);
        }
    }
    if !missing.is_empty() {
        warn!(
            "biome texture(s) missing for '{}': {} — run scripts/download_biome_textures.sh",
            set.ambientcg_id,
            missing
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
}

// ─── main system ────────────────────────────────────────────────────────────

fn spawn_hub_regions(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    assets: Res<AssetServer>,
) {
    // Mirror vaern-server::data::data_root.
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let assets_root = manifest.join("../../assets");
    let world_root = manifest.join("../../src/generated/world");

    let world = match load_world(&world_root) {
        Ok(w) => w,
        Err(e) => {
            warn!("hub_regions: failed to load world ({e}); skipping");
            return;
        }
    };

    let zone_origins = compute_zone_origins(&world);

    // Cached image-loader settings: terrain tiles, so force Repeat on
    // both axes. Matches ground.rs.
    let repeat_settings = |s: &mut ImageLoaderSettings| {
        s.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::Repeat,
            address_mode_v: ImageAddressMode::Repeat,
            address_mode_w: ImageAddressMode::Repeat,
            ..ImageSamplerDescriptor::linear()
        });
    };

    let mut zones_rendered = 0;
    let mut total_cells = 0;
    let mut total_roads = 0;

    for zone in &world.zones {
        // Collect hubs with explicit offsets — legacy radial-layout
        // hubs (no offset) skip this pipeline and render only as the
        // base grass plane, as before.
        let hubs_yaml: Vec<&Hub> = world
            .hubs_in_zone(&zone.id)
            .filter(|h| h.offset_from_zone_origin.is_some())
            .collect();
        if hubs_yaml.len() < 2 {
            continue;
        }

        let origin = zone_origins
            .get(&zone.id)
            .copied()
            .unwrap_or((0.0, 0.0));

        // Build Hub2 list in world XZ coords.
        let hubs2: Vec<Hub2> = hubs_yaml
            .iter()
            .map(|h| {
                let o = h.offset_from_zone_origin.as_ref().unwrap();
                Hub2 {
                    id: h.id.clone(),
                    pos: Point2::new(origin.0 + o.x, origin.1 + o.z),
                }
            })
            .collect();

        let bounds = Bounds2::around_hubs(&hubs2, BOUNDS_PADDING);
        let cells = partition(&hubs2, bounds, CELLS_PER_AXIS);

        // Per-hub floor-patch mesh. Skip hubs that ended up with zero
        // cells (shouldn't happen with reasonable padding but guard
        // anyway).
        for (hub_idx, hub_yaml) in hubs_yaml.iter().enumerate() {
            let owned: Vec<GridCell> = cells
                .iter()
                .filter(|c| c.owner == hub_idx)
                .copied()
                .collect();
            if owned.is_empty() {
                continue;
            }
            total_cells += owned.len();

            let tex = biome_table(&hub_yaml.biome);
            log_missing_textures(tex, &assets_root);

            let color = assets.load_with_settings(tex.color, repeat_settings);
            let normal = assets.load_with_settings(tex.normal, repeat_settings);
            let ao = tex
                .ao
                .map(|p| assets.load_with_settings(p, repeat_settings));

            let mesh = build_hub_patch_mesh(&owned);
            commands.spawn((
                Name::new(format!("hub-patch-{}", hub_yaml.id)),
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color_texture: Some(color),
                    normal_map_texture: Some(normal),
                    occlusion_texture: ao,
                    perceptual_roughness: 0.95,
                    metallic: 0.0,
                    uv_transform: Affine2::from_scale(Vec2::splat(1.0 / TEXTURE_TILE_M)),
                    depth_bias: HUB_PATCH_DEPTH_BIAS,
                    ..default()
                })),
                Transform::IDENTITY,
                GameWorld,
            ));
        }

        // Roads between Voronoi-adjacent hub pairs.
        let pairs = neighbor_pairs(&cells, CELLS_PER_AXIS);

        let road_color =
            assets.load_with_settings(ROAD_TEXTURE.color, repeat_settings);
        let road_normal =
            assets.load_with_settings(ROAD_TEXTURE.normal, repeat_settings);
        let road_ao = ROAD_TEXTURE
            .ao
            .map(|p| assets.load_with_settings(p, repeat_settings));
        let road_material = materials.add(StandardMaterial {
            base_color_texture: Some(road_color),
            normal_map_texture: Some(road_normal),
            occlusion_texture: road_ao,
            perceptual_roughness: 0.95,
            metallic: 0.0,
            uv_transform: Affine2::from_scale(Vec2::splat(1.0 / ROAD_TEX_TILE_M)),
            depth_bias: ROAD_DEPTH_BIAS,
            ..default()
        });

        // Deterministic per-zone seed so the same zone wiggles the
        // same way across runs — easier to memorize for playtesters.
        let seed: u64 = zone
            .id
            .bytes()
            .fold(0xCBF29CE484222325u64, |h, b| h.wrapping_mul(0x100000001B3).wrapping_add(b as u64));

        for (a_idx, b_idx) in pairs {
            let a = hubs2[a_idx].pos;
            let b = hubs2[b_idx].pos;
            let mid = a.lerp(b, 0.5);
            let ctrl = vec![a, mid, b];
            let wiggled = wiggle_midpoints(&ctrl, ROAD_WIGGLE_AMP, seed.wrapping_add(a_idx as u64 * 131 + b_idx as u64));
            let path = catmull_rom(&wiggled, ROAD_SAMPLES_PER_SEGMENT);

            if path.len() < 2 {
                continue;
            }
            total_roads += 1;

            let mesh = build_road_ribbon_mesh(&path, ROAD_WIDTH);
            commands.spawn((
                Name::new(format!(
                    "road-{}-{}",
                    hubs_yaml[a_idx].id, hubs_yaml[b_idx].id
                )),
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(road_material.clone()),
                Transform::IDENTITY,
                GameWorld,
            ));
        }

        zones_rendered += 1;
    }

    info!(
        "hub_regions: rendered {} zones — {} patch cells, {} roads",
        zones_rendered, total_cells, total_roads
    );
}

// ─── zone origin computation (mirrors server) ──────────────────────────────

/// Mirror of `vaern-server::data::load_game_data`'s zone-ring logic.
/// Returns zone_id → (x, z). Must stay in sync with the server's
/// formula or client biomes/roads will paint in the wrong place.
fn compute_zone_origins(world: &vaern_data::World) -> HashMap<String, (f32, f32)> {
    let mut starters: Vec<&str> = world
        .zones
        .iter()
        .filter_map(|z| z.starter_race.as_deref().map(|_| z.id.as_str()))
        .collect();
    starters.sort();
    let n = starters.len().max(1) as f32;
    let mut out = HashMap::new();
    for (i, zid) in starters.iter().enumerate() {
        let angle = (i as f32 / n) * std::f32::consts::TAU;
        out.insert(
            zid.to_string(),
            (ZONE_RING_RADIUS * angle.cos(), ZONE_RING_RADIUS * angle.sin()),
        );
    }
    out
}

// ─── mesh builders ──────────────────────────────────────────────────────────

fn build_hub_patch_mesh(cells: &[GridCell]) -> Mesh {
    // Two triangles per cell quad, Y-snapped to the shared terrain so
    // the patch hugs the hills under it. Duplicated-vertex strategy
    // keeps the builder trivial; a shared-vertex indexed version would
    // halve memory but the total cell count stays small.
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(cells.len() * 4);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(cells.len() * 4);
    let mut indices: Vec<u32> = Vec::with_capacity(cells.len() * 6);

    for cell in cells {
        // Cell is cell.width × cell.height — NOT necessarily square.
        // When hub-cluster bounds are non-square (e.g. Dalewatch's
        // 850×500u box), using min(w,h) leaves gaps between cells on
        // the longer axis. Use each dimension separately to fill the
        // full cell footprint.
        let hx = cell.width * 0.5;
        let hz = cell.height * 0.5;
        let (cx, cz) = (cell.center.x, cell.center.z);
        // Corners laid out NW, NE, SE, SW (indices 0..3).
        let corners = [
            (cx - hx, cz - hz), // 0 NW
            (cx + hx, cz - hz), // 1 NE
            (cx + hx, cz + hz), // 2 SE
            (cx - hx, cz + hz), // 3 SW
        ];
        let base = positions.len() as u32;
        for (x, z) in corners {
            // ground_surface_y (not height!) — matches the rendered
            // ground's triangle interp exactly so the patch hugs the
            // visible surface instead of the analytical smooth field.
            let y = ground_surface_y(x, z) + HUB_PATCH_LIFT;
            positions.push([x, y, z]);
            // World-meter UVs; material's uv_transform handles tiling.
            uvs.push([x, z]);
        }
        // Winding that produces +Y face normals (patch faces UP):
        //   Triangle 1: NW, SW, SE — (B−A) × (C−A) = (0, +4h², 0)
        //   Triangle 2: NW, SE, NE — same sign
        // Matches ground.rs's style. The prior NW→NE→SE order gave
        // −Y normals, so Bevy's default back-face cull hid the patch
        // from any above-ground camera.
        indices.extend_from_slice(&[base, base + 3, base + 2, base, base + 2, base + 1]);
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh.compute_normals();
    mesh
}

fn build_road_ribbon_mesh(path: &[Point2], width: f32) -> Mesh {
    let half = width * 0.5;
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(path.len() * 2);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(path.len() * 2);

    let mut along = 0.0_f32;

    for (i, p) in path.iter().enumerate() {
        // Tangent: average of incoming/outgoing segment direction for
        // smooth perpendicular at the joint. Clamp at the endpoints.
        let (tx, tz) = if i == 0 {
            (path[1].x - p.x, path[1].z - p.z)
        } else if i == path.len() - 1 {
            (p.x - path[i - 1].x, p.z - path[i - 1].z)
        } else {
            (
                path[i + 1].x - path[i - 1].x,
                path[i + 1].z - path[i - 1].z,
            )
        };
        let len = (tx * tx + tz * tz).sqrt().max(1e-4);
        // Perpendicular in the XZ plane: rotate 90° CCW. (-z, +x)/len.
        let px = -tz / len;
        let pz = tx / len;

        if i > 0 {
            let prev = path[i - 1];
            let seg = ((p.x - prev.x).powi(2) + (p.z - prev.z).powi(2)).sqrt();
            along += seg;
        }

        let lx = p.x + px * half;
        let lz = p.z + pz * half;
        let rx = p.x - px * half;
        let rz = p.z - pz * half;

        // Y-snap each ribbon vertex to the rendered ground surface so
        // the road hugs the visible terrain (not the smooth analytical
        // height field). Lift above hub-patch by ROAD_LIFT - HUB_PATCH_LIFT.
        let ly = ground_surface_y(lx, lz) + ROAD_LIFT;
        let ry = ground_surface_y(rx, rz) + ROAD_LIFT;

        positions.push([lx, ly, lz]);
        positions.push([rx, ry, rz]);
        // U: 0 on the left edge, 1 on the right. V: distance along
        // the path in world meters; material uv_transform scales to
        // one tile per 4m.
        uvs.push([0.0, along]);
        uvs.push([1.0, along]);
    }

    let mut indices: Vec<u32> = Vec::with_capacity((path.len() - 1) * 6);
    for i in 0..(path.len() - 1) as u32 {
        let tl = i * 2;
        let tr = tl + 1;
        let bl = tl + 2;
        let br = tl + 3;
        // Wind so normals face +Y (up). Mesh::compute_normals averages
        // from position winding so getting this right matters.
        indices.extend_from_slice(&[tl, bl, tr, tr, bl, br]);
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh.compute_normals();
    mesh
}
