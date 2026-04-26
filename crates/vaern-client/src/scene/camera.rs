//! Orbit/follow camera + mouse-look input + cursor grab. Driven by
//! [`CameraController`] — a single resource holding spherical
//! coordinates around the player.

use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use vaern_core::terrain;
use vaern_voxel::chunk::ChunkStore;
use vaern_voxel::query::ground_y;

use crate::menu::AppState;
use crate::shared::{MainCamera, Player};

// --- tuning knobs -----------------------------------------------------------

const CAMERA_MOUSE_SENSITIVITY: f32 = 0.006;
const CAMERA_ZOOM_STEP: f32 = 1.0;
const CAMERA_MIN_PITCH: f32 = -1.2;
const CAMERA_MAX_PITCH: f32 = 1.2;
const CAMERA_MIN_DISTANCE: f32 = 3.0;
const CAMERA_MAX_DISTANCE: f32 = 40.0;

/// Vertical clearance held above the ground when the orbit position
/// would otherwise sink the camera into terrain (player on a slope or
/// inside a crater + camera angled low). Big enough to avoid the
/// near-plane skimming the surface; small enough that the camera still
/// hugs the ground when you tilt down.
const CAMERA_GROUND_CLEARANCE: f32 = 0.6;
/// Voxel-store probe parameters for the ground query at the camera's
/// XZ. Mirrors `vaern-server::movement::resolve_ground_y` so server +
/// client agree on what counts as "ground" — the camera can't sink
/// into a crater the server has carved.
const CAMERA_GROUND_PROBE_TOP: f32 = 64.0;
const CAMERA_GROUND_PROBE_MAX_DESCENT: f32 = 96.0;

// --- system set -------------------------------------------------------------

/// Ordering label for camera systems. External modules add `.after(…)`
/// or `.before(…)` against these rather than naming specific functions,
/// so internal restructuring doesn't break downstream ordering
/// guarantees.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CameraSet {
    /// Mouse-look + scroll + cursor management. Drains input events
    /// and writes to [`CameraController`] / `CursorOptions`. Systems
    /// that read the camera's final yaw (e.g. player target
    /// reorientation) should `.after(CameraSet::Input)`.
    Input,
}

// --- resource ---------------------------------------------------------------

/// Orbit-camera state: spherical coordinates around the player. Yaw=0
/// points the camera at -Z (north). Pitch is clamped to avoid gimbal
/// flips.
#[derive(Resource, Debug, Clone, Copy)]
pub struct CameraController {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
}

impl Default for CameraController {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: std::f32::consts::FRAC_PI_6, // ~30° above horizon
            distance: 10.0,
        }
    }
}

