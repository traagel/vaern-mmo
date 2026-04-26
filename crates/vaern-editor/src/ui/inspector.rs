//! Right side panel — inspector for the currently-selected entity or
//! mode-specific tunables (brush, biome paint).

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use vaern_core::terrain;

use crate::dressing::selection::SelectedProp;
use crate::dressing::EditorDressingEntity;
use crate::modes::biome_paint::{BiomePaintState, MAX_PAINT_RADIUS_CHUNKS};
use crate::modes::voxel_brush::{
    BrushPresets, BrushTool, MirrorPlane, VoxelBrushState, MAX_BRUSH_RADIUS, MIN_BRUSH_RADIUS,
};
use crate::modes::{ActiveMode, EditorMode};
use crate::state::EditorContext;
use crate::ui::console::ConsoleLog;
use crate::voxel::biomes::BiomeKey;
use crate::voxel::overrides::BiomeOverrideMap;
use crate::world::ActiveZoneHubs;
use vaern_voxel::edit::smooth::MAX_SMOOTH_ITERATIONS;
use vaern_voxel::edit::{BrushMode, Falloff, StampShape};

#[allow(clippy::too_many_arguments)]
pub fn draw_inspector(
    mut commands: Commands,
    mut egui: EguiContexts,
    mut selected: ResMut<SelectedProp>,
    active: Res<ActiveMode>,
    ctx: Res<EditorContext>,
    mut brush: ResMut<VoxelBrushState>,
    mut presets: ResMut<BrushPresets>,
    mut paint: ResMut<BiomePaintState>,
    overrides: Res<BiomeOverrideMap>,
    hubs: Res<ActiveZoneHubs>,
    mut props: Query<(&mut Transform, &mut EditorDressingEntity)>,
    keys: Res<ButtonInput<KeyCode>>,
    mut log: ResMut<ConsoleLog>,
) {
    let Ok(egui_ctx) = egui.ctx_mut() else {
        return;
    };

    let delete_pressed = keys.just_pressed(KeyCode::Delete);

    egui::SidePanel::right("editor_inspector")
        .default_width(280.0)
        .show(egui_ctx, |ui| {
            ui.heading("Inspector");
            ui.separator();
            ui.label(format!("Mode: {}", active.0.label()));
            ui.label(format!("Zone: {}", ctx.active_zone));
            ui.separator();

            match (selected.0, active.0) {
                (Some(entity), _) => {
                    if !draw_prop_inspector(
                        ui,
                        entity,
                        &mut commands,
                        &mut selected,
                        &hubs,
                        &mut props,
                        &mut log,
                        delete_pressed,
                    ) {
                        ui.label("(selection cleared)");
                    }
                }
                (None, EditorMode::VoxelBrush) => {
                    draw_brush_inspector(ui, &mut brush, &mut presets)
                }
                (None, EditorMode::BiomePaint) => {
                    draw_biome_paint_inspector(ui, &mut paint, &overrides)
                }
                (None, mode) => {
                    ui.label(format!(
                        "{} mode — select a prop to edit it",
                        mode.label()
                    ));
                }
            }
        });
}

