//! World dressing: authored hub props + biome-rule scatter.
//!
//! On entering `AppState::InGame`, loads the zone / hub YAML under
//! `src/generated/world/`, walks the `props:` on each hub and the
//! `scatter:` on each zone, and spawns Poly Haven GLB instances into
//! the live scene tagged with `GameWorld` for teardown.
//!
//! Scatter placement is deterministic: seeded by a hash of
//! `(zone_id, rule.seed_salt, rule.biome, rule.category)` so every
//! client sees the same world without per-prop replication.
//!
//! Y height is sampled from `vaern_core::terrain::height`, matching
//! what the server uses for NPC spawn Y-snap.
//!
//! Collision is deferred — today the player walks through every
//! prop. Added to post-slice-1 cleanup.

use std::path::Path;

use bevy::prelude::*;

use vaern_assets::{PolyHavenCatalog, PolyHavenCategory, PolyHavenEntry};
use vaern_core::terrain;
use vaern_data::{load_world, ScatterRule, World, Zone};

use crate::menu::AppState;
use crate::shared::GameWorld;

/// Ring radius for the zone layout. Mirrors
/// `vaern_server::data::load_game_data` + `voxel_biomes` — must stay in
/// sync; if the server changes, props paint in the wrong place.
const ZONE_RING_RADIUS: f32 = 2800.0;
/// Half-extent of each zone's playable box (meters). 1200u per the
/// Dalewatch redesign doc. Scatter is confined to this AABB around each
/// zone origin to keep total prop count bounded.
const ZONE_FOOTPRINT_HALF: f32 = 600.0;
/// Above this camera distance, dressing entities are hidden. Keeps
/// draw-call count manageable when thousands of props are in the scene.
const DRESSING_VIEW_RANGE: f32 = 250.0;
const DRESSING_VIEW_RANGE_SQ: f32 = DRESSING_VIEW_RANGE * DRESSING_VIEW_RANGE;
/// Safety cap on total scatter spawns per zone. Authored hub props are
/// always spawned regardless. At ~1000 props per zone each client runs
/// fine; much above 2000 and first-launch stutter gets noticeable.
const MAX_SCATTER_PER_ZONE: usize = 1500;

#[derive(Component)]
pub struct Dressing;

pub struct DressingPlugin;

impl Plugin for DressingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_dressing);
        app.add_systems(
            Update,
            dressing_distance_cull.run_if(in_state(AppState::InGame)),
        );
    }
}

fn spawn_dressing(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    catalog: Res<PolyHavenCatalog>,
) {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let world_root = manifest.join("../../src/generated/world");
    let world = match load_world(&world_root) {
        Ok(w) => w,
        Err(e) => {
            warn!("dressing: failed to load world YAML ({e}); skipping");
            return;
        }
    };

    let zone_origins = compute_zone_origins(&world);
    let mut hub_total = 0usize;
    let mut scatter_total = 0usize;

    for zone in &world.zones {
        let Some(&(ox, oz)) = zone_origins.get(zone.id.as_str()) else {
            continue;
        };

        // Authored hub props.
        for hub in world.hubs_in_zone(&zone.id) {
            let Some(off) = hub.offset_from_zone_origin.as_ref() else {
                continue;
            };
            let hub_world_x = ox + off.x;
            let hub_world_z = oz + off.z;
            for prop in &hub.props {
                if let Some(entry) = catalog.get(&prop.slug) {
                    spawn_one(
                        &mut commands,
                        &asset_server,
                        entry,
                        hub_world_x + prop.offset.x,
                        hub_world_z + prop.offset.z,
                        prop.absolute_y,
                        prop.rotation_y_deg,
                        prop.scale,
                    );
                    hub_total += 1;
                } else {
                    warn!(
                        "dressing: hub {} references unknown slug {:?}",
                        hub.id, prop.slug
                    );
                }
            }
        }

        // Zone scatter. Cap per-zone spawn total so a misconfigured
        // density rule can't lock up first-launch.
        let mut zone_scatter_count = 0usize;
        for rule in &zone.scatter {
            if zone_scatter_count >= MAX_SCATTER_PER_ZONE {
                warn!(
                    "dressing: hit MAX_SCATTER_PER_ZONE ({}) in zone {}; \
                     remaining rules skipped",
                    MAX_SCATTER_PER_ZONE, zone.id
                );
                break;
            }
            let Some(category) = category_from_yaml(&rule.category) else {
                warn!(
                    "dressing: unknown scatter category {:?} in zone {}",
                    rule.category, zone.id
                );
                continue;
            };
            let pool: Vec<&PolyHavenEntry> = catalog.by_category(category).collect();
            if pool.is_empty() {
                continue;
            }
            let placements = scatter_placements(&zone.id, ox, oz, rule);
            let mut rng = LcgRng::seed(hash_rule(&zone.id, rule) ^ 0xA5A5_5A5A);
            for (x, z) in placements {
                if zone_scatter_count >= MAX_SCATTER_PER_ZONE {
                    break;
                }
                let entry = pool[(rng.next_u32() as usize) % pool.len()];
                let yaw = (rng.next_u32() as f32 / u32::MAX as f32) * std::f32::consts::TAU;
                spawn_one(
                    &mut commands,
                    &asset_server,
                    entry,
                    x,
                    z,
                    None,
                    yaw.to_degrees(),
                    1.0,
                );
                scatter_total += 1;
                zone_scatter_count += 1;
            }
        }
    }

    info!(
        "dressing: spawned {} hub props + {} scatter instances",
        hub_total, scatter_total
    );
}

