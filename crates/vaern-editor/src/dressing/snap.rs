//! Snap helpers for the place / transform-gizmo modes.
//!
//! V1: only `snap_to_grid` lands. `snap_to_ground` lives in
//! `camera::ground_clamp` already and is the canonical helper for
//! XZ → ground-Y queries.

use bevy::math::Vec3;

/// Snap a world position to the nearest multiple of `step` along X and
/// Z. Y is left untouched. Useful for grid-aligned prop placement.
pub fn snap_to_grid(p: Vec3, step: f32) -> Vec3 {
    if step <= 0.0 {
        return p;
    }
    Vec3::new(
        (p.x / step).round() * step,
        p.y,
        (p.z / step).round() * step,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_to_one_meter_grid() {
        let p = Vec3::new(3.7, 5.0, -2.3);
        let s = snap_to_grid(p, 1.0);
        assert_eq!(s, Vec3::new(4.0, 5.0, -2.0));
    }

    #[test]
    fn zero_step_is_identity() {
        let p = Vec3::new(3.7, 5.0, -2.3);
        assert_eq!(snap_to_grid(p, 0.0), p);
    }

    #[test]
    fn snap_preserves_y() {
        let p = Vec3::new(0.0, 99.0, 0.0);
        assert_eq!(snap_to_grid(p, 4.0).y, 99.0);
    }
}
