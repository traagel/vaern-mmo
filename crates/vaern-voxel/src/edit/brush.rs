//! Built-in brush shapes.
//!
//! Each brush has three concerns:
//!
//! 1. **AABB** — the world-space range the brush touches. The edit
//!    loop uses this to limit the voxels it visits.
//! 2. **Sample** — the SDF value the brush thinks should be at a given
//!    point. This is the brush's intrinsic shape.
//! 3. **Blend** — how the sampled value combines with whatever sample
//!    the chunk already has stored. Add-union, subtract, paint-over,
//!    etc.
//!
//! Splitting AABB from sample lets the edit loop skip voxels well
//! outside the brush bounds even when the sample function can
//! evaluate them; splitting sample from blend means the same shape
//! can carve (subtract) or build (union) without reimplementing.

use crate::sdf::{SdfField, primitive::Sphere};
use bevy::math::Vec3;

/// Shape + blend spec for one authoritative edit.
pub trait Brush: Send + Sync {
    /// World-space axis-aligned bounding box of the brush's effect.
    /// Voxels outside this box are never touched.
    fn aabb(&self) -> (Vec3, Vec3);

    /// Evaluate the brush's intrinsic SDF at world point `p`.
    fn sample(&self, p: Vec3) -> f32;

    /// Combine the brush's sampled value with whatever is currently
    /// stored for this voxel. Default [`BrushMode::Union`].
    fn blend(&self, existing: f32, brush_sample: f32) -> f32;
}

/// Standard SDF blend modes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrushMode {
    /// Union — the new world contains both the existing and brush
    /// shapes. `min(existing, brush_sample)` — solid wherever either
    /// was solid.
    Union,
    /// Subtract — carve the brush shape out of the existing world.
    /// `max(existing, -brush_sample)` — solid wherever the existing
    /// was solid AND the brush was NOT solid.
    Subtract,
    /// Intersect — keep only the region where both the existing world
    /// and the brush are solid. Useful for cylindrical mining cores.
    Intersect,
    /// Paint — overwrite the existing value unconditionally with the
    /// brush sample. Useful for generator-replay passes.
    Paint,
}

impl BrushMode {
    #[inline]
    pub fn blend(self, existing: f32, brush_sample: f32) -> f32 {
        match self {
            BrushMode::Union => existing.min(brush_sample),
            BrushMode::Subtract => existing.max(-brush_sample),
            BrushMode::Intersect => existing.max(brush_sample),
            BrushMode::Paint => brush_sample,
        }
    }
}

/// Sphere brush — the workhorse. Boss stomp craters =
/// `SphereBrush { mode: Subtract, ... }`; raising a boss mound =
/// `SphereBrush { mode: Union, ... }`.
#[derive(Clone, Copy, Debug)]
pub struct SphereBrush {
    pub center: Vec3,
    pub radius: f32,
    pub mode: BrushMode,
}

impl Brush for SphereBrush {
    fn aabb(&self) -> (Vec3, Vec3) {
        let r = Vec3::splat(self.radius);
        (self.center - r, self.center + r)
    }

    #[inline]
    fn sample(&self, p: Vec3) -> f32 {
        Sphere::new(self.center, self.radius).sample(p)
    }

    fn blend(&self, existing: f32, brush_sample: f32) -> f32 {
        self.mode.blend(existing, brush_sample)
    }
}

/// Convenience alias: "add a sphere of solid material." Saves boss-code
/// from spelling out `BrushMode::Union`.
pub type AddSphereBrush = SphereBrush;

/// Axis-aligned box brush. For rotated boxes, transform the query
/// point into the box's local frame before calling — the brush itself
/// is AABB-only to keep the inner loop branch-free.
#[derive(Clone, Copy, Debug)]
pub struct BoxBrush {
    pub center: Vec3,
    pub half_extents: Vec3,
    pub mode: BrushMode,
}

impl Brush for BoxBrush {
    fn aabb(&self) -> (Vec3, Vec3) {
        (self.center - self.half_extents, self.center + self.half_extents)
    }

    #[inline]
    fn sample(&self, p: Vec3) -> f32 {
        let d = (p - self.center).abs() - self.half_extents;
        let outside = d.max(Vec3::ZERO).length();
        let inside = d.x.max(d.y.max(d.z)).min(0.0);
        outside + inside
    }

    fn blend(&self, existing: f32, brush_sample: f32) -> f32 {
        self.mode.blend(existing, brush_sample)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brush_mode_union_is_min() {
        assert_eq!(BrushMode::Union.blend(2.0, -3.0), -3.0);
        assert_eq!(BrushMode::Union.blend(-5.0, 1.0), -5.0);
    }

    #[test]
    fn brush_mode_subtract_carves() {
        // Existing = solid (-5), brush = solid (-2) → subtract leaves
        // the region where existing was solid and brush was NOT, i.e.
        // positive (air).
        assert!(BrushMode::Subtract.blend(-5.0, -2.0) > 0.0);
        // Existing = air (+1), brush = solid (-2) → still air.
        assert!(BrushMode::Subtract.blend(1.0, -2.0) > 0.0);
    }
}
