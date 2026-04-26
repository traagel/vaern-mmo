//! Voxel-brush mode — landscaping toolkit + drag/falloff/mirror polish.
//!
//! Eight tools share one set of input plumbing:
//!
//! | tool     | trigger      | drag? |
//! |----------|--------------|-------|
//! | Sphere   | LMB          | yes   |
//! | Smooth   | LMB          | yes   |
//! | Flatten  | LMB          | yes   |
//! | Reset    | LMB          | yes   |
//! | Cylinder | LMB          | yes   |
//! | Box      | LMB          | yes   |
//! | Stamp    | LMB          | yes   |
//! | Ramp     | LMB×2 + ESC  | no    |

use bevy::input::mouse::MouseWheel;
use bevy::math::IVec3;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::EguiContexts;
use std::collections::HashMap;
use vaern_voxel::chunk::{ChunkCoord, ChunkStore, DirtyChunks, VoxelChunk};
use vaern_voxel::edit::{
    BrushMode, CylinderBrush, EditStroke, Falloff, FlattenBrush, RampBrush, ResetBrush,
    SmoothStroke, SphereBrush, StampBrush, StampShape,
};
use vaern_voxel::query::{raycast, RayHit};

use super::{active_mode_is, EditorMode};
use crate::camera::FreeFlyCamera;
use crate::state::EditorAppState;
use crate::ui::console::ConsoleLog;
use crate::voxel::store::EditorHeightfield;
use crate::voxel::undo::{VoxelUndoEntry, VoxelUndoLog};

const BRUSH_RAY_MAX_DIST: f32 = 500.0;
pub const DEFAULT_BRUSH_RADIUS: f32 = 4.0;
pub const MIN_BRUSH_RADIUS: f32 = 0.5;
pub const MAX_BRUSH_RADIUS: f32 = 32.0;

pub const SCROLL_RADIUS_FACTOR_DEFAULT: f32 = 0.10;
pub const SCROLL_RADIUS_FACTOR_FINE: f32 = 0.01;
pub const SCROLL_RADIUS_FACTOR_COARSE: f32 = 0.50;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BrushTool {
    #[default]
    Sphere,
    Smooth,
    Flatten,
    Ramp,
    Reset,
    Cylinder,
    Box,
    Stamp,
}

impl BrushTool {
    pub fn label(self) -> &'static str {
        match self {
            Self::Sphere => "Sphere",
            Self::Smooth => "Smooth",
            Self::Flatten => "Flatten",
            Self::Ramp => "Ramp",
            Self::Reset => "Reset",
            Self::Cylinder => "Cylinder",
            Self::Box => "Box",
            Self::Stamp => "Stamp",
        }
    }

    pub const ALL: [BrushTool; 8] = [
        Self::Sphere,
        Self::Smooth,
        Self::Flatten,
        Self::Ramp,
        Self::Reset,
        Self::Cylinder,
        Self::Box,
        Self::Stamp,
    ];

    pub fn supports_drag(self) -> bool {
        !matches!(self, Self::Ramp)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MirrorPlane {
    #[default]
    None,
    X,
    Z,
    Both,
}

impl MirrorPlane {
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::X => "X",
            Self::Z => "Z",
            Self::Both => "Both",
        }
    }
    pub const ALL: [MirrorPlane; 4] = [Self::None, Self::X, Self::Z, Self::Both];
}

pub fn mirror_points(plane: MirrorPlane, origin_x: f32, origin_z: f32, p: Vec3) -> Vec<Vec3> {
    match plane {
        MirrorPlane::None => Vec::new(),
        MirrorPlane::X => vec![Vec3::new(2.0 * origin_x - p.x, p.y, p.z)],
        MirrorPlane::Z => vec![Vec3::new(p.x, p.y, 2.0 * origin_z - p.z)],
        MirrorPlane::Both => vec![
            Vec3::new(2.0 * origin_x - p.x, p.y, p.z),
            Vec3::new(p.x, p.y, 2.0 * origin_z - p.z),
            Vec3::new(2.0 * origin_x - p.x, p.y, 2.0 * origin_z - p.z),
        ],
    }
}

#[derive(Resource, Debug, Clone)]
pub struct VoxelBrushState {
    pub radius: f32,
    pub tool: BrushTool,
    pub falloff: Falloff,
    pub subtract: bool,
    pub flatten_use_cursor_y: bool,
    pub flatten_target_y: f32,
    pub flatten_half_height: f32,
    pub smooth_strength: f32,
    pub smooth_iterations: u32,
    pub ramp_endpoint_a: Option<Vec3>,
    pub ramp_half_width: f32,
    pub ramp_half_height: f32,
    pub cylinder_subtract: bool,
    pub cylinder_half_height: f32,
    pub box_subtract: bool,
    pub box_half_extents: Vec3,
    pub stamp_shape: StampShape,
    pub stamp_mode: BrushMode,
    pub stamp_rotation_y_deg: f32,
    pub mirror: MirrorPlane,
    pub mirror_origin_x: f32,
    pub mirror_origin_z: f32,
    pub drag_spacing_factor: f32,
}