#[allow(clippy::too_many_arguments)]
fn draw_prop_inspector(
    ui: &mut egui::Ui,
    entity: Entity,
    commands: &mut Commands,
    selected: &mut SelectedProp,
    hubs: &ActiveZoneHubs,
    props: &mut Query<(&mut Transform, &mut EditorDressingEntity)>,
    log: &mut ConsoleLog,
    delete_pressed: bool,
) -> bool {
    let Ok((mut tf, mut dressing)) = props.get_mut(entity) else {
        return false;
    };

    let hub_origin = hubs.origins.get(&dressing.hub_id).copied();

    ui.heading(&dressing.slug);
    ui.label(format!("Hub: {}", dressing.hub_id));
    ui.separator();

    let mut offset_x = dressing.authored.offset.x;
    let mut offset_z = dressing.authored.offset.z;
    let mut rot = dressing.authored.rotation_y_deg;
    let mut scale = dressing.authored.scale;
    let had_abs = dressing.authored.absolute_y.is_some();
    let mut abs_y = dressing.authored.absolute_y.unwrap_or(0.0);
    let mut use_abs = had_abs;

    let mut changed = false;

    ui.label("Offset (hub-local):");
    ui.horizontal(|ui| {
        ui.label("X");
        if ui.add(egui::DragValue::new(&mut offset_x).speed(0.1).suffix("u")).changed() {
            changed = true;
        }
        ui.label("Z");
        if ui.add(egui::DragValue::new(&mut offset_z).speed(0.1).suffix("u")).changed() {
            changed = true;
        }
    });

    ui.horizontal(|ui| {
        ui.label("Rotation Y");
        if ui.add(egui::DragValue::new(&mut rot).speed(1.0).range(-360.0..=360.0).suffix("°")).changed() {
            changed = true;
        }
    });

    ui.horizontal(|ui| {
        ui.label("Scale");
        if ui.add(egui::DragValue::new(&mut scale).speed(0.05).range(0.05..=20.0)).changed() {
            changed = true;
        }
    });

    ui.horizontal(|ui| {
        if ui.checkbox(&mut use_abs, "Override Y").changed() {
            changed = true;
        }
        ui.add_enabled_ui(use_abs, |ui| {
            if ui.add(egui::DragValue::new(&mut abs_y).speed(0.1).suffix("u")).changed() {
                changed = true;
            }
        });
    });

    ui.separator();
    let delete_clicked = ui.button("Delete").clicked();

    if changed {
        dressing.authored.offset.x = offset_x;
        dressing.authored.offset.z = offset_z;
        dressing.authored.rotation_y_deg = rot;
        dressing.authored.scale = scale.max(0.01);
        dressing.authored.absolute_y = if use_abs { Some(abs_y) } else { None };

        if let Some((hub_x, hub_z)) = hub_origin {
            let world_x = hub_x + offset_x;
            let world_z = hub_z + offset_z;
            let world_y = if use_abs {
                abs_y
            } else {
                terrain::height(world_x, world_z)
            };
            tf.translation = Vec3::new(world_x, world_y, world_z);
            tf.rotation = Quat::from_rotation_y(rot.to_radians());
            tf.scale = Vec3::splat(scale.max(0.01));
        }
    }

    if delete_clicked || delete_pressed {
        let slug = dressing.slug.clone();
        let hub_id = dressing.hub_id.clone();
        commands.entity(entity).despawn();
        selected.clear();
        log.push(format!("deleted {slug} from {hub_id}"));
    }

    true
}

fn draw_brush_inspector(
    ui: &mut egui::Ui,
    brush: &mut VoxelBrushState,
    presets: &mut BrushPresets,
) {
    ui.label("Voxel brush — landscaping toolkit");
    ui.separator();

    let prev_tool = brush.tool;
    ui.horizontal(|ui| {
        for tool in &BrushTool::ALL[..4] {
            let active = brush.tool == *tool;
            if ui.selectable_label(active, tool.label()).clicked() {
                brush.tool = *tool;
            }
        }
    });
    ui.horizontal(|ui| {
        for tool in &BrushTool::ALL[4..] {
            let active = brush.tool == *tool;
            if ui.selectable_label(active, tool.label()).clicked() {
                brush.tool = *tool;
            }
        }
    });
    if prev_tool == BrushTool::Ramp && brush.tool != BrushTool::Ramp {
        brush.ramp_endpoint_a = None;
    }
    ui.separator();

    draw_mirror_panel(ui, brush);
    ui.separator();

    draw_falloff_selector(ui, brush);
    ui.separator();

    let mut radius = brush.radius;
    if ui.add(egui::Slider::new(&mut radius, MIN_BRUSH_RADIUS..=MAX_BRUSH_RADIUS).text("Radius (u)")).changed() {
        brush.radius = radius;
    }
    let mut spacing = brush.drag_spacing_factor;
    if ui.add(egui::Slider::new(&mut spacing, 0.1..=2.0).text("Drag spacing (× radius)")).changed() {
        brush.drag_spacing_factor = spacing;
    }

    ui.separator();
    match brush.tool {
        BrushTool::Sphere => draw_sphere_panel(ui, brush),
        BrushTool::Smooth => draw_smooth_panel(ui, brush),
        BrushTool::Flatten => draw_flatten_panel(ui, brush),
        BrushTool::Ramp => draw_ramp_panel(ui, brush),
        BrushTool::Reset => draw_reset_panel(ui),
        BrushTool::Cylinder => draw_cylinder_panel(ui, brush),
        BrushTool::Box => draw_box_panel(ui, brush),
        BrushTool::Stamp => draw_stamp_panel(ui, brush),
    }

    ui.separator();
    draw_presets_panel(ui, brush, presets);

    ui.separator();
    ui.small("LMB to apply (drag for continuous) · scroll to resize · Ctrl+Z undo · Ctrl+1..4 load preset");
}

