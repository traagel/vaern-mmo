//! Left-side egui panel — environment controls (time of day, fog,
//! sky, draw distance, voxel diagnostics). Bound to the live
//! `EnvSettings` resource; `apply_environment` reads the resource each
//! frame and updates the sun, ambient, fog, and atmosphere state
//! accordingly. The diagnostics block reads voxel-pipeline resources
//! (`ChunkStore`, `DirtyChunks`, `PendingMeshes`, frame time) live so
//! the user can feel the cost when they crank the draw-distance
//! slider.

use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::ViewVisibility;
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::environment::{
    ambient_brightness_at_time, sun_direction_at_time, sun_illuminance_at_time, EnvSettings,
    FogFalloffMode,
};
use vaern_voxel::chunk::{ChunkStore, DirtyChunks};
use vaern_voxel::config::CHUNK_WORLD_SIZE;
use vaern_voxel::perf::SystemFrameTimes;
use vaern_voxel::plugin::{ChunkRenderTag, MeshLifecycleStats, PendingMeshes};

use crate::voxel::biome_blend::BiomeBlendEnabled;
use crate::voxel::stream::NeedsBlendAttach;
use crate::voxel::PerfToggles;

#[allow(clippy::too_many_arguments)]
pub fn draw_environment_panel(
    mut env: ResMut<EnvSettings>,
    mut blend_enabled: ResMut<BiomeBlendEnabled>,
    mut perf_toggles: ResMut<PerfToggles>,
    sft: Res<SystemFrameTimes>,
    mesh_stats: Res<MeshLifecycleStats>,
    mut egui: EguiContexts,
    store: Res<ChunkStore>,
    dirty: Res<DirtyChunks>,
    pending: Res<PendingMeshes>,
    diagnostics: Res<DiagnosticsStore>,
    chunks_with_aabb: Query<(), (With<ChunkRenderTag>, With<Aabb>)>,
    chunks_view_vis: Query<&ViewVisibility, With<ChunkRenderTag>>,
    chunks_pending_attach: Query<(), With<NeedsBlendAttach>>,
) {
    let Ok(ctx) = egui.ctx_mut() else {
        return;
    };

    let drawn = chunks_view_vis.iter().filter(|v| v.get()).count();
    let render_entities = chunks_view_vis.iter().count();
    let aabb_count = chunks_with_aabb.iter().count();
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed());
    let frame_ms = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .and_then(|d| d.smoothed());

    let stats = StreamingStats {
        store_chunks: store.len(),
        dirty_chunks: dirty.len(),
        in_flight_tasks: pending.tasks.len(),
        render_entities,
        drawn_visible: drawn,
        chunks_with_aabb: aabb_count,
        pending_blend_attach: chunks_pending_attach.iter().count(),
        mesh_dispatched: mesh_stats.dispatched_total,
        mesh_completed_with_surface: mesh_stats.completed_with_surface,
        mesh_completed_empty: mesh_stats.completed_empty,
        fps,
        frame_ms,
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
                .default_open(true)
                .show(ui, |ui| {
                    draw_streaming_section(ui, &mut env, &stats, &mut blend_enabled);
                });

            egui::CollapsingHeader::new("Perf isolation + timings")
                .default_open(false)
                .show(ui, |ui| {
                    draw_perf_section(ui, &mut perf_toggles, &sft);
                });
        });
}

