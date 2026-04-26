//! Voxel store helpers — generator + small wrappers around the
//! `vaern-voxel` resources.
//!
//! The actual `ChunkStore` and `DirtyChunks` resources are inserted by
//! `VoxelCorePlugin` from `vaern-voxel`; we just expose convenience
//! types and the world generator the editor seeds new chunks from.

use bevy::math::Vec3;
use vaern_core::terrain;
use vaern_voxel::generator::WorldGenerator;

/// Constant Y bias applied to the heightfield generator's surface.
/// V1 keeps it at 0 to match the client's voxel surface — once the
/// editor wants to author "underground sculpting starts here" decals,
/// it can override this per-zone.
pub const GROUND_BIAS_Y: f32 = 0.0;

/// World generator: signed distance from `vaern_core::terrain` plus a
/// constant Y bias. Identical to `voxel_demo::BiasedHeightfield` in
/// `vaern-client`; duplicated to keep the editor crate independent.
#[derive(Clone, Copy, Debug, Default)]
pub struct EditorHeightfield;

impl WorldGenerator for EditorHeightfield {
    #[inline]
    fn sample(&self, p: Vec3) -> f32 {
        let h = terrain::height(p.x, p.z) + GROUND_BIAS_Y;
        p.y - h
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_heightfield_solid_below_surface() {
        let g = EditorHeightfield;
        // Sample well below the surface (y=-100): should be very
        // negative (deep inside solid).
        let v = g.sample(Vec3::new(0.0, -100.0, 0.0));
        assert!(v < -50.0);
    }

    #[test]
    fn editor_heightfield_air_high_above_surface() {
        let g = EditorHeightfield;
        // y=200 is far above any terrain feature.
        let v = g.sample(Vec3::new(0.0, 200.0, 0.0));
        assert!(v > 100.0);
    }
}
