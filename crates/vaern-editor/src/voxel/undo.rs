//! Voxel-edit undo/redo log — twin ring buffers of pre-edit chunk
//! snapshots that the editor can replay in either direction.
//!
//! Recording: before [`crate::modes::voxel_brush::apply_brush_on_click`]
//! runs `EditStroke::apply`, it clones every chunk the brush AABB
//! overlaps and pushes the snapshot via [`VoxelUndoLog::record_stroke`].
//! That call also clears the redo stack — any new edit invalidates the
//! redo timeline.
//!
//! Replay: Ctrl+Z pops from `undo` (capturing the current chunk state
//! into `redo` first), then writes the popped chunks back into the
//! `ChunkStore` and marks them dirty for re-mesh. Ctrl+Shift+Z is
//! symmetric over the `redo` stack.
//!
//! Both stacks are bounded at [`MAX_UNDO_DEPTH`] entries; when full,
//! the oldest entry is dropped.

use bevy::prelude::*;
use std::collections::VecDeque;
use vaern_voxel::chunk::{ChunkCoord, ChunkStore, DirtyChunks};
use vaern_voxel::VoxelChunk;

use crate::input::bindings::{EditorAction, EditorActionState};
use crate::state::EditorAppState;
use crate::ui::console::ConsoleLog;

/// Ring-buffer cap. Conservative — each entry can be 50+ KB if a brush
/// touched many chunks, so 32 entries × 8 chunks × 150 KB ≈ 38 MB
/// worst case per stack.
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

impl VoxelUndoEntry {
    /// Coords this entry will restore. Used to mark the dirty set after
    /// replay so the mesher rebuilds them.
    pub fn coords(&self) -> impl Iterator<Item = ChunkCoord> + '_ {
        match self {
            Self::Snapshot { chunks } => chunks.iter().map(|(c, _)| *c),
        }
    }
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

/// Twin FIFO ring buffers — `undo` (back = newest stroke) plus `redo`
/// (back = most-recently-undone stroke). Newest at the back, oldest at
/// the front.
#[derive(Resource, Default)]
pub struct VoxelUndoLog {
    undo: VecDeque<VoxelUndoEntry>,
    redo: VecDeque<VoxelUndoEntry>,
}

impl std::fmt::Debug for VoxelUndoLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VoxelUndoLog")
            .field("undo", &self.undo.len())
            .field("redo", &self.redo.len())
            .field("cap", &MAX_UNDO_DEPTH)
            .finish()
    }
}

impl VoxelUndoLog {
    /// Record a freshly-applied stroke. Pushes the pre-stroke snapshot
    /// onto `undo` and clears the `redo` stack — any new edit ends the
    /// previous redo timeline.
    pub fn record_stroke(&mut self, entry: VoxelUndoEntry) {
        push_capped(&mut self.undo, entry);
        self.redo.clear();
    }

    /// Pop the most-recent undo entry (returns `None` if empty).
    pub fn pop_undo(&mut self) -> Option<VoxelUndoEntry> {
        self.undo.pop_back()
    }

    /// Pop the most-recent redo entry (returns `None` if empty).
    pub fn pop_redo(&mut self) -> Option<VoxelUndoEntry> {
        self.redo.pop_back()
    }

    /// Push an entry onto the redo stack — used by the undo handler
    /// after capturing the current chunk state.
    pub fn push_redo(&mut self, entry: VoxelUndoEntry) {
        push_capped(&mut self.redo, entry);
    }

    /// Push an entry onto the undo stack — used by the redo handler
    /// after capturing the current chunk state. Does NOT clear the redo
    /// stack (that's `record_stroke`'s job for fresh edits).
    pub fn push_undo(&mut self, entry: VoxelUndoEntry) {
        push_capped(&mut self.undo, entry);
    }

    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    pub fn is_empty(&self) -> bool {
        self.undo.is_empty() && self.redo.is_empty()
    }
}

fn push_capped(stack: &mut VecDeque<VoxelUndoEntry>, entry: VoxelUndoEntry) {
    if stack.len() == MAX_UNDO_DEPTH {
        stack.pop_front();
    }
    stack.push_back(entry);
}

/// Plugin: registers the undo log resource + the mode-agnostic Ctrl+Z
/// / Ctrl+Shift+Z handler systems.
pub struct VoxelUndoPlugin;

impl Plugin for VoxelUndoPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VoxelUndoLog>().add_systems(
            Update,
            (apply_undo_action, apply_redo_action).run_if(in_state(EditorAppState::Editing)),
        );
    }
}

