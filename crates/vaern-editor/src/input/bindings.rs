//! [`EditorAction`] enum + key bindings table + per-frame state
//! resource.
//!
//! Other systems read `EditorActionState::just_pressed(action)` instead
//! of inspecting `KeyCode` directly. Lets the binding table change in
//! one place; lets the `focus::EguiFocusGuard` short-circuit the action
//! map when a panel has focus.

use bevy::prelude::*;
use std::collections::HashSet;

use super::focus::EguiFocusGuard;

/// Editor-wide actions the binding table maps to.
///
/// V1 covers: mode hotkeys + save + undo. Mode-internal inputs (e.g.
/// brush radius scrolling) are still raw KeyCode reads in their mode
/// systems — promote here once a second mode wants the same key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EditorAction {
    /// Default mode (1): select / pick / inspect.
    SelectMode,
    /// Place mode (2): drop the palette-selected prop at cursor.
    PlaceMode,
    /// Voxel-brush mode (3): sphere add/subtract.
    VoxelBrushMode,
    /// Biome paint mode (4): paint biome tags onto chunk-aligned tiles.
    BiomePaintMode,
    /// Scatter preview mode (5): live scatter-rule simulation.
    ScatterPreviewMode,
    /// Save current zone (Ctrl+S).
    SaveZone,
    /// Undo last edit (Ctrl+Z).
    Undo,
    /// Redo last undone edit (Ctrl+Shift+Z or Ctrl+Y).
    Redo,
}

/// Static binding table. Returned as a `Vec` so each entry pairs an
/// action with a `Binding`. Keep this small + flat — the table walks
/// linearly per frame.
fn default_bindings() -> Vec<(EditorAction, Binding)> {
    use EditorAction as A;
    use KeyCode as K;
    vec![
        (A::SelectMode, Binding::just(K::Digit1)),
        (A::PlaceMode, Binding::just(K::Digit2)),
        (A::VoxelBrushMode, Binding::just(K::Digit3)),
        (A::BiomePaintMode, Binding::just(K::Digit4)),
        (A::ScatterPreviewMode, Binding::just(K::Digit5)),
        (A::SaveZone, Binding::ctrl(K::KeyS)),
        (A::Undo, Binding::ctrl(K::KeyZ)),
        (A::Redo, Binding::ctrl_shift(K::KeyZ)),
    ]
}

/// Trigger spec for one binding.
#[derive(Debug, Clone, Copy)]
struct Binding {
    key: KeyCode,
    require_ctrl: bool,
    require_shift: bool,
}

impl Binding {
    fn just(key: KeyCode) -> Self {
        Self {
            key,
            require_ctrl: false,
            require_shift: false,
        }
    }
    fn ctrl(key: KeyCode) -> Self {
        Self {
            key,
            require_ctrl: true,
            require_shift: false,
        }
    }
    fn ctrl_shift(key: KeyCode) -> Self {
        Self {
            key,
            require_ctrl: true,
            require_shift: true,
        }
    }
}

/// Per-frame action state. Mode systems read `just_pressed` like
/// `keys.just_pressed(KeyCode::*)`.
#[derive(Resource, Debug, Default)]
pub struct EditorActionState {
    just_pressed: HashSet<EditorAction>,
}

impl EditorActionState {
    pub fn just_pressed(&self, action: EditorAction) -> bool {
        self.just_pressed.contains(&action)
    }

    pub fn iter_just_pressed(&self) -> impl Iterator<Item = &EditorAction> {
        self.just_pressed.iter()
    }

    fn clear(&mut self) {
        self.just_pressed.clear();
    }

    fn fire(&mut self, action: EditorAction) {
        self.just_pressed.insert(action);
    }
}

/// PreUpdate system — refreshes `EditorActionState` from the raw
/// `ButtonInput<KeyCode>` events for this frame. Skipped entirely while
/// egui has keyboard focus so typing in a panel can't fire hotkeys.
pub fn update_action_state(
    keys: Res<ButtonInput<KeyCode>>,
    focus: Res<EguiFocusGuard>,
    mut state: ResMut<EditorActionState>,
) {
    state.clear();
    if focus.keyboard_captured {
        return;
    }

    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);

    for (action, binding) in default_bindings() {
        if !keys.just_pressed(binding.key) {
            continue;
        }
        if binding.require_ctrl != ctrl {
            continue;
        }
        if binding.require_shift != shift {
            continue;
        }
        state.fire(action);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_bindings_table_covers_each_action() {
        let table = default_bindings();
        let actions: HashSet<_> = table.iter().map(|(a, _)| *a).collect();
        for expected in [
            EditorAction::SelectMode,
            EditorAction::PlaceMode,
            EditorAction::VoxelBrushMode,
            EditorAction::BiomePaintMode,
            EditorAction::ScatterPreviewMode,
            EditorAction::SaveZone,
            EditorAction::Undo,
            EditorAction::Redo,
        ] {
            assert!(
                actions.contains(&expected),
                "binding table missing {expected:?}"
            );
        }
    }

    #[test]
    fn action_state_round_trip() {
        let mut s = EditorActionState::default();
        assert!(!s.just_pressed(EditorAction::SaveZone));
        s.fire(EditorAction::SaveZone);
        assert!(s.just_pressed(EditorAction::SaveZone));
        s.clear();
        assert!(!s.just_pressed(EditorAction::SaveZone));
    }
}
