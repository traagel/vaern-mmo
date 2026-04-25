//! Signed-distance-field sample source.
//!
//! An [`SdfField`] is anything that can answer "what is the signed
//! distance to the nearest surface at this world-space point?" Concrete
//! impls come in three shapes:
//!
//! * **Analytic primitives** (`primitive::*`) — closed-form SDFs for
//!   spheres, boxes, capsules, planes, heightfield extrusions. Used by
//!   generators and brushes.
//! * **Composites** (`csg::*`) — union / subtract / intersect + smooth
//!   variants, each `SdfField`-in-`SdfField`-out. Boss stomp craters =
//!   `Subtract(existing_world, Sphere(impact_point, radius))`.
//! * **Stored** ([`ChunkField`]) — sample grid loaded from a
//!   [`crate::VoxelChunk`]. Trilinear interpolation between the 8
//!   surrounding samples. This is the authoritative "what the world
//!   looks like right now" source after edits.
//!
//! All `SdfField` impls are coord-independent (take world `Vec3`, return
//! signed distance). The meshing pass walks a chunk's sample grid, not
//! the field directly, but the same trait generates those samples from
//! either a generator (initial seed) or a CSG composite (edit overlay).

pub mod csg;
pub mod primitive;

pub use csg::{Intersect, SmoothSubtract, SmoothUnion, Subtract, Union};
pub use primitive::{BoxSdf, Capsule, Plane, Sphere};

use bevy::math::Vec3;

/// Scalar type of one SDF sample. Alias rather than a generic so the
/// whole crate agrees on one representation; swap to `i16` fixed-point
/// later if memory ever becomes the dominant cost, by changing this
/// alias + the `into_f32` / `from_f32` helpers.
pub type SdfValue = f32;

/// Convert a typed SDF value to f32 world-unit distance.
#[inline]
pub const fn into_f32(v: SdfValue) -> f32 {
    v
}

/// Convert a raw f32 world-unit distance into an [`SdfValue`].
#[inline]
pub const fn from_f32(v: f32) -> SdfValue {
    v
}

/// Anything that can answer the SDF query "distance to surface at p".
///
/// Implementors must be coord-independent — the value at `p` should not
/// depend on where the caller is sampling from or in what order. That
/// makes CSG composition stateless and parallel-safe.
pub trait SdfField: Send + Sync {
    /// Signed distance to the nearest surface at world-space point `p`.
    /// Negative = inside solid, positive = in air, zero = on surface.
    fn sample(&self, p: Vec3) -> f32;
}

/// Stored sample grid of a [`crate::VoxelChunk`], queried as an
/// [`SdfField`] via trilinear interpolation.
///
/// Wraps a chunk + its origin so a brush or mesher can sample the stored
/// field at arbitrary world-space points (not just on-grid samples).
pub struct ChunkField<'a> {
    chunk: &'a crate::VoxelChunk,
    origin_world: Vec3,
}

impl<'a> ChunkField<'a> {
    /// Create a field view of `chunk`, positioning its (0,0,0) sample
    /// origin at `origin_world` in world space.
    pub fn new(chunk: &'a crate::VoxelChunk, origin_world: Vec3) -> Self {
        Self { chunk, origin_world }
    }
}

impl<'a> SdfField for ChunkField<'a> {
    fn sample(&self, p: Vec3) -> f32 {
        let local = (p - self.origin_world) / crate::VOXEL_SIZE;
        self.chunk.sample_trilinear(local)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sphere_outside_is_positive_inside_is_negative() {
        let s = Sphere::new(Vec3::ZERO, 5.0);
        assert!(s.sample(Vec3::new(10.0, 0.0, 0.0)) > 0.0);
        assert!(s.sample(Vec3::ZERO) < 0.0);
        assert!(s.sample(Vec3::new(5.0, 0.0, 0.0)).abs() < 1e-4);
    }

    #[test]
    fn subtract_is_composition_of_fields() {
        let ground = Plane::horizontal(0.0);
        let crater = Sphere::new(Vec3::new(0.0, 0.0, 0.0), 3.0);
        let combined = Subtract::new(ground, crater);
        // Inside the crater, just below ground — should now read as air
        // (positive) because we subtracted the sphere from the ground.
        assert!(combined.sample(Vec3::new(0.0, -1.0, 0.0)) > 0.0);
        // Well outside the crater but underground — still solid.
        assert!(combined.sample(Vec3::new(100.0, -1.0, 0.0)) < 0.0);
    }
}
