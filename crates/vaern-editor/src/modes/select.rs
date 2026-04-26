//! Select mode — default mode.
//!
//! V1: the camera flies, nothing else happens. The select mode is the
//! resting state where no edit is being applied.
//!
//! Future: cursor raycast → highlight nearest dressing prop or chunk
//! cell; clicking populates `dressing::selection::SelectedProp`.

use bevy::prelude::*;

use super::{active_mode_is, EditorMode};
use crate::state::EditorAppState;

pub struct SelectModePlugin;

impl Plugin for SelectModePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            tick_noop
                .run_if(in_state(EditorAppState::Editing))
                .run_if(active_mode_is(EditorMode::Select)),
        );
    }
}

/// Placeholder system. Holds the slot until the raycast picker lands.
/// Returns immediately — present for the system-graph shape so the
/// Plugin schedule isn't empty when the mode is active.
#[allow(clippy::needless_pass_by_value)]
fn tick_noop(_time: Res<Time>) {
    // TODO(editor): cursor → world raycast → highlight nearest dressing.
}
