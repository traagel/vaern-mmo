//! Sparse chunk map + dirty-set tracking.
//!
//! The [`ChunkStore`] is a Bevy [`Resource`] holding every loaded
//! chunk. Server treats this as the authoritative world state; client
//! rebuilds it from server-pushed deltas. Neither side allocates a
//! dense grid — chunks exist only once they've been explicitly
//! generated or received.
//!
//! [`DirtyChunks`] is a companion resource that tracks which chunks
//! have changed this frame. The mesher drains it to avoid re-meshing
//! untouched chunks.

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;

use super::chunk::VoxelChunk;
use super::coord::ChunkCoord;

/// Sparse map of loaded chunks.
#[derive(Resource, Default)]
pub struct ChunkStore {
    chunks: HashMap<ChunkCoord, VoxelChunk>,
}

impl ChunkStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, coord: ChunkCoord) -> Option<&VoxelChunk> {
        self.chunks.get(&coord)
    }

    pub fn get_mut(&mut self, coord: ChunkCoord) -> Option<&mut VoxelChunk> {
        self.chunks.get_mut(&coord)
    }

    /// Read-or-create: returns a mutable ref, inserting a fresh air
    /// chunk if none exists. Use from brushes that want to write into
    /// a previously-unloaded chunk.
    pub fn get_or_insert_air(&mut self, coord: ChunkCoord) -> &mut VoxelChunk {
        self.chunks
            .entry(coord)
            .or_insert_with(VoxelChunk::new_air)
    }

    pub fn insert(&mut self, coord: ChunkCoord, chunk: VoxelChunk) {
        self.chunks.insert(coord, chunk);
    }

    pub fn remove(&mut self, coord: ChunkCoord) -> Option<VoxelChunk> {
        self.chunks.remove(&coord)
    }

    pub fn contains(&self, coord: ChunkCoord) -> bool {
        self.chunks.contains_key(&coord)
    }

    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    /// Iterate all loaded chunks. No ordering guarantee.
    pub fn iter(&self) -> impl Iterator<Item = (&ChunkCoord, &VoxelChunk)> {
        self.chunks.iter()
    }

    /// Iterate all loaded chunks, mutably. Useful for bulk seeding from
    /// a [`crate::WorldGenerator`].
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&ChunkCoord, &mut VoxelChunk)> {
        self.chunks.iter_mut()
    }

    /// Iterate just the coords (cheap — no borrow of the chunk data).
    pub fn coords(&self) -> impl Iterator<Item = ChunkCoord> + '_ {
        self.chunks.keys().copied()
    }
}

/// Set of chunks that need re-meshing / re-replication this frame.
///
/// Edit systems call [`Self::mark`]; downstream systems call
/// [`Self::drain`] to consume and act on the list.
#[derive(Resource, Default)]
pub struct DirtyChunks {
    pending: HashSet<ChunkCoord>,
}

impl DirtyChunks {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn mark(&mut self, coord: ChunkCoord) {
        self.pending.insert(coord);
    }

    pub fn mark_many(&mut self, coords: impl IntoIterator<Item = ChunkCoord>) {
        self.pending.extend(coords);
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    /// Drain up to `budget` chunks out of the set; the rest stay queued
    /// for next frame. Order is undefined.
    pub fn drain_budget(&mut self, budget: usize) -> Vec<ChunkCoord> {
        let take = budget.min(self.pending.len());
        let out: Vec<_> = self.pending.iter().take(take).copied().collect();
        for c in &out {
            self.pending.remove(c);
        }
        out
    }

    /// Drain the entire set (unbounded). Use for one-shot setup passes,
    /// not per-frame work.
    pub fn drain_all(&mut self) -> Vec<ChunkCoord> {
        self.pending.drain().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_insert_and_lookup() {
        let mut store = ChunkStore::new();
        let c = ChunkCoord::new(1, 2, 3);
        store.insert(c, VoxelChunk::new_air());
        assert!(store.contains(c));
        assert!(store.get(c).is_some());
    }

    #[test]
    fn dirty_budget_caps_drain() {
        let mut d = DirtyChunks::new();
        for i in 0..10 {
            d.mark(ChunkCoord::new(i, 0, 0));
        }
        let drained = d.drain_budget(3);
        assert_eq!(drained.len(), 3);
        assert_eq!(d.len(), 7);
    }
}
