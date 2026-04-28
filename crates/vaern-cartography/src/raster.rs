//! Sub-cell rasterization helpers shared by the heightfield generator and
//! the legacy `import_to_editor` migration path.
//!
//! Every consumer rasterizes onto the same uniform grid:
//! - `SUB_CELL_SIZE_M = 8` metres per cell.
//! - `(sub_x, sub_z)` keys, derived from world-space `(x, z)` via
//!   floor-divide.
//!
//! Helpers are pure functions over `&[Coord2]` polylines / polygons; no
//! global state, no `HashMap` iteration. Determinism is the responsibility
//! of the caller (sort entries before serialisation).

use std::collections::HashMap;

use vaern_data::{point_in_polygon, Coord2};

/// Editor sub-cell size in metres. Matches
/// `vaern-editor::voxel::overrides::SUB_CELL_SIZE_M` exactly.
pub const SUB_CELL_SIZE_M: f32 = 8.0;

/// Sub-cells per chunk along one axis. Matches the editor's voxel chunk
/// streaming layout — must stay in sync with
/// `vaern-editor::voxel::overrides::SUB_CELLS_PER_CHUNK`.
pub const SUB_CELLS_PER_CHUNK: u32 = 4;

/// Distance (m) at which a river's carve depth tapers to 0 from the
/// channel half-width.
pub const RIVER_BANK_M: f32 = 6.0;

/// Centre-of-channel river depth in metres below the surrounding biome.
pub const RIVER_DEPTH_M: f32 = 3.0;

/// World-space `(x, z)` → `(sub_x, sub_z)` integer key.
pub fn world_to_sub(world_x: f32, world_z: f32) -> (i32, i32) {
    (
        (world_x / SUB_CELL_SIZE_M).floor() as i32,
        (world_z / SUB_CELL_SIZE_M).floor() as i32,
    )
}

/// World-space centre of the sub-cell at `(sub_x, sub_z)`.
pub fn sub_cell_center(sub_x: i32, sub_z: i32) -> Coord2 {
    Coord2::new(
        (sub_x as f32 + 0.5) * SUB_CELL_SIZE_M,
        (sub_z as f32 + 0.5) * SUB_CELL_SIZE_M,
    )
}

/// Inclusive AABB of a polygon as integer sub-cell ranges
/// `(sx_min, sz_min, sx_max, sz_max)`. Returns `None` for empty / degenerate
/// inputs.
pub fn polygon_sub_aabb(points: &[Coord2]) -> Option<(i32, i32, i32, i32)> {
    if points.is_empty() {
        return None;
    }
    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_z = f32::INFINITY;
    let mut max_z = f32::NEG_INFINITY;
    for p in points {
        min_x = min_x.min(p.x);
        max_x = max_x.max(p.x);
        min_z = min_z.min(p.z);
        max_z = max_z.max(p.z);
    }
    let (sx_min, sz_min) = world_to_sub(min_x, min_z);
    let (sx_max, sz_max) = world_to_sub(max_x, max_z);
    Some((sx_min, sz_min, sx_max, sz_max))
}