// --- plugin -----------------------------------------------------------------

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraController>()
            .add_systems(OnEnter(AppState::InGame), lock_cursor_on_enter)
            .add_systems(OnExit(AppState::InGame), release_cursor)
            .add_systems(
                Update,
                (
                    (manage_cursor_lock, mouse_look_camera_input).in_set(CameraSet::Input),
                    follow_camera,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

// --- cursor management ------------------------------------------------------

/// Entering InGame: grab + hide the cursor. Mouse-look mode is the default.
fn lock_cursor_on_enter(mut cursors: Query<&mut CursorOptions, With<PrimaryWindow>>) {
    let Ok(mut cursor) = cursors.single_mut() else { return };
    cursor.grab_mode = CursorGrabMode::Locked;
    cursor.visible = false;
}

/// Free the cursor when either:
///   * **LeftAlt** is held (transient UI interaction), OR
///   * an occluding panel is open (inventory, future bag/bank windows).
///
/// Keeps single-button simplicity for quick clicks and auto-frees the
/// cursor when the player explicitly pulls up a panel that needs
/// clicking. When the panel closes, the cursor re-locks.
fn manage_cursor_lock(
    keys: Res<ButtonInput<KeyCode>>,
    inv_open: Res<crate::inventory_ui::InventoryWindowOpen>,
    stat_open: Res<crate::stat_screen::StatScreenOpen>,
    loot_window: Res<crate::loot_ui::LootWindow>,
    mut cursors: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    let Ok(mut cursor) = cursors.single_mut() else { return };
    let free_look =
        keys.pressed(KeyCode::AltLeft) || inv_open.0 || stat_open.0 || loot_window.is_open();
    let desired_grab = if free_look { CursorGrabMode::None } else { CursorGrabMode::Locked };
    let desired_visible = free_look;
    if cursor.grab_mode != desired_grab {
        cursor.grab_mode = desired_grab;
    }
    if cursor.visible != desired_visible {
        cursor.visible = desired_visible;
    }
}

/// On teardown (returning to menu from InGame), make sure the cursor
/// is not left in a locked/invisible state.
fn release_cursor(mut cursors: Query<&mut CursorOptions, With<PrimaryWindow>>) {
    let Ok(mut cursor) = cursors.single_mut() else { return };
    cursor.grab_mode = CursorGrabMode::None;
    cursor.visible = true;
}

// --- mouse-look + zoom ------------------------------------------------------

/// Mouse-look with tab-target-lock interaction:
///   - pitch always mouse-driven (look up/down freely)
///   - yaw mouse-driven only when NO target is held. While a target
///     exists, `update_player_target` sets yaw to face it every frame;
///     mouse yaw delta is discarded here to avoid fighting the lock.
///   - LeftAlt = free-look: all mouse camera input suppressed (cursor
///     is also freed so you can click UI).
///   - Inventory window open: same as free-look — mouse deltas drained
///     without applying, so moving the cursor to click a slot doesn't
///     spin the camera. Scroll wheel also suppressed so scrolling a
///     panel doesn't zoom the world.
///   - scroll wheel always controls zoom (when not in UI mode).
fn mouse_look_camera_input(
    keys: Res<ButtonInput<KeyCode>>,
    inv_open: Res<crate::inventory_ui::InventoryWindowOpen>,
    stat_open: Res<crate::stat_screen::StatScreenOpen>,
    loot_window: Res<crate::loot_ui::LootWindow>,
    mut motion: MessageReader<MouseMotion>,
    mut wheel: MessageReader<MouseWheel>,
    mut controller: ResMut<CameraController>,
    player_target: Query<(), (With<Player>, With<vaern_combat::Target>)>,
) {
    // Drain events without applying when in a UI-interactive mode.
    // Skipping `motion.read()` would let the queue accumulate and flood
    // the camera on the first frame UI closes — read and discard instead.
    if inv_open.0 || stat_open.0 || loot_window.is_open() {
        motion.clear();
        wheel.clear();
        return;
    }
    let free_look = keys.pressed(KeyCode::AltLeft);
    let has_target = player_target.single().is_ok();
    let mut dx = 0.0;
    let mut dy = 0.0;
    for ev in motion.read() {
        dx += ev.delta.x;
        dy += ev.delta.y;
    }
    if !free_look {
        if !has_target {
            controller.yaw -= dx * CAMERA_MOUSE_SENSITIVITY;
        }
        controller.pitch = (controller.pitch - dy * CAMERA_MOUSE_SENSITIVITY)
            .clamp(CAMERA_MIN_PITCH, CAMERA_MAX_PITCH);
    }
    let mut zoom_delta = 0.0;
    for ev in wheel.read() {
        zoom_delta += ev.y;
    }
    if zoom_delta != 0.0 {
        controller.distance = (controller.distance - zoom_delta * CAMERA_ZOOM_STEP)
            .clamp(CAMERA_MIN_DISTANCE, CAMERA_MAX_DISTANCE);
    }
}

/// Spherical orbit follow: place the camera at
/// player+offset(yaw,pitch,distance) and point it at the player's chest
/// height.
///
/// After computing the orbit position, raises the camera to at least
/// [`CAMERA_GROUND_CLEARANCE`] above the voxel ground at its XZ
/// footprint — keeps the camera from clipping into terrain when the
/// player is on a slope or in a server-carved crater and the orbit
/// angle is steep. The look-at re-aim runs *after* the clamp so the
/// view direction follows the lifted position.
fn follow_camera(
    players: Query<&Transform, (With<Player>, Without<MainCamera>)>,
    mut cams: Query<&mut Transform, With<MainCamera>>,
    controller: Res<CameraController>,
    store: Res<ChunkStore>,
) {
    let Ok(player_tf) = players.single() else { return };
    let Ok(mut cam_tf) = cams.single_mut() else { return };

    // yaw=0, pitch=0 → camera at +Z of player looking -Z (north).
    // Positive yaw rotates clockwise when viewed from above (+Y).
    // Positive pitch raises the camera above the horizon.
    let cos_pitch = controller.pitch.cos();
    let sin_pitch = controller.pitch.sin();
    let offset = Vec3::new(
        controller.distance * cos_pitch * controller.yaw.sin(),
        controller.distance * sin_pitch,
        controller.distance * cos_pitch * controller.yaw.cos(),
    );
    let mut pos = player_tf.translation + offset;

    // Ground clamp. Mirrors the server's `resolve_ground_y`: query the
    // voxel store first (catches edits the server has applied), fall
    // back to the analytical heightmap for chunks not yet seeded
    // around the camera. The camera's XZ can range up to
    // `CAMERA_MAX_DISTANCE` from the player's feet, so it routinely
    // peeks into terrain the player hasn't touched the chunk for.
    let probe_top = pos.y.max(player_tf.translation.y) + CAMERA_GROUND_PROBE_TOP;
    let ground = ground_y(&store, pos.x, pos.z, probe_top, CAMERA_GROUND_PROBE_MAX_DESCENT)
        .unwrap_or_else(|| terrain::height(pos.x, pos.z));
    let floor = ground + CAMERA_GROUND_CLEARANCE;
    if pos.y < floor {
        pos.y = floor;
    }

    cam_tf.translation = pos;
    // Aim at player's chest, not their feet — feels less "ground-camera".
    let look_at = player_tf.translation + Vec3::Y * 1.5;
    *cam_tf = cam_tf.looking_at(look_at, Vec3::Y);
}
