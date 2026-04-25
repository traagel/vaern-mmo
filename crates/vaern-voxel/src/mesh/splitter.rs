//! Quad-to-triangles splitter.
//!
//! Surface Nets emits quads, but GPUs eat triangles. Splitting a quad
//! along one of its two diagonals is a choice that affects the silhouette
//! of the output mesh — splitting along the shorter diagonal reduces
//! the worst-case triangle aspect ratio and produces a less "kinked"
//! surface when the quad is non-planar.

use bevy::math::Vec3A;

/// Strategy trait for splitting a quad (`v1, v2, v3, v4` — v1/v3 front
/// face, v2/v4 back face when viewed as a face plane) into two
/// triangles. `negative_face` flips the winding when the SDF sign
/// crossing runs the other direction along the edge.
pub trait QuadSplitter: Send + Sync + Clone + Copy + 'static {
    fn split(
        &self,
        v1: u32,
        v2: u32,
        v3: u32,
        v4: u32,
        pos1: Vec3A,
        pos2: Vec3A,
        pos3: Vec3A,
        pos4: Vec3A,
        negative_face: bool,
    ) -> [u32; 6];
}

/// Default: pick the shorter of the two diagonals. Minimizes aspect
/// ratio of the worst triangle.
#[derive(Clone, Copy, Debug, Default)]
pub struct ShortDiagonalSplitter;

impl QuadSplitter for ShortDiagonalSplitter {
    fn split(
        &self,
        v1: u32,
        v2: u32,
        v3: u32,
        v4: u32,
        pos1: Vec3A,
        pos2: Vec3A,
        pos3: Vec3A,
        pos4: Vec3A,
        negative_face: bool,
    ) -> [u32; 6] {
        let d14 = pos1.distance_squared(pos4);
        let d23 = pos2.distance_squared(pos3);
        if d14 < d23 {
            if negative_face {
                [v1, v4, v2, v1, v3, v4]
            } else {
                [v1, v2, v4, v1, v4, v3]
            }
        } else if negative_face {
            [v2, v3, v4, v2, v1, v3]
        } else {
            [v2, v4, v3, v2, v3, v1]
        }
    }
}

/// Alternative: always split along the v1-v4 diagonal. Faster (no
/// distance compare) but can produce more triangle-aspect-ratio extremes
/// when the quad is non-planar.
#[derive(Clone, Copy, Debug, Default)]
pub struct FixedDiagonalSplitter;

impl QuadSplitter for FixedDiagonalSplitter {
    fn split(
        &self,
        v1: u32,
        v2: u32,
        v3: u32,
        v4: u32,
        _pos1: Vec3A,
        _pos2: Vec3A,
        _pos3: Vec3A,
        _pos4: Vec3A,
        negative_face: bool,
    ) -> [u32; 6] {
        if negative_face {
            [v1, v4, v2, v1, v3, v4]
        } else {
            [v1, v2, v4, v1, v4, v3]
        }
    }
}
