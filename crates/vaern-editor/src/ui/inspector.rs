//! Right side panel — inspector for the currently-selected entity or
//! mode-specific tunables.
//!
//! When a dressing prop is selected, the inspector shows editable
//! drag-values for its authored offset / rotation / scale + a Delete
//! button. Mutations write through to BOTH the live `Transform` and
//! the `EditorDressingEntity.authored` mirror so save round-trips.
//!
//! When no prop is selected and Voxel Brush mode is active, the
//! inspector renders the brush radius + carve/raise toggle.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use vaern_core::terrain;

use crate::dressing::selection::SelectedProp;
use crate::dressing::EditorDressingEntity;
use crate::modes::{voxel_brush::VoxelBrushState, ActiveMode, EditorMode};
use crate::state::EditorContext;
use crate::ui::console::ConsoleLog;
use crate::world::ActiveZoneHubs;

#[allow(clippy::too_many_arguments)]
pub fn draw_inspector(
    mut commands: Commands,
    mut egui: EguiContexts,
    mut selected: ResMut<SelectedProp>,
    active: Res<ActiveMode>,
    ctx: Res<EditorContext>,
    mut brush: ResMut<VoxelBrushState>,
    hubs: Res<ActiveZoneHubs>,
    mut props: Query<(&mut Transform, &mut EditorDressingEntity)>,
    keys: Res<ButtonInput<KeyCode>>,
    mut log: ResMut<ConsoleLog>,
) {
    let Ok(egui_ctx) = egui.ctx_mut() else {
        return;
    };

    // Delete keyboard shortcut. Same effect as the Delete button in
    // the inspector — works regardless of whether the inspector is
    // rendered (which it always is, but the keybind feels natural).
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
                        // Stale entity (despawned this frame) — fall through.
                        ui.label("(selection cleared)");
                    }
                }
                (None, EditorMode::VoxelBrush) => draw_brush_inspector(ui, &mut brush),
                (None, mode) => {
                    ui.label(format!(
                        "{} mode — select a prop to edit it",
                        mode.label()
                    ));
                }
            }
        });
}

/// Render the per-prop edit panel for a selected dressing entity.
/// Returns `false` if the entity is no longer present (caller falls
/// back to a placeholder line).
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

    // Offset (hub-local). DragValues advance per pixel; speed = 0.1u
    // for fine control.
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
        if ui
            .add(egui::DragValue::new(&mut offset_x).speed(0.1).suffix("u"))
            .changed()
        {
            changed = true;
        }
        ui.label("Z");
        if ui
            .add(egui::DragValue::new(&mut offset_z).speed(0.1).suffix("u"))
            .changed()
        {
            changed = true;
        }
    });

    ui.horizontal(|ui| {
        ui.label("Rotation Y");
        if ui
            .add(
                egui::DragValue::new(&mut rot)
                    .speed(1.0)
                    .range(-360.0..=360.0)
                    .suffix("°"),
            )
            .changed()
        {
            changed = true;
        }
    });

    ui.horizontal(|ui| {
        ui.label("Scale");
        if ui
            .add(
                egui::DragValue::new(&mut scale)
                    .speed(0.05)
                    .range(0.05..=20.0),
            )
            .changed()
        {
            changed = true;
        }
    });

    ui.horizontal(|ui| {
        if ui.checkbox(&mut use_abs, "Override Y").changed() {
            changed = true;
        }
        ui.add_enabled_ui(use_abs, |ui| {
            if ui
                .add(egui::DragValue::new(&mut abs_y).speed(0.1).suffix("u"))
                .changed()
            {
                changed = true;
            }
        });
    });

    ui.separator();
    let delete_clicked = ui.button("Delete").clicked();

    if changed {
        // Write back to authored mirror.
        dressing.authored.offset.x = offset_x;
        dressing.authored.offset.z = offset_z;
        dressing.authored.rotation_y_deg = rot;
        dressing.authored.scale = scale.max(0.01);
        dressing.authored.absolute_y = if use_abs { Some(abs_y) } else { None };

        // Recompute world transform if hub origin is known. Fall back
        // to keeping current world position unchanged otherwise.
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

fn draw_brush_inspector(ui: &mut egui::Ui, brush: &mut VoxelBrushState) {
    ui.label("Voxel brush settings");
    let mut radius = brush.radius;
    let response = ui.add(
        egui::Slider::new(
            &mut radius,
            crate::modes::voxel_brush::MIN_BRUSH_RADIUS
                ..=crate::modes::voxel_brush::MAX_BRUSH_RADIUS,
        )
        .text("Radius (u)"),
    );
    if response.changed() {
        brush.radius = radius;
    }

    ui.checkbox(&mut brush.subtract, "Subtract (carve)");
    if !brush.subtract {
        ui.label("Mode: Add (raise mound)");
    }
    ui.separator();
    ui.small("Click ground to apply. Hold Shift to invert.");
}
