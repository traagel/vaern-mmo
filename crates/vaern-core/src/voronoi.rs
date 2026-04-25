//! Grid-based Voronoi partition + Catmull-Rom spline utilities.
//!
//! Given a set of 2D hub centers and a rectangular bounds, partition the
//! bounds into a fine grid, assigning each grid cell to its nearest hub.
//! The union of cells owned by a hub is that hub's (quantized) Voronoi
//! region. Blocky boundaries at cell resolution; the quantization is
//! deliberate so downstream mesh generation is trivial (each cell = one
//! quad).
//!
//! No Bevy dep — pure math. Consumers (currently `vaern-client`) walk
//! the cells to build textured floor patches per hub and road ribbons
//! between neighbor pairs.

use std::collections::HashMap;

/// 2D point in world XZ coordinates (Bevy's ground plane).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point2 {
    pub x: f32,
    pub z: f32,
}

impl Point2 {
    pub const fn new(x: f32, z: f32) -> Self {
        Self { x, z }
    }

    pub fn distance_squared(self, other: Point2) -> f32 {
        let dx = self.x - other.x;
        let dz = self.z - other.z;
        dx * dx + dz * dz
    }

    pub fn distance(self, other: Point2) -> f32 {
        self.distance_squared(other).sqrt()
    }

    pub fn lerp(self, other: Point2, t: f32) -> Point2 {
        Point2::new(self.x + (other.x - self.x) * t, self.z + (other.z - self.z) * t)
    }
}

/// A hub anchor — id + world-space XZ center.
#[derive(Debug, Clone)]
pub struct Hub2 {
    pub id: String,
    pub pos: Point2,
}

/// Axis-aligned rectangular region in world XZ.
#[derive(Debug, Clone, Copy)]
pub struct Bounds2 {
    pub min: Point2,
    pub max: Point2,
}

impl Bounds2 {
    pub fn new(min: Point2, max: Point2) -> Self {
        Self { min, max }
    }

    /// Bounding box around `hubs`, expanded by `padding` on each axis.
    pub fn around_hubs(hubs: &[Hub2], padding: f32) -> Self {
        if hubs.is_empty() {
            return Self {
                min: Point2::new(-padding, -padding),
                max: Point2::new(padding, padding),
            };
        }
        let mut min = hubs[0].pos;
        let mut max = hubs[0].pos;
        for h in &hubs[1..] {
            min.x = min.x.min(h.pos.x);
            min.z = min.z.min(h.pos.z);
            max.x = max.x.max(h.pos.x);
            max.z = max.z.max(h.pos.z);
        }
        Self {
            min: Point2::new(min.x - padding, min.z - padding),
            max: Point2::new(max.x + padding, max.z + padding),
        }
    }

    pub fn width(&self) -> f32 {
        self.max.x - self.min.x
    }
    pub fn height(&self) -> f32 {
        self.max.z - self.min.z
    }
}

/// One cell of the partition grid. `owner` is an index into the hubs
/// slice passed to `partition`, or `usize::MAX` if no hub owns it
/// (empty hubs slice edge case).
#[derive(Debug, Clone, Copy)]
pub struct GridCell {
    /// (col, row) index — column is X axis, row is Z axis.
    pub col: u32,
    pub row: u32,
    /// World-space XZ center of this cell.
    pub center: Point2,
    /// Full cell width on the X axis.
    pub width: f32,
    /// Full cell height on the Z axis.
    pub height: f32,
    /// Index into the `hubs` slice passed to `partition`. `usize::MAX`
    /// means no assignment (shouldn't happen for non-empty hubs).
    pub owner: usize,
}

/// Grid-based Voronoi partition. Each of `cells_per_axis × cells_per_axis`
/// cells is assigned to its nearest hub (Euclidean, cell center).
///
/// Returns the cells in row-major order (row × cells_per_axis + col).
///
/// O(cells × hubs) — fine for ≤32 hubs × ≤64² cells in interactive
/// scenarios. For wider zones bump `cells_per_axis` knowing the cost
/// scales quadratically.
pub fn partition(hubs: &[Hub2], bounds: Bounds2, cells_per_axis: u32) -> Vec<GridCell> {
    let cols = cells_per_axis.max(1);
    let rows = cells_per_axis.max(1);
    let cell_w = bounds.width() / cols as f32;
    let cell_h = bounds.height() / rows as f32;

    let mut cells = Vec::with_capacity((cols * rows) as usize);
    for row in 0..rows {
        for col in 0..cols {
            let cx = bounds.min.x + (col as f32 + 0.5) * cell_w;
            let cz = bounds.min.z + (row as f32 + 0.5) * cell_h;
            let center = Point2::new(cx, cz);
            let owner = nearest_hub_idx(hubs, center);
            cells.push(GridCell {
                col,
                row,
                center,
                width: cell_w,
                height: cell_h,
                owner,
            });
        }
    }
    cells
}

