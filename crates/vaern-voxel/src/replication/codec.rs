//! Helpers for packing chunk deltas efficiently.
//!
//! Two utilities today:
//! * [`encode_delta`] — picks between full-snapshot and sparse-writes
//!   based on which representation is smaller.
//! * [`ChunkDigest`] — a cheap rolling hash of a chunk's samples,
//!   used server-side to detect no-op edits (brush writes that leave
//!   the sample within epsilon of the stored value).

use super::{ChunkDelta, ChunkDeltaBody};
use crate::chunk::VoxelChunk;
use crate::config::CHUNK_TOTAL_SAMPLES;
use bevy::math::IVec3;

/// Compare a modified chunk against a sample-index set of writes and
/// emit whichever representation is smaller on the wire.
///
/// A sparse writes payload runs ~8 bytes per entry (4 for index, 4 for
/// value) — so switching to full snapshot pays off roughly when the
/// write-set covers more than 1/8 of the chunk.
pub fn encode_delta(
    coord: IVec3,
    chunk: &VoxelChunk,
    writes: &[(u32, f32)],
) -> ChunkDelta {
    let sparse_bytes = writes.len() * 8;
    let snapshot_bytes = CHUNK_TOTAL_SAMPLES * 4;

    if sparse_bytes < snapshot_bytes {
        ChunkDelta {
            coord: coord.to_array(),
            version: chunk.version,
            body: ChunkDeltaBody::SparseWrites {
                writes: writes.to_vec(),
            },
        }
    } else {
        ChunkDelta::full_snapshot(coord, chunk)
    }
}

/// Lightweight digest of a chunk's samples. Swap to a stronger hash
/// (wyhash / xxh3) if we ever see false-collision issues in practice;
/// for now, a FNV-1a variant keyed on the bit pattern of f32 samples
/// is plenty.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChunkDigest(pub u64);

impl ChunkDigest {
    pub fn compute(chunk: &VoxelChunk) -> Self {
        let mut h: u64 = 0xcbf29ce484222325;
        for &s in chunk.samples.iter() {
            // FNV-1a over the 4 bytes of the f32 bit pattern.
            let bits = s.to_bits();
            for b in bits.to_le_bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(0x100000001b3);
            }
        }
        ChunkDigest(h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::VoxelChunk;

    #[test]
    fn digest_changes_on_any_edit() {
        let c = VoxelChunk::new_air();
        let d1 = ChunkDigest::compute(&c);

        let mut c2 = c.clone();
        c2.set([5, 5, 5], -1.0);
        let d2 = ChunkDigest::compute(&c2);

        assert_ne!(d1, d2);
    }

    #[test]
    fn encode_picks_sparse_for_small_patches() {
        let c = VoxelChunk::new_air();
        let writes = vec![(0u32, -1.0), (1u32, -1.0), (2u32, -1.0)];
        let d = encode_delta(IVec3::ZERO, &c, &writes);
        matches!(d.body, ChunkDeltaBody::SparseWrites { .. })
            .then_some(())
            .expect("small patch should be sparse");
    }

    #[test]
    fn encode_picks_snapshot_for_dense_patches() {
        let c = VoxelChunk::new_air();
        let writes: Vec<_> = (0..(CHUNK_TOTAL_SAMPLES as u32))
            .map(|i| (i, -1.0))
            .collect();
        let d = encode_delta(IVec3::ZERO, &c, &writes);
        matches!(d.body, ChunkDeltaBody::FullSnapshot { .. })
            .then_some(())
            .expect("dense patch should be snapshot");
    }
}
