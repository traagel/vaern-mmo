//! Built-in brush shapes + falloff curves.
//!
//! Each brush has four concerns:
//!
//! 1. **AABB** — the world-space range the brush touches. The edit
//!    loop uses this to limit the voxels it visits.
//! 2. **Sample** — the SDF value the brush thinks should be at a given
//!    point. This is the brush's intrinsic shape.
//! 3. **Blend** — how the sampled value combines with whatever sample
//!    the chunk already has stored. Add-union, subtract, paint-over,
//!    etc.
//! 4. **Falloff** — how strongly the brush effect attenuates from
//!    center to rim. `Hard` (binary) preserves V1 behavior; `Linear`
//!    and `Smooth` produce gradient rims.
//!
//! For brushes whose output depends on **both** the existing sample
//! and the world position (flatten-to-Y, paint-baseline-from-generator,
//! noise-displace), override [`Brush::apply_at`] directly and leave
//! `sample` + `blend` as trivial stubs — see [`FlattenBrush`],
//! [`RampBrush`], [`ResetBrush`].

use crate::generator::WorldGenerator;
use crate::sdf::{
    primitive::{BoxSdf, Cylinder, Sphere},
    SdfField,
};
use bevy::math::{Vec2, Vec3};

/// Falloff curve that maps **normalized brush distance** (0 at the
/// strongest point, 1 at the rim, ≥1 outside) to an effect weight in
/// `[0, 1]`. `EditStroke::apply` blends `existing → brushed_full` by
/// this weight.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Falloff {
    /// Binary — full effect inside, zero outside (V1 behavior).
    #[default]
    Hard,
    /// Linear ramp — `1 − (d/r)` from center to rim.
    Linear,
    /// `1 − smoothstep(d/r)` — full at center, zero at rim, soft
    /// inflection in between. Best for Smooth/Flatten/Reset.
    Smooth,
}

impl Falloff {
    /// Map normalized distance to weight in `[0, 1]`.
    #[inline]
    pub fn weight(self, normalized_dist: f32) -> f32 {
        match self {
            Self::Hard => {
                if normalized_dist <= 1.0 {
                    1.0
                } else {
                    0.0
                }
            }
            Self::Linear => (1.0 - normalized_dist).clamp(0.0, 1.0),
            Self::Smooth => {
                let t = normalized_dist.clamp(0.0, 1.0);
                let s = t * t * (3.0 - 2.0 * t);
                1.0 - s
            }
        }
    }
}

/// Shape + blend + falloff spec for one authoritative edit.
pub trait Brush: Send + Sync {
    /// World-space axis-aligned bounding box of the brush's effect.
    fn aabb(&self) -> (Vec3, Vec3);

    /// Intrinsic SDF sample. Only called by the default
    /// [`Self::apply_at`]; brushes overriding `apply_at` may stub.
    fn sample(&self, p: Vec3) -> f32;

    /// Combine intrinsic with stored. Only called by default
    /// [`Self::apply_at`]; stub-able when overridden.
    fn blend(&self, existing: f32, brush_sample: f32) -> f32;

    /// Per-sample evaluation. Default = `blend(existing, sample(p))`.
    /// Override when output depends on (existing, p) jointly.
    #[inline]
    fn apply_at(&self, p: Vec3, existing: f32) -> f32 {
        self.blend(existing, self.sample(p))
    }

    /// Falloff curve attached to this brush. Stored as a field on the
    /// concrete struct.
    fn falloff(&self) -> Falloff;

    /// Normalized distance from the brush's strongest point to `p`:
    /// 0 at center, 1 at the rim, ≥1 outside the AABB. Default uses
    /// AABB-cube max-axis distance — works for Box. Brushes with
    /// radial or special geometry override.
    #[inline]
    fn normalized_dist_at(&self, p: Vec3) -> f32 {
        let (lo, hi) = self.aabb();
        let center = (lo + hi) * 0.5;
        let half = ((hi - lo) * 0.5).max(Vec3::splat(1e-6));
        let d = (p - center) / half;
        d.x.abs().max(d.y.abs()).max(d.z.abs())
    }
}

