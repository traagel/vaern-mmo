//! Top toolbar — mode selector + zone label + FPS readout + save
//! button.

use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::camera::FreeFlyCamera;
use crate::modes::{switch_mode, ActiveMode, EditorMode};
use crate::persistence::voxel_io::SaveVoxelEditsRequested;
use crate::state::EditorContext;
use crate::ui::console::ConsoleLog;

/// Frame-time diagnostic id for the FPS display. Resolved through the
/// runtime store; if the diagnostic isn't registered (caller didn't
/// add `FrameTimeDiagnosticsPlugin`), the toolbar shows `--`.
fn fps_value(diag: &DiagnosticsStore) -> Option<f32> {
    diag.get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
        .map(|v| v as f32)
}

/// Render the top toolbar each frame.
#[allow(clippy::too_many_arguments)]
pub fn draw_toolbar(
    mut egui: EguiContexts,
    mut active: ResMut<ActiveMode>,
    mut ctx: ResMut<EditorContext>,
    mut log: ResMut<ConsoleLog>,
    mut save_req: ResMut<SaveVoxelEditsRequested>,
    diag: Option<Res<DiagnosticsStore>>,
    cameras: Query<&Transform, With<FreeFlyCamera>>,
) {
    let Ok(egui_ctx) = egui.ctx_mut() else {
        return;
    };

    egui::TopBottomPanel::top("editor_toolbar")
        .show_separator_line(true)
        .show(egui_ctx, |ui| {
            ui.horizontal(|ui| {
                for mode in EditorMode::all() {
                    let is_active = active.0 == mode;
                    let label = mode.label();
                    let mut button = egui::Button::new(label);
                    if is_active {
                        button = button.fill(egui::Color32::from_rgb(60, 90, 150));
                    } else if !mode.is_implemented() {
                        button = button.fill(egui::Color32::from_rgb(48, 48, 48));
                    }
                    if ui.add(button).clicked() {
                        switch_mode(&mut active, &mut ctx, mode);
                        if !mode.is_implemented() {
                            log.push(format!("{label} mode is not implemented yet"));
                        } else {
                            log.push(format!("switched to {label} mode"));
                        }
                    }
                }

                ui.separator();
                if ui
                    .button("Save")
                    .on_hover_text(
                        "Diff every loaded chunk against the heightfield baseline and \
                         write src/generated/world/voxel_edits.bin. The runtime server \
                         loads this file at startup; clients receive the deltas via \
                         the existing reconnect-snapshot path on connect.",
                    )
                    .clicked()
                {
                    save_req.requested = true;
                    log.push("save: queued — diff + write next frame");
                }

                // Right-aligned: zone id + camera coords + FPS.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let fps = diag
                        .as_deref()
                        .and_then(fps_value)
                        .map(|v| format!("{v:5.1}"))
                        .unwrap_or_else(|| "  --".into());
                    ui.label(format!("FPS {fps}"));
                    ui.separator();
                    if let Ok(cam) = cameras.single() {
                        let p = cam.translation;
                        ui.label(format!("cam ({:6.1}, {:6.1}, {:6.1})", p.x, p.y, p.z));
                        ui.separator();
                    }
                    ui.label(format!("zone: {}", ctx.active_zone));
                });
            });
        });
}
