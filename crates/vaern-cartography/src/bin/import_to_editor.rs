//! `cargo run -p vaern-cartography --bin vaern-import-editor -- [--zone <id>] [--clean]`
//!
//! Rasterizes the cartography `geography.yaml::biome_regions` polygons
//! into the editor's `biome_overrides.bin` file. The editor's biome
//! paint pipeline reads the same file on Startup, so after this runs
//! the voxel ground renders with the cartography vision: forest pockets
//! at Thornroot Grove, marsh patches at Reed-Brake / Blackwash Fens,
//! ridge-rocky at Drifter's Lair, fields backdrop everywhere else.
//!
//! ## Mapping
//!
//! Cartography biome strings → editor `BiomeKey` collapse table (kept
//! in sync with `vaern-editor/src/voxel/biomes.rs::from_yaml` and the
//! mirror in `vaern-client/src/voxel_biomes.rs`):
//!
//! | cartography           | BiomeKey | id |
//! |-----------------------|----------|----|
//! | grass / fields / river_valley / pasture / sand | Grass     | 0 |
//! | grass_lush / highland | GrassLush | 1 |
//! | mossy / forest / temperate_forest | Mossy | 2 |
//! | dirt / ruin / cropland / tilled_soil | Dirt | 3 |
//! | snow                  | Snow      | 4 |
//! | stone / cobblestone   | Stone     | 5 |
//! | scorched / ashland    | Scorched  | 6 |
//! | marsh / marshland / mud | Marsh   | 7 |
//! | rocky / mountain / mountain_rock / coastal_cliff / fjord / ridge_scrub | Rocky | 8 |
//!
//! The 9-slot collapse is required because the editor's blend pipeline
//! is hardcoded to 9 biomes (see `voxel/biomes.rs` doc comment).
//!
//! ## Sub-cell rasterization
//!
//! Each chunk's XZ footprint is divided into `SUB_CELLS_PER_CHUNK² = 16`
//! cells. With `CHUNK_WORLD_SIZE = 32` and `SUB_CELLS_PER_CHUNK = 4`,
//! each cell is **8 m on a side**. We rasterize each cartography
//! polygon by walking the AABB sub-cell range and testing each
//! sub-cell's center via `point_in_polygon`.
//!
//! ## File format
//!
//! Writes `OverridesFileV2` (matches `vaern-editor::voxel::overrides`
//! exactly). Bincode-encoded enum wrapper so the editor's loader reads
//! it identically. Existing entries on disk are merged with the new
//! ones — user paint deltas on sub-cells the cartography didn't touch
//! are preserved unless `--clean` is passed.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::ExitCode,
};

use serde::{Deserialize, Serialize};
use vaern_data::{
    load_all_geography, load_world, load_world_layout, point_in_polygon, Coord2, Geography,
};

const SUB_CELL_SIZE_M: f32 = 8.0;
/// Distance (m) at which a river's carve depth falls to 0 from the
/// polyline. Inside this band the depth tapers linearly. The band
/// half-width is taken from `river.width_units * 0.5 + RIVER_BANK_M`.
const RIVER_BANK_M: f32 = 6.0;
/// Center-of-channel depth in meters below the surrounding biome.
const RIVER_DEPTH_M: f32 = 3.0;

/// Mirror of `vaern-editor::voxel::overrides::OverridesFileV1`. Kept
/// here only so we can read existing files for merge.
#[derive(Debug, Default, Serialize, Deserialize)]
struct OverridesFileV1 {
    entries: Vec<((i32, i32), u8)>,
}

/// Mirror of `vaern-editor::voxel::overrides::OverridesFileV2`.
#[derive(Debug, Default, Serialize, Deserialize)]
struct OverridesFileV2 {
    sub_cells_per_chunk: u32,
    entries: Vec<((i32, i32), u8)>,
}

#[derive(Debug, Serialize, Deserialize)]
enum OverridesFile {
    V1(OverridesFileV1),
    V2(OverridesFileV2),
}

/// On-disk elevation overlay format. Mirror of
/// `vaern-editor::voxel::elevation::ElevationFileV1` (kept in sync
/// here so the editor's loader reads it identically).
///
/// Per sub-cell, an `i16` height offset in centimeters. Range
/// ±327.67 m per cell — easily covers the +30m mountains / -3m river
/// channels we need today.
#[derive(Debug, Default, Serialize, Deserialize)]
struct ElevationFileV1 {
    sub_cells_per_chunk: u32,
    /// `(sub_x, sub_z) -> centimeters`. Sorted by key for byte-determinism.
    entries: Vec<((i32, i32), i16)>,
}

