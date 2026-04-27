//! Wire-format types for sending chunk edits across the network.
//!
//! Design goals:
//! 1. **Diff-first**, not snapshot-first — a boss stomp that changes
//!    0.1% of a chunk's samples should ship as a small payload, not a
//!    full 157 KB blob.
//! 2. **Ordered by version** — the server's authoritative
//!    [`crate::VoxelChunk::version`] travels with each delta so the
//!    client can drop out-of-order packets without corrupting state.
//! 3. **Coder-agnostic** — `serde` derives cover both lightyear's
//!    bincode codec and any future JSON / flatbuffer path. The crate
//!    doesn't link lightyear — protocol-level integration happens in
//!    `vaern-protocol` where this module's types get wrapped in a
//!    message.
//!
//! Two payload shapes, pick by ratio at encode time:
//!
//! * [`ChunkDelta::FullSnapshot`] — every sample of the chunk. Used
//!   on initial chunk load (client had nothing) or when diff entropy
//!   is so high the snapshot is smaller.
//! * [`ChunkDelta::SparseWrites`] — list of `(linear_index, value)`
//!   updates. Used for small surgical edits.

pub mod codec;

pub use codec::{ChunkDigest, encode_delta};

use crate::chunk::VoxelChunk;
use bevy::math::IVec3;
use serde::{Deserialize, Serialize};

/// One chunk-state change on the wire.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChunkDelta {
    /// Chunk coord as a raw `IVec3` (serde-friendly; protocol crate
    /// wraps in its own newtype for API clarity).
    pub coord: [i32; 3],
    /// The server's version number *after* applying this delta.
    /// Clients skip deltas whose `version <= last_observed`.
    pub version: u64,
    pub body: ChunkDeltaBody,
}

/// Actual sample data — either a full snapshot or a sparse patch.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ChunkDeltaBody {
    /// Every padded sample in `x → y → z` order. Length must equal
    /// [`crate::CHUNK_TOTAL_SAMPLES`].
    FullSnapshot { samples: Vec<f32> },
    /// List of `(linear_padded_index, new_sample_value)` pairs.
    SparseWrites { writes: Vec<(u32, f32)> },
}

impl ChunkDelta {
    /// Build a full-snapshot delta from a chunk. For uniform chunks
    /// this materializes a `Vec<f32>` of `CHUNK_TOTAL_SAMPLES` copies
    /// of the uniform value — a wire-format inefficiency that would be
    /// fixed by adding a `UniformSnapshot` body variant; not done here
    /// because nothing currently round-trips uniform chunks over the
    /// wire (they're seeded by the generator on the receiving end).
    pub fn full_snapshot(coord: IVec3, chunk: &VoxelChunk) -> Self {
        Self {
            coord: coord.to_array(),
            version: chunk.version,
            body: ChunkDeltaBody::FullSnapshot {
                samples: chunk.samples_to_vec(),
            },
        }
    }

    /// Build a sparse-writes delta from an explicit write list.
    pub fn sparse(coord: IVec3, version: u64, writes: Vec<(u32, f32)>) -> Self {
        Self {
            coord: coord.to_array(),
            version,
            body: ChunkDeltaBody::SparseWrites { writes },
        }
    }

    /// Apply this delta to a chunk in-place. No-ops if the chunk's
    /// version is already >= the delta's (replay-safe).
    ///
    /// Promotes the chunk to `Dense` storage if it was `Uniform`; for
    /// snapshots that turn out to be themselves uniform, calls
    /// `try_compact` so we don't hold 157 KB longer than necessary.
    pub fn apply_to(&self, chunk: &mut VoxelChunk) {
        if chunk.version >= self.version {
            return;
        }
        match &self.body {
            ChunkDeltaBody::FullSnapshot { samples } => {
                let dense = chunk.make_dense();
                for (dst, &src) in dense.iter_mut().zip(samples.iter()) {
                    *dst = src;
                }
            }
            ChunkDeltaBody::SparseWrites { writes } => {
                let dense = chunk.make_dense();
                let len = dense.len();
                for &(idx, v) in writes {
                    let i = idx as usize;
                    if i < len {
                        dense[i] = v;
                    }
                }
            }
        }
        chunk.version = self.version;
        chunk.try_compact();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::ChunkShape;

    #[test]
    fn full_snapshot_roundtrips() {
        let mut src = VoxelChunk::new_air();
        src.set([5, 5, 5], -2.5);
        let delta = ChunkDelta::full_snapshot(IVec3::new(1, 2, 3), &src);
        assert_eq!(delta.coord, [1, 2, 3]);

        let mut dst = VoxelChunk::new_air();
        delta.apply_to(&mut dst);
        assert_eq!(dst.version, src.version);
        assert_eq!(dst.get([5, 5, 5]), -2.5);
    }

    #[test]
    fn sparse_writes_apply_minimal_changes() {
        let mut dst = VoxelChunk::new_air();
        let idx = ChunkShape::linearize([3, 3, 3]);
        let delta = ChunkDelta::sparse(IVec3::ZERO, 1, vec![(idx, -7.0)]);
        delta.apply_to(&mut dst);
        assert_eq!(dst.get([3, 3, 3]), -7.0);
    }

    #[test]
    fn out_of_order_delta_is_dropped() {
        let mut dst = VoxelChunk::new_air();
        dst.version = 10;
        let delta = ChunkDelta::sparse(IVec3::ZERO, 5, vec![(0, -1.0)]);
        delta.apply_to(&mut dst);
        // Version stays, samples unchanged.
        assert_eq!(dst.version, 10);
        assert!(dst.get([0, 0, 0]) > 0.0);
    }
}