fn draw_falloff_selector(ui: &mut egui::Ui, brush: &mut VoxelBrushState) {
    ui.horizontal(|ui| {
        ui.label("Falloff:");
        for f in [Falloff::Hard, Falloff::Linear, Falloff::Smooth] {
            let label = match f {
                Falloff::Hard => "Hard",
                Falloff::Linear => "Linear",
                Falloff::Smooth => "Smooth",
            };
            let active = brush.falloff == f;
            if ui.selectable_label(active, label).clicked() {
                brush.falloff = f;
            }
        }
    });
}

fn draw_mirror_panel(ui: &mut egui::Ui, brush: &mut VoxelBrushState) {
    ui.horizontal(|ui| {
        ui.label("Mirror:");
        for m in MirrorPlane::ALL {
            let active = brush.mirror == m;
            if ui.selectable_label(active, m.label()).clicked() {
                brush.mirror = m;
            }
        }
    });
    if brush.mirror != MirrorPlane::None {
        ui.horizontal(|ui| {
            ui.label("Origin X");
            ui.add(egui::DragValue::new(&mut brush.mirror_origin_x).speed(1.0).suffix("u"));
            ui.label("Z");
            ui.add(egui::DragValue::new(&mut brush.mirror_origin_z).speed(1.0).suffix("u"));
        });
    }
}

fn draw_sphere_panel(ui: &mut egui::Ui, brush: &mut VoxelBrushState) {
    ui.checkbox(&mut brush.subtract, "Subtract (carve)");
    if !brush.subtract {
        ui.label("Mode: Add (raise mound)");
    }
    ui.small("Hold Shift to invert this stroke.");
}

fn draw_smooth_panel(ui: &mut egui::Ui, brush: &mut VoxelBrushState) {
    ui.label("Smooth (neighbor-average blur)");
    let mut strength = brush.smooth_strength;
    if ui.add(egui::Slider::new(&mut strength, 0.0..=1.0).text("Strength")).changed() {
        brush.smooth_strength = strength;
    }
    let mut iters = brush.smooth_iterations as i32;
    if ui.add(egui::Slider::new(&mut iters, 1..=(MAX_SMOOTH_ITERATIONS as i32)).text("Iterations")).changed() {
        brush.smooth_iterations = iters.max(1) as u32;
    }
}

fn draw_flatten_panel(ui: &mut egui::Ui, brush: &mut VoxelBrushState) {
    ui.label("Flatten (paint Y onto XZ disc)");
    ui.checkbox(&mut brush.flatten_use_cursor_y, "Use cursor Y");
    ui.add_enabled_ui(!brush.flatten_use_cursor_y, |ui| {
        ui.horizontal(|ui| {
            ui.label("Target Y");
            ui.add(egui::DragValue::new(&mut brush.flatten_target_y).speed(0.5).suffix("u"));
        });
    });
    ui.horizontal(|ui| {
        ui.label("Half-height");
        ui.add(egui::DragValue::new(&mut brush.flatten_half_height).speed(0.25).range(1.0..=64.0).suffix("u"));
    });
}

fn draw_ramp_panel(ui: &mut egui::Ui, brush: &mut VoxelBrushState) {
    ui.label("Ramp (Y-lerp between two clicks)");
    match brush.ramp_endpoint_a {
        None => {
            ui.colored_label(egui::Color32::from_gray(180), "Endpoint A: not set — click ground to set");
        }
        Some(a) => {
            ui.colored_label(
                egui::Color32::from_rgb(120, 220, 140),
                format!("Endpoint A: ({:.1}, {:.1}, {:.1})", a.x, a.y, a.z),
            );
            ui.label("Click ground for B (or ESC to cancel)");
            if ui.button("Cancel A").clicked() {
                brush.ramp_endpoint_a = None;
            }
        }
    }
    ui.horizontal(|ui| {
        ui.label("Half-width");
        ui.add(egui::DragValue::new(&mut brush.ramp_half_width).speed(0.25).range(0.5..=32.0).suffix("u"));
    });
    ui.horizontal(|ui| {
        ui.label("Half-height");
        ui.add(egui::DragValue::new(&mut brush.ramp_half_height).speed(0.25).range(1.0..=64.0).suffix("u"));
    });
}

fn draw_reset_panel(ui: &mut egui::Ui) {
    ui.label("Reset (paint heightfield baseline)");
    ui.small("Reverts to the world-generator surface within radius.");
}

