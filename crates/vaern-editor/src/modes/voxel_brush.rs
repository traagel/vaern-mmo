//! Voxel-brush mode — sphere add / subtract at the cursor.
//!
//! Wired in V2: LMB at cursor → screen-ray → voxel raycast → sphere
//! brush at the hit point → `EditStroke::apply` → dirty chunks remesh
//! next frame.
//!
//! UX:
//!
//! | input        | effect                                   |
//! |--------------|------------------------------------------|
//! | LMB click    | apply current brush mode (subtract/add)  |
//! | Shift + LMB  | invert current mode for this stroke      |
//! | Inspector    | radius slider + Subtract/Add toggle      |
//!
//! Not wired yet: undo recording, brush-preview gizmo, scroll-wheel
//! radius (scroll still adjusts camera speed). Those land alongside
//! the next pass on the mode.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::EguiContexts;
use vaern_voxel::chunk::{ChunkStore, DirtyChunks};
use vaern_voxel::edit::{BrushMode, EditStroke, SphereBrush};
use vaern_voxel::query::raycast;

use super::{active_mode_is, EditorMode};
use crate::camera::FreeFlyCamera;
use crate::state::EditorAppState;
use crate::ui::console::ConsoleLog;

/// Maximum world-space distance the cursor ray walks before giving up.
/// 500u covers the streamed chunk footprint + slack for steep look-down.
const BRUSH_RAY_MAX_DIST: f32 = 500.0;

/// Default sphere radius in world units when this mode activates.
pub const DEFAULT_BRUSH_RADIUS: f32 = 4.0;
/// Min / max brush radius — clamps once scroll-wheel control lands.
pub const MIN_BRUSH_RADIUS: f32 = 0.5;
pub const MAX_BRUSH_RADIUS: f32 = 32.0;

/// Per-frame brush state. Persists across frames so a held cursor
/// stroke produces a continuous carve.
#[derive(Resource, Debug, Clone, Copy)]
pub struct VoxelBrushState {
    pub radius: f32,
    pub subtract: bool,
}

impl Default for VoxelBrushState {
    fn default() -> Self {
        Self {
            radius: DEFAULT_BRUSH_RADIUS,
            subtract: true,
        }
    }
}

pub struct VoxelBrushModePlugin;

impl Plugin for VoxelBrushModePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VoxelBrushState>().add_systems(
            Update,
            apply_brush_on_click
                .run_if(in_state(EditorAppState::Editing))
                .run_if(active_mode_is(EditorMode::VoxelBrush)),
        );
    }
}

/// Cursor + LMB → SphereBrush at the voxel-raycast hit point.
///
/// Sequence:
///
/// 1. Skip if egui has the pointer (so clicking the Save button or a
///    palette row doesn't carve the world behind it).
/// 2. Project the cursor through the camera into a world ray.
/// 3. March the ray against the live `ChunkStore` until it crosses the
///    SDF surface.
/// 4. Resolve the brush mode: state.subtract toggles base direction;
///    LShift inverts for the current stroke.
/// 5. Apply via `EditStroke::new(brush, store, dirty).apply()` — that
///    walks the brush AABB, blends each affected sample, marks dirty
///    chunks. The voxel mesh plugin re-extracts surfaces in PostUpdate.
#[allow(clippy::too_many_arguments)]
pub fn apply_brush_on_click(
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<FreeFlyCamera>>,
    mut store: ResMut<ChunkStore>,
    mut dirty: ResMut<DirtyChunks>,
    state: Res<VoxelBrushState>,
    mut log: ResMut<ConsoleLog>,
    mut egui: EguiContexts,
) {
    if !mouse_buttons.just_pressed(MouseButton::Left) {
        return;
    }
    let egui_owns = egui
        .ctx_mut()
        .map(|c| c.is_pointer_over_area() || c.wants_pointer_input())
        .unwrap_or(false);
    if egui_owns {
        return;
    }

    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let Ok((cam, cam_xform)) = cameras.single() else {
        return;
    };
    let ray = match cam.viewport_to_world(cam_xform, cursor) {
        Ok(r) => r,
        Err(_) => return,
    };

    let Some(hit) = raycast(&store, ray.origin, *ray.direction, BRUSH_RAY_MAX_DIST) else {
        log.push("brush: no surface under cursor");
        return;
    };

    let inverted = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let subtract = state.subtract ^ inverted;
    let mode = if subtract { BrushMode::Subtract } else { BrushMode::Union };

    let brush = SphereBrush {
        center: hit.position,
        radius: state.radius,
        mode,
    };
    let touched = EditStroke::new(brush, &mut store, &mut dirty).apply();

    let label = if subtract { "lower" } else { "raise" };
    log.push(format!(
        "brush: {label} r={:.1} at ({:.1}, {:.1}, {:.1}) → {} chunks",
        state.radius,
        hit.position.x,
        hit.position.y,
        hit.position.z,
        touched.len(),
    ));
}
