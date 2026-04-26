//! Voxel-edit undo log — ring buffer of applied edit strokes that the
//! editor can replay in reverse.
//!
//! V1: data structure only, **no recording wired**. Once
//! `modes::voxel_brush` actually applies edits, it should:
//!
//! 1. Snapshot the affected chunks' samples *before* the brush runs.
//! 2. Push a [`VoxelUndoEntry::Snapshot`] onto the log.
//! 3. Apply the brush.
//!
//! On `EditorAction::Undo`, pop the most-recent entry and overwrite
//! the chunks. Ring buffer caps growth at [`MAX_UNDO_DEPTH`] entries
//! so a long session doesn't OOM the heap.

use bevy::prelude::*;
use std::collections::VecDeque;
use vaern_voxel::chunk::ChunkCoord;
use vaern_voxel::VoxelChunk;

/// Ring-buffer cap. Conservative — each entry can be 50+ KB if a brush
/// touched many chunks, so 32 entries × 8 chunks × 150 KB ≈ 38 MB
/// worst case.
pub const MAX_UNDO_DEPTH: usize = 32;

/// One undo step. Variant kept opaque so future encoded-delta entries
/// can land alongside the bulk-snapshot variant.
#[derive(Clone)]
pub enum VoxelUndoEntry {
    /// Full pre-edit snapshot of every chunk the stroke touched. Easy
    /// to apply, expensive in memory.
    Snapshot {
        chunks: Vec<(ChunkCoord, VoxelChunk)>,
    },
}

impl std::fmt::Debug for VoxelUndoEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Don't print chunk samples — too noisy. Print the count.
        match self {
            Self::Snapshot { chunks } => f
                .debug_struct("Snapshot")
                .field("chunks", &chunks.len())
                .finish(),
        }
    }
}

/// FIFO ring buffer of undo entries. Newest at the back.
#[derive(Resource, Default)]
pub struct VoxelUndoLog {
    entries: VecDeque<VoxelUndoEntry>,
}

impl std::fmt::Debug for VoxelUndoLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VoxelUndoLog")
            .field("entries", &self.entries.len())
            .field("cap", &MAX_UNDO_DEPTH)
            .finish()
    }
}

impl VoxelUndoLog {
    /// Push a new entry. Drops the oldest if the buffer is at cap.
    pub fn push(&mut self, entry: VoxelUndoEntry) {
        if self.entries.len() == MAX_UNDO_DEPTH {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    /// Pop the most-recent entry for an undo replay.
    pub fn pop(&mut self) -> Option<VoxelUndoEntry> {
        self.entries.pop_back()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Plugin: registers the empty undo log resource. No systems wired in
/// V1 — recording is a per-mode concern that lands when each mode does.
pub struct VoxelUndoPlugin;

impl Plugin for VoxelUndoPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VoxelUndoLog>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot_entry() -> VoxelUndoEntry {
        VoxelUndoEntry::Snapshot { chunks: Vec::new() }
    }

    #[test]
    fn push_pop_round_trip() {
        let mut log = VoxelUndoLog::default();
        log.push(snapshot_entry());
        assert_eq!(log.len(), 1);
        assert!(log.pop().is_some());
        assert!(log.is_empty());
    }

    #[test]
    fn ring_buffer_drops_oldest_at_cap() {
        let mut log = VoxelUndoLog::default();
        for _ in 0..(MAX_UNDO_DEPTH + 5) {
            log.push(snapshot_entry());
        }
        assert_eq!(log.len(), MAX_UNDO_DEPTH);
    }
}