#[derive(Debug, Serialize, Deserialize)]
enum ElevationFile {
    V1(ElevationFileV1),
}

const SUB_CELLS_PER_CHUNK: u32 = 4;

/// Per-biome resting elevation in meters above the global Y=0 baseline.
/// Cartography polygons rasterized as biome regions stamp this height
/// into every sub-cell they cover. Rivers carve over this. Hard
/// transitions at polygon edges are accepted for v1 — the user can
/// hand-smooth with the existing voxel brush.
fn biome_height_m(name: &str) -> f32 {
    match name {
        "mountain" | "mountain_rock" => 30.0,
        "highland" => 12.0,
        "ridge_scrub" => 7.0,
        "coastal_cliff" | "fjord" => 15.0,
        "ashland" => 4.0,    // gentle uplift around lairs
        "marsh" | "marshland" => -1.5,
        "ruin" => 1.0,
        // fields, river_valley, forest, temperate_forest, grass,
        // grass_lush, dirt, snow, stone, sand, cropland, pasture,
        // cobblestone, tilled_soil, mud → flat (0)
        _ => 0.0,
    }
}

/// Cartography biome string → editor BiomeKey id (0..=8). Kept in
/// sync with `vaern-editor::voxel::biomes::BiomeKey::from_yaml`. If
/// you add a cartography biome key, add a row here too.
fn biome_id_for(name: &str) -> u8 {
    match name {
        "grass" | "fields" | "river_valley" | "pasture" | "sand" => 0, // Grass
        "grass_lush" | "highland" => 1,                                 // GrassLush
        "mossy" | "forest" | "temperate_forest" => 2,                   // Mossy
        "dirt" | "ruin" | "cropland" | "tilled_soil" => 3,              // Dirt
        "snow" => 4,                                                    // Snow
        "stone" | "cobblestone" => 5,                                   // Stone
        "scorched" | "ashland" => 6,                                    // Scorched
        "marsh" | "marshland" | "mud" => 7,                             // Marsh
        "rocky" | "mountain" | "mountain_rock"
        | "coastal_cliff" | "fjord" | "ridge_scrub" => 8,               // Rocky
        _ => 0, // unknown → Grass
    }
}

fn world_to_sub(world_x: f32, world_z: f32) -> (i32, i32) {
    (
        (world_x / SUB_CELL_SIZE_M).floor() as i32,
        (world_z / SUB_CELL_SIZE_M).floor() as i32,
    )
}

fn sub_cell_center(sub_x: i32, sub_z: i32) -> Coord2 {
    Coord2::new(
        (sub_x as f32 + 0.5) * SUB_CELL_SIZE_M,
        (sub_z as f32 + 0.5) * SUB_CELL_SIZE_M,
    )
}

fn world_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../src/generated/world")
        .canonicalize()
        .expect("world root not found")
}

fn overrides_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../src/generated/world/biome_overrides.bin")
}

/// Rasterizes one polygon (already in world-space coords) into the
/// override map. Walks the polygon's AABB at 8m granularity and
/// point-in-polygon-tests each sub-cell center. Returns the number of
/// sub-cells written for this polygon.
fn rasterize_polygon(
    world_polygon: &[Coord2],
    biome_id: u8,
    overrides: &mut HashMap<(i32, i32), u8>,
) -> usize {
    if world_polygon.len() < 3 {
        return 0;
    }
    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_z = f32::INFINITY;
    let mut max_z = f32::NEG_INFINITY;
    for p in world_polygon {
        min_x = min_x.min(p.x);
        max_x = max_x.max(p.x);
        min_z = min_z.min(p.z);
        max_z = max_z.max(p.z);
    }
    let (sx_min, sz_min) = world_to_sub(min_x, min_z);
    let (sx_max, sz_max) = world_to_sub(max_x, max_z);

    let mut count = 0usize;
    for sz in sz_min..=sz_max {
        for sx in sx_min..=sx_max {
            let center = sub_cell_center(sx, sz);
            if point_in_polygon(center, world_polygon) {
                overrides.insert((sx, sz), biome_id);
                count += 1;
            }
        }
    }
    count
}

