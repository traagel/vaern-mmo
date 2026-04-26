//! Editor input — high-level action layer that sits above raw Bevy
//! `ButtonInput<KeyCode>`.
//!
//! Defines an [`bindings::EditorAction`] enum mapped from key + mouse
//! events, and an [`focus::EguiFocusGuard`] resource that flags when an
//! egui panel has keyboard / pointer focus so gameplay-class hotkeys
//! suppress themselves.
//!
//! V1 is intentionally minimal: only the actions the toolbar / mode
//! switcher needs are wired. Per-mode input lives inside each mode's
//! own systems and reads `ButtonInput<KeyCode>` directly until pressure
//! to centralize.

pub mod bindings;
pub mod focus;

use bevy::prelude::*;

use crate::state::EditorAppState;

/// Plugin: registers [`bindings::EditorActionState`] +
/// [`focus::EguiFocusGuard`] resources and runs their per-frame
/// updaters.
pub struct EditorInputPlugin;

impl Plugin for EditorInputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<bindings::EditorActionState>()
            .init_resource::<focus::EguiFocusGuard>()
            .add_systems(
                PreUpdate,
                (
                    focus::update_egui_focus,
                    bindings::update_action_state.after(focus::update_egui_focus),
                )
                    .run_if(in_state(EditorAppState::Editing)),
            );
    }
}
