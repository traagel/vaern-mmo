//! One chunk's data: padded sample array + version + dirty flag.
//!
//! The sample storage is **sparse**: chunks whose every sample carries
//! the same SDF value (entirely-air or entirely-solid columns above or
//! below the surface band) hold a single `f32` instead of allocating
//! the full padded array. The first write through any of the mutating
//! APIs promotes the chunk to `Dense` storage. After bulk writes,
//! `try_compact` collapses back to `Uniform` if all samples now match.
//!
//! Why it matters: at editor draw distance 64, ~16k XZ chunks × 3 Y
//! layers = ~50k chunks. Two of those Y layers are uniform for any
//! flat-ish terrain, so ~33k chunks shrink from 157 KB → 4 bytes apiece
//! and the mesher's surface-nets extractor can short-circuit them in
//! O(1) instead of scanning 32,768 cubes only to emit nothing.

use crate::config::{CHUNK_SAMPLES_PER_AXIS, UNINITIALIZED_SDF};

use super::shape::ChunkShape;
use bevy::math::Vec3;

/// A single chunk of SDF samples.
///
/// Index reads with [`Self::get`] (padded coord) or
/// [`Self::sample_at_stride`] (raw linear stride from
/// `ChunkShape::linearize`). Bulk reads use [`Self::iter_samples`] or
/// [`Self::samples_to_vec`].
///
/// `version` is bumped on every write. Consumers (mesher, replicator)
/// compare their last-observed version to decide whether to re-run.
#[derive(Clone)]
pub struct VoxelChunk {
    storage: SampleStorage,
    pub version: u64,
}

/// Backing storage variants. `Uniform` is the fast path for chunks that
/// are entirely one SDF value (air or solid); `Dense` is the standard
/// padded sample array.
#[derive(Clone)]
enum SampleStorage {
    /// Every sample is this value. 4 bytes total.
    Uniform(f32),
    /// Standard padded array of `ChunkShape::TOTAL` f32 samples.
    Dense(Box<[f32]>),
}

impl VoxelChunk {
    /// New chunk, all samples initialized to [`UNINITIALIZED_SDF`]
    /// (solidly "air"). Version starts at 0; first write bumps to 1.
    /// Stored as `Uniform` — no heap allocation.
    pub fn new_air() -> Self {
        Self {
            storage: SampleStorage::Uniform(UNINITIALIZED_SDF),
            version: 0,
        }
    }

    /// New chunk holding a single uniform SDF value across every
    /// sample. Useful when a generator can declare "this chunk is fully
    /// solid" or "fully air" without allocating.
    pub fn new_uniform(value: f32) -> Self {
        Self {
            storage: SampleStorage::Uniform(value),
            version: 0,
        }
    }

    /// Read one sample by padded coord. Panics on out-of-bounds (Dense
    /// path). Uniform path always returns the stored value.
    #[inline]
    pub fn get(&self, xyz: [u32; 3]) -> f32 {
        match &self.storage {
            SampleStorage::Uniform(v) => *v,
            SampleStorage::Dense(s) => s[ChunkShape::linearize(xyz) as usize],
        }
    }

    /// Read one sample by raw linear stride (faster than [`Self::get`]
    /// for inner loops where the caller already has the stride from
    /// `ChunkShape::linearize`).
    #[inline]
    pub fn sample_at_stride(&self, stride: usize) -> f32 {
        match &self.storage {
            SampleStorage::Uniform(v) => *v,
            SampleStorage::Dense(s) => s[stride],
        }
    }

    /// Write one sample by padded coord (bumps version). Promotes the
    /// chunk to `Dense` storage if it was `Uniform`.
    #[inline]
    pub fn set(&mut self, xyz: [u32; 3], v: f32) {
        let stride = ChunkShape::linearize(xyz) as usize;
        let dense = self.make_dense();
        dense[stride] = v;
        self.version = self.version.wrapping_add(1);
    }

    /// `Some(value)` if every sample in this chunk equals the same
    /// scalar. The mesher uses this to short-circuit empty (all-air or
    /// all-solid) chunks in O(1) instead of scanning 32,768 cubes.
    #[inline]
    pub fn uniform_value(&self) -> Option<f32> {
        match &self.storage {
            SampleStorage::Uniform(v) => Some(*v),
            SampleStorage::Dense(_) => None,
        }
    }

    /// Promote to `Dense` if this chunk was `Uniform`, returning a
    /// mutable slice over the full padded sample array. Caller is
    /// responsible for bumping `version` after writing.
    pub fn make_dense(&mut self) -> &mut [f32] {
        if let SampleStorage::Uniform(v) = self.storage {
            self.storage = SampleStorage::Dense(
                vec![v; ChunkShape::TOTAL].into_boxed_slice(),
            );
        }
        match &mut self.storage {
            SampleStorage::Dense(s) => &mut s[..],
            SampleStorage::Uniform(_) => unreachable!("just promoted above"),
        }
    }

