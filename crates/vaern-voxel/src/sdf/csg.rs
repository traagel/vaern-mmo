//! Constructive solid geometry over [`SdfField`]s.
//!
//! Each composite takes two fields by value and exposes a new field
//! that evaluates the appropriate min/max combination on demand. All
//! composites are themselves `SdfField`s, so they nest — e.g. a "dug
//! crater in the terrain capped by an arch" is
//! `Union(Subtract(terrain, crater), arch)`.
//!
//! **Smooth variants** use the polynomial-smin formulation from Inigo
//! Quilez — they soften the seam between two fields by a configurable
//! `k` radius. Useful for organic boss deformation where hard
//! set-theoretic edges would read as floating geometry.
//!
//! All composites are `Copy`-friendly when their inner fields are, so
//! they're cheap to pass as values into brushes / generators.

use super::SdfField;
use bevy::math::Vec3;

/// `A ∪ B` — belongs to either field.
pub struct Union<A, B> {
    a: A,
    b: B,
}

impl<A, B> Union<A, B> {
    pub const fn new(a: A, b: B) -> Self {
        Self { a, b }
    }
}

impl<A: SdfField, B: SdfField> SdfField for Union<A, B> {
    #[inline]
    fn sample(&self, p: Vec3) -> f32 {
        self.a.sample(p).min(self.b.sample(p))
    }
}

/// `A \ B` — in A but not in B (carves B out of A).
pub struct Subtract<A, B> {
    a: A,
    b: B,
}

impl<A, B> Subtract<A, B> {
    pub const fn new(a: A, b: B) -> Self {
        Self { a, b }
    }
}

impl<A: SdfField, B: SdfField> SdfField for Subtract<A, B> {
    #[inline]
    fn sample(&self, p: Vec3) -> f32 {
        self.a.sample(p).max(-self.b.sample(p))
    }
}

/// `A ∩ B` — in both fields.
pub struct Intersect<A, B> {
    a: A,
    b: B,
}

impl<A, B> Intersect<A, B> {
    pub const fn new(a: A, b: B) -> Self {
        Self { a, b }
    }
}

impl<A: SdfField, B: SdfField> SdfField for Intersect<A, B> {
    #[inline]
    fn sample(&self, p: Vec3) -> f32 {
        self.a.sample(p).max(self.b.sample(p))
    }
}

/// Smooth union with blend radius `k` (world units).
///
/// Follows the polynomial-smin formulation: as `k → 0` this reduces to
/// [`Union`]; as `k` grows, the seam between the two fields widens
/// into a rounded fillet.
pub struct SmoothUnion<A, B> {
    a: A,
    b: B,
    k: f32,
}

impl<A, B> SmoothUnion<A, B> {
    pub const fn new(a: A, b: B, k: f32) -> Self {
        Self { a, b, k }
    }
}

impl<A: SdfField, B: SdfField> SdfField for SmoothUnion<A, B> {
    #[inline]
    fn sample(&self, p: Vec3) -> f32 {
        let d1 = self.a.sample(p);
        let d2 = self.b.sample(p);
        smooth_min(d1, d2, self.k)
    }
}

/// Smooth subtract with blend radius `k` — rounded-edge crater.
pub struct SmoothSubtract<A, B> {
    a: A,
    b: B,
    k: f32,
}

impl<A, B> SmoothSubtract<A, B> {
    pub const fn new(a: A, b: B, k: f32) -> Self {
        Self { a, b, k }
    }
}

impl<A: SdfField, B: SdfField> SdfField for SmoothSubtract<A, B> {
    #[inline]
    fn sample(&self, p: Vec3) -> f32 {
        let d1 = self.a.sample(p);
        let d2 = self.b.sample(p);
        // smooth_min(-(-d1), -d2, k) with one of the args negated is
        // the standard smooth-subtract identity (max + smoothing).
        -smooth_min(-d1, d2, self.k)
    }
}

/// Polynomial smin — the core smoothing kernel shared by every Smooth*
/// variant. Exposed so custom composites can reuse it.
#[inline]
pub fn smooth_min(a: f32, b: f32, k: f32) -> f32 {
    if k <= 0.0 {
        return a.min(b);
    }
    let h = (0.5 + 0.5 * (b - a) / k).clamp(0.0, 1.0);
    let lerp = b * (1.0 - h) + a * h;
    lerp - k * h * (1.0 - h)
}

#[cfg(test)]
mod tests {
    use super::super::primitive::Sphere;
    use super::*;

    #[test]
    fn union_is_min() {
        let s1 = Sphere::new(Vec3::ZERO, 1.0);
        let s2 = Sphere::new(Vec3::new(5.0, 0.0, 0.0), 1.0);
        let u = Union::new(s1, s2);
        // Inside first sphere → negative.
        assert!(u.sample(Vec3::ZERO) < 0.0);
        // Inside second sphere → negative.
        assert!(u.sample(Vec3::new(5.0, 0.0, 0.0)) < 0.0);
        // Between them → positive.
        assert!(u.sample(Vec3::new(2.5, 0.0, 0.0)) > 0.0);
    }

    #[test]
    fn subtract_removes_b_from_a() {
        let big = Sphere::new(Vec3::ZERO, 5.0);
        let hole = Sphere::new(Vec3::ZERO, 2.0);
        let shell = Subtract::new(big, hole);
        // Center is inside both → subtracted to empty (positive).
        assert!(shell.sample(Vec3::ZERO) > 0.0);
        // Outer shell region is solid.
        assert!(shell.sample(Vec3::new(3.5, 0.0, 0.0)) < 0.0);
    }

    #[test]
    fn smooth_min_degenerates_to_min_at_k_zero() {
        for (a, b) in [(0.0, 1.0), (-2.0, 3.0), (5.0, 5.0)] {
            assert_eq!(smooth_min(a, b, 0.0), a.min(b));
        }
    }
}
