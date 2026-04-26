//! Left-side egui panel — environment controls (time of day, fog,
//! sky, draw distance). Bound to the live `EnvSettings` resource;
//! `apply_environment` reads the resource each frame and updates the
//! sun, ambient, fog, and atmosphere state accordingly.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::environment::{
    ambient_brightness_at_time, sun_direction_at_time, sun_illuminance_at_time, EnvSettings,
    FogFalloffMode,
};
use vaern_voxel::config::CHUNK_WORLD_SIZE;

pub fn draw_environment_panel(mut env: ResMut<EnvSettings>, mut egui: EguiContexts) {
    let Ok(ctx) = egui.ctx_mut() else {
        return;
    };

    egui::SidePanel::left("editor_environment")
        .default_width(260.0)
        .resizable(true)
        .show(ctx, |ui| {
            ui.heading("Environment");
            ui.separator();

            egui::CollapsingHeader::new("Time of day")
                .default_open(true)
                .show(ui, |ui| draw_time_section(ui, &mut env));

            egui::CollapsingHeader::new("Fog")
                .default_open(false)
                .show(ui, |ui| draw_fog_section(ui, &mut env));

            egui::CollapsingHeader::new("Sky")
                .default_open(false)
                .show(ui, |ui| draw_sky_section(ui, &mut env));

            egui::CollapsingHeader::new("Streaming")
                .default_open(false)
                .show(ui, |ui| draw_streaming_section(ui, &mut env));
        });
}

fn draw_time_section(ui: &mut egui::Ui, env: &mut EnvSettings) {
    let h = env.time_hours.floor() as i32;
    let m = ((env.time_hours - h as f32) * 60.0).floor() as i32;
    ui.add(
        egui::Slider::new(&mut env.time_hours, 0.0..=24.0)
            .text(format!("{:02}:{:02}", h, m)),
    );

    ui.checkbox(&mut env.autoplay, "Autoplay");
    ui.add_enabled_ui(env.autoplay, |ui| {
        ui.add(
            egui::Slider::new(&mut env.autoplay_seconds_per_hour, 5.0..=600.0)
                .text("Real seconds / game hour")
                .logarithmic(true),
        );
    });

    ui.separator();
    let dir = sun_direction_at_time(env.time_hours);
    let elev_deg = (-dir.y).asin().to_degrees();
    let lux = sun_illuminance_at_time(env.time_hours);
    let amb = ambient_brightness_at_time(env.time_hours);
    ui.small(format!("Sun elevation: {:.0}°", elev_deg));
    ui.small(format!("Illuminance: {:.0} lux", lux));
    ui.small(format!("Ambient: {:.0}", amb));
}

fn draw_fog_section(ui: &mut egui::Ui, env: &mut EnvSettings) {
    ui.checkbox(&mut env.fog_enabled, "Enabled");
    ui.add_enabled_ui(env.fog_enabled, |ui| {
        ui.add(
            egui::Slider::new(&mut env.fog_visibility_u, 200.0..=3000.0)
                .text("Visibility (u)")
                .logarithmic(true),
        );
        ui.horizontal(|ui| {
            ui.label("Falloff:");
            for mode in FogFalloffMode::ALL {
                let active = env.fog_falloff == mode;
                if ui.selectable_label(active, mode.label()).clicked() {
                    env.fog_falloff = mode;
                }
            }
        });
        ui.checkbox(&mut env.fog_color_auto, "Auto color (track sun)");
        ui.add_enabled_ui(!env.fog_color_auto, |ui| {
            ui.horizontal(|ui| {
                ui.label("Manual:");
                ui.color_edit_button_rgba_unmultiplied(&mut env.fog_color_manual);
            });
        });
    });
}

fn draw_sky_section(ui: &mut egui::Ui, env: &mut EnvSettings) {
    ui.checkbox(&mut env.atmosphere_enabled, "Procedural atmosphere");
    if !env.atmosphere_enabled {
        ui.small("Off → solid clear color (matches ClearColor)");
    }
}

fn draw_streaming_section(ui: &mut egui::Ui, env: &mut EnvSettings) {
    ui.add(
        egui::Slider::new(&mut env.draw_distance_chunks, 1..=64)
            .text("Draw distance (chunks)")
            .logarithmic(true),
    );
    let n = env.draw_distance_chunks as f32;
    let radius_m = n * CHUNK_WORLD_SIZE;
    let side_chunks = (env.draw_distance_chunks * 2 + 1) as u32;
    let side_m = side_chunks as f32 * CHUNK_WORLD_SIZE;
    let chunks_visible = side_chunks * side_chunks;
    ui.small(format!(
        "Radius: {:.2} km ({:.0} m)",
        radius_m / 1000.0,
        radius_m
    ));
    ui.small(format!(
        "Square: {:.0}×{:.0} m  ({} surface chunks)",
        side_m, side_m, chunks_visible
    ));
    ui.small("Perf scales O(N²); 64 → ~27× the chunks of 12.");
}