fn nearest_hub_idx(hubs: &[Hub2], p: Point2) -> usize {
    if hubs.is_empty() {
        return usize::MAX;
    }
    let mut best = 0usize;
    let mut best_d = f32::INFINITY;
    for (i, h) in hubs.iter().enumerate() {
        let d = p.distance_squared(h.pos);
        if d < best_d {
            best_d = d;
            best = i;
        }
    }
    best
}

/// Pairs of hub indices whose cells are grid-adjacent (share an edge).
/// Produces each unordered pair once (sorted by index). Use this to
/// decide which hub pairs get connected with a road — naturally gives
/// the Voronoi adjacency graph for free.
pub fn neighbor_pairs(cells: &[GridCell], cells_per_axis: u32) -> Vec<(usize, usize)> {
    let cols = cells_per_axis.max(1) as usize;
    let rows = cells_per_axis.max(1) as usize;
    let idx = |c: usize, r: usize| r * cols + c;
    let mut pairs: HashMap<(usize, usize), ()> = HashMap::new();

    for r in 0..rows {
        for c in 0..cols {
            let here = cells[idx(c, r)].owner;
            if here == usize::MAX {
                continue;
            }
            // east neighbor
            if c + 1 < cols {
                let other = cells[idx(c + 1, r)].owner;
                if other != usize::MAX && other != here {
                    let key = if here < other { (here, other) } else { (other, here) };
                    pairs.insert(key, ());
                }
            }
            // south neighbor
            if r + 1 < rows {
                let other = cells[idx(c, r + 1)].owner;
                if other != usize::MAX && other != here {
                    let key = if here < other { (here, other) } else { (other, here) };
                    pairs.insert(key, ());
                }
            }
        }
    }

    let mut out: Vec<_> = pairs.into_keys().collect();
    out.sort_unstable();
    out
}

/// Sample a Catmull-Rom spline through `control` at `samples_per_segment`
/// points per segment. With N controls you get N-1 segments; the first
/// and last segments duplicate their endpoint tangent (clamped).
///
/// Good for: a wiggly road through a handful of hub centers. Add a
/// jittered midpoint between two hubs and it produces a smooth curved
/// path through all three.
pub fn catmull_rom(control: &[Point2], samples_per_segment: usize) -> Vec<Point2> {
    if control.len() < 2 {
        return control.to_vec();
    }
    let samples = samples_per_segment.max(1);
    let mut out = Vec::with_capacity((control.len() - 1) * samples + 1);

    for i in 0..control.len() - 1 {
        // p0 clamped at start, p3 clamped at end — avoids overshoot on
        // open endpoints.
        let p0 = if i == 0 { control[0] } else { control[i - 1] };
        let p1 = control[i];
        let p2 = control[i + 1];
        let p3 = if i + 2 < control.len() { control[i + 2] } else { control[i + 1] };

        for s in 0..samples {
            let t = s as f32 / samples as f32;
            out.push(catmull_rom_point(p0, p1, p2, p3, t));
        }
    }
    out.push(*control.last().unwrap());
    out
}

fn catmull_rom_point(p0: Point2, p1: Point2, p2: Point2, p3: Point2, t: f32) -> Point2 {
    // Uniform Catmull-Rom (0.5 tension). Expanded Hermite form.
    let t2 = t * t;
    let t3 = t2 * t;
    let f0 = -0.5 * t3 + t2 - 0.5 * t;
    let f1 = 1.5 * t3 - 2.5 * t2 + 1.0;
    let f2 = -1.5 * t3 + 2.0 * t2 + 0.5 * t;
    let f3 = 0.5 * t3 - 0.5 * t2;
    Point2::new(
        f0 * p0.x + f1 * p1.x + f2 * p2.x + f3 * p3.x,
        f0 * p0.z + f1 * p1.z + f2 * p2.z + f3 * p3.z,
    )
}