/// Ctrl+Z handler.
pub fn apply_undo_action(
    actions: Res<EditorActionState>,
    mut log: ResMut<VoxelUndoLog>,
    mut store: ResMut<ChunkStore>,
    mut dirty: ResMut<DirtyChunks>,
    mut console: ResMut<ConsoleLog>,
) {
    if !actions.just_pressed(EditorAction::Undo) {
        return;
    }
    let Some(entry) = log.pop_undo() else {
        console.push("undo: nothing to undo");
        return;
    };
    let current = capture_current(&store, &entry);
    apply_entry(&mut store, &mut dirty, &entry);
    log.push_redo(current);

    let restored_count = match &entry {
        VoxelUndoEntry::Snapshot { chunks } => chunks.len(),
    };
    console.push(format!(
        "undo: restored {restored_count} chunks ({} undo / {} redo)",
        log.undo_len(),
        log.redo_len(),
    ));
}

/// Ctrl+Shift+Z handler.
pub fn apply_redo_action(
    actions: Res<EditorActionState>,
    mut log: ResMut<VoxelUndoLog>,
    mut store: ResMut<ChunkStore>,
    mut dirty: ResMut<DirtyChunks>,
    mut console: ResMut<ConsoleLog>,
) {
    if !actions.just_pressed(EditorAction::Redo) {
        return;
    }
    let Some(entry) = log.pop_redo() else {
        console.push("redo: nothing to redo");
        return;
    };
    let current = capture_current(&store, &entry);
    apply_entry(&mut store, &mut dirty, &entry);
    log.push_undo(current);

    let restored_count = match &entry {
        VoxelUndoEntry::Snapshot { chunks } => chunks.len(),
    };
    console.push(format!(
        "redo: replayed {restored_count} chunks ({} undo / {} redo)",
        log.undo_len(),
        log.redo_len(),
    ));
}

fn capture_current(store: &ChunkStore, entry: &VoxelUndoEntry) -> VoxelUndoEntry {
    let mut chunks = Vec::new();
    for coord in entry.coords() {
        if let Some(chunk) = store.get(coord) {
            chunks.push((coord, chunk.clone()));
        }
    }
    VoxelUndoEntry::Snapshot { chunks }
}

fn apply_entry(store: &mut ChunkStore, dirty: &mut DirtyChunks, entry: &VoxelUndoEntry) {
    match entry {
        VoxelUndoEntry::Snapshot { chunks } => {
            for (coord, chunk) in chunks {
                store.insert(*coord, chunk.clone());
                dirty.mark(*coord);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot_with(coord: ChunkCoord) -> VoxelUndoEntry {
        VoxelUndoEntry::Snapshot {
            chunks: vec![(coord, VoxelChunk::new_air())],
        }
    }

    fn empty_snapshot() -> VoxelUndoEntry {
        VoxelUndoEntry::Snapshot { chunks: Vec::new() }
    }

    #[test]
    fn record_stroke_clears_redo() {
        let mut log = VoxelUndoLog::default();
        log.push_redo(empty_snapshot());
        log.push_redo(empty_snapshot());
        assert_eq!(log.redo_len(), 2);

        log.record_stroke(empty_snapshot());
        assert_eq!(log.undo_len(), 1);
        assert_eq!(log.redo_len(), 0);
    }

    #[test]
    fn undo_redo_round_trip_preserves_chunks() {
        let mut log = VoxelUndoLog::default();
        let probe = ChunkCoord::new(7, 1, 3);
        log.record_stroke(snapshot_with(probe));

        let popped = log.pop_undo().expect("undo entry");
        log.push_redo(empty_snapshot());
        assert_eq!(log.undo_len(), 0);
        assert_eq!(log.redo_len(), 1);

        let coords: Vec<_> = popped.coords().collect();
        assert_eq!(coords, vec![probe]);

        let _redo = log.pop_redo().expect("redo entry");
        log.push_undo(empty_snapshot());
        assert_eq!(log.undo_len(), 1);
        assert_eq!(log.redo_len(), 0);
    }

    #[test]
    fn ring_buffer_caps_each_stack_independently() {
        let mut log = VoxelUndoLog::default();
        for _ in 0..(MAX_UNDO_DEPTH + 5) {
            log.record_stroke(empty_snapshot());
        }
        assert_eq!(log.undo_len(), MAX_UNDO_DEPTH);

        for _ in 0..MAX_UNDO_DEPTH {
            let _ = log.pop_undo();
            log.push_redo(empty_snapshot());
        }
        for _ in 0..5 {
            log.push_redo(empty_snapshot());
        }
        assert_eq!(log.redo_len(), MAX_UNDO_DEPTH);
    }

    #[test]
    fn pop_returns_none_when_empty() {
        let mut log = VoxelUndoLog::default();
        assert!(log.pop_undo().is_none());
        assert!(log.pop_redo().is_none());
    }
}
