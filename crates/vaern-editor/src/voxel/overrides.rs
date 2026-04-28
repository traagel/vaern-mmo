//! Per-sub-cell biome override map — what the BiomePaint mode writes,
//! what `compute_blend_weights` consults.
//!
//! ## Granularity
//!
//! Storage is **sub-chunk-XZ**: each chunk's XZ footprint is divided
//! into [`SUB_CELLS_PER_CHUNK`]² square cells. With CHUNK_DIM=32 world
//! units and SUB_CELLS_PER_CHUNK=4, each sub-cell is 8 m on a side.
//! The painted biome is stored per sub-cell.
//!
//! ## Source of truth
//!
//! On Startup, the resource is populated from two layers:
//! 1. **Cartography baseline** — the active zone's `geography.yaml`
//!    biome polygons rasterised in-process via
//!    `vaern_cartography::raster::rasterize_polygon`. Same code path
//!    the (now-retired) `vaern-import-editor` binary used.
//! 2. **Per-zone paint deltas** — `world/zones/<zone>/biome_edits.bin`
//!    via `vaern_cartography::edits::load_biome_edits`. Sparse —
//!    typically empty until the brush is used.
//!
//! On save, the same cartography rasterisation is re-run and the
//! current `BiomeOverrideMap` is diffed against it. Only differing
//! cells (= true brush paint) are written back to `biome_edits.bin`.
//! Empty diff → file is deleted.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use vaern_cartography::edits::{load_biome_edits, save_biome_edits};
use vaern_cartography::raster::{rasterize_polygon, rasterize_road_strip};
use vaern_data::{load_all_geography, load_world_layout, Coord2};
use vaern_voxel::config::CHUNK_WORLD_SIZE;

use super::biomes::BiomeKey;
use crate::state::EditorContext;

/// Sub-cells per chunk axis. 4 = 8 m per cell (CHUNK_WORLD_SIZE / 4).
pub const SUB_CELLS_PER_CHUNK: u32 = 4;

/// World-space side length of one sub-cell.
pub const SUB_CELL_SIZE_M: f32 = CHUNK_WORLD_SIZE / SUB_CELLS_PER_CHUNK as f32;

/// Live biome overrides keyed by (sub_cell_x, sub_cell_z). Sub-cell
/// coordinates are integer indices on a global grid: chunk (cx, cz)
/// owns sub-cells `[cx*N..cx*N+N, cz*N..cz*N+N)` where N =
/// `SUB_CELLS_PER_CHUNK`.
#[derive(Resource, Default, Debug, Clone)]
pub struct BiomeOverrideMap {
    pub by_sub: HashMap<(i32, i32), BiomeKey>,
}

impl BiomeOverrideMap {
    pub fn get(&self, sub_x: i32, sub_z: i32) -> Option<BiomeKey> {
        self.by_sub.get(&(sub_x, sub_z)).copied()
    }

    pub fn set(&mut self, sub_x: i32, sub_z: i32, biome: BiomeKey) {
        self.by_sub.insert((sub_x, sub_z), biome);
    }

    pub fn clear(&mut self, sub_x: i32, sub_z: i32) {
        self.by_sub.remove(&(sub_x, sub_z));
    }

    pub fn len(&self) -> usize {
        self.by_sub.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_sub.is_empty()
    }

    pub fn world_to_sub(world_x: f32, world_z: f32) -> (i32, i32) {
        (
            (world_x / SUB_CELL_SIZE_M).floor() as i32,
            (world_z / SUB_CELL_SIZE_M).floor() as i32,
        )
    }

    pub fn sub_cell_center(sub_x: i32, sub_z: i32) -> (f32, f32) {
        (
            (sub_x as f32 + 0.5) * SUB_CELL_SIZE_M,
            (sub_z as f32 + 0.5) * SUB_CELL_SIZE_M,
        )
    }
}

/// Workspace world root.
fn world_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src/generated/world")
}