/// Inject a midpoint between each consecutive pair in `control`, offset
/// perpendicular to the segment by `amplitude × pseudo_random(i)` on
/// alternating sides. Purely a prompt for Catmull-Rom to produce a
/// wiggly path instead of a straight one through collinear hubs.
pub fn wiggle_midpoints(control: &[Point2], amplitude: f32, seed: u64) -> Vec<Point2> {
    if control.len() < 2 {
        return control.to_vec();
    }
    let mut out = Vec::with_capacity(control.len() * 2 - 1);
    for i in 0..control.len() - 1 {
        let a = control[i];
        let b = control[i + 1];
        out.push(a);
        let mid = a.lerp(b, 0.5);
        let dx = b.x - a.x;
        let dz = b.z - a.z;
        let len = (dx * dx + dz * dz).sqrt().max(1e-4);
        // unit perpendicular (rotate 90° in XZ): (-dz, dx) / len
        let px = -dz / len;
        let pz = dx / len;
        // deterministic pseudo-random sign + magnitude per segment
        let h = wrap_hash(seed.wrapping_add(i as u64));
        let sign = if h & 1 == 0 { 1.0 } else { -1.0 };
        let jitter = ((h >> 1) % 1000) as f32 / 1000.0; // 0..1
        let off = sign * amplitude * (0.4 + jitter * 0.6);
        out.push(Point2::new(mid.x + px * off, mid.z + pz * off));
    }
    out.push(*control.last().unwrap());
    out
}

fn wrap_hash(x: u64) -> u64 {
    // SplitMix64 one-step — good enough for visual jitter.
    let mut z = x.wrapping_add(0x9E3779B97F4A7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(id: &str, x: f32, z: f32) -> Hub2 {
        Hub2 { id: id.into(), pos: Point2::new(x, z) }
    }

    #[test]
    fn partition_single_hub_owns_everything() {
        let hubs = vec![h("solo", 0.0, 0.0)];
        let bounds = Bounds2::new(Point2::new(-10.0, -10.0), Point2::new(10.0, 10.0));
        let cells = partition(&hubs, bounds, 4);
        assert_eq!(cells.len(), 16);
        assert!(cells.iter().all(|c| c.owner == 0));
    }

    #[test]
    fn partition_two_hubs_split_roughly_evenly() {
        let hubs = vec![h("west", -10.0, 0.0), h("east", 10.0, 0.0)];
        let bounds = Bounds2::new(Point2::new(-20.0, -10.0), Point2::new(20.0, 10.0));
        let cells = partition(&hubs, bounds, 8);
        let west_count = cells.iter().filter(|c| c.owner == 0).count();
        let east_count = cells.iter().filter(|c| c.owner == 1).count();
        assert_eq!(west_count + east_count, 64);
        assert_eq!(west_count, east_count, "vertical split → equal halves");
    }

    #[test]
    fn neighbor_pairs_two_hubs_have_one_edge() {
        let hubs = vec![h("a", -10.0, 0.0), h("b", 10.0, 0.0)];
        let bounds = Bounds2::new(Point2::new(-20.0, -10.0), Point2::new(20.0, 10.0));
        let cells = partition(&hubs, bounds, 8);
        let pairs = neighbor_pairs(&cells, 8);
        assert_eq!(pairs, vec![(0, 1)]);
    }

    #[test]
    fn catmull_rom_passes_through_controls() {
        let pts = vec![Point2::new(0.0, 0.0), Point2::new(10.0, 0.0), Point2::new(20.0, 10.0)];
        let sampled = catmull_rom(&pts, 4);
        assert!(sampled.len() > pts.len());
        // First and last sample points match the endpoints exactly.
        assert_eq!(sampled.first().map(|p| (p.x, p.z)), Some((0.0, 0.0)));
        assert_eq!(sampled.last().map(|p| (p.x, p.z)), Some((20.0, 10.0)));
    }

    #[test]
    fn wiggle_midpoints_doubles_segment_count_minus_one() {
        let pts = vec![
            Point2::new(0.0, 0.0),
            Point2::new(10.0, 0.0),
            Point2::new(20.0, 0.0),
        ];
        let wig = wiggle_midpoints(&pts, 2.0, 42);
        assert_eq!(wig.len(), 5); // 3 originals + 2 midpoints
        // Midpoint between (0,0)→(10,0) should have x ≈ 5 and |z| > 0.
        assert!((wig[1].x - 5.0).abs() < 0.01);
        assert!(wig[1].z.abs() > 0.01);
    }
}
