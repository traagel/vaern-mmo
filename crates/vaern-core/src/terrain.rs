//! Shared terrain height field.
//!
//! Both client (mesh generation) and server (entity Y-snapping) sample
//! `height` so the two agree on where the ground is at any `(x, z)`.
//! Deterministic, no RNG state, no external crate — amplitude is kept
//! small so differences between the server's Transform Y=0 baseline
//! and an entity snapped onto this surface remain subtle (≤ ~2u) when
//! the resolver is unset (legacy mode).
//!
//! ## Resolver injection
//!
//! [`register_resolver`] lets a higher layer (server / client / editor)
//! plug in the procedural heightfield from `vaern-cartography` so the
//! runtime ground matches the parchment + the editor preview. The
//! resolver is a `Fn(f32, f32) -> f32` closure stored in a OnceLock,
//! settable exactly once per process. When unset, [`height`] falls
//! back to the analytical sin/cos so unit tests in this crate keep
//! passing without ceremony.

use std::sync::OnceLock;

type HeightFn = Box<dyn Fn(f32, f32) -> f32 + Send + Sync + 'static>;
static RESOLVER: OnceLock<HeightFn> = OnceLock::new();

/// Plug in a procedural heightfield. Call once at startup before any
/// voxel chunk is sampled. Subsequent calls are no-ops (OnceLock).
///
/// The closure should be byte-deterministic across machines for AoI
/// replication parity — server and client must produce identical
/// chunks. `vaern-cartography::ZoneTerrain::final_height` satisfies
/// this when both ends share the same YAML + binary.
pub fn register_resolver<F>(f: F)
where
    F: Fn(f32, f32) -> f32 + Send + Sync + 'static,
{
    let _ = RESOLVER.set(Box::new(f));
}

/// `true` when a resolver has been registered. Useful for tests +
/// startup diagnostics.
#[inline]
pub fn resolver_is_registered() -> bool {
    RESOLVER.get().is_some()
}

/// Terrain surface Y at world-space `(x, z)`. Routes through the
/// registered resolver when present; falls back to [`legacy_height`]
/// otherwise. The ground mesh samples this ONLY at 25u grid points
/// (see `GROUND_CELL`) and renders triangles between them, so for
/// overlay geometry that must visually line up with the rendered
/// ground, prefer [`ground_surface_y`].
#[inline]
pub fn height(x: f32, z: f32) -> f32 {
    if let Some(r) = RESOLVER.get() {
        r(x, z)
    } else {
        legacy_height(x, z)
    }
}

/// Analytical sin/cos heightfield, used as the resolver-unset fallback
/// and as a unit-test anchor. Deterministic across runs and machines;
/// amplitude ≤ ~2u.
#[inline]
pub fn legacy_height(x: f32, z: f32) -> f32 {
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
    fn legacy_height_matches_old_formula_at_origin() {
        // Sanity check: legacy fallback is the historical sin/cos.
        let h = legacy_height(0.0, 0.0);
        assert!(h.abs() < 1e-4, "legacy_height(0,0) ≈ 0, got {h}");
    }

    #[test]
    fn height_falls_back_to_legacy_when_resolver_unset() {
        // OnceLock state is shared across tests in the same process;
        // we can't reset it. So this test only runs meaningfully when
        // it's the FIRST test to touch the resolver — it asserts that
        // the resolver-unset path gives the legacy result. If another
        // test set the resolver first, this becomes a no-op.
        if !resolver_is_registered() {
            let h = height(10.0, 20.0);
            let expected = legacy_height(10.0, 20.0);
            assert_eq!(h.to_bits(), expected.to_bits());
        }
    }

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