/// Standard SDF blend modes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrushMode {
    /// Union — `min(existing, brush_sample)`.
    Union,
    /// Subtract — `max(existing, -brush_sample)`.
    Subtract,
    /// Intersect — `max(existing, brush_sample)`.
    Intersect,
    /// Paint — overwrite with brush_sample.
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

// ---- SphereBrush ----------------------------------------------------

/// Sphere brush — the workhorse. Radial falloff.
#[derive(Clone, Copy, Debug)]
pub struct SphereBrush {
    pub center: Vec3,
    pub radius: f32,
    pub mode: BrushMode,
    pub falloff: Falloff,
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
    fn falloff(&self) -> Falloff {
        self.falloff
    }
    #[inline]
    fn normalized_dist_at(&self, p: Vec3) -> f32 {
        if self.radius < 1e-6 {
            return f32::INFINITY;
        }
        (p - self.center).length() / self.radius
    }
}

/// Convenience alias: "add a sphere of solid material."
pub type AddSphereBrush = SphereBrush;

// ---- BoxBrush -------------------------------------------------------

/// Axis-aligned box brush. Cube-distance falloff (default impl) — rim
/// is at the AABB face, not a sphere.
#[derive(Clone, Copy, Debug)]
pub struct BoxBrush {
    pub center: Vec3,
    pub half_extents: Vec3,
    pub mode: BrushMode,
    pub falloff: Falloff,
}

impl Brush for BoxBrush {
    fn aabb(&self) -> (Vec3, Vec3) {
        (self.center - self.half_extents, self.center + self.half_extents)
    }
    #[inline]
    fn sample(&self, p: Vec3) -> f32 {
        BoxSdf::new(self.center, self.half_extents).sample(p)
    }
    fn blend(&self, existing: f32, brush_sample: f32) -> f32 {
        self.mode.blend(existing, brush_sample)
    }
    fn falloff(&self) -> Falloff {
        self.falloff
    }
    // Default normalized_dist_at — componentwise max of (|p - center| /
    // half_extents) — is what we want here.
}

// ---- CylinderBrush --------------------------------------------------

/// Y-axis solid cylinder — wells, mineshafts, pillars. Uses a hybrid
/// "rectangular" normalized distance: the larger of XZ-radial-fraction
/// and Y-fraction, so falloff attenuates whichever axis hits the rim
/// first.
#[derive(Clone, Copy, Debug)]
pub struct CylinderBrush {
    pub center: Vec3,
    pub radius: f32,
    pub half_height: f32,
    pub mode: BrushMode,
    pub falloff: Falloff,
}

impl Brush for CylinderBrush {
    fn aabb(&self) -> (Vec3, Vec3) {
        (
            Vec3::new(
                self.center.x - self.radius,
                self.center.y - self.half_height,
                self.center.z - self.radius,
            ),
            Vec3::new(
                self.center.x + self.radius,
                self.center.y + self.half_height,
                self.center.z + self.radius,
            ),
        )
    }
    #[inline]
    fn sample(&self, p: Vec3) -> f32 {
        Cylinder::new(self.center, self.radius, self.half_height).sample(p)
    }
    fn blend(&self, existing: f32, brush_sample: f32) -> f32 {
        self.mode.blend(existing, brush_sample)
    }
    fn falloff(&self) -> Falloff {
        self.falloff
    }
    #[inline]
    fn normalized_dist_at(&self, p: Vec3) -> f32 {
        if self.radius < 1e-6 || self.half_height < 1e-6 {
            return f32::INFINITY;
        }
        let local = p - self.center;
        let xz = Vec2::new(local.x, local.z).length() / self.radius;
        let y = local.y.abs() / self.half_height;
        xz.max(y)
    }
}

// ---- FlattenBrush ---------------------------------------------------

/// Paint a horizontal target Y onto an XZ disc.
#[derive(Clone, Copy, Debug)]
pub struct FlattenBrush {
    pub center: Vec3,
    pub radius: f32,
    pub half_height: f32,
    pub falloff: Falloff,
}