fn load_existing_overrides(path: &Path) -> HashMap<(i32, i32), u8> {
    let mut map: HashMap<(i32, i32), u8> = HashMap::new();
    if !path.exists() {
        return map;
    }
    let Ok(bytes) = std::fs::read(path) else {
        return map;
    };
    if let Ok(file) = bincode::deserialize::<OverridesFile>(&bytes) {
        match file {
            OverridesFile::V2(v2) => {
                for ((x, z), id) in v2.entries {
                    map.insert((x, z), id);
                }
            }
            OverridesFile::V1(v1) => {
                let n = SUB_CELLS_PER_CHUNK as i32;
                for ((cx, cz), id) in v1.entries {
                    for dz in 0..n {
                        for dx in 0..n {
                            map.insert((cx * n + dx, cz * n + dz), id);
                        }
                    }
                }
            }
        }
    } else if let Ok(v1) = bincode::deserialize::<OverridesFileV1>(&bytes) {
        let n = SUB_CELLS_PER_CHUNK as i32;
        for ((cx, cz), id) in v1.entries {
            for dz in 0..n {
                for dx in 0..n {
                    map.insert((cx * n + dx, cz * n + dz), id);
                }
            }
        }
    }
    map
}

/// Rasterize one river polyline into the elevation map. For every
/// sub-cell whose center is within `(width_units * 0.5 + RIVER_BANK_M)`
/// of any segment, the cell's elevation is *lowered* (more negative)
/// to a value that linearly tapers from `-RIVER_DEPTH_M` at the
/// channel center to 0 at the bank's outer edge.
///
/// "Lowered" here means: take the most-negative of (existing offset,
/// new river offset). So a river through a mountain biome carves the
/// channel from the +30m baseline down rather than overwriting to
/// -3m absolute.
fn rasterize_river(
    polyline_world: &[Coord2],
    width_units: f32,
    elevations: &mut HashMap<(i32, i32), f32>,
) -> usize {
    if polyline_world.len() < 2 {
        return 0;
    }
    let half_width = width_units * 0.5;
    let band = half_width + RIVER_BANK_M;
    let band_sq = band * band;
    // Walk the polyline's full bounding box, expanded by `band`.
    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_z = f32::INFINITY;
    let mut max_z = f32::NEG_INFINITY;
    for p in polyline_world {
        min_x = min_x.min(p.x);
        max_x = max_x.max(p.x);
        min_z = min_z.min(p.z);
        max_z = max_z.max(p.z);
    }
    min_x -= band;
    max_x += band;
    min_z -= band;
    max_z += band;
    let (sx_min, sz_min) = world_to_sub(min_x, min_z);
    let (sx_max, sz_max) = world_to_sub(max_x, max_z);

    let mut count = 0usize;
    for sz in sz_min..=sz_max {
        for sx in sx_min..=sx_max {
            let p = sub_cell_center(sx, sz);
            // Distance from this point to the polyline = min over each segment.
            let mut best_d2 = f32::INFINITY;
            for w in polyline_world.windows(2) {
                let a = w[0];
                let b = w[1];
                let d2 = point_segment_distance_sq(p, a, b);
                if d2 < best_d2 {
                    best_d2 = d2;
                }
            }
            if best_d2 > band_sq {
                continue;
            }
            let d = best_d2.sqrt();
            // Linear taper: depth = -RIVER_DEPTH_M at center, 0 at band.
            let depth = if d <= half_width {
                -RIVER_DEPTH_M
            } else {
                let t = (d - half_width) / RIVER_BANK_M;
                -RIVER_DEPTH_M * (1.0 - t.clamp(0.0, 1.0))
            };
            let entry = elevations.entry((sx, sz)).or_insert(0.0);
            // Carve over biome heights → take the more-negative value.
            if depth < *entry {
                *entry = depth;
            }
            count += 1;
        }
    }
    count
}

/// Squared distance from point `p` to segment `(a, b)`.
fn point_segment_distance_sq(p: Coord2, a: Coord2, b: Coord2) -> f32 {
    let abx = b.x - a.x;
    let abz = b.z - a.z;
    let len_sq = abx * abx + abz * abz;
    if len_sq < 1e-6 {
        let dx = p.x - a.x;
        let dz = p.z - a.z;
        return dx * dx + dz * dz;
    }
    let apx = p.x - a.x;
    let apz = p.z - a.z;
    let t = ((apx * abx + apz * abz) / len_sq).clamp(0.0, 1.0);
    let qx = a.x + abx * t;
    let qz = a.z + abz * t;
    let dx = p.x - qx;
    let dz = p.z - qz;
    dx * dx + dz * dz
}