impl Default for VoxelBrushState {
    fn default() -> Self {
        Self {
            radius: DEFAULT_BRUSH_RADIUS,
            tool: BrushTool::default(),
            falloff: Falloff::Hard,
            subtract: true,
            flatten_use_cursor_y: true,
            flatten_target_y: 0.0,
            flatten_half_height: 8.0,
            smooth_strength: 0.5,
            smooth_iterations: 2,
            ramp_endpoint_a: None,
            ramp_half_width: 4.0,
            ramp_half_height: 8.0,
            cylinder_subtract: true,
            cylinder_half_height: 8.0,
            box_subtract: true,
            box_half_extents: Vec3::new(4.0, 2.0, 4.0),
            stamp_shape: StampShape::Crater,
            stamp_mode: BrushMode::Paint,
            stamp_rotation_y_deg: 0.0,
            mirror: MirrorPlane::None,
            mirror_origin_x: 0.0,
            mirror_origin_z: 0.0,
            drag_spacing_factor: 0.25,
        }
    }
}

#[derive(Resource, Default)]
pub struct DragState {
    pub active: Option<ActiveDrag>,
}

pub struct ActiveDrag {
    pub snapshot: HashMap<ChunkCoord, VoxelChunk>,
    pub last_stamp: Vec3,
    pub stamp_spacing: f32,
    pub tool_at_start: BrushTool,
    pub stamp_count: u32,
}

pub fn should_fire_next_stamp(last_stamp: Vec3, hit: Vec3, spacing: f32) -> bool {
    (hit - last_stamp).length_squared() >= spacing * spacing
}

#[derive(Clone, Debug)]
pub struct BrushPreset {
    pub name: String,
    pub state: VoxelBrushState,
}

#[derive(Resource, Debug)]
pub struct BrushPresets {
    pub slots: Vec<BrushPreset>,
}

impl Default for BrushPresets {
    fn default() -> Self {
        Self {
            slots: vec![
                BrushPreset {
                    name: "Carve fast".into(),
                    state: VoxelBrushState {
                        radius: 8.0,
                        tool: BrushTool::Sphere,
                        falloff: Falloff::Hard,
                        subtract: true,
                        ..Default::default()
                    },
                },
                BrushPreset {
                    name: "Sculpt fine".into(),
                    state: VoxelBrushState {
                        radius: 1.5,
                        tool: BrushTool::Sphere,
                        falloff: Falloff::Linear,
                        subtract: true,
                        ..Default::default()
                    },
                },
                BrushPreset {
                    name: "Smooth strong".into(),
                    state: VoxelBrushState {
                        radius: 12.0,
                        tool: BrushTool::Smooth,
                        falloff: Falloff::Smooth,
                        smooth_strength: 0.7,
                        smooth_iterations: 3,
                        ..Default::default()
                    },
                },
                BrushPreset {
                    name: "Flatten pad".into(),
                    state: VoxelBrushState {
                        radius: 10.0,
                        tool: BrushTool::Flatten,
                        falloff: Falloff::Smooth,
                        flatten_half_height: 8.0,
                        flatten_use_cursor_y: true,
                        ..Default::default()
                    },
                },
            ],
        }
    }
}

pub struct VoxelBrushModePlugin;

impl Plugin for VoxelBrushModePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VoxelBrushState>()
            .init_resource::<DragState>()
            .init_resource::<BrushPresets>()
            .add_systems(
                Update,
                (
                    apply_brush_drag,
                    apply_brush_on_click,
                    handle_preset_keybinds,
                    draw_brush_preview,
                    scroll_brush_radius,
                    handle_ramp_cancel,
                )
                    .run_if(in_state(EditorAppState::Editing))
                    .run_if(active_mode_is(EditorMode::VoxelBrush)),
            );
    }
}

