//! Cartography overlay — visualizes the YAML-authored map data
//! (roads, landmarks, hubs) in 3D so authoring lines up with the
//! parchment SVG view.
//!
//! Reads the same `geography.yaml::roads`, `landmarks.yaml`, and
//! `world.yaml`/hub YAMLs that the cartography crate renders to SVG,
//! and:
//!
//! - Spawns a flat ribbon mesh on the ground for every road segment.
//!   Width and color are per-type (kingsroad = wide, light brown;
//!   dirt path = narrow, dark brown). Y comes from `terrain::height`
//!   so the ribbon hugs the procedural baseline.
//! - Spawns invisible "label anchor" entities at every landmark and
//!   hub world position. A per-frame egui pass projects them to
//!   screen space and draws their name as floating text.
//!
//! All entities are tagged `EditorWorld` so they get cleaned up when
//! the active zone is torn down (per the world-lifecycle convention).
//!
//! ## Toggling
//!
//! [`CartographyOverlaySettings`] is a Resource with three booleans
//! (`show_roads`, `show_landmarks`, `show_hubs`). Inspector wires
//! checkboxes; defaults are all `true`.
//!
//! ## Persistence
//!
//! Pure visual overlay — no `voxel_edits.bin` writes. Edit the
//! cartography source YAML (or run the seed scripts) and reopen the
//! editor to see updates.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use vaern_core::terrain;
use vaern_data::{
    load_all_geography, load_all_landmarks, load_world, load_world_layout, Geography,
};

use crate::state::EditorAppState;
use crate::ui::console::ConsoleLog;
use crate::world::markers::EditorWorld;
use crate::state::EditorContext;

/// Marker on every road ribbon mesh entity.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct CartographyRoad;

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

/// User-toggleable show flags. Inspector checkboxes flip these; the
/// label-render and road-visibility systems read them.
#[derive(Resource, Debug, Clone)]
pub struct CartographyOverlaySettings {
    pub show_roads: bool,
    pub show_landmarks: bool,
    pub show_hubs: bool,
}

impl Default for CartographyOverlaySettings {
    fn default() -> Self {
        Self {
            show_roads: true,
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
                spawn_cartography_overlay
                    // run AFTER load::load_active_zone so zone_origin
                    // + active hubs are available
                    .after(crate::world::load::load_active_zone),
            )
            .add_systems(Update, (toggle_road_visibility, draw_cartography_labels));
    }
}

/// Per-road-type styling. Width is in world meters.
struct RoadStyle {
    width_m: f32,
    color: Color,
}

fn road_style(road_type: &str) -> RoadStyle {
    match road_type {
        "kingsroad" | "highway" => RoadStyle {
            width_m: 6.0,
            color: Color::srgb(0.78, 0.65, 0.42),
        },
        "track" | "trade_road" => RoadStyle {
            width_m: 4.0,
            color: Color::srgb(0.62, 0.50, 0.30),
        },
        "dirt_path" | "path" => RoadStyle {
            width_m: 2.5,
            color: Color::srgb(0.50, 0.40, 0.25),
        },
        _ => RoadStyle {
            width_m: 3.0,
            color: Color::srgb(0.55, 0.45, 0.28),
        },
    }
}