    /// If the chunk is `Dense` and every sample equals the same scalar,
    /// collapse to `Uniform` to free 157 KB. Cheap epsilon compare.
    /// Called automatically after bulk writes via `fill_*` helpers.
    pub fn try_compact(&mut self) {
        let SampleStorage::Dense(s) = &self.storage else {
            return;
        };
        if s.is_empty() {
            return;
        }
        let first = s[0];
        const COMPACT_EPSILON: f32 = 1e-6;
        if s.iter().all(|&v| (v - first).abs() < COMPACT_EPSILON) {
            self.storage = SampleStorage::Uniform(first);
        }
    }

    /// Iterator over every sample in the padded array. Works for both
    /// `Uniform` and `Dense` storage; the `Uniform` path yields the
    /// same value `ChunkShape::TOTAL` times without allocating.
    pub fn iter_samples(&self) -> SampleIter<'_> {
        match &self.storage {
            SampleStorage::Uniform(v) => SampleIter {
                kind: SampleIterKind::Uniform { value: *v, remaining: ChunkShape::TOTAL },
            },
            SampleStorage::Dense(s) => SampleIter {
                kind: SampleIterKind::Dense(s.iter()),
            },
        }
    }

    /// Materialize a fresh `Vec<f32>` of length `ChunkShape::TOTAL`
    /// holding every sample. Allocates for both variants — prefer
    /// [`Self::iter_samples`] when streaming is enough.
    pub fn samples_to_vec(&self) -> Vec<f32> {
        match &self.storage {
            SampleStorage::Uniform(v) => vec![*v; ChunkShape::TOTAL],
            SampleStorage::Dense(s) => s.to_vec(),
        }
    }

    /// `true` iff this chunk is currently storing a full padded array
    /// (rather than a single uniform value). Useful for diagnostics
    /// counters; correctness should not depend on this.
    pub fn is_dense(&self) -> bool {
        matches!(self.storage, SampleStorage::Dense(_))
    }

    /// Apply `f` to every sample in the chunk's content region
    /// (excluding padding). Promotes to `Dense`, runs the closure,
    /// bumps version, then attempts to compact back to `Uniform`.
    pub fn fill_content<F: FnMut([u32; 3]) -> f32>(&mut self, mut f: F) {
        let dense = self.make_dense();
        for z in ChunkShape::CONTENT_MIN..ChunkShape::CONTENT_MAX {
            for y in ChunkShape::CONTENT_MIN..ChunkShape::CONTENT_MAX {
                for x in ChunkShape::CONTENT_MIN..ChunkShape::CONTENT_MAX {
                    dense[ChunkShape::linearize([x, y, z]) as usize] = f([x, y, z]);
                }
            }
        }
        self.version = self.version.wrapping_add(1);
        self.try_compact();
    }

    /// Apply `f` to every sample in the full padded array (including
    /// padding). Promotes to `Dense`, runs the closure, bumps version,
    /// then attempts to compact back to `Uniform` (the common case for
    /// vertical-stack chunks far above or below the surface).
    pub fn fill_all_padded<F: FnMut([u32; 3]) -> f32>(&mut self, mut f: F) {
        let dense = self.make_dense();
        for z in 0..CHUNK_SAMPLES_PER_AXIS {
            for y in 0..CHUNK_SAMPLES_PER_AXIS {
                for x in 0..CHUNK_SAMPLES_PER_AXIS {
                    dense[ChunkShape::linearize([x, y, z]) as usize] = f([x, y, z]);
                }
            }
        }
        self.version = self.version.wrapping_add(1);
        self.try_compact();
    }

    /// Trilinear sample in *local padded coord space* — local `(0,0,0)`
    /// is the chunk's first padding sample; floats between samples
    /// interpolate between the 8 surrounding cube corners.
    ///
    /// Out-of-range values clamp to the nearest padded sample; callers
    /// that need neighbor-aware sampling should assemble a local
    /// sample view before calling this.
    pub fn sample_trilinear(&self, local_padded: Vec3) -> f32 {
        // Uniform fast path: every corner is the same, so the trilinear
        // result is the uniform value regardless of the fractional offset.
        if let Some(v) = self.uniform_value() {
            return v;
        }

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

/// Iterator returned by [`VoxelChunk::iter_samples`]. Two-variant
/// internal so the `Uniform` path doesn't allocate or even build a
/// slice — it just yields the scalar `ChunkShape::TOTAL` times.
pub struct SampleIter<'a> {
    kind: SampleIterKind<'a>,
}

enum SampleIterKind<'a> {
    Uniform { value: f32, remaining: usize },
    Dense(std::slice::Iter<'a, f32>),
}