#[allow(clippy::too_many_arguments)]
pub fn apply_brush_on_click(
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<FreeFlyCamera>>,
    mut store: ResMut<ChunkStore>,
    mut dirty: ResMut<DirtyChunks>,
    mut undo: ResMut<VoxelUndoLog>,
    mut state: ResMut<VoxelBrushState>,
    mut log: ResMut<ConsoleLog>,
    mut egui: EguiContexts,
) {
    if state.tool != BrushTool::Ramp {
        return;
    }
    if !mouse_buttons.just_pressed(MouseButton::Left) {
        return;
    }
    if egui_owns_pointer(&mut egui) {
        return;
    }
    let Some(ray) = cursor_world_ray(&windows, &cameras) else {
        return;
    };
    let Some(hit) = raycast(&store, ray.origin, *ray.direction, BRUSH_RAY_MAX_DIST) else {
        log.push("brush: no surface under cursor");
        return;
    };
    apply_ramp(hit, &mut state, &mut store, &mut dirty, &mut undo, &mut log);
}

#[allow(clippy::too_many_arguments)]
pub fn apply_brush_drag(
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<FreeFlyCamera>>,
    mut store: ResMut<ChunkStore>,
    mut dirty: ResMut<DirtyChunks>,
    mut undo: ResMut<VoxelUndoLog>,
    state: Res<VoxelBrushState>,
    mut drag: ResMut<DragState>,
    mut log: ResMut<ConsoleLog>,
    mut egui: EguiContexts,
) {
    if mouse_buttons.just_released(MouseButton::Left) {
        if let Some(active) = drag.active.take() {
            let stamp_count = active.stamp_count;
            let chunks: Vec<(ChunkCoord, VoxelChunk)> = active.snapshot.into_iter().collect();
            if !chunks.is_empty() {
                undo.record_stroke(VoxelUndoEntry::Snapshot { chunks: chunks.clone() });
                log.push(format!(
                    "drag: {} stamps over {} chunks",
                    stamp_count,
                    chunks.len()
                ));
            }
        }
        return;
    }

    if !state.tool.supports_drag() {
        return;
    }
    if !mouse_buttons.pressed(MouseButton::Left) {
        return;
    }
    if egui_owns_pointer(&mut egui) {
        return;
    }
    let Some(ray) = cursor_world_ray(&windows, &cameras) else {
        return;
    };
    let Some(hit) = raycast(&store, ray.origin, *ray.direction, BRUSH_RAY_MAX_DIST) else {
        return;
    };

    if drag.active.is_none() && mouse_buttons.just_pressed(MouseButton::Left) {
        let spacing = (state.radius * state.drag_spacing_factor).max(0.05);
        drag.active = Some(ActiveDrag {
            snapshot: HashMap::new(),
            last_stamp: hit.position + Vec3::splat(spacing * 100.0),
            stamp_spacing: spacing,
            tool_at_start: state.tool,
            stamp_count: 0,
        });
    }

    let Some(active) = drag.active.as_mut() else {
        return;
    };

    if !should_fire_next_stamp(active.last_stamp, hit.position, active.stamp_spacing) {
        return;
    }

    let primary = hit.position;
    let mirrors = mirror_points(state.mirror, state.mirror_origin_x, state.mirror_origin_z, primary);
    let mut all_points = vec![primary];
    all_points.extend(mirrors);

    for p in &all_points {
        apply_stamp_for_tool(
            active.tool_at_start,
            *p,
            &keys,
            &state,
            &mut store,
            &mut dirty,
            &mut active.snapshot,
        );
    }

    active.last_stamp = primary;
    active.stamp_count += 1;
}