/// Map cartography biome name → editor BiomeKey id. Mirrors the
/// (now-retired) `vaern-import-editor::biome_id_for` table — kept here
/// as the editor-side authority. If a cartography biome key is added
/// upstream, add a row here too.
fn biome_id_for(name: &str) -> u8 {
    match name {
        "grass" | "fields" | "river_valley" | "pasture" | "sand" => 0,
        "grass_lush" | "highland" => 1,
        "mossy" | "forest" | "temperate_forest" => 2,
        "dirt" | "ruin" | "cropland" | "tilled_soil" => 3,
        "snow" => 4,
        "stone" | "cobblestone" => 5,
        "scorched" | "ashland" => 6,
        "marsh" | "marshland" | "mud" => 7,
        "rocky" | "mountain" | "mountain_rock" | "coastal_cliff" | "fjord" | "ridge_scrub" => 8,
        _ => 0,
    }
}

/// Map cartography road type → editor BiomeKey id. Roads are painted
/// directly into the biome map so the existing voxel-ground PBR
/// pipeline renders them as a strip of cobble / dirt — no separate
/// mesh, no Z-fighting, no terrain-floating.
fn road_biome_id_for(road_type: &str) -> u8 {
    match road_type {
        "kingsroad" | "highway" => 5, // Stone (cobblestone)
        // Tracks + dirt paths + spurs all fall back to the Dirt biome.
        // The blend pipeline ships only one dirt texture today;
        // distinguishing kingsroad / track by width is enough at the
        // editor's draw distance.
        _ => 3, // Dirt
    }
}

/// Per-type road strip width in metres. Mirrors the legacy ribbon
/// `RoadStyle.width_m` table that's been removed from the cartography
/// overlay.
fn road_width_m(road_type: &str) -> f32 {
    match road_type {
        "kingsroad" | "highway" => 6.0,
        "track" | "trade_road" => 4.0,
        "dirt_path" | "path" => 2.5,
        _ => 3.0,
    }
}

/// Re-rasterize the active zone's cartography biome polygons into a
/// fresh `(sub_x, sub_z) → BiomeKey` map. Same algorithm the legacy
/// importer used: backdrop polygons (`*_main`) first, then pockets
/// paint over them; point-in-polygon test at each sub-cell centre.
///
/// Used both at load time (build the resource) and at save time
/// (diff against to find paint deltas).
fn rasterize_active_zone_baseline(zone_id: &str) -> HashMap<(i32, i32), BiomeKey> {
    let root = world_root();
    let layout = load_world_layout(&root).unwrap_or_default();
    let geography = match load_all_geography(&root) {
        Ok(g) => g,
        Err(_) => return HashMap::new(),
    };
    let Some(geo) = geography.get(zone_id) else {
        return HashMap::new();
    };
    let Some(origin) = layout.zone_origin(zone_id) else {
        return HashMap::new();
    };

    // Process backdrop first so pockets paint over it.
    let mut regions: Vec<&vaern_data::BiomeRegion> = geo.biome_regions.iter().collect();
    regions.sort_by_key(|r| if r.id.ends_with("_main") { 0 } else { 1 });

    let mut raw: HashMap<(i32, i32), u8> = HashMap::new();
    for region in regions {
        let world_polygon: Vec<Coord2> = region
            .polygon
            .points
            .iter()
            .map(|p| Coord2::new(p.x + origin.x, p.z + origin.z))
            .collect();
        let id = biome_id_for(&region.biome);
        rasterize_polygon(&world_polygon, id, &mut raw);
    }

    // Roads paint over biome polygons — kingsroad becomes a strip of
    // cobblestone, tracks + dirt paths become dirt. Sorted by id so
    // overlapping segments resolve deterministically (last-paint
    // wins, but the same input order produces the same bytes).
    let mut roads: Vec<&vaern_data::Road> = geo.roads.iter().collect();
    roads.sort_by_key(|r| r.id.clone());
    for road in roads {
        if road.path.points.len() < 2 {
            continue;
        }
        let world_path: Vec<Coord2> = road
            .path
            .points
            .iter()
            .map(|p| Coord2::new(p.x + origin.x, p.z + origin.z))
            .collect();
        let id = road_biome_id_for(&road.type_);
        let width = road_width_m(&road.type_);
        rasterize_road_strip(&world_path, width, id, &mut raw);
    }

    raw.into_iter()
        .filter_map(|(xz, id)| BiomeKey::from_id(id).map(|b| (xz, b)))
        .collect()
}

