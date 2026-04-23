//! Heads-up-display widgets drawn via bevy_egui.
//!
//! Currently: a WoW-style compass strip at the top of the screen that
//! scrolls as the camera yaw changes. N / NE / E / SE / S / SW / W / NW
//! are laid out across the strip; the vertical mark at center shows the
//! camera's current facing direction.

use std::f32::consts::{PI, TAU};

use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};

use crate::menu::AppState;
use crate::scene::CameraController;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            EguiPrimaryContextPass,
            compass_ui.run_if(in_state(AppState::InGame)),
        );
    }
}

/// 16 labels at 22.5° spacing — eight cardinals/intercardinals plus minor
/// ticks in between for a denser strip feel.
const CARDINALS: &[(f32, &str)] = &[
    (0.0, "N"),
    (22.5, "·"),
    (45.0, "NE"),
    (67.5, "·"),
    (90.0, "E"),
    (112.5, "·"),
    (135.0, "SE"),
    (157.5, "·"),
    (180.0, "S"),
    (202.5, "·"),
    (225.0, "SW"),
    (247.5, "·"),
    (270.0, "W"),
    (292.5, "·"),
    (315.0, "NW"),
    (337.5, "·"),
];

/// Compass strip width in pixels. Covers ±90° of the camera's current yaw
/// (so 180° of the horizon is visible at any time).
const STRIP_WIDTH: f32 = 360.0;
const STRIP_HEIGHT: f32 = 26.0;
/// Degrees of horizon displayed per half-strip (180° total → ±90° from center).
const DEGREES_PER_HALF_STRIP: f32 = 90.0;

fn compass_ui(mut contexts: EguiContexts, controller: Res<CameraController>) {
    let Ok(ctx) = contexts.ctx_mut() else { return };

    // Camera yaw is stored in radians where yaw=0 points the camera forward
    // at -Z (north). Convert to degrees for compass math.
    let cam_yaw_deg = controller.yaw.rem_euclid(TAU).to_degrees();

    egui::Area::new(egui::Id::new("compass_strip"))
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 12.0))
        .show(ctx, |ui| {
            let (rect, _resp) = ui.allocate_exact_size(
                egui::vec2(STRIP_WIDTH, STRIP_HEIGHT),
                egui::Sense::hover(),
            );
            let painter = ui.painter();

            // Backdrop
            painter.rect_filled(
                rect,
                4.0,
                egui::Color32::from_rgba_unmultiplied(0, 0, 0, 170),
            );
            painter.rect_stroke(
                rect,
                4.0,
                egui::Stroke::new(1.0, egui::Color32::from_gray(80)),
                egui::StrokeKind::Outside,
            );

            // Per-label: compute screen x from shortest signed diff
            // (cardinal_deg - camera_yaw) mod 360 → [-180, 180]. Drop
            // anything > ±90° (outside the strip).
            for (cardinal_deg, label) in CARDINALS {
                let mut diff = cardinal_deg - cam_yaw_deg;
                diff = ((diff + 180.0).rem_euclid(360.0)) - 180.0;
                if diff.abs() > DEGREES_PER_HALF_STRIP {
                    continue;
                }
                let fraction = diff / DEGREES_PER_HALF_STRIP; // -1..=1
                let x = rect.center().x + fraction * (STRIP_WIDTH / 2.0 - 14.0);

                // Major cardinals (N/E/S/W/NE/...) are brighter and larger
                // than the ticks.
                let is_tick = *label == "·";
                let (font_size, color) = if is_tick {
                    (12.0, egui::Color32::from_gray(140))
                } else {
                    let major = matches!(*label, "N" | "E" | "S" | "W");
                    let col = if major {
                        egui::Color32::from_rgb(255, 220, 140)
                    } else {
                        egui::Color32::from_gray(220)
                    };
                    (if major { 16.0 } else { 13.0 }, col)
                };
                painter.text(
                    egui::pos2(x, rect.center().y),
                    egui::Align2::CENTER_CENTER,
                    *label,
                    egui::FontId::proportional(font_size),
                    color,
                );
            }

            // Center indicator: a red vertical line marking "straight ahead".
            let center_x = rect.center().x;
            painter.line_segment(
                [
                    egui::pos2(center_x, rect.min.y + 2.0),
                    egui::pos2(center_x, rect.max.y - 2.0),
                ],
                egui::Stroke::new(2.0, egui::Color32::from_rgb(220, 60, 60)),
            );

            // Small "°" readout under the strip for precision.
            let degrees = cam_yaw_deg;
            painter.text(
                egui::pos2(center_x, rect.max.y + 2.0),
                egui::Align2::CENTER_TOP,
                format!("{:.0}°", degrees),
                egui::FontId::monospace(10.0),
                egui::Color32::from_gray(160),
            );
        });
    // Silence a warning if we ever add unused imports; keeps the PI import
    // meaningful when someone extends this file.
    let _ = PI;
}
