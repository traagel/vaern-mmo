//! Shared spatial primitives used across world data: hub/landmark/prop
//! offsets, zone bounds, biome polygons, river/road paths.
//!
//! Single source of truth for 2D coordinates in zone-local meters.
//! `+x` is east; `+z` is south by convention (declared per-zone in
//! `core.yaml` under `coordinate_system`). `y` (elevation) is sampled
//! from the procedural heightmap at runtime and is not persisted here.

use serde::{Deserialize, Serialize};

/// 2D point in zone-local meters. `(x, z)` matches the engine's
/// horizontal plane; `y` is sampled from terrain at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
pub struct Coord2 {
    pub x: f32,
    pub z: f32,
}

impl Coord2 {
    pub const fn new(x: f32, z: f32) -> Self {
        Self { x, z }
    }

    pub const ZERO: Self = Self { x: 0.0, z: 0.0 };

    pub fn distance_to(self, other: Coord2) -> f32 {
        let dx = self.x - other.x;
        let dz = self.z - other.z;
        (dx * dx + dz * dz).sqrt()
    }
}

/// Legacy aliases. Preserved so existing constructors in vaern-editor
/// and elsewhere keep compiling.
pub type HubOffset = Coord2;
pub type LandmarkOffset = Coord2;
pub type PropOffset = Coord2;

/// Axis-aligned bounding box in zone-local meters. `min` is the NW
/// (or top-left in screen-space) corner; `max` is SE.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
pub struct Bounds {
    pub min: Coord2,
    pub max: Coord2,
}

impl Bounds {
    pub fn contains(&self, p: Coord2) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.z >= self.min.z && p.z <= self.max.z
    }

    pub fn width(&self) -> f32 {
        self.max.x - self.min.x
    }

    pub fn height(&self) -> f32 {
        self.max.z - self.min.z
    }

    pub fn center(&self) -> Coord2 {
        Coord2::new(
            (self.min.x + self.max.x) * 0.5,
            (self.min.z + self.max.z) * 0.5,
        )
    }
}

/// A closed polygon in zone-local meters. The first and last points
/// are NOT required to match — closure is implicit.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Polygon {
    pub points: Vec<Coord2>,
}

impl Polygon {
    /// Standard ray-casting point-in-polygon test. Returns true if
    /// `p` lies strictly inside; boundary-coincident points may go
    /// either way (acceptable for cartographic validation).
    pub fn contains(&self, p: Coord2) -> bool {
        point_in_polygon(p, &self.points)
    }
}

/// Standalone helper for point-in-polygon over an arbitrary slice.
/// Used by both the renderer (scatter inside biome regions) and the
/// validator (anchor inside zone_cell).
pub fn point_in_polygon(p: Coord2, poly: &[Coord2]) -> bool {
    if poly.len() < 3 {
        return false;
    }
    let mut inside = false;
    let n = poly.len();
    let mut j = n - 1;
    for i in 0..n {
        let a = poly[i];
        let b = poly[j];
        if (a.z > p.z) != (b.z > p.z) {
            let denom = b.z - a.z;
            if denom.abs() > f32::EPSILON {
                let t = (p.z - a.z) / denom;
                let x_at = a.x + t * (b.x - a.x);
                if p.x < x_at {
                    inside = !inside;
                }
            }
        }
        j = i;
    }
    inside
}

/// An open polyline used for rivers and roads.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PolyPath {
    pub points: Vec<Coord2>,
}

/// Cardinal directions for zone-to-zone connections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Cardinal {
    N,
    Ne,
    E,
    Se,
    S,
    Sw,
    W,
    Nw,
}

/// How a zone's local axes map to compass directions. Declared in each
/// zone's `core.yaml` so the renderer doesn't have to guess.
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct AxisMapping {
    pub x_positive: Compass,
    pub z_positive: Compass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Compass {
    East,
    West,
    North,
    South,
}

impl Default for AxisMapping {
    fn default() -> Self {
        Self {
            x_positive: Compass::East,
            z_positive: Compass::South,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Unit {
    Meter,
}

impl Default for Unit {
    fn default() -> Self {
        Unit::Meter
    }
}

/// Zone coordinate-system declaration. Lives at `Zone.coordinate_system`
/// in `core.yaml`. Optional during migration; the validator promotes it
/// to required.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CoordinateSystem {
    /// Hub id whose offset is `(0, 0)`.
    pub origin: String,
    #[serde(default)]
    pub axes: AxisMapping,
    #[serde(default)]
    pub unit: Unit,
}