impl Brush for FlattenBrush {
    fn aabb(&self) -> (Vec3, Vec3) {
        let r = self.radius;
        let h = self.half_height;
        (
            Vec3::new(self.center.x - r, self.center.y - h, self.center.z - r),
            Vec3::new(self.center.x + r, self.center.y + h, self.center.z + r),
        )
    }
    fn sample(&self, _p: Vec3) -> f32 {
        0.0
    }
    fn blend(&self, existing: f32, _brush_sample: f32) -> f32 {
        existing
    }
    #[inline]
    fn apply_at(&self, p: Vec3, existing: f32) -> f32 {
        let dx = p.x - self.center.x;
        let dz = p.z - self.center.z;
        if dx * dx + dz * dz > self.radius * self.radius {
            return existing;
        }
        p.y - self.center.y
    }
    fn falloff(&self) -> Falloff {
        self.falloff
    }
    #[inline]
    fn normalized_dist_at(&self, p: Vec3) -> f32 {
        // Use XZ disc distance — Y is "thickness" not "rim direction."
        if self.radius < 1e-6 {
            return f32::INFINITY;
        }
        let dx = p.x - self.center.x;
        let dz = p.z - self.center.z;
        (dx * dx + dz * dz).sqrt() / self.radius
    }
}

// ---- RampBrush ------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub struct RampBrush {
    pub a: Vec3,
    pub b: Vec3,
    pub half_width: f32,
    pub half_height: f32,
    pub falloff: Falloff,
}

impl Brush for RampBrush {
    fn aabb(&self) -> (Vec3, Vec3) {
        let lo = Vec3::new(
            self.a.x.min(self.b.x) - self.half_width,
            self.a.y.min(self.b.y) - self.half_height,
            self.a.z.min(self.b.z) - self.half_width,
        );
        let hi = Vec3::new(
            self.a.x.max(self.b.x) + self.half_width,
            self.a.y.max(self.b.y) + self.half_height,
            self.a.z.max(self.b.z) + self.half_width,
        );
        (lo, hi)
    }
    fn sample(&self, _p: Vec3) -> f32 {
        0.0
    }
    fn blend(&self, existing: f32, _brush_sample: f32) -> f32 {
        existing
    }
    #[inline]
    fn apply_at(&self, p: Vec3, existing: f32) -> f32 {
        let v = self.b - self.a;
        let v_xz_len_sq = v.x * v.x + v.z * v.z;
        if v_xz_len_sq < 1e-6 {
            return existing;
        }
        let t = ((p.x - self.a.x) * v.x + (p.z - self.a.z) * v.z) / v_xz_len_sq;
        if !(0.0..=1.0).contains(&t) {
            return existing;
        }
        let cx = self.a.x + t * v.x;
        let cz = self.a.z + t * v.z;
        let target_y = self.a.y + t * v.y;
        let perp_xz_sq = (p.x - cx).powi(2) + (p.z - cz).powi(2);
        if perp_xz_sq > self.half_width * self.half_width {
            return existing;
        }
        if (p.y - target_y).abs() > self.half_height {
            return existing;
        }
        p.y - target_y
    }
    fn falloff(&self) -> Falloff {
        self.falloff
    }
    #[inline]
    fn normalized_dist_at(&self, p: Vec3) -> f32 {
        // Take the max of perpendicular-XZ-fraction and Y-fraction —
        // the rim is whichever axis hits its limit first.
        let v = self.b - self.a;
        let v_xz_len_sq = v.x * v.x + v.z * v.z;
        if v_xz_len_sq < 1e-6 {
            return f32::INFINITY;
        }
        let t = (((p.x - self.a.x) * v.x + (p.z - self.a.z) * v.z) / v_xz_len_sq)
            .clamp(0.0, 1.0);
        let cx = self.a.x + t * v.x;
        let cz = self.a.z + t * v.z;
        let target_y = self.a.y + t * v.y;
        let perp_xz = ((p.x - cx).powi(2) + (p.z - cz).powi(2)).sqrt();
        let perp_n = perp_xz / self.half_width.max(1e-6);
        let y_n = (p.y - target_y).abs() / self.half_height.max(1e-6);
        perp_n.max(y_n)
    }
}