fn spawn_one(
    commands: &mut Commands,
    asset_server: &AssetServer,
    entry: &PolyHavenEntry,
    x: f32,
    z: f32,
    absolute_y: Option<f32>,
    rotation_y_deg: f32,
    scale: f32,
) {
    let y = absolute_y.unwrap_or_else(|| terrain::height(x, z));
    let mut transform =
        Transform::from_translation(Vec3::new(x, y, z)).with_scale(Vec3::splat(scale));
    transform.rotation = Quat::from_rotation_y(rotation_y_deg.to_radians());
    commands.spawn((
        SceneRoot(asset_server.load(entry.scene_path())),
        transform,
        Dressing,
        GameWorld,
        Name::new(format!("Dressing:{}", entry.slug)),
    ));
}

fn category_from_yaml(s: &str) -> Option<PolyHavenCategory> {
    match s {
        "tree" => Some(PolyHavenCategory::Tree),
        "dead_wood" => Some(PolyHavenCategory::DeadWood),
        "rock" => Some(PolyHavenCategory::Rock),
        "ground_cover" => Some(PolyHavenCategory::GroundCover),
        "shrub" => Some(PolyHavenCategory::Shrub),
        "hub_prop" => Some(PolyHavenCategory::HubProp),
        "weapon_rack_dressing" => Some(PolyHavenCategory::WeaponRackDressing),
        _ => None,
    }
}

/// Deterministic scatter positions for one rule inside a zone's
/// footprint. Grid-cell Bernoulli sampling: divide the AABB into
/// `min_spacing`-sized cells, decide per-cell whether to place, jitter
/// inside the cell.
fn scatter_placements(
    zone_id: &str,
    origin_x: f32,
    origin_z: f32,
    rule: &ScatterRule,
) -> Vec<(f32, f32)> {
    if rule.density_per_100m2 <= 0.0 || rule.min_spacing <= 0.0 {
        return Vec::new();
    }

    let cell = rule.min_spacing.max(0.5);
    let exclude_r_sq = rule.exclude_radius_from_hubs * rule.exclude_radius_from_hubs;
    // placement probability per cell: density is per 100 m², cell area = cell² m².
    // expected props per cell = density / 100 * cell².
    let p_place = (rule.density_per_100m2 / 100.0) * cell * cell;
    let p_place = p_place.clamp(0.0, 1.0);

    let min = -ZONE_FOOTPRINT_HALF;
    let max = ZONE_FOOTPRINT_HALF;
    let n_cells = ((ZONE_FOOTPRINT_HALF * 2.0) / cell).ceil() as i32;

    let mut rng = LcgRng::seed(hash_rule(zone_id, rule));
    let mut out = Vec::new();

    for iz in 0..n_cells {
        for ix in 0..n_cells {
            let roll = rng.next_u32() as f32 / u32::MAX as f32;
            if roll >= p_place {
                continue;
            }
            let base_x = min + (ix as f32 + 0.5) * cell;
            let base_z = min + (iz as f32 + 0.5) * cell;
            if base_x > max || base_z > max {
                continue;
            }
            let jx = (rng.next_u32() as f32 / u32::MAX as f32 - 0.5) * cell;
            let jz = (rng.next_u32() as f32 / u32::MAX as f32 - 0.5) * cell;
            let local_x = base_x + jx;
            let local_z = base_z + jz;

            // Hub-exclusion in zone-local coords: distance from (0, 0)
            // is a cheap proxy for "distance from the capital hub,"
            // which is always at the zone origin for Dalewatch. Outpost
            // hubs further from center still get props from scatter,
            // which is the correct visual (outposts sit in the wild).
            if exclude_r_sq > 0.0 && (local_x * local_x + local_z * local_z) < exclude_r_sq {
                continue;
            }

            out.push((origin_x + local_x, origin_z + local_z));
        }
    }
    out
}