/// Snapshot of voxel-pipeline counters fed to the diagnostics block.
struct StreamingStats {
    store_chunks: usize,
    dirty_chunks: usize,
    in_flight_tasks: usize,
    render_entities: usize,
    drawn_visible: usize,
    chunks_with_aabb: usize,
    pending_blend_attach: usize,
    mesh_dispatched: u64,
    mesh_completed_with_surface: u64,
    mesh_completed_empty: u64,
    fps: Option<f64>,
    frame_ms: Option<f64>,
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

fn draw_streaming_section(
    ui: &mut egui::Ui,
    env: &mut EnvSettings,
    stats: &StreamingStats,
    blend_enabled: &mut BiomeBlendEnabled,
) {
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
        "Square: {:.0}×{:.0} m  ({} surface chunks expected)",
        side_m, side_m, chunks_visible
    ));

    ui.separator();
    ui.label(egui::RichText::new("Live diagnostics").strong());
    egui::Grid::new("streaming-diagnostics")
        .num_columns(2)
        .spacing([8.0, 2.0])
        .show(ui, |ui| {
            ui.label("ChunkStore");
            ui.label(format!("{} chunks", stats.store_chunks));
            ui.end_row();

            ui.label("Dirty queue");
            ui.label(format!("{}", stats.dirty_chunks));
            ui.end_row();

            ui.label("In-flight tasks");
            ui.label(format!("{}", stats.in_flight_tasks));
            ui.end_row();

            ui.label("Render entities");
            ui.label(format!("{}", stats.render_entities));
            ui.end_row();

            ui.label("With AABB");
            ui.label(format!("{}", stats.chunks_with_aabb));
            ui.end_row();

            ui.label("Visible (post-cull)");
            ui.label(format!(
                "{}  ({:.0}% drawn)",
                stats.drawn_visible,
                if stats.render_entities > 0 {
                    (stats.drawn_visible as f32 / stats.render_entities as f32) * 100.0
                } else {
                    0.0
                }
            ));
            ui.end_row();

            ui.label("FPS / frame");
            ui.label(match (stats.fps, stats.frame_ms) {
                (Some(f), Some(ms)) => format!("{:>5.1} / {:>5.2} ms", f, ms),
                _ => "—".to_string(),
            });
            ui.end_row();

            // Load-path diagnostics (Phase 1D). If unedited chunks
            // aren't rendering after a saved-world load, watch these:
            //   • pending_blend_attach should drain to 0 within a sec
            //   • mesh_dispatched should climb past edit-count rapidly
            //   • completed_with_surface should ≈ render_entities
            ui.label("Pending attach");
            ui.label(format!("{}", stats.pending_blend_attach));
            ui.end_row();

            ui.label("Mesh dispatched");
            ui.label(format!("{}", stats.mesh_dispatched));
            ui.end_row();

            ui.label("→ with surface");
            ui.label(format!("{}", stats.mesh_completed_with_surface));
            ui.end_row();

            ui.label("→ empty");
            ui.label(format!("{}", stats.mesh_completed_empty));
            ui.end_row();
        });
    ui.small("Drawn % well below 100 → frustum cull is doing its job.");
    ui.small("In-flight saturates at MeshingBudget (64) under heavy edits.");

    ui.separator();
    ui.label(egui::RichText::new("Material A/B").strong());
    let mut on = blend_enabled.0;
    if ui
        .checkbox(&mut on, "Use biome blend material (off = plain PBR)")
        .changed()
    {
        blend_enabled.0 = on;
    }
    ui.small("Toggle off → chunks render with a vanilla StandardMaterial");
    ui.small("(no biome textures). FPS jump = custom shader was the cost;");
    ui.small("no jump = cost is in atmosphere / bloom / something else.");
}

/// Phase 2 perf-isolation section: subsystem skip toggles + per-system
/// frame-time table. Each toggle gates one suspect system at runtime
/// so the user can A/B without code changes.
fn draw_perf_section(
    ui: &mut egui::Ui,
    toggles: &mut PerfToggles,
    sft: &SystemFrameTimes,
) {
    ui.label(egui::RichText::new("Subsystem skips").strong());
    ui.checkbox(&mut toggles.hide_chunks, "Hide all chunks (Visibility::Hidden)");
    ui.checkbox(&mut toggles.skip_eviction, "Skip eviction system");
    ui.checkbox(&mut toggles.skip_streamer, "Skip streamer system");
    ui.small("Toggle each → FPS delta tells you that system's contribution.");

    ui.separator();
    ui.label(egui::RichText::new("Per-system timings (last 1s)").strong());

    // Sort by mean descending so the dominant system lands at the top.
    let mut entries: Vec<(&'static str, f64, f64, usize)> = sft
        .entries()
        .map(|(name, avg)| {
            let mean_us = avg.mean().as_nanos() as f64 / 1000.0;
            let max_us = avg.max().as_nanos() as f64 / 1000.0;
            (name, mean_us, max_us, avg.sample_count())
        })
        .collect();
    entries.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    if entries.is_empty() {
        ui.small("(no samples yet)");
    } else {
        egui::Grid::new("system-timings")
            .num_columns(4)
            .spacing([8.0, 2.0])
            .striped(true)
            .show(ui, |ui| {
                ui.label(egui::RichText::new("system").strong());
                ui.label(egui::RichText::new("mean µs").strong());
                ui.label(egui::RichText::new("max µs").strong());
                ui.label(egui::RichText::new("n").strong());
                ui.end_row();
                for (name, mean, max, n) in entries {
                    ui.label(name);
                    ui.label(format!("{:>7.1}", mean));
                    ui.label(format!("{:>7.1}", max));
                    ui.label(format!("{}", n));
                    ui.end_row();
                }
            });
    }
}
