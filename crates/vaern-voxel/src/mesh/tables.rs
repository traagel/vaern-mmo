//! Cube-topology lookup tables shared by every vertex-placement and
//! normal-estimation strategy.
//!
//! The cube-corner enumeration follows the bit pattern
//! `corner = x_bit | (y_bit << 1) | (z_bit << 2)`:
//!
//! ```text
//!     6 ------- 7
//!    /|        /|
//!   4 ------- 5 |
//!   | 2 ------|-3
//!   |/        |/
//!   0 ------- 1
//! ```
//!
//! A cube has 12 edges, grouped as 4 along each axis. The 8 corners and
//! 12 edges are compiled-in constants so the extractor has no runtime
//! table-build cost.

use bevy::math::Vec3A;

/// 8 cube corners as `[x, y, z]` offsets from the minimum corner.
/// Index with the 3-bit `corner` encoding.
pub const CUBE_CORNERS: [[u32; 3]; 8] = [
    [0, 0, 0], // 0b000
    [1, 0, 0], // 0b001
    [0, 1, 0], // 0b010
    [1, 1, 0], // 0b011
    [0, 0, 1], // 0b100
    [1, 0, 1], // 0b101
    [0, 1, 1], // 0b110
    [1, 1, 1], // 0b111
];

/// Same 8 corners as `Vec3A` for vertex-placement interpolation math.
pub const CUBE_CORNER_VECTORS: [Vec3A; 8] = [
    Vec3A::from_array([0.0, 0.0, 0.0]),
    Vec3A::from_array([1.0, 0.0, 0.0]),
    Vec3A::from_array([0.0, 1.0, 0.0]),
    Vec3A::from_array([1.0, 1.0, 0.0]),
    Vec3A::from_array([0.0, 0.0, 1.0]),
    Vec3A::from_array([1.0, 0.0, 1.0]),
    Vec3A::from_array([0.0, 1.0, 1.0]),
    Vec3A::from_array([1.0, 1.0, 1.0]),
];

/// 12 cube edges as `[corner_a, corner_b]` pairs. Order:
/// * edges 0-3  — along +X
/// * edges 4-7  — along +Y
/// * edges 8-11 — along +Z
pub const CUBE_EDGES: [[u32; 2]; 12] = [
    [0b000, 0b001],
    [0b010, 0b011],
    [0b100, 0b101],
    [0b110, 0b111],
    [0b000, 0b010],
    [0b001, 0b011],
    [0b100, 0b110],
    [0b101, 0b111],
    [0b000, 0b100],
    [0b001, 0b101],
    [0b010, 0b110],
    [0b011, 0b111],
];