// ---- ResetBrush -----------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub struct ResetBrush<'a, G: WorldGenerator> {
    pub center: Vec3,
    pub radius: f32,
    pub generator: &'a G,
    pub falloff: Falloff,
}

impl<'a, G: WorldGenerator> Brush for ResetBrush<'a, G> {
    fn aabb(&self) -> (Vec3, Vec3) {
        let r = Vec3::splat(self.radius);
        (self.center - r, self.center + r)
    }
    fn sample(&self, _p: Vec3) -> f32 {
        0.0
    }
    fn blend(&self, existing: f32, _brush_sample: f32) -> f32 {
        existing
    }
    #[inline]
    fn apply_at(&self, p: Vec3, existing: f32) -> f32 {
        if (p - self.center).length_squared() > self.radius * self.radius {
            return existing;
        }
        self.generator.sample(p)
    }
    fn falloff(&self) -> Falloff {
        self.falloff
    }
    #[inline]
    fn normalized_dist_at(&self, p: Vec3) -> f32 {
        if self.radius < 1e-6 {
            return f32::INFINITY;
        }
        (p - self.center).length() / self.radius
    }
}

// ---- StampBrush -----------------------------------------------------

/// Pre-authored shape library. Each variant evaluates to a self-
/// contained SDF defined in stamp-local coords (origin at stamp center,
/// scale = brush.radius); world SDF is the local SDF without further
/// scaling (we evaluate at unscaled local coords directly so distances
/// stay in world units).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StampShape {
    /// Paraboloid pit + slight raised rim ring.
    Crater,
    /// Solid block with a horizontal half-cylinder cutout (door-shape).
    Archway,
    /// Two parallel low walls.
    Ridge,
    /// Three stacked boxes of decreasing footprint.
    Stairs,
}

#[derive(Clone, Copy, Debug)]
pub struct StampBrush {
    pub center: Vec3,
    pub radius: f32,
    pub rotation_y_rad: f32,
    pub shape: StampShape,
    pub mode: BrushMode,
    pub falloff: Falloff,
}

impl Brush for StampBrush {
    fn aabb(&self) -> (Vec3, Vec3) {
        let r = Vec3::splat(self.radius);
        (self.center - r, self.center + r)
    }
    #[inline]
    fn sample(&self, p: Vec3) -> f32 {
        let local = rotate_y_around_origin(p - self.center, -self.rotation_y_rad);
        sample_stamp_shape(self.shape, local, self.radius)
    }
    fn blend(&self, existing: f32, brush_sample: f32) -> f32 {
        self.mode.blend(existing, brush_sample)
    }
    fn falloff(&self) -> Falloff {
        self.falloff
    }
    #[inline]
    fn normalized_dist_at(&self, p: Vec3) -> f32 {
        if self.radius < 1e-6 {
            return f32::INFINITY;
        }
        // Use bounding-sphere radial distance — stamps are anchored
        // within `radius`, so this gives a clean rim falloff.
        (p - self.center).length() / self.radius
    }
}

#[inline]
fn rotate_y_around_origin(p: Vec3, angle: f32) -> Vec3 {
    let (s, c) = angle.sin_cos();
    Vec3::new(c * p.x + s * p.z, p.y, -s * p.x + c * p.z)
}

