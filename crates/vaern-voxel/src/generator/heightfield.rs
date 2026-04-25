//! Bridge from the existing `vaern_core::terrain` analytical heightmap
//! into a [`WorldGenerator`].
//!
//! The conversion is literal: `sdf(x, y, z) = y - terrain::height(x, z)`.
//! Points above the heightmap read positive (air), points below read
//! negative (solid). The voxel world seeded from this generator will
//! visually match the current ground mesh exactly — it's a drop-in
//! replacement that lets the rest of the codebase swap from the
//! analytical heightmap to the voxel world without visible change.
//!
//! When bolting on boss destructibility, a player-facing boss stomp
//! just needs to `SubtractSphere`-brush into the [`ChunkStore`]; the
//! heightmap-seeded SDF is already loaded and the edit writes over it
//! in the affected chunks.

use super::WorldGenerator;
use bevy::math::Vec3;

/// `WorldGenerator` backed by `vaern_core::terrain::height(x, z)`.
///
/// Carries no state — the height function is a pure f32 → f32 → f32.
#[derive(Clone, Copy, Debug, Default)]
pub struct HeightfieldGenerator;

impl HeightfieldGenerator {
    pub const fn new() -> Self {
        Self
    }
}

impl WorldGenerator for HeightfieldGenerator {
    #[inline]
    fn sample(&self, p: Vec3) -> f32 {
        let h = vaern_core::terrain::height(p.x, p.z);
        p.y - h
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::{ChunkCoord, VoxelChunk};
    use crate::config::PADDING;

    #[test]
    fn points_above_heightmap_are_positive() {
        let generator = HeightfieldGenerator::new();
        // World (0, 100, 0): terrain::height(0,0) ~ 0, so sdf = +100.
        let v = generator.sample(Vec3::new(0.0, 100.0, 0.0));
        assert!(v > 50.0);
    }

    #[test]
    fn points_below_heightmap_are_negative() {
        let generator = HeightfieldGenerator::new();
        let v = generator.sample(Vec3::new(0.0, -100.0, 0.0));
        assert!(v < -50.0);
    }

    #[test]
    fn heightfield_seeds_a_chunk_at_origin_consistently() {
        let generator = HeightfieldGenerator::new();
        let mut chunk = VoxelChunk::new_air();
        generator.seed_chunk(ChunkCoord::new(0, 0, 0), &mut chunk);
        // Content-space (0,0,0) is world (0,0,0). terrain::height(0,0)
        // ~ 0, so sdf ~ 0.
        let v = chunk.get([PADDING, PADDING, PADDING]);
        assert!(v.abs() < 2.0, "expected sdf near surface at origin, got {v}");
    }
}
