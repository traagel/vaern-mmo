//! Voxel store helpers — generator + small wrappers around the
//! `vaern-voxel` resources.
//!
//! The actual `ChunkStore` and `DirtyChunks` resources are inserted by
//! `VoxelCorePlugin` from `vaern-voxel`; we just expose convenience
//! types and the world generator the editor seeds new chunks from.

use bevy::math::Vec3;
use vaern_voxel::generator::WorldGenerator;

/// Flat ground height in world units. Every voxel sample returns
/// `p.y - GROUND_BIAS_Y` — a perfectly horizontal infinite plane.
/// Brushes carve this baseline.
///
/// **Half-voxel offset is deliberate**: with `VOXEL_SIZE = 1.0`, integer
/// Y values land exactly on sample positions and chunk boundaries.
/// An iso-surface at world Y=0 sits on the seam between chunk_y=−1
/// and chunk_y=0, where surface nets either misses it (cube j=1 has
/// samples [0, +1] — both non-negative, no sign change) or extracts
/// degenerate top-edge vertices in chunk_y=−1's padding cube. Result:
/// chunks don't render. `0.5` shifts the plane mid-voxel so cube j=1
/// has clean `−0.5 / +0.5` corners → vertex at exactly Y=0.5.
pub const GROUND_BIAS_Y: f32 = 0.5;

/// Flat-plane world generator. The previous version sampled
/// `vaern_core::terrain::height(x, z)` for analytical noise; that's
/// removed so the editor starts from a clean horizontal slate.
///
/// Side benefit: with no terrain variation, every chunk's surface
/// lands at world Y = `GROUND_BIAS_Y`, which always falls inside the
/// streamer's vertical band — no more "distant chunk invisible
/// because its surface is above the seeded Y range" edge case.
#[derive(Clone, Copy, Debug, Default)]
pub struct EditorHeightfield;

impl WorldGenerator for EditorHeightfield {
    #[inline]
    fn sample(&self, p: Vec3) -> f32 {
        // Negative below the plane (solid), positive above (air).
        p.y - GROUND_BIAS_Y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_heightfield_solid_below_surface() {
        let g = EditorHeightfield;
        // y=-100 is well below the flat plane → deep inside solid.
        let v = g.sample(Vec3::new(0.0, -100.0, 0.0));
        assert!(v < -50.0);
    }

    #[test]
    fn editor_heightfield_air_high_above_surface() {
        let g = EditorHeightfield;
        // y=200 is far above the plane.
        let v = g.sample(Vec3::new(0.0, 200.0, 0.0));
        assert!(v > 100.0);
    }

    #[test]
    fn editor_heightfield_zero_at_plane() {
        let g = EditorHeightfield;
        let v = g.sample(Vec3::new(123.0, GROUND_BIAS_Y, -456.0));
        assert!(v.abs() < 1e-4);
    }
}
