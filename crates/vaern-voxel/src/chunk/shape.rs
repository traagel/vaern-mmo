//! Padded-chunk sample indexer.
//!
//! Replaces the external `ndshape` crate with a purpose-built indexer
//! for our fixed [`CHUNK_SAMPLES_PER_AXIS`]³ layout. Keeping the formula
//! here (rather than inlined across the codebase) means an axis-order
//! change would be one-file instead of grep-and-hope.

use crate::config::{CHUNK_DIM, CHUNK_SAMPLES_PER_AXIS, PADDING};

/// Indexer for the padded sample array of a single chunk.
///
/// Samples are stored in `x → y → z` order (x fastest-varying,
/// z slowest) — matches the Surface Nets convention and keeps the
/// inner loop cache-friendly for the common x-major iteration.
pub struct ChunkShape;

impl ChunkShape {
    /// Samples per axis in the padded array.
    pub const AXIS: u32 = CHUNK_SAMPLES_PER_AXIS;

    /// Total samples in the padded array.
    pub const TOTAL: usize =
        (Self::AXIS * Self::AXIS * Self::AXIS) as usize;

    /// Stride to move +1 along X.
    pub const STRIDE_X: u32 = 1;
    /// Stride to move +1 along Y.
    pub const STRIDE_Y: u32 = Self::AXIS;
    /// Stride to move +1 along Z.
    pub const STRIDE_Z: u32 = Self::AXIS * Self::AXIS;

    /// Axis range of *content* samples (excludes padding on both sides).
    /// Iterate `CONTENT_MIN..CONTENT_MAX` to touch only real voxels.
    pub const CONTENT_MIN: u32 = PADDING;
    pub const CONTENT_MAX: u32 = PADDING + CHUNK_DIM;

    /// Axis range the mesher iterates. Extended one cube into -side
    /// padding so the mesh closes its own +side seam against the
    /// neighbor: chunk A's cube at index 0 duplicates chunk A-1's cube
    /// at index CHUNK_DIM (same world region, same SDF via halo sync,
    /// same centroid vertex). With cube 0 in range, pass 2's -X/-Y/-Z
    /// edge emission on cube 1 lands the quad at the chunk boundary,
    /// closing what used to be a 1-voxel gap.
    pub const MESH_MIN: u32 = PADDING - 1;
    pub const MESH_MAX: u32 = PADDING + CHUNK_DIM;

    /// 3D → linear index. `xyz` components must be < [`Self::AXIS`].
    #[inline]
    pub const fn linearize(xyz: [u32; 3]) -> u32 {
        xyz[0] + Self::AXIS * (xyz[1] + Self::AXIS * xyz[2])
    }

    /// Linear index → 3D components.
    #[inline]
    pub const fn delinearize(i: u32) -> [u32; 3] {
        let x = i % Self::AXIS;
        let y = (i / Self::AXIS) % Self::AXIS;
        let z = i / (Self::AXIS * Self::AXIS);
        [x, y, z]
    }

    /// Convenience: linearize an unpadded content-space coord. Pass
    /// `[0, 0, 0]` to get the first real content sample (padding
    /// already accounted for).
    #[inline]
    pub const fn linearize_content(content_xyz: [u32; 3]) -> u32 {
        Self::linearize([
            content_xyz[0] + PADDING,
            content_xyz[1] + PADDING,
            content_xyz[2] + PADDING,
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linearize_roundtrip() {
        for &xyz in &[[0, 0, 0], [1, 2, 3], [10, 20, 30], [33, 33, 33]] {
            let i = ChunkShape::linearize(xyz);
            assert_eq!(ChunkShape::delinearize(i), xyz);
        }
    }

    #[test]
    fn total_matches_axis_cubed() {
        assert_eq!(
            ChunkShape::TOTAL,
            (ChunkShape::AXIS * ChunkShape::AXIS * ChunkShape::AXIS) as usize
        );
    }

    #[test]
    fn strides_move_by_one_linear_position() {
        let base = ChunkShape::linearize([5, 5, 5]);
        assert_eq!(
            ChunkShape::linearize([6, 5, 5]) - base,
            ChunkShape::STRIDE_X
        );
        assert_eq!(
            ChunkShape::linearize([5, 6, 5]) - base,
            ChunkShape::STRIDE_Y
        );
        assert_eq!(
            ChunkShape::linearize([5, 5, 6]) - base,
            ChunkShape::STRIDE_Z
        );
    }
}