fn rasterize_biome_height(
    world_polygon: &[Coord2],
    height_m: f32,
    elevations: &mut HashMap<(i32, i32), f32>,
) -> usize {
    if world_polygon.len() < 3 || height_m == 0.0 {
        return 0;
    }
    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_z = f32::INFINITY;
    let mut max_z = f32::NEG_INFINITY;
    for p in world_polygon {
        min_x = min_x.min(p.x);
        max_x = max_x.max(p.x);
        min_z = min_z.min(p.z);
        max_z = max_z.max(p.z);
    }
    let (sx_min, sz_min) = world_to_sub(min_x, min_z);
    let (sx_max, sz_max) = world_to_sub(max_x, max_z);
    let mut count = 0usize;
    for sz in sz_min..=sz_max {
        for sx in sx_min..=sx_max {
            let center = sub_cell_center(sx, sz);
            if point_in_polygon(center, world_polygon) {
                elevations.insert((sx, sz), height_m);
                count += 1;
            }
        }
    }
    count
}

/// Rasterize all rivers + tributaries in a zone. Returns
/// `(segments_processed, sub_cells_carved_total)`.
fn rasterize_zone_rivers(
    geo: &Geography,
    origin: vaern_data::Coord2,
    elevations: &mut HashMap<(i32, i32), f32>,
) -> (usize, usize) {
    let mut segments = 0usize;
    let mut cells = 0usize;
    for river in &geo.rivers {
        let world_path: Vec<Coord2> = river
            .path
            .points
            .iter()
            .map(|p| Coord2::new(p.x + origin.x, p.z + origin.z))
            .collect();
        let n = rasterize_river(&world_path, river.width_units, elevations);
        segments += 1;
        cells += n;
        println!(
            "    river {} (width {:.1}u) → carved {} sub-cells",
            river.id, river.width_units, n
        );
        for trib in &river.tributaries {
            let world_path: Vec<Coord2> = trib
                .path
                .points
                .iter()
                .map(|p| Coord2::new(p.x + origin.x, p.z + origin.z))
                .collect();
            let n = rasterize_river(&world_path, trib.width_units, elevations);
            segments += 1;
            cells += n;
            println!(
                "    tributary {} (width {:.1}u) → carved {} sub-cells",
                trib.id, trib.width_units, n
            );
        }
    }
    (segments, cells)
}

fn elevation_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../src/generated/world/elevation_overrides.bin")
}

fn save_elevations(
    path: &Path,
    elevations: &HashMap<(i32, i32), f32>,
) -> std::io::Result<usize> {
    // Convert meters → centimeters, clamp to i16 range, drop near-zero
    // entries to keep the file small.
    let mut entries: Vec<((i32, i32), i16)> = elevations
        .iter()
        .filter_map(|(&xz, &m)| {
            let cm = (m * 100.0).round();
            if cm.abs() < 1.0 {
                return None;
            }
            let clamped = cm.clamp(i16::MIN as f32, i16::MAX as f32);
            Some((xz, clamped as i16))
        })
        .collect();
    entries.sort_by_key(|(xz, _)| *xz);
    let count = entries.len();
    let payload = ElevationFile::V1(ElevationFileV1 {
        sub_cells_per_chunk: SUB_CELLS_PER_CHUNK,
        entries,
    });
    let bytes = bincode::serialize(&payload)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, bytes)?;
    Ok(count)
}

fn save_overrides(path: &Path, overrides: &HashMap<(i32, i32), u8>) -> std::io::Result<usize> {
    let mut entries: Vec<((i32, i32), u8)> =
        overrides.iter().map(|(&xz, &id)| (xz, id)).collect();
    entries.sort_by_key(|(xz, _)| *xz);
    let count = entries.len();
    let payload = OverridesFile::V2(OverridesFileV2 {
        sub_cells_per_chunk: SUB_CELLS_PER_CHUNK,
        entries,
    });
    let bytes = bincode::serialize(&payload)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, bytes)?;
    Ok(count)
}

struct Args {
    zone_filter: Option<String>,
    clean: bool,
}

