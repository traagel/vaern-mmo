//! Scatter-preview mode — live re-run of `ScatterRule`s while the
//! author tweaks density / spacing / hub-exclusion in the inspector.
//!
//! V1: stub. The scatter algorithm itself lives in
//! `vaern-client/src/scene/dressing.rs` (private helpers like
//! `scatter_placements`). Promoting those into a shared `vaern-editor`
//! function — or extracting them out of the client crate — is a
//! prerequisite for this mode.

use bevy::prelude::*;

use super::{active_mode_is, EditorMode};
use crate::state::EditorAppState;

pub struct ScatterPreviewModePlugin;

impl Plugin for ScatterPreviewModePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            tick_stub
                .run_if(in_state(EditorAppState::Editing))
                .run_if(active_mode_is(EditorMode::ScatterPreview)),
        );
    }
}

fn tick_stub() {
    // TODO(editor): re-run scatter on rule edits + diff against current.
}