/// Bevy system: build `BiomeOverrideMap` from cartography polygons +
/// per-zone paint deltas. Runs on `OnEnter(EditorAppState::Editing)`
/// because it needs `EditorContext.active_zone` (set during Startup
/// by `seed_context_from_boot`).
pub fn load_biome_overrides_into_resource(
    ctx: Res<EditorContext>,
    mut overrides: ResMut<BiomeOverrideMap>,
    mut log: ResMut<crate::ui::console::ConsoleLog>,
) {
    let zone_id = ctx.active_zone.clone();
    let baseline = rasterize_active_zone_baseline(&zone_id);
    let baseline_count = baseline.len();
    overrides.by_sub = baseline;

    // Apply per-zone paint deltas on top.
    let edits = load_biome_edits(&world_root(), &zone_id);
    let edits_count = edits.len();
    for ((x, z), id) in edits {
        if let Some(b) = BiomeKey::from_id(id) {
            overrides.by_sub.insert((x, z), b);
        }
    }

    log.push(format!(
        "biome overrides: {baseline_count} cartography cells + {edits_count} paint deltas (zone {zone_id})"
    ));
}

/// Save the brush's paint deltas. Re-rasterises the cartography
/// baseline, diffs against the current resource, writes only differing
/// cells to `world/zones/<zone>/biome_edits.bin`. Empty diff → file is
/// deleted.
///
/// Returns the count of paint cells written (zero is fine).
pub fn save_biome_overrides_for_zone(
    zone_id: &str,
    overrides: &BiomeOverrideMap,
) -> std::io::Result<usize> {
    let baseline = rasterize_active_zone_baseline(zone_id);
    let mut paint: HashMap<(i32, i32), u8> = HashMap::new();
    for (&cell, &biome) in &overrides.by_sub {
        match baseline.get(&cell) {
            Some(&b) if b == biome => {} // matches baseline → cartography, drop
            _ => {
                paint.insert(cell, biome.id());
            }
        }
    }
    save_biome_edits(&world_root(), zone_id, &paint)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_round_trips_for_every_biome() {
        for b in BiomeKey::ALL {
            let id = b.id();
            assert_eq!(BiomeKey::from_id(id), Some(b));
        }
    }

    #[test]
    fn unknown_id_returns_none() {
        assert!(BiomeKey::from_id(255).is_none());
    }

    #[test]
    fn world_to_sub_round_trip_at_centers() {
        let (cx, cz) = BiomeOverrideMap::sub_cell_center(3, 5);
        assert_eq!(cx, 3.5 * SUB_CELL_SIZE_M);
        assert_eq!(cz, 5.5 * SUB_CELL_SIZE_M);
        assert_eq!(BiomeOverrideMap::world_to_sub(cx, cz), (3, 5));
    }

    #[test]
    fn biome_id_for_table_covers_known_keys() {
        assert_eq!(biome_id_for("fields"), 0);
        assert_eq!(biome_id_for("forest"), 2);
        assert_eq!(biome_id_for("highland"), 1);
        assert_eq!(biome_id_for("mountain"), 8);
        assert_eq!(biome_id_for("marsh"), 7);
        assert_eq!(biome_id_for("snow"), 4);
        assert_eq!(biome_id_for("unknown_biome"), 0);
    }
}
