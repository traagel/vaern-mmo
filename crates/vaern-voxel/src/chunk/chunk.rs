//! One chunk's data: padded sample array + version + dirty flag.

use crate::config::{CHUNK_SAMPLES_PER_AXIS, UNINITIALIZED_SDF};

use super::shape::ChunkShape;
use bevy::math::Vec3;

/// A single chunk of SDF samples.
///
/// `samples.len() == ChunkShape::TOTAL`. Index with
/// `ChunkShape::linearize([x, y, z])` where all components are in
/// `[0, CHUNK_SAMPLES_PER_AXIS)`. Use `ChunkShape::linearize_content`
/// when you have a 0-based content-space coord and want padding added.
///
/// `version` is bumped on every write. Consumers (mesher, replicator)
/// compare their last-observed version to decide whether to re-run.
#[derive(Clone)]
pub struct VoxelChunk {
    pub samples: Box<[f32]>,
    pub version: u64,
}

impl VoxelChunk {
    /// New chunk, all samples initialized to [`UNINITIALIZED_SDF`]
    /// (solidly "air"). Version starts at 0; first write bumps to 1.
    pub fn new_air() -> Self {
        Self {
            samples: vec![UNINITIALIZED_SDF; ChunkShape::TOTAL].into_boxed_slice(),
            version: 0,
        }
    }

    /// Read one sample by padded coord. Panics on out-of-bounds.
    #[inline]
    pub fn get(&self, xyz: [u32; 3]) -> f32 {
        self.samples[ChunkShape::linearize(xyz) as usize]
    }

    /// Write one sample by padded coord (bumps version).
    #[inline]
    pub fn set(&mut self, xyz: [u32; 3], v: f32) {
        self.samples[ChunkShape::linearize(xyz) as usize] = v;
        self.version = self.version.wrapping_add(1);
    }

    /// Apply `f` to every sample in the chunk's content region
    /// (excluding padding). Useful when seeding from a generator; does
    /// one version bump at the end.
    pub fn fill_content<F: FnMut([u32; 3]) -> f32>(&mut self, mut f: F) {
        for z in ChunkShape::CONTENT_MIN..ChunkShape::CONTENT_MAX {
            for y in ChunkShape::CONTENT_MIN..ChunkShape::CONTENT_MAX {
                for x in ChunkShape::CONTENT_MIN..ChunkShape::CONTENT_MAX {
                    self.samples[ChunkShape::linearize([x, y, z]) as usize] = f([x, y, z]);
                }
            }
        }
        self.version = self.version.wrapping_add(1);
    }

    /// Apply `f` to every sample in the full padded array (including
    /// padding). The caller handles version bump — call only during
    /// bulk init before the chunk is observed by other systems.
    pub fn fill_all_padded<F: FnMut([u32; 3]) -> f32>(&mut self, mut f: F) {
        for z in 0..CHUNK_SAMPLES_PER_AXIS {
            for y in 0..CHUNK_SAMPLES_PER_AXIS {
                for x in 0..CHUNK_SAMPLES_PER_AXIS {
                    self.samples[ChunkShape::linearize([x, y, z]) as usize] = f([x, y, z]);
                }
            }
        }
        self.version = self.version.wrapping_add(1);
    }

    /// Trilinear sample in *local padded coord space* — local `(0,0,0)`
    /// is the chunk's first padding sample; floats between samples
    /// interpolate between the 8 surrounding cube corners.
    ///
    /// Out-of-range values clamp to the nearest padded sample; callers
    /// that need neighbor-aware sampling should assemble a local
    /// sample view before calling this.
    pub fn sample_trilinear(&self, local_padded: Vec3) -> f32 {
        let axis = (CHUNK_SAMPLES_PER_AXIS - 1) as f32;
        let x = local_padded.x.clamp(0.0, axis);
        let y = local_padded.y.clamp(0.0, axis);
        let z = local_padded.z.clamp(0.0, axis);

        let x0 = x.floor();
        let y0 = y.floor();
        let z0 = z.floor();
        let tx = x - x0;
        let ty = y - y0;
        let tz = z - z0;

        let ix0 = x0 as u32;
        let iy0 = y0 as u32;
        let iz0 = z0 as u32;
        let max = CHUNK_SAMPLES_PER_AXIS - 1;
        let ix1 = (ix0 + 1).min(max);
        let iy1 = (iy0 + 1).min(max);
        let iz1 = (iz0 + 1).min(max);

        let c000 = self.get([ix0, iy0, iz0]);
        let c100 = self.get([ix1, iy0, iz0]);
        let c010 = self.get([ix0, iy1, iz0]);
        let c110 = self.get([ix1, iy1, iz0]);
        let c001 = self.get([ix0, iy0, iz1]);
        let c101 = self.get([ix1, iy0, iz1]);
        let c011 = self.get([ix0, iy1, iz1]);
        let c111 = self.get([ix1, iy1, iz1]);

        let c00 = c000 * (1.0 - tx) + c100 * tx;
        let c10 = c010 * (1.0 - tx) + c110 * tx;
        let c01 = c001 * (1.0 - tx) + c101 * tx;
        let c11 = c011 * (1.0 - tx) + c111 * tx;

        let c0 = c00 * (1.0 - ty) + c10 * ty;
        let c1 = c01 * (1.0 - ty) + c11 * ty;

        c0 * (1.0 - tz) + c1 * tz
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_air_has_positive_samples_everywhere() {
        let c = VoxelChunk::new_air();
        for v in c.samples.iter() {
            assert!(*v > 0.0);
        }
    }

    #[test]
    fn set_bumps_version() {
        let mut c = VoxelChunk::new_air();
        assert_eq!(c.version, 0);
        c.set([1, 1, 1], -5.0);
        assert_eq!(c.version, 1);
    }

    #[test]
    fn trilinear_returns_exact_sample_on_grid() {
        let mut c = VoxelChunk::new_air();
        c.set([10, 10, 10], -3.5);
        assert!((c.sample_trilinear(Vec3::new(10.0, 10.0, 10.0)) + 3.5).abs() < 1e-4);
    }
}
