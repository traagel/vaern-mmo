//! egui visuals tweak — slightly darker than the default to read as a
//! tool, not a game UI.

use bevy_egui::{egui, EguiContexts};

pub fn apply_editor_theme(mut egui: EguiContexts) {
    let Ok(ctx) = egui.ctx_mut() else {
        return;
    };
    let mut visuals = egui::Visuals::dark();
    visuals.window_fill = egui::Color32::from_rgb(24, 26, 30);
    visuals.panel_fill = egui::Color32::from_rgb(28, 30, 35);
    ctx.set_visuals(visuals);
}
