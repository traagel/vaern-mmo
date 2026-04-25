//! Normal estimation strategy.
//!
//! Normals from the SDF gradient are dramatically cheaper than computing
//! face normals from triangle topology and handle creased geometry
//! better near cube boundaries. We ship [`SdfGradientNormals`] as the
//! default; the trait is here so a caller that wants flat-shaded output
//! or externally-computed normals can swap in.

use super::tables::CUBE_CORNER_VECTORS;
use bevy::math::Vec3A;

/// Strategy trait for per-vertex normal estimation.
pub trait NormalStrategy: Send + Sync + Clone + Copy + 'static {
    /// Compute a normal at cube-local position `s` given the 8 corner
    /// SDF values. Output is intentionally not unit-length — downstream
    /// rendering normalizes per-fragment, so we save a sqrt per vertex.
    fn normal(&self, corner_dists: &[f32; 8], s: Vec3A) -> Vec3A;
}

/// Central-differences gradient of the tri-linearly interpolated SDF
/// inside the cube. Exactly reproduces the method in the reference
/// Surface Nets impl.
///
/// The three components of the returned vector are the gradient of the
/// SDF along +X, +Y, +Z. Along each axis the gradient is a bilinear
/// blend of the four parallel edges' endpoint differences.
#[derive(Clone, Copy, Debug, Default)]
pub struct SdfGradientNormals;

impl NormalStrategy for SdfGradientNormals {
    fn normal(&self, dists: &[f32; 8], s: Vec3A) -> Vec3A {
        // The 12 edges group as 4 parallel edges per axis; one endpoint
        // on the -side of that axis, other on the +side. Difference =
        // directional derivative along that edge.
        //
        // Corner bit-encoding: bit 0 = x, bit 1 = y, bit 2 = z.
        // So edges along +X have corners differing in bit 0 only, etc.
        let p00 = Vec3A::from([dists[0b001], dists[0b010], dists[0b100]]);
        let n00 = Vec3A::from([dists[0b000], dists[0b000], dists[0b000]]);

        let p10 = Vec3A::from([dists[0b101], dists[0b011], dists[0b110]]);
        let n10 = Vec3A::from([dists[0b100], dists[0b001], dists[0b010]]);

        let p01 = Vec3A::from([dists[0b011], dists[0b110], dists[0b101]]);
        let n01 = Vec3A::from([dists[0b010], dists[0b100], dists[0b001]]);

        let p11 = Vec3A::from([dists[0b111], dists[0b111], dists[0b111]]);
        let n11 = Vec3A::from([dists[0b110], dists[0b101], dists[0b011]]);

        let d00 = p00 - n00;
        let d10 = p10 - n10;
        let d01 = p01 - n01;
        let d11 = p11 - n11;

        let neg = Vec3A::ONE - s;

        // Bilinear blend of the 4 parallel edges' derivatives per axis.
        bevy::math::Vec3Swizzles::yzx(neg) * bevy::math::Vec3Swizzles::zxy(neg) * d00
            + bevy::math::Vec3Swizzles::yzx(neg) * bevy::math::Vec3Swizzles::zxy(s) * d10
            + bevy::math::Vec3Swizzles::yzx(s) * bevy::math::Vec3Swizzles::zxy(neg) * d01
            + bevy::math::Vec3Swizzles::yzx(s) * bevy::math::Vec3Swizzles::zxy(s) * d11
    }
}

/// Alternative: flat normal from the corner values alone, ignoring the
/// in-cube position. Cheaper; faceted look. Kept as a reference impl
/// for callers that want stylized shading.
#[derive(Clone, Copy, Debug, Default)]
pub struct FlatCornerNormals;

impl NormalStrategy for FlatCornerNormals {
    fn normal(&self, dists: &[f32; 8], _s: Vec3A) -> Vec3A {
        // Sum of corner-weighted directions — negative corners (inside)
        // contribute toward them, positive corners (outside) contribute
        // outward. Direction = -∇ at the cube level.
        let mut n = Vec3A::ZERO;
        for (i, &d) in dists.iter().enumerate() {
            let dir = CUBE_CORNER_VECTORS[i] * 2.0 - Vec3A::ONE;
            n -= dir * d;
        }
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gradient_points_outward_for_horizontal_floor() {
        // Corner-bit encoding: bit 0 = x, bit 1 = y, bit 2 = z.
        // "Floor" splits along y, so corners with y=0 (bits with the
        // y-bit clear) are solid, corners with y=1 are air.
        let mut dists = [0.0_f32; 8];
        for i in 0..8 {
            let y_bit = (i >> 1) & 1;
            dists[i] = if y_bit == 0 { -1.0 } else { 1.0 };
        }
        let n = SdfGradientNormals.normal(&dists, Vec3A::splat(0.5));
        assert!(n.y > 0.0, "expected +y gradient, got {n:?}");
        assert!(n.x.abs() < 1e-4, "unexpected x component: {n:?}");
        assert!(n.z.abs() < 1e-4, "unexpected z component: {n:?}");
    }
}
