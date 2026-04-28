//! Cartography overlay — visualises the YAML-authored map's labels in
//! 3D so authoring lines up with the parchment SVG view.
//!
//! Roads themselves are no longer rendered as separate ribbon meshes;
//! they're rasterised into the biome map at zone-load time
//! (`vaern-editor/src/voxel/overrides.rs::rasterize_active_zone_baseline`)
//! and rendered as part of the voxel ground itself, sharing the
//! same PBR pipeline as every other biome. This file is now
//! label-only:
//!
//! - Spawns invisible "label anchor" entities at every landmark and
//!   hub world position. A per-frame egui pass projects them to
//!   screen space and draws their name as floating text.
//!
//! All entities are tagged `EditorWorld` so they get cleaned up when
//! the active zone is torn down.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use vaern_core::terrain;
use vaern_data::{load_all_landmarks, load_world, load_world_layout};

use crate::state::EditorAppState;
use crate::state::EditorContext;
use crate::ui::console::ConsoleLog;
use crate::world::markers::EditorWorld;

/// World-anchored label — projected each frame and drawn via egui.
#[derive(Component, Debug, Clone)]
pub struct CartographyLabel {
    pub text: String,
    pub kind: LabelKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelKind {
    Hub,
    Landmark,
}

/// User-toggleable show flags. Inspector checkboxes flip these.
#[derive(Resource, Debug, Clone)]
pub struct CartographyOverlaySettings {
    pub show_landmarks: bool,
    pub show_hubs: bool,
}

impl Default for CartographyOverlaySettings {
    fn default() -> Self {
        Self {
            show_landmarks: true,
            show_hubs: true,
        }
    }
}

pub struct CartographyOverlayPlugin;

impl Plugin for CartographyOverlayPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CartographyOverlaySettings>()
            .add_systems(
                OnEnter(EditorAppState::Editing),
                spawn_cartography_labels.after(crate::world::load::load_active_zone),
            )
            .add_systems(Update, draw_cartography_labels);
    }
}

/// Spawn label anchor entities for every hub + landmark in the
/// active zone.
pub fn spawn_cartography_labels(
    mut commands: Commands,
    ctx: Res<EditorContext>,
    mut log: ResMut<ConsoleLog>,
) {
    let root = crate::persistence::zone_io::world_root();
    let world = match load_world(&root) {
        Ok(w) => w,
        Err(e) => {
            warn!("cartography overlay: load_world failed: {e}");
            return;
        }
    };
    let layout = load_world_layout(&root).unwrap_or_default();
    let landmarks = match load_all_landmarks(&root) {
        Ok(l) => l,
        Err(e) => {
            warn!("cartography overlay: load_all_landmarks failed: {e}");
            return;
        }
    };

    let zone_id = ctx.active_zone.clone();
    let Some(origin) = layout.zone_origin(&zone_id) else {
        log.push(format!(
            "cartography overlay: no world.yaml placement for zone {zone_id}"
        ));
        return;
    };
    let (ox, oz) = (origin.x, origin.z);
    let mut label_count = 0usize;

    for hub in world.hubs_in_zone(&zone_id) {
        let Some(off) = hub.offset_from_zone_origin.as_ref() else {
            continue;
        };
        let world_x = ox + off.x;
        let world_z = oz + off.z;
        let world_y = terrain::height(world_x, world_z) + 4.0;
        commands.spawn((
            Transform::from_xyz(world_x, world_y, world_z),
            CartographyLabel {
                text: hub.name.clone(),
                kind: LabelKind::Hub,
            },
            EditorWorld,
            Name::new(format!("CartographyLabel:{}", hub.id)),
        ));
        label_count += 1;
    }

    for lm in landmarks.iter_zone(&zone_id) {
        let world_x = ox + lm.offset_from_zone_origin.x;
        let world_z = oz + lm.offset_from_zone_origin.z;
        let world_y = terrain::height(world_x, world_z) + 2.5;
        commands.spawn((
            Transform::from_xyz(world_x, world_y, world_z),
            CartographyLabel {
                text: lm.name.clone(),
                kind: LabelKind::Landmark,
            },
            EditorWorld,
            Name::new(format!("CartographyLabel:{}", lm.id)),
        ));
        label_count += 1;
    }

    log.push(format!("cartography overlay: {label_count} labels"));
}

/// Project every label anchor to screen space and draw the text via
/// egui. Distance-culled at 1500 m so labels don't clutter the screen
/// when zoomed way out. Labels behind the camera are skipped.
fn draw_cartography_labels(
    mut egui: EguiContexts,
    settings: Res<CartographyOverlaySettings>,
    cams: Query<(&Camera, &GlobalTransform), With<crate::camera::FreeFlyCamera>>,
    labels: Query<(&Transform, &CartographyLabel)>,
) {
    if !settings.show_landmarks && !settings.show_hubs {
        return;
    }
    let Ok((cam, cam_tf)) = cams.single() else {
        return;
    };
    let cam_pos = cam_tf.translation();
    let Ok(ctx) = egui.ctx_mut() else {
        return;
    };

    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("cartography_overlay_labels"),
    ));

    const RANGE_SQ: f32 = 1500.0 * 1500.0;

    for (tf, label) in &labels {
        let visible_for_kind = match label.kind {
            LabelKind::Hub => settings.show_hubs,
            LabelKind::Landmark => settings.show_landmarks,
        };
        if !visible_for_kind {
            continue;
        }
        let world = tf.translation;
        if world.distance_squared(cam_pos) > RANGE_SQ {
            continue;
        }
        let Ok(screen) = cam.world_to_viewport(cam_tf, world) else {
            continue;
        };
        let pos = egui::pos2(screen.x, screen.y);
        let (font, fill, stroke) = match label.kind {
            LabelKind::Hub => (
                egui::FontId::proportional(18.0),
                egui::Color32::from_rgb(255, 230, 180),
                egui::Color32::from_rgb(60, 30, 10),
            ),
            LabelKind::Landmark => (
                egui::FontId::proportional(13.0),
                egui::Color32::from_rgb(220, 220, 235),
                egui::Color32::from_rgb(30, 30, 50),
            ),
        };
        for dx in [-1.0, 1.0] {
            for dy in [-1.0, 1.0] {
                painter.text(
                    pos + egui::vec2(dx, dy),
                    egui::Align2::CENTER_CENTER,
                    &label.text,
                    font.clone(),
                    stroke,
                );
            }
        }
        painter.text(pos, egui::Align2::CENTER_CENTER, &label.text, font, fill);
    }
}