/// Squared distance from point `p` to segment `(a, b)`.
pub fn point_segment_distance_sq(p: Coord2, a: Coord2, b: Coord2) -> f32 {
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

/// Rasterise one polygon (already in world-space coords) into the override
/// map. Walks the polygon's AABB at 8m granularity and point-in-polygon
/// tests each sub-cell centre.
///
/// Returns the number of sub-cells written.
pub fn rasterize_polygon(
    world_polygon: &[Coord2],
    biome_id: u8,
    overrides: &mut HashMap<(i32, i32), u8>,
) -> usize {
    if world_polygon.len() < 3 {
        return 0;
    }
    let Some((sx_min, sz_min, sx_max, sz_max)) = polygon_sub_aabb(world_polygon) else {
        return 0;
    };
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

/// Stamp a constant `height_m` into every sub-cell whose centre is inside
/// `world_polygon`. Skips zero-height polygons.
pub fn rasterize_biome_height(
    world_polygon: &[Coord2],
    height_m: f32,
    elevations: &mut HashMap<(i32, i32), f32>,
) -> usize {
    if world_polygon.len() < 3 || height_m == 0.0 {
        return 0;
    }
    let Some((sx_min, sz_min, sx_max, sz_max)) = polygon_sub_aabb(world_polygon) else {
        return 0;
    };
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

/// Rasterise one road polyline into the biome override map. Every
/// sub-cell whose centre is within `width_m * 0.5` of any segment of
/// `polyline_world` gets stamped with `biome_id`. This is the
/// cartography → ground texture path: the editor's biome blend
/// shader then renders the road as a strip of cobble / dirt
/// alongside the rest of the ground, with no separate mesh.
///
/// Returns the count of sub-cells stamped.
pub fn rasterize_road_strip(
    polyline_world: &[Coord2],
    width_m: f32,
    biome_id: u8,
    overrides: &mut HashMap<(i32, i32), u8>,
) -> usize {
    if polyline_world.len() < 2 {
        return 0;
    }
    let half_width = width_m * 0.5;
    // Minimum-width clamp: even narrow paths (2.5 m) need at least one
    // 8 m sub-cell of strip on either side of the centreline so the
    // road actually shows up. Without this, a 2.5 m path can land
    // entirely between two sub-cell centres and miss every cell.
    let half_band = half_width.max(SUB_CELL_SIZE_M * 0.6);
    let band_sq = half_band * half_band;

    let Some((sx_min0, sz_min0, sx_max0, sz_max0)) = polygon_sub_aabb(polyline_world) else {
        return 0;
    };
    let pad = (half_band / SUB_CELL_SIZE_M).ceil() as i32 + 1;
    let sx_min = sx_min0 - pad;
    let sz_min = sz_min0 - pad;
    let sx_max = sx_max0 + pad;
    let sz_max = sz_max0 + pad;

    let mut count = 0usize;
    for sz in sz_min..=sz_max {
        for sx in sx_min..=sx_max {
            let p = sub_cell_center(sx, sz);
            let mut best_d2 = f32::INFINITY;
            for w in polyline_world.windows(2) {
                let d2 = point_segment_distance_sq(p, w[0], w[1]);
                if d2 < best_d2 {
                    best_d2 = d2;
                }
            }
            if best_d2 <= band_sq {
                overrides.insert((sx, sz), biome_id);
                count += 1;
            }
        }
    }
    count
}

/// Rasterise one river polyline into the elevation map. Every sub-cell
/// whose centre is within `(width_units * 0.5 + RIVER_BANK_M)` of any
/// segment is *lowered* (more negative): linear taper from
/// `-RIVER_DEPTH_M` at the channel centre to 0 at the bank's outer edge.
///
/// "Lowered" = take the more-negative of (existing offset, new river
/// offset). A river through a mountain still cuts the channel down from
/// the +30m baseline.
pub fn rasterize_river(
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
    let Some((sx_min0, sz_min0, sx_max0, sz_max0)) = polygon_sub_aabb(polyline_world) else {
        return 0;
    };
    // Expand AABB by `band` (in cells, ceil).
    let pad = (band / SUB_CELL_SIZE_M).ceil() as i32 + 1;
    let sx_min = sx_min0 - pad;
    let sz_min = sz_min0 - pad;
    let sx_max = sx_max0 + pad;
    let sz_max = sz_max0 + pad;

    let mut count = 0usize;
    for sz in sz_min..=sz_max {
        for sx in sx_min..=sx_max {
            let p = sub_cell_center(sx, sz);
            let mut best_d2 = f32::INFINITY;
            for w in polyline_world.windows(2) {
                let d2 = point_segment_distance_sq(p, w[0], w[1]);
                if d2 < best_d2 {
                    best_d2 = d2;
                }
            }
            if best_d2 > band_sq {
                continue;
            }
            let d = best_d2.sqrt();
            let depth = if d <= half_width {
                -RIVER_DEPTH_M
            } else {
                let t = (d - half_width) / RIVER_BANK_M;
                -RIVER_DEPTH_M * (1.0 - t.clamp(0.0, 1.0))
            };
            let entry = elevations.entry((sx, sz)).or_insert(0.0);
            if depth < *entry {
                *entry = depth;
            }
            count += 1;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_to_sub_round_trips_at_centers() {
        let center = sub_cell_center(3, -5);
        assert_eq!(world_to_sub(center.x, center.z), (3, -5));
    }

    #[test]
    fn rasterize_unit_triangle_into_correct_sub_cells() {
        let tri = vec![
            Coord2::new(40.0, 40.0),
            Coord2::new(80.0, 40.0),
            Coord2::new(40.0, 80.0),
        ];
        let mut overrides: HashMap<(i32, i32), u8> = HashMap::new();
        let n = rasterize_polygon(&tri, 7, &mut overrides);
        assert!(n >= 4, "expected at least 4 sub-cells, got {}", n);
        // The (5, 5) sub-cell is at world centre (44, 44) — inside the
        // triangle, should be set to 7 (Marsh).
        assert_eq!(overrides.get(&(5, 5)), Some(&7));
        // The (15, 15) sub-cell is at world (124, 124) — outside.
        assert_eq!(overrides.get(&(15, 15)), None);
    }

    #[test]
    fn river_carves_channel_at_polyline_with_taper() {
        // Horizontal river along z=2, x∈[0, 80]. Width 4u → half=2u.
        // Bank extends to half + RIVER_BANK_M = 2 + 6 = 8u. Sub-cell rows
        // at z-centre = ..., -4, 4, 12, ... Distances from line:
        //   z=4 cells: |4-2| = 2  (inside half) → full depth
        //   z=12 cells: |12-2| = 10 (past bank) → not carved
        //   z=-4 cells: |−4−2| = 6 (inside bank, taper) → partial
        let path = vec![Coord2::new(0.0, 2.0), Coord2::new(80.0, 2.0)];
        let mut e: HashMap<(i32, i32), f32> = HashMap::new();
        let n = rasterize_river(&path, 4.0, &mut e);
        assert!(n > 0, "expected carved cells, got {n}");
        let min_v = e.values().copied().fold(f32::INFINITY, f32::min);
        assert!(
            (min_v - (-RIVER_DEPTH_M)).abs() < 0.1,
            "deepest cell should be ~{} m, got {}",
            -RIVER_DEPTH_M,
            min_v
        );
        let v = e.get(&(5, -1)).copied().unwrap();
        assert!(
            v < 0.0 && v > -RIVER_DEPTH_M,
            "expected partial taper at (5, -1), got {v}"
        );
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

    #[test]
    fn road_strip_stamps_cells_along_polyline() {
        // Horizontal kingsroad-like strip along z=4, x∈[0, 80]. Width
        // 6m → half=3m → clamp to 4.8m (= 0.6×8). Cells with centre
        // within 4.8m of the line should be stamped.
        let path = vec![Coord2::new(0.0, 4.0), Coord2::new(80.0, 4.0)];
        let mut overrides: HashMap<(i32, i32), u8> = HashMap::new();
        let n = rasterize_road_strip(&path, 6.0, 5, &mut overrides);
        assert!(n > 0, "expected stamped cells, got {n}");
        // Sub-cell row z=0 is centred at z=4, so distance = 0 → stamped.
        assert_eq!(overrides.get(&(5, 0)), Some(&5));
        // Row z=2 centred at z=20 → distance 16 → not stamped.
        assert_eq!(overrides.get(&(5, 2)), None);
    }

    #[test]
    fn point_segment_distance_handles_degenerate_segment() {
        // Collapsed segment (a == b) → distance is just point-to-point.
        let a = Coord2::new(5.0, 5.0);
        let p = Coord2::new(8.0, 9.0);
        let d2 = point_segment_distance_sq(p, a, a);
        assert_eq!(d2, 25.0); // 3² + 4² = 25
    }
}
