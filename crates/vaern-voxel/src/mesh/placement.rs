//! Vertex-placement strategy — given 8 corner SDF values of a cube,
//! choose where the vertex sits inside that cube.
//!
//! This is the layer that distinguishes Surface Nets (centroid of edge
//! crossings) from Dual Contouring (QEF minimizer on Hermite data). We
//! ship [`CentroidPlacement`] as the default; swap the generic parameter
//! on [`crate::SurfaceNetsExtractor`] to plug in a different strategy.

use super::tables::{CUBE_CORNER_VECTORS, CUBE_EDGES};
use bevy::math::Vec3A;

/// Strategy trait for choosing the vertex position inside a sign-change
/// cube. All coords are cube-local in `[0, 1]` along each axis.
pub trait VertexPlacement: Send + Sync + Clone + Copy + 'static {
    /// Place the vertex given the 8 corner SDF values. Returns `None`
    /// when the cube contains no sign change (no vertex should be
    /// emitted).
    fn place(&self, corner_dists: &[f32; 8]) -> Option<Vec3A>;
}

/// Surface-Nets default: vertex at the centroid (arithmetic mean) of
/// the edge crossings. Fast, stable, slightly rounds off sharp features
/// — which is the right trade for organic boss-destructed terrain.
#[derive(Clone, Copy, Debug, Default)]
pub struct CentroidPlacement;

impl VertexPlacement for CentroidPlacement {
    fn place(&self, corner_dists: &[f32; 8]) -> Option<Vec3A> {
        let mut count = 0u32;
        let mut sum = Vec3A::ZERO;

        for &[a, b] in CUBE_EDGES.iter() {
            let d1 = corner_dists[a as usize];
            let d2 = corner_dists[b as usize];
            if (d1 < 0.0) != (d2 < 0.0) {
                count += 1;
                sum += edge_crossing(a, b, d1, d2);
            }
        }

        if count == 0 {
            return None;
        }
        Some(sum / count as f32)
    }
}

/// Linearly interpolate along a cube edge to find the point where the
/// SDF crosses zero. Endpoint values must have opposite signs.
#[inline]
pub fn edge_crossing(corner_a: u32, corner_b: u32, value_a: f32, value_b: f32) -> Vec3A {
    let t_a = value_a / (value_a - value_b);
    let t_b = 1.0 - t_a;
    t_b * CUBE_CORNER_VECTORS[corner_a as usize] + t_a * CUBE_CORNER_VECTORS[corner_b as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_sign_change_returns_none() {
        let all_positive = [1.0_f32; 8];
        assert!(CentroidPlacement.place(&all_positive).is_none());
        let all_negative = [-1.0_f32; 8];
        assert!(CentroidPlacement.place(&all_negative).is_none());
    }

    #[test]
    fn symmetric_sign_change_yields_centered_vertex() {
        // Bottom 4 corners inside (-1), top 4 corners outside (+1).
        // Expected vertex lies at z=0.5 in cube-local space.
        let dists = [-1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0];
        let v = CentroidPlacement.place(&dists).unwrap();
        assert!((v.z - 0.5).abs() < 1e-4);
    }

    #[test]
    fn edge_crossing_is_midpoint_for_symmetric_values() {
        // Corner 0 at (-1) and corner 1 at (+1) → crossing at midpoint
        // along the +X edge.
        let cross = edge_crossing(0b000, 0b001, -1.0, 1.0);
        assert!((cross.x - 0.5).abs() < 1e-4);
    }
}
