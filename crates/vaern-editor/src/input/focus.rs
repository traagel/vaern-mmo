//! egui focus guard.
//!
//! Each frame we ask the egui context whether the user is interacting
//! with a panel (typing in a text field, dragging a slider). When they
//! are, gameplay-class hotkeys + camera mouse-look get suppressed so
//! the input goes where the user expects.

use bevy::prelude::*;
use bevy_egui::EguiContexts;

/// Per-frame snapshot of egui's input focus. Populated PreUpdate.
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct EguiFocusGuard {
    /// True when egui owns the keyboard for this frame (e.g. typing in
    /// a text field). Action bindings short-circuit while this is set.
    pub keyboard_captured: bool,
    /// True when the mouse pointer is over an egui panel. Camera
    /// mouse-look suppresses itself while this is set so dragging a
    /// slider doesn't spin the world.
    pub pointer_over_panel: bool,
}

/// PreUpdate system — read egui state into the guard resource. Other
/// systems read this rather than calling `EguiContexts` directly so
/// `EditorActionState` ordering is deterministic.
pub fn update_egui_focus(mut egui: EguiContexts, mut guard: ResMut<EguiFocusGuard>) {
    let Ok(ctx) = egui.ctx_mut() else {
        guard.keyboard_captured = false;
        guard.pointer_over_panel = false;
        return;
    };
    guard.keyboard_captured = ctx.wants_keyboard_input();
    guard.pointer_over_panel = ctx.is_pointer_over_area() || ctx.wants_pointer_input();
}