fn draw_cylinder_panel(ui: &mut egui::Ui, brush: &mut VoxelBrushState) {
    ui.label("Cylinder (vertical column carve)");
    ui.checkbox(&mut brush.cylinder_subtract, "Subtract (carve)");
    ui.horizontal(|ui| {
        ui.label("Half-height");
        ui.add(egui::DragValue::new(&mut brush.cylinder_half_height).speed(0.25).range(1.0..=64.0).suffix("u"));
    });
}

fn draw_box_panel(ui: &mut egui::Ui, brush: &mut VoxelBrushState) {
    ui.label("Box (rectangular volume)");
    ui.checkbox(&mut brush.box_subtract, "Subtract (carve)");
    ui.horizontal(|ui| {
        ui.label("Half-extents");
        ui.add(egui::DragValue::new(&mut brush.box_half_extents.x).speed(0.25).range(0.5..=32.0).prefix("X "));
        ui.add(egui::DragValue::new(&mut brush.box_half_extents.y).speed(0.25).range(0.5..=32.0).prefix("Y "));
        ui.add(egui::DragValue::new(&mut brush.box_half_extents.z).speed(0.25).range(0.5..=32.0).prefix("Z "));
    });
}

fn draw_stamp_panel(ui: &mut egui::Ui, brush: &mut VoxelBrushState) {
    ui.label("Stamp (procedural shape library)");
    ui.horizontal(|ui| {
        ui.label("Shape:");
        for s in [StampShape::Crater, StampShape::Archway, StampShape::Ridge, StampShape::Stairs] {
            let label = match s {
                StampShape::Crater => "Crater",
                StampShape::Archway => "Archway",
                StampShape::Ridge => "Ridge",
                StampShape::Stairs => "Stairs",
            };
            let active = brush.stamp_shape == s;
            if ui.selectable_label(active, label).clicked() {
                brush.stamp_shape = s;
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label("Mode:");
        for m in [BrushMode::Paint, BrushMode::Subtract, BrushMode::Union] {
            let label = match m {
                BrushMode::Paint => "Paint",
                BrushMode::Subtract => "Subtract",
                BrushMode::Union => "Union",
                BrushMode::Intersect => "Intersect",
            };
            let active = brush.stamp_mode == m;
            if ui.selectable_label(active, label).clicked() {
                brush.stamp_mode = m;
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label("Rotation Y");
        ui.add(egui::DragValue::new(&mut brush.stamp_rotation_y_deg).speed(5.0).range(-180.0..=180.0).suffix("°"));
    });
}

fn draw_biome_paint_inspector(
    ui: &mut egui::Ui,
    paint: &mut BiomePaintState,
    overrides: &BiomeOverrideMap,
) {
    ui.label("Biome paint — chunk-aligned texture stamping");
    ui.separator();

    ui.label("Selected biome:");
    for chunk in BiomeKey::ALL.chunks(3) {
        ui.horizontal(|ui| {
            for biome in chunk {
                let active = paint.selected == *biome;
                if ui.selectable_label(active, biome.label()).clicked() {
                    paint.selected = *biome;
                }
            }
        });
    }
    ui.separator();

    let footprint = paint.radius_chunks * 2 + 1;
    let mut r = paint.radius_chunks as i32;
    if ui.add(egui::Slider::new(&mut r, 0..=(MAX_PAINT_RADIUS_CHUNKS as i32)).text(format!("Radius (chunks) — {0}×{0}", footprint))).changed() {
        paint.radius_chunks = r.max(0) as u32;
    }

    ui.separator();
    ui.small(format!("Override map: {} columns painted", overrides.by_xz.len()));
    ui.small("LMB to paint · Ctrl+S to save · paint is loaded automatically on next launch.");
}

fn draw_presets_panel(
    ui: &mut egui::Ui,
    brush: &mut VoxelBrushState,
    presets: &mut BrushPresets,
) {
    ui.label("Presets (Ctrl+N load · Ctrl+Shift+N save)");
    for (i, preset) in presets.slots.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.label(format!("{}", i + 1));
            ui.add_sized(
                [120.0, 20.0],
                egui::TextEdit::singleline(&mut preset.name).hint_text("name"),
            );
            if ui.button("Save").clicked() {
                let mut snap = brush.clone();
                snap.ramp_endpoint_a = None;
                preset.state = snap;
            }
            if ui.button("Load").clicked() {
                *brush = preset.state.clone();
            }
        });
    }
}