impl<'a> Iterator for SampleIter<'a> {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        match &mut self.kind {
            SampleIterKind::Uniform { value, remaining } => {
                if *remaining == 0 {
                    None
                } else {
                    *remaining -= 1;
                    Some(*value)
                }
            }
            SampleIterKind::Dense(it) => it.next().copied(),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match &self.kind {
            SampleIterKind::Uniform { remaining, .. } => (*remaining, Some(*remaining)),
            SampleIterKind::Dense(it) => it.size_hint(),
        }
    }
}

impl<'a> ExactSizeIterator for SampleIter<'a> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_air_has_positive_samples_everywhere() {
        let c = VoxelChunk::new_air();
        for v in c.iter_samples() {
            assert!(v > 0.0);
        }
    }

    #[test]
    fn new_air_is_uniform() {
        let c = VoxelChunk::new_air();
        assert_eq!(c.uniform_value(), Some(UNINITIALIZED_SDF));
        assert!(!c.is_dense());
    }

    #[test]
    fn set_bumps_version() {
        let mut c = VoxelChunk::new_air();
        assert_eq!(c.version, 0);
        c.set([1, 1, 1], -5.0);
        assert_eq!(c.version, 1);
    }

    #[test]
    fn set_promotes_to_dense() {
        let mut c = VoxelChunk::new_air();
        assert!(c.uniform_value().is_some());
        c.set([1, 1, 1], -5.0);
        assert!(c.uniform_value().is_none());
        assert!(c.is_dense());
    }

    #[test]
    fn set_preserves_other_samples() {
        // Promoting Uniform → Dense must seed the new array with the
        // prior uniform value, otherwise unrelated samples would
        // silently change after the first write.
        let mut c = VoxelChunk::new_uniform(0.5);
        c.set([1, 1, 1], -5.0);
        assert_eq!(c.get([1, 1, 1]), -5.0);
        assert_eq!(c.get([2, 2, 2]), 0.5);
        assert_eq!(c.get([0, 0, 0]), 0.5);
    }

    #[test]
    fn fill_all_padded_with_uniform_output_compacts() {
        let mut c = VoxelChunk::new_air();
        c.fill_all_padded(|_| -10.0);
        assert_eq!(c.uniform_value(), Some(-10.0));
        assert!(!c.is_dense());
    }

    #[test]
    fn fill_all_padded_with_varied_output_stays_dense() {
        let mut c = VoxelChunk::new_air();
        c.fill_all_padded(|[x, _, _]| x as f32);
        assert!(c.is_dense());
        assert_eq!(c.get([5, 0, 0]), 5.0);
        assert_eq!(c.get([10, 0, 0]), 10.0);
    }

    #[test]
    fn iter_samples_yields_total_count_uniform() {
        let c = VoxelChunk::new_air();
        assert_eq!(c.iter_samples().count(), ChunkShape::TOTAL);
    }

    #[test]
    fn iter_samples_yields_total_count_dense() {
        let mut c = VoxelChunk::new_air();
        c.set([1, 1, 1], -5.0); // promote to dense
        assert_eq!(c.iter_samples().count(), ChunkShape::TOTAL);
    }

    #[test]
    fn sample_at_stride_matches_get_for_dense() {
        let mut c = VoxelChunk::new_air();
        c.fill_all_padded(|[x, y, z]| (x + y * 7 + z * 13) as f32);
        let stride = ChunkShape::linearize([5, 6, 7]) as usize;
        assert_eq!(c.sample_at_stride(stride), c.get([5, 6, 7]));
    }

    #[test]
    fn sample_at_stride_returns_uniform_value() {
        let c = VoxelChunk::new_uniform(2.5);
        assert_eq!(c.sample_at_stride(0), 2.5);
        assert_eq!(c.sample_at_stride(ChunkShape::TOTAL - 1), 2.5);
    }

    #[test]
    fn make_dense_seeds_with_prior_uniform_value() {
        let mut c = VoxelChunk::new_uniform(7.0);
        let dense = c.make_dense();
        assert!(dense.iter().all(|&v| v == 7.0));
    }

    #[test]
    fn try_compact_collapses_uniform_dense() {
        let mut c = VoxelChunk::new_air();
        c.set([1, 1, 1], -5.0); // promote to dense
        assert!(c.is_dense());
        // Restore the sample so the chunk is uniform again.
        c.set([1, 1, 1], UNINITIALIZED_SDF);
        c.try_compact();
        assert_eq!(c.uniform_value(), Some(UNINITIALIZED_SDF));
    }

    #[test]
    fn trilinear_returns_exact_sample_on_grid() {
        let mut c = VoxelChunk::new_air();
        c.set([10, 10, 10], -3.5);
        assert!((c.sample_trilinear(Vec3::new(10.0, 10.0, 10.0)) + 3.5).abs() < 1e-4);
    }

    #[test]
    fn trilinear_uniform_short_circuits() {
        let c = VoxelChunk::new_uniform(2.5);
        // Any fractional coord should return the uniform value without
        // touching a sample slice.
        assert_eq!(c.sample_trilinear(Vec3::new(7.3, 5.1, 9.9)), 2.5);
    }
}