/// Mirror of `voxel_biomes::compute_zone_origins` — duplicated because
/// that resolver is pub(crate). If either changes, the other must follow.
fn compute_zone_origins(world: &World) -> std::collections::HashMap<String, (f32, f32)> {
    use std::collections::HashMap;
    let mut starters: Vec<&str> = world
        .zones
        .iter()
        .filter_map(|z: &Zone| z.starter_race.as_deref().map(|_| z.id.as_str()))
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

fn hash_rule(zone_id: &str, rule: &ScatterRule) -> u64 {
    // FNV-1a over zone id + biome + category, XOR'd with seed_salt.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in zone_id.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    for b in rule.biome.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    for b in rule.category.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h ^ (rule.seed_salt as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
}

/// Minimal xorshift/LCG hybrid — pure, no deps, deterministic.
struct LcgRng(u64);

impl LcgRng {
    fn seed(s: u64) -> Self {
        Self(if s == 0 { 0x9E37_79B9_7F4A_7C15 } else { s })
    }
    fn next_u32(&mut self) -> u32 {
        // splitmix64
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) >> 32) as u32
    }
}

fn dressing_distance_cull(
    camera_q: Query<&GlobalTransform, With<Camera3d>>,
    mut dressing_q: Query<(&GlobalTransform, &mut Visibility), With<Dressing>>,
) {
    let Ok(cam) = camera_q.single() else {
        return;
    };
    let cam_pos = cam.translation();
    for (xf, mut vis) in &mut dressing_q {
        let p = xf.translation();
        let dx = p.x - cam_pos.x;
        let dz = p.z - cam_pos.z;
        let d2 = dx * dx + dz * dz;
        let want = if d2 < DRESSING_VIEW_RANGE_SQ {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *vis != want {
            *vis = want;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_rule(biome: &str, category: &str, density: f32, min_spacing: f32) -> ScatterRule {
        ScatterRule {
            biome: biome.to_string(),
            category: category.to_string(),
            density_per_100m2: density,
            min_spacing,
            max_slope_deg: 45.0,
            exclude_radius_from_hubs: 0.0,
            seed_salt: 0,
        }
    }

    #[test]
    fn scatter_is_deterministic() {
        let rule = test_rule("river_valley", "tree", 0.2, 4.0);
        let a = scatter_placements("dalewatch_marches", 0.0, 0.0, &rule);
        let b = scatter_placements("dalewatch_marches", 0.0, 0.0, &rule);
        assert_eq!(a, b, "same inputs must produce identical placements");
        assert!(!a.is_empty(), "scatter produced no placements");
    }

    #[test]
    fn scatter_respects_zone_footprint() {
        let rule = test_rule("*", "tree", 0.3, 3.0);
        let placements = scatter_placements("test_zone", 1000.0, 2000.0, &rule);
        for (x, z) in placements {
            assert!((x - 1000.0).abs() <= ZONE_FOOTPRINT_HALF + 3.0);
            assert!((z - 2000.0).abs() <= ZONE_FOOTPRINT_HALF + 3.0);
        }
    }

    #[test]
    fn hub_exclusion_radius_works() {
        let mut rule = test_rule("*", "tree", 1.0, 2.0);
        rule.exclude_radius_from_hubs = 50.0;
        let placements = scatter_placements("test_zone", 0.0, 0.0, &rule);
        for (x, z) in placements {
            assert!(
                x * x + z * z >= 50.0 * 50.0 - 1.0,
                "prop at ({x}, {z}) is inside hub exclusion"
            );
        }
    }

    #[test]
    fn category_yaml_maps_all_variants() {
        assert_eq!(category_from_yaml("tree"), Some(PolyHavenCategory::Tree));
        assert_eq!(category_from_yaml("rock"), Some(PolyHavenCategory::Rock));
        assert_eq!(
            category_from_yaml("ground_cover"),
            Some(PolyHavenCategory::GroundCover)
        );
        assert_eq!(category_from_yaml("unknown"), None);
    }
}
