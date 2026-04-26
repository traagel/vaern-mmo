//! Voxel-aware ground sampler.
//!
//! Mirrors the ground-clamp logic in `vaern-client/src/scene/camera.rs`:
//! ask the live `ChunkStore` first (catches edits that have changed
//! the surface), fall back to `vaern_core::terrain::height` for chunks
//! outside the streamer radius.
//!
//! Free-fly camera intentionally does *not* clamp by default — flying
//! underground is useful for inspecting carved caves. Higher-level
//! editor modes (e.g. snapping a placed prop to ground) call this
//! directly.

use bevy::math::Vec3;
use vaern_core::terrain;
use vaern_voxel::chunk::ChunkStore;
use vaern_voxel::query::ground_y;

/// Maximum upward probe start, in world units. Mirrors
/// `vaern-server::movement::resolve_ground_y`.
pub const PROBE_TOP: f32 = 64.0;
/// How far below `PROBE_TOP` the probe walks before giving up.
pub const PROBE_MAX_DESCENT: f32 = 96.0;

/// Sample the terrain Y at world (x, z), preferring the voxel store
/// where chunks are seeded, falling back to the analytical heightmap.
pub fn sample_ground_y(store: &ChunkStore, x: f32, z: f32) -> f32 {
    let probe_top = PROBE_TOP;
    ground_y(store, x, z, probe_top, PROBE_MAX_DESCENT)
        .unwrap_or_else(|| terrain::height(x, z))
}

/// Snap a 3D point to the ground at its XZ. Useful when placing a prop
/// at the cursor — caller passes the cursor's hit point, gets back the
/// same XZ at ground level.
pub fn snap_to_ground(store: &ChunkStore, p: Vec3) -> Vec3 {
    Vec3::new(p.x, sample_ground_y(store, p.x, p.z), p.z)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_to_ground_preserves_xz() {
        let store = ChunkStore::new();
        // Empty store → fallback to terrain::height. We don't assert
        // the Y value (depends on terrain), but XZ must round-trip.
        let snapped = snap_to_ground(&store, Vec3::new(12.0, 999.0, -7.0));
        assert!((snapped.x - 12.0).abs() < 1e-5);
        assert!((snapped.z - (-7.0)).abs() < 1e-5);
    }
}