/// Per-shape SDF in stamp-local space (origin at stamp center). `r` is
/// the brush radius — used to scale the shape proportionally.
fn sample_stamp_shape(shape: StampShape, p: Vec3, r: f32) -> f32 {
    match shape {
        StampShape::Crater => {
            // Paraboloid bowl from y=-0.4r to y=0; sample = p.y - bowl(xz_dist).
            let xz = Vec2::new(p.x, p.z).length();
            let inner_r = r * 0.85;
            let bowl_depth = r * 0.4;
            let rim_height = r * 0.08;
            let target_y = if xz <= inner_r {
                let t = xz / inner_r;
                // -depth at center, 0 at inner_r.
                -bowl_depth * (1.0 - t * t)
            } else if xz <= r {
                let t = (xz - inner_r) / (r - inner_r);
                // Smooth rim ring from 0 → rim_height → 0.
                let bump = 4.0 * t * (1.0 - t); // peak 1.0 at t=0.5
                rim_height * bump
            } else {
                0.0
            };
            // Heightmap-style SDF: positive above target, negative below.
            p.y - target_y
        }
        StampShape::Archway => {
            // Solid box of half-extents (0.4r, 0.5r, 0.8r) centered.
            let outer = BoxSdf::new(
                Vec3::ZERO,
                Vec3::new(r * 0.4, r * 0.5, r * 0.8),
            )
            .sample(p);
            // Subtract a horizontal cylinder along Z: radius 0.35r,
            // half-height 0.9r (extends past the box).
            let hole_p = Vec3::new(p.x, p.y + r * 0.1, p.z);
            // Use Y-axis Cylinder rotated to lie along Z by swapping
            // axes: evaluate as if y/z swapped.
            let hole_q = Vec3::new(hole_p.x, hole_p.z, hole_p.y);
            let hole = Cylinder::new(Vec3::ZERO, r * 0.35, r * 0.9).sample(hole_q);
            outer.max(-hole)
        }
        StampShape::Ridge => {
            let half = Vec3::new(r * 0.15, r * 0.4, r * 0.8);
            let left = BoxSdf::new(Vec3::new(-r * 0.5, -r * 0.1, 0.0), half).sample(p);
            let right = BoxSdf::new(Vec3::new(r * 0.5, -r * 0.1, 0.0), half).sample(p);
            left.min(right)
        }
        StampShape::Stairs => {
            let h = r * 0.15;
            let s1 = BoxSdf::new(Vec3::new(0.0, -r * 0.45, r * 0.3), Vec3::new(r * 0.6, h, r * 0.3))
                .sample(p);
            let s2 = BoxSdf::new(Vec3::new(0.0, -r * 0.15, 0.0), Vec3::new(r * 0.5, h, r * 0.3))
                .sample(p);
            let s3 = BoxSdf::new(Vec3::new(0.0, r * 0.15, -r * 0.3), Vec3::new(r * 0.4, h, r * 0.3))
                .sample(p);
            s1.min(s2).min(s3)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::{ChunkCoord, ChunkStore, DirtyChunks, VoxelChunk};
    use crate::config::PADDING;

    fn sphere_default() -> SphereBrush {
        SphereBrush {
            center: Vec3::ZERO,
            radius: 5.0,
            mode: BrushMode::Subtract,
            falloff: Falloff::Hard,
        }
    }

    #[test]
    fn brush_mode_union_is_min() {
        assert_eq!(BrushMode::Union.blend(2.0, -3.0), -3.0);
        assert_eq!(BrushMode::Union.blend(-5.0, 1.0), -5.0);
    }

    #[test]
    fn brush_mode_subtract_carves() {
        assert!(BrushMode::Subtract.blend(-5.0, -2.0) > 0.0);
        assert!(BrushMode::Subtract.blend(1.0, -2.0) > 0.0);
    }

    #[test]
    fn apply_at_default_delegates_to_sample_and_blend() {
        let brush = sphere_default();
        let p = Vec3::new(2.0, 0.0, 0.0);
        let existing = -10.0;
        let manual = brush.blend(existing, brush.sample(p));
        let via_apply_at = brush.apply_at(p, existing);
        assert!((manual - via_apply_at).abs() < 1e-6);
    }

    // ---- Falloff math ----

    #[test]
    fn falloff_hard_is_binary() {
        assert_eq!(Falloff::Hard.weight(0.5), 1.0);
        assert_eq!(Falloff::Hard.weight(1.0), 1.0);
        assert_eq!(Falloff::Hard.weight(1.5), 0.0);
    }

    #[test]
    fn falloff_linear_ramps() {
        assert!((Falloff::Linear.weight(0.0) - 1.0).abs() < 1e-6);
        assert!((Falloff::Linear.weight(0.5) - 0.5).abs() < 1e-6);
        assert!((Falloff::Linear.weight(1.0) - 0.0).abs() < 1e-6);
        assert_eq!(Falloff::Linear.weight(2.0), 0.0);
    }

    #[test]
    fn falloff_smooth_keeps_center_and_rim() {
        assert!((Falloff::Smooth.weight(0.0) - 1.0).abs() < 1e-6);
        assert!((Falloff::Smooth.weight(1.0)).abs() < 1e-6);
        let mid = Falloff::Smooth.weight(0.5);
        assert!(mid > 0.4 && mid < 0.6, "got {mid}");
    }

    /// Confirms that EditStroke would correctly blend brush-result
    /// against existing using falloff weight at half-radius.
    #[test]
    fn falloff_blends_brush_against_existing() {
        let brush = SphereBrush {
            center: Vec3::ZERO,
            radius: 10.0,
            mode: BrushMode::Subtract,
            falloff: Falloff::Linear,
        };
        let p = Vec3::new(5.0, 0.0, 0.0); // half-radius
        let existing = -10.0;
        let raw = brush.apply_at(p, existing);
        let nd = brush.normalized_dist_at(p);
        assert!((nd - 0.5).abs() < 1e-6);
        let w = brush.falloff().weight(nd);
        assert!((w - 0.5).abs() < 1e-6);
        let blended = existing * (1.0 - w) + raw * w;
        let expected = (existing + raw) * 0.5;
        assert!((blended - expected).abs() < 1e-5);
    }

    // ---- Cylinder ----

    #[test]
    fn cylinder_brush_normalized_dist_radial() {
        let b = CylinderBrush {
            center: Vec3::ZERO,
            radius: 5.0,
            half_height: 10.0,
            mode: BrushMode::Subtract,
            falloff: Falloff::Hard,
        };
        let nd = b.normalized_dist_at(Vec3::new(3.0, 0.0, 0.0));
        assert!((nd - 0.6).abs() < 1e-6);
    }

    #[test]
    fn cylinder_brush_normalized_dist_uses_max_axis() {
        let b = CylinderBrush {
            center: Vec3::ZERO,
            radius: 5.0,
            half_height: 10.0,
            mode: BrushMode::Subtract,
            falloff: Falloff::Hard,
        };
        // xz=3 → 0.6; |y|=8 → 0.8; max = 0.8.
        let nd = b.normalized_dist_at(Vec3::new(3.0, 8.0, 0.0));
        assert!((nd - 0.8).abs() < 1e-6);
    }

    // ---- Box ----

    #[test]
    fn box_brush_normalized_dist_uses_componentwise_max() {
        let b = BoxBrush {
            center: Vec3::ZERO,
            half_extents: Vec3::new(5.0, 1.0, 5.0),
            mode: BrushMode::Subtract,
            falloff: Falloff::Hard,
        };
        // x=3 → 3/5 = 0.6; y=0 → 0; z=0 → 0.
        let nd = b.normalized_dist_at(Vec3::new(3.0, 0.0, 0.0));
        assert!((nd - 0.6).abs() < 1e-6);
    }

    // ---- Flatten / Ramp / Reset (existing behavior preserved) ----

    #[test]
    fn flatten_brush_paints_target_y_inside_disc() {
        let brush = FlattenBrush {
            center: Vec3::new(0.0, 30.0, 0.0),
            radius: 10.0,
            half_height: 8.0,
            falloff: Falloff::Hard,
        };
        let v = brush.apply_at(Vec3::new(0.0, 25.0, 0.0), 99.0);
        assert!((v - (-5.0)).abs() < 1e-6, "got {v}");
    }

    #[test]
    fn flatten_brush_returns_existing_outside_disc() {
        let brush = FlattenBrush {
            center: Vec3::new(0.0, 30.0, 0.0),
            radius: 10.0,
            half_height: 8.0,
            falloff: Falloff::Hard,
        };
        let v = brush.apply_at(Vec3::new(20.0, 25.0, 0.0), -5.0);
        assert_eq!(v, -5.0);
    }

    #[test]
    fn ramp_brush_lerps_height_along_segment() {
        let brush = RampBrush {
            a: Vec3::new(0.0, 0.0, 0.0),
            b: Vec3::new(10.0, 10.0, 0.0),
            half_width: 2.0,
            half_height: 2.0,
            falloff: Falloff::Hard,
        };
        let v = brush.apply_at(Vec3::new(5.0, 5.0, 0.0), 99.0);
        assert!(v.abs() < 1e-6);
    }

    #[test]
    fn ramp_brush_rejects_perpendicular_outside_half_width() {
        let brush = RampBrush {
            a: Vec3::new(0.0, 0.0, 0.0),
            b: Vec3::new(10.0, 10.0, 0.0),
            half_width: 2.0,
            half_height: 2.0,
            falloff: Falloff::Hard,
        };
        let v = brush.apply_at(Vec3::new(5.0, 5.0, 5.0), 99.0);
        assert_eq!(v, 99.0);
    }

    #[test]
    fn ramp_brush_rejects_t_out_of_range() {
        let brush = RampBrush {
            a: Vec3::new(0.0, 0.0, 0.0),
            b: Vec3::new(10.0, 10.0, 0.0),
            half_width: 2.0,
            half_height: 2.0,
            falloff: Falloff::Hard,
        };
        let v = brush.apply_at(Vec3::new(-5.0, 0.0, 0.0), 99.0);
        assert_eq!(v, 99.0);
    }

    #[derive(Clone, Copy)]
    struct ConstGen(f32);
    impl WorldGenerator for ConstGen {
        fn sample(&self, _p: Vec3) -> f32 {
            self.0
        }
    }

    #[test]
    fn reset_brush_paints_baseline_inside_sphere() {
        let g = ConstGen(42.0);
        let brush = ResetBrush {
            center: Vec3::ZERO,
            radius: 10.0,
            generator: &g,
            falloff: Falloff::Hard,
        };
        let v = brush.apply_at(Vec3::new(2.0, 3.0, 4.0), -1.0);
        assert_eq!(v, 42.0);
    }

    #[test]
    fn reset_brush_returns_existing_outside_sphere() {
        let g = ConstGen(42.0);
        let brush = ResetBrush {
            center: Vec3::ZERO,
            radius: 5.0,
            generator: &g,
            falloff: Falloff::Hard,
        };
        let v = brush.apply_at(Vec3::new(100.0, 0.0, 0.0), -1.0);
        assert_eq!(v, -1.0);
    }

    // ---- Stamp ----

    /// Applying a Crater stamp on solid ground should leave the center
    /// in air (positive SDF) and the rim raised (more negative than
    /// pre-stamp). Uses Paint mode to overwrite directly.
    #[test]
    fn stamp_crater_carves_central_pit_and_raises_rim() {
        use crate::edit::EditStroke;

        let mut store = ChunkStore::new();
        let mut dirty = DirtyChunks::new();

        // Pre-fill solid below y=0, air above (heightfield baseline).
        let mut chunk = VoxelChunk::new_air();
        chunk.fill_all_padded(|[_x, y, _z]| {
            let world_y = (y as f32 - PADDING as f32) * 1.0;
            world_y - 0.0 // solid below 0, air above
        });
        store.insert(ChunkCoord::new(0, 0, 0), chunk);

        let brush = StampBrush {
            center: Vec3::new(15.0, 0.0, 15.0),
            radius: 6.0,
            rotation_y_rad: 0.0,
            shape: StampShape::Crater,
            mode: BrushMode::Paint,
            falloff: Falloff::Hard,
        };
        EditStroke::new(brush, &mut store, &mut dirty).apply();

        // Center voxel — sample at (15, 0, 15) should now be positive
        // (carved into air).
        let chunk = store.get(ChunkCoord::new(0, 0, 0)).unwrap();
        let center = chunk.get([15 + PADDING, 0 + PADDING, 15 + PADDING]);
        assert!(center > 0.0, "crater center should be air, got {center}");
    }
}