/// Run once on `OnEnter(Editing)` after `load_active_zone`. Loads the
/// cartography data for the active zone and spawns roads + label
/// anchors.
pub fn spawn_cartography_overlay(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
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
    let geography = match load_all_geography(&root) {
        Ok(g) => g,
        Err(e) => {
            warn!("cartography overlay: load_all_geography failed: {e}");
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

    let mut road_count = 0usize;
    let mut label_count = 0usize;

    if let Some(geo) = geography.get(&zone_id) {
        road_count = spawn_roads(
            &mut commands,
            &mut meshes,
            &mut materials,
            geo,
            (ox, oz),
        );
    }

    // Hub labels — at each hub's world XZ.
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

    // Landmark labels — at each landmark's world XZ.
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

    log.push(format!(
        "cartography overlay: {road_count} road meshes, {label_count} labels"
    ));
}

fn spawn_roads(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    geo: &Geography,
    origin: (f32, f32),
) -> usize {
    let (ox, oz) = origin;
    let mut count = 0usize;
    for road in &geo.roads {
        if road.path.points.len() < 2 {
            continue;
        }
        let style = road_style(&road.type_);
        // Convert zone-local to world XZ + sample terrain Y.
        let path_world: Vec<Vec3> = road
            .path
            .points
            .iter()
            .map(|p| {
                let wx = ox + p.x;
                let wz = oz + p.z;
                Vec3::new(wx, terrain::height(wx, wz) + 0.25, wz)
            })
            .collect();
        let mesh = build_road_ribbon_mesh(&path_world, style.width_m);
        let mesh_handle = meshes.add(mesh);
        let mat = materials.add(StandardMaterial {
            base_color: style.color,
            perceptual_roughness: 0.95,
            metallic: 0.0,
            ..default()
        });
        commands.spawn((
            Mesh3d(mesh_handle),
            MeshMaterial3d(mat),
            Transform::IDENTITY,
            CartographyRoad,
            EditorWorld,
            Name::new(format!("CartographyRoad:{}", road.id)),
        ));
        count += 1;
    }
    count
}

/// Build a flat ribbon mesh along the polyline. Each segment becomes a
/// quad whose width is perpendicular to the segment in the XZ plane.
/// Y values are baked from the input points (the path is already
/// terrain-Y-snapped by the caller). Vertices are world-space — the
/// road entity's transform is identity.
fn build_road_ribbon_mesh(path: &[Vec3], width_m: f32) -> Mesh {
    let n = path.len();
    let half_w = width_m * 0.5;
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(n * 2);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(n * 2);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(n * 2);
    let mut indices: Vec<u32> = Vec::with_capacity((n - 1) * 6);

    // Walk along the path, computing the perpendicular at each vertex
    // by averaging neighbor segment directions (smooth corners).
    for i in 0..n {
        let prev = if i == 0 { path[i] } else { path[i - 1] };
        let next = if i + 1 == n { path[i] } else { path[i + 1] };
        // Direction along path in XZ plane.
        let mut dir = Vec3::new(next.x - prev.x, 0.0, next.z - prev.z);
        if dir.length_squared() < 1e-6 {
            dir = Vec3::Z;
        } else {
            dir = dir.normalize();
        }
        // Perpendicular in XZ plane (rotate dir 90° around Y).
        let perp = Vec3::new(-dir.z, 0.0, dir.x);
        let left = path[i] + perp * half_w;
        let right = path[i] - perp * half_w;
        positions.push([left.x, left.y, left.z]);
        positions.push([right.x, right.y, right.z]);
        normals.push([0.0, 1.0, 0.0]);
        normals.push([0.0, 1.0, 0.0]);
        let v = i as f32 / (n.max(2) - 1) as f32;
        uvs.push([0.0, v]);
        uvs.push([1.0, v]);
    }

    for i in 0..(n - 1) {
        let a = (i * 2) as u32;
        let b = (i * 2 + 1) as u32;
        let c = ((i + 1) * 2) as u32;
        let d = ((i + 1) * 2 + 1) as u32;
        // Two triangles per segment, CCW from above.
        indices.push(a);
        indices.push(c);
        indices.push(b);
        indices.push(b);
        indices.push(c);
        indices.push(d);
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// Toggle road visibility per the inspector flag.
fn toggle_road_visibility(
    settings: Res<CartographyOverlaySettings>,
    mut q: Query<&mut Visibility, With<CartographyRoad>>,
) {
    if !settings.is_changed() {
        return;
    }
    let v = if settings.show_roads {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    for mut vis in &mut q {
        *vis = v;
    }
}

/// Project every label anchor to screen space and draw the text via
/// egui. Distance-culled at 1500m so labels don't clutter the screen
/// when zoomed way out. Labels behind the camera are skipped (the
/// projection returns Err).
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
        // Outline: 4 dark copies offset by 1px, then the bright text on top.
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