fn apply_stamp_for_tool(
    tool: BrushTool,
    p: Vec3,
    keys: &ButtonInput<KeyCode>,
    state: &VoxelBrushState,
    store: &mut ChunkStore,
    dirty: &mut DirtyChunks,
    snapshot: &mut HashMap<ChunkCoord, VoxelChunk>,
) {
    let (lo, hi) = brush_aabb_for_tool(tool, p, state);
    capture_chunks_for_drag(store, snapshot, lo, hi);
    match tool {
        BrushTool::Sphere => {
            let inverted =
                keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
            let subtract = state.subtract ^ inverted;
            let mode = if subtract {
                BrushMode::Subtract
            } else {
                BrushMode::Union
            };
            let brush = SphereBrush {
                center: p,
                radius: state.radius,
                mode,
                falloff: state.falloff,
            };
            EditStroke::new(brush, store, dirty).apply();
        }
        BrushTool::Smooth => {
            SmoothStroke::new(
                p,
                state.radius,
                state.smooth_strength,
                state.smooth_iterations,
                state.falloff,
                store,
                dirty,
            )
            .apply();
        }
        BrushTool::Flatten => {
            let target_y = if state.flatten_use_cursor_y {
                p.y
            } else {
                state.flatten_target_y
            };
            let center = Vec3::new(p.x, target_y, p.z);
            let brush = FlattenBrush {
                center,
                radius: state.radius,
                half_height: state.flatten_half_height,
                falloff: state.falloff,
            };
            EditStroke::new(brush, store, dirty).apply();
        }
        BrushTool::Reset => {
            let generator = EditorHeightfield;
            let brush = ResetBrush {
                center: p,
                radius: state.radius,
                generator: &generator,
                falloff: state.falloff,
            };
            EditStroke::new(brush, store, dirty).apply();
        }
        BrushTool::Cylinder => {
            let inverted =
                keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
            let subtract = state.cylinder_subtract ^ inverted;
            let mode = if subtract {
                BrushMode::Subtract
            } else {
                BrushMode::Union
            };
            let brush = CylinderBrush {
                center: p,
                radius: state.radius,
                half_height: state.cylinder_half_height,
                mode,
                falloff: state.falloff,
            };
            EditStroke::new(brush, store, dirty).apply();
        }
        BrushTool::Box => {
            let inverted =
                keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
            let subtract = state.box_subtract ^ inverted;
            let mode = if subtract {
                BrushMode::Subtract
            } else {
                BrushMode::Union
            };
            let brush = vaern_voxel::edit::BoxBrush {
                center: p,
                half_extents: state.box_half_extents,
                mode,
                falloff: state.falloff,
            };
            EditStroke::new(brush, store, dirty).apply();
        }
        BrushTool::Stamp => {
            let brush = StampBrush {
                center: p,
                radius: state.radius,
                rotation_y_rad: state.stamp_rotation_y_deg.to_radians(),
                shape: state.stamp_shape,
                mode: state.stamp_mode,
                falloff: state.falloff,
            };
            EditStroke::new(brush, store, dirty).apply();
        }
        BrushTool::Ramp => {}
    }
}

fn brush_aabb_for_tool(tool: BrushTool, p: Vec3, state: &VoxelBrushState) -> (Vec3, Vec3) {
    match tool {
        BrushTool::Sphere | BrushTool::Smooth | BrushTool::Reset | BrushTool::Stamp => {
            let r = Vec3::splat(state.radius);
            (p - r, p + r)
        }
        BrushTool::Flatten => {
            let target_y = if state.flatten_use_cursor_y {
                p.y
            } else {
                state.flatten_target_y
            };
            let r = state.radius;
            let h = state.flatten_half_height;
            (
                Vec3::new(p.x - r, target_y - h, p.z - r),
                Vec3::new(p.x + r, target_y + h, p.z + r),
            )
        }
        BrushTool::Cylinder => {
            let r = state.radius;
            let h = state.cylinder_half_height;
            (Vec3::new(p.x - r, p.y - h, p.z - r), Vec3::new(p.x + r, p.y + h, p.z + r))
        }
        BrushTool::Box => (p - state.box_half_extents, p + state.box_half_extents),
        BrushTool::Ramp => (p, p),
    }
}

