//! Chunked sparse voxel storage.
//!
//! The world is partitioned into cubic chunks of [`crate::CHUNK_DIM`]³
//! content voxels. Each chunk owns its own sample array and version
//! counter. The [`ChunkStore`] is the sparse map of all currently-loaded
//! chunks; it is the single authoritative resource on both client and
//! server (server owns the truth; client owns a replica + any
//! client-predicted edits).
//!
//! Coordinate conventions and indexer math live in [`coord`] and
//! [`shape`] — see those modules for the exact formulas.

pub mod chunk;
pub mod coord;
pub mod shape;
pub mod store;

pub use chunk::VoxelChunk;
pub use coord::{ChunkCoord, VoxelCoord};
pub use shape::ChunkShape;
pub use store::{ChunkStore, DirtyChunks};