fn parse_args() -> Args {
    let mut zone_filter = None;
    let mut clean = false;
    let mut iter = std::env::args().skip(1);
    while let Some(a) = iter.next() {
        match a.as_str() {
            "--zone" => {
                zone_filter = iter.next();
            }
            "--clean" => {
                clean = true;
            }
            "--help" | "-h" => {
                eprintln!(
                    "usage: vaern-import-editor [--zone <zone_id>] [--clean]\n\n\
                     Rasterizes cartography geography.yaml polygons into\n\
                     src/generated/world/biome_overrides.bin for the editor.\n\n\
                       --zone <id>  only import this zone (default: every zone\n\
                                    with a geography.yaml)\n\
                       --clean      discard existing biome_overrides.bin entries\n\
                                    before writing (default: merge — cartography\n\
                                    overwrites only sub-cells inside its polygons)\n"
                );
                std::process::exit(0);
            }
            other => {
                eprintln!("unknown arg: {other}\n(try --help)");
                std::process::exit(1);
            }
        }
    }
    Args { zone_filter, clean }
}

fn main() -> ExitCode {
    let args = parse_args();
    let root = world_root();

    let world = match load_world(&root) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("load_world failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    let layout = match load_world_layout(&root) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("load_world_layout failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    let geography = match load_all_geography(&root) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("load_all_geography failed: {e}");
            return ExitCode::FAILURE;
        }
    };

    let path = overrides_path();
    let mut overrides: HashMap<(i32, i32), u8> = if args.clean {
        HashMap::new()
    } else {
        load_existing_overrides(&path)
    };
    let pre_existing = overrides.len();

    // Elevation always re-derives from cartography on each import —
    // no merge with prior on-disk state. Rivers are authored via
    // geography.yaml and biome height bumps come from the table
    // above; both are deterministic functions of the YAML, so a
    // re-run produces a byte-identical file.
    let mut elevations: HashMap<(i32, i32), f32> = HashMap::new();

    let mut zones_processed = 0usize;
    let mut polygons_processed = 0usize;
    let mut sub_cells_written = 0usize;
    let mut river_segments = 0usize;
    let mut river_cells_carved = 0usize;

    let mut zone_ids: Vec<&str> = world.zones.iter().map(|z| z.id.as_str()).collect();
    zone_ids.sort();

    for zone_id in zone_ids {
        if let Some(filter) = &args.zone_filter {
            if filter != zone_id {
                continue;
            }
        }
        let Some(geo) = geography.get(zone_id) else {
            continue;
        };
        let Some(origin) = layout.zone_origin(zone_id) else {
            eprintln!("warn: zone {zone_id} has no world.yaml placement, skipping");
            continue;
        };
        zones_processed += 1;

        // Process backdrop first so pockets paint over it.
        let mut regions: Vec<&vaern_data::BiomeRegion> = geo.biome_regions.iter().collect();
        regions.sort_by_key(|r| if r.id.ends_with("_main") { 0 } else { 1 });

        for region in regions {
            let world_polygon: Vec<Coord2> = region
                .polygon
                .points
                .iter()
                .map(|p| Coord2::new(p.x + origin.x, p.z + origin.z))
                .collect();
            let id = biome_id_for(&region.biome);
            let n = rasterize_polygon(&world_polygon, id, &mut overrides);
            polygons_processed += 1;
            sub_cells_written += n;
            // Stamp this biome's resting height into the elevation
            // map. Rivers (rasterized below) can carve channels over
            // it.
            let height = biome_height_m(&region.biome);
            let m = rasterize_biome_height(&world_polygon, height, &mut elevations);
            println!(
                "  {zone_id} :: {} ({}) → biome_id {} ({} sub-cells, height {:+.1}m, {} elev cells)",
                region.id, region.biome, id, n, height, m
            );
        }

        // Rivers + tributaries — carve over biome heights.
        let river_count = rasterize_zone_rivers(geo, origin, &mut elevations);
        river_segments += river_count.0;
        river_cells_carved += river_count.1;
    }

    let total = overrides.len();
    let added = total.saturating_sub(pre_existing);
    let updated = sub_cells_written.saturating_sub(added);

    let biome_save = save_overrides(&path, &overrides);
    let elev_path = elevation_path();
    let elev_save = save_elevations(&elev_path, &elevations);

    match (biome_save, elev_save) {
        (Ok(n), Ok(en)) => {
            println!(
                "\nwrote biomes  → {} ({n} sub-cell entries)\n  zones processed: {zones_processed}\n  polygons rasterized: {polygons_processed}\n  sub-cells written this run: {sub_cells_written} (new: {added}, overwritten: {updated})\n  pre-existing biome entries: {pre_existing}",
                path.display()
            );
            println!(
                "wrote elevation → {} ({en} sub-cell entries, signed cm)\n  river segments rasterized: {river_segments}\n  river cells carved (incl. taper bands): {river_cells_carved}",
                elev_path.display()
            );
            ExitCode::SUCCESS
        }
        (Err(e), _) => {
            eprintln!("biome save failed: {e}");
            ExitCode::FAILURE
        }
        (_, Err(e)) => {
            eprintln!("elevation save failed: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rasterize_unit_triangle_into_correct_sub_cells() {
        // Triangle covering a small area at world (40, 40) — expect a
        // handful of sub-cells inside.
        let tri = vec![
            Coord2::new(40.0, 40.0),
            Coord2::new(80.0, 40.0),
            Coord2::new(40.0, 80.0),
        ];
        let mut overrides: HashMap<(i32, i32), u8> = HashMap::new();
        let n = rasterize_polygon(&tri, 7, &mut overrides);
        assert!(n >= 4, "expected at least 4 sub-cells, got {}", n);
        // The (5, 5) sub-cell is at world center (44, 44) — inside the
        // triangle, should be set to 7 (Marsh).
        assert_eq!(overrides.get(&(5, 5)), Some(&7));
        // The (15, 15) sub-cell is at world (124, 124) — outside.
        assert_eq!(overrides.get(&(15, 15)), None);
    }

    #[test]
    fn biome_id_table_covers_known_cartography_keys() {
        assert_eq!(biome_id_for("fields"), 0);
        assert_eq!(biome_id_for("forest"), 2);
        assert_eq!(biome_id_for("highland"), 1);
        assert_eq!(biome_id_for("mountain"), 8);
        assert_eq!(biome_id_for("ashland"), 6);
        assert_eq!(biome_id_for("marsh"), 7);
        assert_eq!(biome_id_for("ruin"), 3);
        assert_eq!(biome_id_for("cobblestone"), 5);
        assert_eq!(biome_id_for("snow"), 4);
        assert_eq!(biome_id_for("unknown"), 0);
    }

    #[test]
    fn world_to_sub_round_trips_at_centers() {
        let center = sub_cell_center(3, -5);
        assert_eq!(world_to_sub(center.x, center.z), (3, -5));
    }

    #[test]
    fn river_carves_channel_at_polyline_with_taper() {
        // Horizontal river along z=2, x∈[0, 80]. Width 4u → half=2u.
        // Bank extends to half + RIVER_BANK_M = 2 + 6 = 8u. Sub-cell
        // rows at z-center = ..., -4, 4, 12, ... Distances from line:
        //   z=4 cells: |4-2| = 2  (inside half) → full depth
        //   z=12 cells: |12-2| = 10 (past bank) → not carved
        //   z=-4 cells: |−4−2| = 6 (inside bank, taper) → partial
        let path = vec![Coord2::new(0.0, 2.0), Coord2::new(80.0, 2.0)];
        let mut e: HashMap<(i32, i32), f32> = HashMap::new();
        let n = rasterize_river(&path, 4.0, &mut e);
        assert!(n > 0, "expected carved cells, got {n}");
        // Deepest cell should be near full depth.
        let min_v = e.values().copied().fold(f32::INFINITY, f32::min);
        assert!(
            (min_v - (-RIVER_DEPTH_M)).abs() < 0.1,
            "deepest cell should be ~{} m, got {}",
            -RIVER_DEPTH_M,
            min_v
        );
        // The z=-4 row should have a partial taper.
        let v = e.get(&(5, -1)).copied().unwrap();
        assert!(
            v < 0.0 && v > -RIVER_DEPTH_M,
            "expected partial taper at (5, -1), got {v}"
        );
    }

    #[test]
    fn biome_height_table_covers_known_keys() {
        assert_eq!(biome_height_m("mountain"), 30.0);
        assert_eq!(biome_height_m("highland"), 12.0);
        assert_eq!(biome_height_m("ridge_scrub"), 7.0);
        assert_eq!(biome_height_m("marsh"), -1.5);
        assert_eq!(biome_height_m("fields"), 0.0);
        assert_eq!(biome_height_m("forest"), 0.0);
        assert_eq!(biome_height_m("unknown"), 0.0);
    }

    #[test]
    fn biome_height_rasterizer_skips_zero_height_regions() {
        let tri = vec![
            Coord2::new(0.0, 0.0),
            Coord2::new(80.0, 0.0),
            Coord2::new(0.0, 80.0),
        ];
        let mut e: HashMap<(i32, i32), f32> = HashMap::new();
        let n = rasterize_biome_height(&tri, 0.0, &mut e);
        assert_eq!(n, 0);
        assert!(e.is_empty());
    }
}