fn apply_ramp(
    hit: RayHit,
    state: &mut VoxelBrushState,
    store: &mut ChunkStore,
    dirty: &mut DirtyChunks,
    undo: &mut VoxelUndoLog,
    log: &mut ConsoleLog,
) {
    match decide_ramp_action(state.ramp_endpoint_a, hit.position) {
        RampAction::StoreA(a) => {
            state.ramp_endpoint_a = Some(a);
            log.push(format!(
                "ramp: A set ({:.1}, {:.1}, {:.1}); click B (or ESC to cancel)",
                a.x, a.y, a.z
            ));
        }
        RampAction::BuildBrush { a, b } => {
            let primaries = std::iter::once((a, b))
                .chain(
                    mirror_points(state.mirror, state.mirror_origin_x, state.mirror_origin_z, a)
                        .into_iter()
                        .zip(mirror_points(
                            state.mirror,
                            state.mirror_origin_x,
                            state.mirror_origin_z,
                            b,
                        ))
                        .map(|(ma, mb)| (ma, mb)),
                )
                .collect::<Vec<_>>();
            let mut snapshot: HashMap<ChunkCoord, VoxelChunk> = HashMap::new();
            for (ra, rb) in &primaries {
                let brush = RampBrush {
                    a: *ra,
                    b: *rb,
                    half_width: state.ramp_half_width,
                    half_height: state.ramp_half_height,
                    falloff: state.falloff,
                };
                let (lo, hi) = aabb_of_ramp(*ra, *rb, state.ramp_half_width, state.ramp_half_height);
                capture_chunks_for_drag(store, &mut snapshot, lo, hi);
                EditStroke::new(brush, store, dirty).apply();
            }
            let chunks: Vec<_> = snapshot.into_iter().collect();
            if !chunks.is_empty() {
                undo.record_stroke(VoxelUndoEntry::Snapshot { chunks });
            }
            state.ramp_endpoint_a = None;
            log.push(format!(
                "ramp: built ({:.1},{:.1},{:.1}) → ({:.1},{:.1},{:.1}); {} ramps",
                a.x,
                a.y,
                a.z,
                b.x,
                b.y,
                b.z,
                primaries.len(),
            ));
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum RampAction {
    StoreA(Vec3),
    BuildBrush { a: Vec3, b: Vec3 },
}

pub fn decide_ramp_action(prev_a: Option<Vec3>, hit: Vec3) -> RampAction {
    match prev_a {
        None => RampAction::StoreA(hit),
        Some(a) => RampAction::BuildBrush { a, b: hit },
    }
}

fn aabb_of_ramp(a: Vec3, b: Vec3, half_width: f32, half_height: f32) -> (Vec3, Vec3) {
    let lo = Vec3::new(
        a.x.min(b.x) - half_width,
        a.y.min(b.y) - half_height,
        a.z.min(b.z) - half_width,
    );
    let hi = Vec3::new(
        a.x.max(b.x) + half_width,
        a.y.max(b.y) + half_height,
        a.z.max(b.z) + half_width,
    );
    (lo, hi)
}

pub fn handle_ramp_cancel(
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<VoxelBrushState>,
    mut log: ResMut<ConsoleLog>,
) {
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    if state.tool != BrushTool::Ramp {
        return;
    }
    if state.ramp_endpoint_a.take().is_some() {
        log.push("ramp: cancelled");
    }
}

pub fn handle_preset_keybinds(
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<VoxelBrushState>,
    mut presets: ResMut<BrushPresets>,
    mut log: ResMut<ConsoleLog>,
) {
    if !(keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight)) {
        return;
    }
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let digits = [
        (KeyCode::Digit1, 0),
        (KeyCode::Digit2, 1),
        (KeyCode::Digit3, 2),
        (KeyCode::Digit4, 3),
    ];
    for (key, slot) in digits {
        if !keys.just_pressed(key) {
            continue;
        }
        if slot >= presets.slots.len() {
            continue;
        }
        if shift {
            let mut snapshot = state.clone();
            snapshot.ramp_endpoint_a = None;
            presets.slots[slot].state = snapshot;
            log.push(format!("preset: saved slot {}", slot + 1));
        } else {
            *state = presets.slots[slot].state.clone();
            log.push(format!(
                "preset: loaded slot {} ({})",
                slot + 1,
                presets.slots[slot].name
            ));
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_brush_preview(
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<FreeFlyCamera>>,
    store: Res<ChunkStore>,
    state: Res<VoxelBrushState>,
    mut egui: EguiContexts,
    mut gizmos: Gizmos,
) {
    if egui_owns_pointer(&mut egui) {
        return;
    }
    let Some(ray) = cursor_world_ray(&windows, &cameras) else {
        return;
    };
    let Some(hit) = raycast(&store, ray.origin, *ray.direction, BRUSH_RAY_MAX_DIST) else {
        return;
    };

    let primary = hit.position;
    let mut points = vec![primary];
    points.extend(mirror_points(
        state.mirror,
        state.mirror_origin_x,
        state.mirror_origin_z,
        primary,
    ));

    for p in &points {
        draw_tool_preview(&keys, &state, *p, &mut gizmos);
    }

    if state.mirror != MirrorPlane::None {
        let color = Color::srgba(0.7, 0.5, 0.9, 0.6);
        let span = 200.0;
        let y = primary.y + 0.1;
        if matches!(state.mirror, MirrorPlane::X | MirrorPlane::Both) {
            let x = state.mirror_origin_x;
            gizmos.line(Vec3::new(x, y, -span), Vec3::new(x, y, span), color);
        }
        if matches!(state.mirror, MirrorPlane::Z | MirrorPlane::Both) {
            let z = state.mirror_origin_z;
            gizmos.line(Vec3::new(-span, y, z), Vec3::new(span, y, z), color);
        }
    }
}

fn draw_tool_preview(
    keys: &ButtonInput<KeyCode>,
    state: &VoxelBrushState,
    hit: Vec3,
    gizmos: &mut Gizmos,
) {
    match state.tool {
        BrushTool::Sphere => {
            let inverted =
                keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
            let subtract = state.subtract ^ inverted;
            let color = if subtract {
                Color::srgb(0.4, 0.8, 1.0)
            } else {
                Color::srgb(1.0, 0.7, 0.3)
            };
            gizmos.sphere(hit, state.radius, color);
        }
        BrushTool::Smooth => {
            gizmos.sphere(hit, state.radius, Color::srgb(0.6, 0.85, 0.95));
        }
        BrushTool::Flatten => {
            let target_y = if state.flatten_use_cursor_y {
                hit.y
            } else {
                state.flatten_target_y
            };
            let disc_center = Vec3::new(hit.x, target_y, hit.z);
            draw_disc_ring(gizmos, disc_center, state.radius, Color::srgb(0.4, 0.8, 1.0));
            gizmos.line(disc_center, hit, Color::srgb(0.6, 0.6, 0.6));
        }
        BrushTool::Ramp => match state.ramp_endpoint_a {
            None => {
                gizmos.sphere(hit, 0.4, Color::srgb(0.4, 0.9, 0.5));
            }
            Some(a) => {
                gizmos.sphere(a, 0.5, Color::srgb(0.4, 0.9, 0.5));
                gizmos.line(a, hit, Color::srgb(0.4, 0.9, 0.5));
                draw_ramp_sweep_outline(
                    gizmos,
                    a,
                    hit,
                    state.ramp_half_width,
                    state.ramp_half_height,
                    Color::srgb(0.3, 0.7, 0.4),
                );
            }
        },
        BrushTool::Reset => {
            gizmos.sphere(hit, state.radius, Color::srgb(0.4, 0.9, 0.5));
        }
        BrushTool::Cylinder => {
            let inverted =
                keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
            let subtract = state.cylinder_subtract ^ inverted;
            let color = if subtract {
                Color::srgb(0.4, 0.8, 1.0)
            } else {
                Color::srgb(1.0, 0.7, 0.3)
            };
            let top = Vec3::new(hit.x, hit.y + state.cylinder_half_height, hit.z);
            let bot = Vec3::new(hit.x, hit.y - state.cylinder_half_height, hit.z);
            draw_disc_ring(gizmos, top, state.radius, color);
            draw_disc_ring(gizmos, bot, state.radius, color);
            for i in 0..4 {
                let theta = (i as f32) / 4.0 * std::f32::consts::TAU;
                let dx = theta.cos() * state.radius;
                let dz = theta.sin() * state.radius;
                gizmos.line(
                    Vec3::new(top.x + dx, top.y, top.z + dz),
                    Vec3::new(bot.x + dx, bot.y, bot.z + dz),
                    color,
                );
            }
        }
        BrushTool::Box => {
            let inverted =
                keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
            let subtract = state.box_subtract ^ inverted;
            let color = if subtract {
                Color::srgb(0.4, 0.8, 1.0)
            } else {
                Color::srgb(1.0, 0.7, 0.3)
            };
            let size = state.box_half_extents * 2.0;
            gizmos.cube(Transform::from_translation(hit).with_scale(size), color);
        }
        BrushTool::Stamp => {
            gizmos.sphere(hit, state.radius, Color::srgb(0.9, 0.8, 0.4));
        }
    }
}

pub fn pick_scroll_factor(shift: bool, ctrl: bool) -> f32 {
    if shift {
        SCROLL_RADIUS_FACTOR_FINE
    } else if ctrl {
        SCROLL_RADIUS_FACTOR_COARSE
    } else {
        SCROLL_RADIUS_FACTOR_DEFAULT
    }
}

pub fn scroll_brush_radius(
    mut wheel: MessageReader<MouseWheel>,
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<VoxelBrushState>,
    mut log: ResMut<ConsoleLog>,
    mut egui: EguiContexts,
) {
    if egui_owns_pointer(&mut egui) {
        wheel.clear();
        return;
    }
    let mut ticks = 0.0_f32;
    for ev in wheel.read() {
        ticks += ev.y;
    }
    if ticks == 0.0 {
        return;
    }

    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    let factor_per_tick = pick_scroll_factor(shift, ctrl);
    let factor = (1.0 + factor_per_tick).powf(ticks);
    let new_radius = (state.radius * factor).clamp(MIN_BRUSH_RADIUS, MAX_BRUSH_RADIUS);
    if (new_radius - state.radius).abs() > f32::EPSILON {
        state.radius = new_radius;
        log.push(format!("brush radius: {:.2}u", state.radius));
    }
}

fn egui_owns_pointer(egui: &mut EguiContexts) -> bool {
    egui.ctx_mut()
        .map(|c| c.is_pointer_over_area() || c.wants_pointer_input())
        .unwrap_or(false)
}

fn cursor_world_ray(
    windows: &Query<&Window, With<PrimaryWindow>>,
    cameras: &Query<(&Camera, &GlobalTransform), With<FreeFlyCamera>>,
) -> Option<Ray3d> {
    let window = windows.single().ok()?;
    let cursor = window.cursor_position()?;
    let (cam, cam_xform) = cameras.single().ok()?;
    cam.viewport_to_world(cam_xform, cursor).ok()
}

fn capture_chunks_for_drag(
    store: &ChunkStore,
    snapshot: &mut HashMap<ChunkCoord, VoxelChunk>,
    lo: Vec3,
    hi: Vec3,
) {
    for coord in chunks_intersecting_aabb(lo, hi) {
        if snapshot.contains_key(&coord) {
            continue;
        }
        if let Some(chunk) = store.get(coord) {
            snapshot.insert(coord, chunk.clone());
        }
    }
}

fn chunks_intersecting_aabb(lo: Vec3, hi: Vec3) -> impl Iterator<Item = ChunkCoord> {
    let lo_c = ChunkCoord::containing(lo).0;
    let hi_c = ChunkCoord::containing(hi).0;
    let lo_xyz = IVec3::new(lo_c.x.min(hi_c.x), lo_c.y.min(hi_c.y), lo_c.z.min(hi_c.z));
    let hi_xyz = IVec3::new(lo_c.x.max(hi_c.x), lo_c.y.max(hi_c.y), lo_c.z.max(hi_c.z));
    (lo_xyz.x..=hi_xyz.x).flat_map(move |x| {
        (lo_xyz.y..=hi_xyz.y)
            .flat_map(move |y| (lo_xyz.z..=hi_xyz.z).map(move |z| ChunkCoord::new(x, y, z)))
    })
}

fn draw_disc_ring(gizmos: &mut Gizmos, center: Vec3, radius: f32, color: Color) {
    const N: usize = 24;
    let two_pi = std::f32::consts::TAU;
    let mut prev = center + Vec3::new(radius, 0.0, 0.0);
    for i in 1..=N {
        let t = (i as f32) / (N as f32) * two_pi;
        let next = center + Vec3::new(t.cos() * radius, 0.0, t.sin() * radius);
        gizmos.line(prev, next, color);
        prev = next;
    }
}

fn draw_ramp_sweep_outline(
    gizmos: &mut Gizmos,
    a: Vec3,
    b: Vec3,
    half_width: f32,
    half_height: f32,
    color: Color,
) {
    let v_xz = Vec3::new(b.x - a.x, 0.0, b.z - a.z);
    let len = v_xz.length();
    if len < 1e-3 {
        return;
    }
    let perp = Vec3::new(-v_xz.z, 0.0, v_xz.x).normalize() * half_width;
    let up = Vec3::new(0.0, half_height, 0.0);

    let a_lb = a - perp - up;
    let a_lt = a - perp + up;
    let a_rb = a + perp - up;
    let a_rt = a + perp + up;
    let b_lb = b - perp - up;
    let b_lt = b - perp + up;
    let b_rb = b + perp - up;
    let b_rt = b + perp + up;

    gizmos.line(a_lb, a_rb, color);
    gizmos.line(a_rb, b_rb, color);
    gizmos.line(b_rb, b_lb, color);
    gizmos.line(b_lb, a_lb, color);
    gizmos.line(a_lt, a_rt, color);
    gizmos.line(a_rt, b_rt, color);
    gizmos.line(b_rt, b_lt, color);
    gizmos.line(b_lt, a_lt, color);
    gizmos.line(a_lb, a_lt, color);
    gizmos.line(a_rb, a_rt, color);
    gizmos.line(b_lb, b_lt, color);
    gizmos.line(b_rb, b_rt, color);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brush_tool_default_is_sphere() {
        assert_eq!(BrushTool::default(), BrushTool::Sphere);
    }

    #[test]
    fn brush_tool_all_lists_each_variant_once() {
        assert_eq!(BrushTool::ALL.len(), 8);
        assert!(BrushTool::ALL.contains(&BrushTool::Cylinder));
        assert!(BrushTool::ALL.contains(&BrushTool::Box));
        assert!(BrushTool::ALL.contains(&BrushTool::Stamp));
    }

    #[test]
    fn ramp_alone_does_not_support_drag() {
        for t in BrushTool::ALL {
            let expected = !matches!(t, BrushTool::Ramp);
            assert_eq!(t.supports_drag(), expected, "tool {:?}", t);
        }
    }

    #[test]
    fn decide_ramp_action_idle_stores_a() {
        let action = decide_ramp_action(None, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(action, RampAction::StoreA(Vec3::new(1.0, 2.0, 3.0)));
    }

    #[test]
    fn decide_ramp_action_with_a_returns_brush() {
        let a = Vec3::new(1.0, 0.0, 0.0);
        let b = Vec3::new(10.0, 5.0, 0.0);
        assert_eq!(
            decide_ramp_action(Some(a), b),
            RampAction::BuildBrush { a, b }
        );
    }

    #[test]
    fn chunks_intersecting_aabb_includes_origin_chunk() {
        let coords: Vec<_> =
            chunks_intersecting_aabb(Vec3::splat(-1.0), Vec3::splat(1.0)).collect();
        assert!(coords.contains(&ChunkCoord::new(0, 0, 0)));
    }

    #[test]
    fn chunks_intersecting_aabb_spans_negative_and_positive() {
        let coords: Vec<_> =
            chunks_intersecting_aabb(Vec3::splat(-4.0), Vec3::splat(4.0)).collect();
        assert!(coords.contains(&ChunkCoord::new(0, 0, 0)));
        assert!(coords.contains(&ChunkCoord::new(-1, -1, -1)));
    }

    #[test]
    fn drag_stamp_only_fires_after_spacing_traveled() {
        let last = Vec3::ZERO;
        let close = Vec3::new(0.5, 0.0, 0.0);
        let far = Vec3::new(2.0, 0.0, 0.0);
        assert!(!should_fire_next_stamp(last, close, 1.0));
        assert!(should_fire_next_stamp(last, far, 1.0));
    }

    #[test]
    fn mirror_plane_x_reflects_x_coord() {
        let p = Vec3::new(5.0, 1.0, 2.0);
        let mirrors = mirror_points(MirrorPlane::X, 0.0, 0.0, p);
        assert_eq!(mirrors.len(), 1);
        assert_eq!(mirrors[0], Vec3::new(-5.0, 1.0, 2.0));
    }

    #[test]
    fn mirror_plane_z_reflects_z_coord() {
        let p = Vec3::new(5.0, 1.0, 2.0);
        let mirrors = mirror_points(MirrorPlane::Z, 0.0, 0.0, p);
        assert_eq!(mirrors[0], Vec3::new(5.0, 1.0, -2.0));
    }

    #[test]
    fn mirror_plane_both_yields_three_extra_points() {
        let p = Vec3::new(5.0, 1.0, 2.0);
        let mirrors = mirror_points(MirrorPlane::Both, 0.0, 0.0, p);
        assert_eq!(mirrors.len(), 3);
        assert!(mirrors.contains(&Vec3::new(-5.0, 1.0, 2.0)));
        assert!(mirrors.contains(&Vec3::new(5.0, 1.0, -2.0)));
        assert!(mirrors.contains(&Vec3::new(-5.0, 1.0, -2.0)));
    }

    #[test]
    fn mirror_plane_with_offset_origin() {
        let p = Vec3::new(5.0, 0.0, 0.0);
        let mirrors = mirror_points(MirrorPlane::X, 10.0, 0.0, p);
        assert_eq!(mirrors[0].x, 15.0);
    }

    #[test]
    fn preset_save_load_round_trips_radius() {
        let mut presets = BrushPresets::default();
        let mut state = VoxelBrushState::default();
        state.radius = 12.0;
        let mut snap = state.clone();
        snap.ramp_endpoint_a = None;
        presets.slots[0].state = snap;
        state.radius = 4.0;
        assert!((state.radius - 4.0).abs() < 1e-6, "mutation took");
        state = presets.slots[0].state.clone();
        assert!((state.radius - 12.0).abs() < 1e-6, "load round-trips");
    }

    #[test]
    fn default_presets_seeds_four_slots() {
        let presets = BrushPresets::default();
        assert_eq!(presets.slots.len(), 4);
        assert_eq!(presets.slots[0].name, "Carve fast");
        assert_eq!(presets.slots[3].state.tool, BrushTool::Flatten);
    }

    #[test]
    fn scroll_modifier_default_is_ten_percent() {
        assert!((pick_scroll_factor(false, false) - 0.10).abs() < 1e-6);
    }

    #[test]
    fn scroll_modifier_shift_uses_fine_factor() {
        assert!((pick_scroll_factor(true, false) - 0.01).abs() < 1e-6);
    }

    #[test]
    fn scroll_modifier_ctrl_uses_coarse_factor() {
        assert!((pick_scroll_factor(false, true) - 0.50).abs() < 1e-6);
    }

    #[test]
    fn scroll_modifier_shift_wins_over_ctrl() {
        assert!((pick_scroll_factor(true, true) - 0.01).abs() < 1e-6);
    }
}
