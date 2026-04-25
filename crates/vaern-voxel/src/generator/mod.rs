//! World-generation adapter — source of the *initial* SDF samples
//! before any edits are applied.
//!
//! A [`WorldGenerator`] exposes one method, [`sample`](WorldGenerator::sample),
//! that answers "what is the signed distance here if nothing has been
//! edited?" The default impl [`HeightfieldGenerator`] bridges the
//! existing `vaern_core::terrain` analytical heightmap into a 3D SDF so
//! the voxel world seeds exactly as the current scaffold looks today.
//!
//! Seeding a chunk happens lazily: when a system asks the
//! [`crate::ChunkStore`] for a chunk that isn't present, the generator
//! is invoked to populate every sample in its padded array. From that
//! point on, the chunk is editable — brush ops write into the stored
//! samples, diverging from the generator permanently.
//!
//! Keeping the generator and the edit layer separate is what makes
//! "boss carves a permanent crater" work: the generator says "there's
//! a hill here," the brush subtracts a sphere, the chunk remembers the
//! carved shape forever without needing to re-ask the generator.

pub mod heightfield;

pub use heightfield::HeightfieldGenerator;

use crate::chunk::{ChunkCoord, VoxelChunk};
use crate::config::{PADDING, VOXEL_SIZE};
use crate::sdf::SdfField;
use bevy::math::Vec3;

/// Initial world SDF source.
pub trait WorldGenerator: Send + Sync {
    /// Signed distance at world-space point `p` *if no edits have been
    /// applied*. Negative inside solid, positive in air.
    fn sample(&self, p: Vec3) -> f32;

    /// Fill a fresh chunk's padded sample array by evaluating [`Self::sample`]
    /// at every sample position. Called once per chunk on first load.
    ///
    /// Default impl walks the padded array in linear order and writes
    /// one sample per call. Override if you have a vectorized
    /// generator that can produce a strided row cheaper.
    fn seed_chunk(&self, coord: ChunkCoord, chunk: &mut VoxelChunk) {
        let origin = coord.world_origin();
        chunk.fill_all_padded(|[ix, iy, iz]| {
            // Padding subtracted so the first *content* sample lands
            // exactly at the chunk's world origin.
            let lx = ix as f32 - PADDING as f32;
            let ly = iy as f32 - PADDING as f32;
            let lz = iz as f32 - PADDING as f32;
            let p = origin + Vec3::new(lx, ly, lz) * VOXEL_SIZE;
            self.sample(p)
        });
    }
}

/// Adapter: any [`SdfField`] is trivially a [`WorldGenerator`].
///
/// Use when you want to seed the world from an analytic SDF
/// (primitive / CSG) instead of a heightfield.
pub struct SdfWorldGenerator<F: SdfField>(pub F);

impl<F: SdfField> WorldGenerator for SdfWorldGenerator<F> {
    fn sample(&self, p: Vec3) -> f32 {
        self.0.sample(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdf::primitive::Plane;

    #[test]
    fn sdf_world_generator_seeds_chunk_consistently() {
        let generator = SdfWorldGenerator(Plane::horizontal(10.0));
        let mut chunk = VoxelChunk::new_air();
        let coord = ChunkCoord::new(0, 0, 0);
        generator.seed_chunk(coord, &mut chunk);

        // World y = 0 sits *below* the plane at y=10, so the sample
        // reads negative (solid): sdf = p.y - plane.y = 0 - 10 = -10.
        let v = chunk.get([PADDING, PADDING, PADDING]);
        assert!((v + 10.0).abs() < 1e-3, "expected -10 below plane, got {v}");

        // At world y=10 we're exactly on the plane → sdf = 0.
        let at_surface = chunk.get([PADDING, PADDING + 10, PADDING]);
        assert!(at_surface.abs() < 1e-3, "expected 0 at plane surface, got {at_surface}");
    }
}
