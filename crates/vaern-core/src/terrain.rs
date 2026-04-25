//! Shared terrain height field.
//!
//! Both client (mesh generation) and server (entity Y-snapping) sample
//! `height` so the two agree on where the ground is at any `(x, z)`.
//! Deterministic, no RNG state, no external crate — amplitude is kept
//! small so differences between the server's Transform Y=0 baseline
//! and an entity snapped onto this surface remain subtle (≤ ~2u).
//!
//! When swapping to a real heightmap or tiled noise, replace `height`
//! here and the rest of the workspace picks it up automatically.

/// Terrain surface Y at world-space `(x, z)` — the analytical smooth
/// height field. The ground mesh samples this ONLY at 25u grid points
/// (see `GROUND_CELL`) and then renders triangles between those points,
/// so for overlay geometry that needs to visually line up with the
/// rendered ground, prefer [`ground_surface_y`].
#[inline]
pub fn height(x: f32, z: f32) -> f32 {
    let low = (x * 0.03).sin() * (z * 0.03).cos() * 1.2;
    let mid = (x * 0.07).sin() * (z * 0.09).cos() * 0.6;
    low + mid
}

/// Client ground mesh parameters — must stay in sync with
/// `vaern-client/src/scene/ground.rs` so `ground_surface_y` reproduces
/// the exact rendered surface.
pub const GROUND_SIZE: f32 = 8000.0;
pub const GROUND_HALF: f32 = GROUND_SIZE * 0.5;
pub const GROUND_CELL: f32 = 25.0;

/// Triangle-interpolated Y at `(x, z)` matching what the client ground
/// mesh actually renders. Between 25u grid points the ground is a flat
/// triangle (two per quad, diagonal TL→BR), so a point inside the quad
/// gets its Y from the barycentric blend of the three triangle corners
/// rather than the smooth `height` field.
///
/// Use this for overlay meshes (hub floor patches, road ribbons) that
/// must hug the visible ground without drifting above or below it.
pub fn ground_surface_y(x: f32, z: f32) -> f32 {
    // Locate (x, z) in ground-grid coords.
    let fx = (x + GROUND_HALF) / GROUND_CELL;
    let fz = (z + GROUND_HALF) / GROUND_CELL;
    let ix = fx.floor();
    let iz = fz.floor();
    let tx = (fx - ix).clamp(0.0, 1.0);
    let tz = (fz - iz).clamp(0.0, 1.0);

    let x0 = ix * GROUND_CELL - GROUND_HALF;
    let x1 = x0 + GROUND_CELL;
    let z0 = iz * GROUND_CELL - GROUND_HALF;
    let z1 = z0 + GROUND_CELL;

    let y_tl = height(x0, z0);
    let y_tr = height(x1, z0);
    let y_bl = height(x0, z1);
    let y_br = height(x1, z1);

    // ground.rs builds each quad as two triangles:
    //   T1: TL, BL, BR  (lower-left half, tz >= tx)
    //   T2: TL, BR, TR  (upper-right half, tx >= tz)
    // Both triangles share the TL–BR diagonal.
    if tx >= tz {
        // T2: alpha_TL = 1 - tx, beta_BR = tz, gamma_TR = tx - tz
        y_tl * (1.0 - tx) + y_br * tz + y_tr * (tx - tz)
    } else {
        // T1: alpha_TL = 1 - tz, beta_BL = tz - tx, gamma_BR = tx
        y_tl * (1.0 - tz) + y_bl * (tz - tx) + y_br * tx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ground_surface_matches_at_grid_corners() {
        // At grid corners the triangle interp should return exactly
        // `height(x, z)` (same formula, same point).
        for &(x, z) in &[(0.0, 0.0), (25.0, 25.0), (50.0, -25.0), (-100.0, 75.0)] {
            let h = height(x, z);
            let g = ground_surface_y(x, z);
            assert!((h - g).abs() < 1e-4, "grid-corner mismatch at ({x}, {z}): {h} vs {g}");
        }
    }

    #[test]
    fn ground_surface_interpolates_linearly_along_diagonal() {
        // On the TL–BR diagonal the two triangles share the same formula,
        // and the interp should be a linear blend of the two corners.
        let x0 = 0.0;
        let z0 = 0.0;
        let x1 = GROUND_CELL;
        let z1 = GROUND_CELL;
        let y_tl = height(x0, z0);
        let y_br = height(x1, z1);
        for t in [0.1, 0.25, 0.5, 0.75, 0.9] {
            let mid_x = x0 + (x1 - x0) * t;
            let mid_z = z0 + (z1 - z0) * t;
            let g = ground_surface_y(mid_x, mid_z);
            let expected = y_tl * (1.0 - t) + y_br * t;
            assert!((g - expected).abs() < 1e-3, "diagonal mismatch at t={t}");
        }
    }
}
