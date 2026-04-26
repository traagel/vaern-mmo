//! Free-fly movement + mouse-look + scroll-speed controller.
//!
//! Driven by [`FreeFlyState`] (yaw / pitch / speed) + per-frame Bevy
//! `ButtonInput<KeyCode>` and `MessageReader<MouseMotion / MouseWheel>`
//! events.

use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use bevy_egui::EguiContexts;

use super::FreeFlyCamera;

/// Mouse-look sensitivity (radians per pixel of motion).
pub const MOUSE_SENSITIVITY: f32 = 0.0035;
/// Pitch clamp. Slightly under ±π/2 so up/down look doesn't flip.
pub const PITCH_LIMIT: f32 = 1.45;
/// Default move speed in world units per second.
pub const DEFAULT_SPEED: f32 = 12.0;
/// Min / max speed (scroll wheel adjusts within this range).
pub const MIN_SPEED: f32 = 1.0;
pub const MAX_SPEED: f32 = 200.0;
/// Multiplier applied while LShift is held.
pub const SPEED_BOOST: f32 = 4.0;
/// Each scroll-tick scales speed by this factor (positive = speed up).
pub const SPEED_SCROLL_FACTOR: f32 = 1.15;

/// Mutable free-fly state. Yaw=0 looks toward -Z (north). Positive pitch
/// raises the view above horizon.
#[derive(Resource, Debug, Clone, Copy)]
pub struct FreeFlyState {
    pub yaw: f32,
    pub pitch: f32,
    pub speed: f32,
}

impl Default for FreeFlyState {
    fn default() -> Self {
        // Match the spawn camera's looking_at(ZERO) from (0, 80, 80) —
        // ~45° pitched down, facing -Z.
        Self {
            yaw: 0.0,
            pitch: -std::f32::consts::FRAC_PI_4,
            speed: DEFAULT_SPEED,
        }
    }
}

/// Drain `MouseMotion` events when the right mouse button is held and
/// rotate the camera. Cursor is grabbed + hidden while RMB is down.
///
/// Suppressed when egui has pointer capture so dragging inside a panel
/// can't spin the world view.
pub fn apply_mouse_look(
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mut motion: MessageReader<MouseMotion>,
    mut state: ResMut<FreeFlyState>,
    mut cams: Query<&mut Transform, With<FreeFlyCamera>>,
    mut cursors: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut egui: EguiContexts,
) {
    let egui_owns = egui
        .ctx_mut()
        .map(|c| c.is_pointer_over_area() || c.wants_pointer_input())
        .unwrap_or(false);

    let looking = mouse_buttons.pressed(MouseButton::Right) && !egui_owns;

    // Manage cursor grab. Only mutate when the desired state changes so
    // we don't fight a Window plugin tick.
    if let Ok(mut cursor) = cursors.single_mut() {
        let (want_grab, want_visible) = if looking {
            (CursorGrabMode::Locked, false)
        } else {
            (CursorGrabMode::None, true)
        };
        if cursor.grab_mode != want_grab {
            cursor.grab_mode = want_grab;
        }
        if cursor.visible != want_visible {
            cursor.visible = want_visible;
        }
    }

    if !looking {
        // Drain events so they don't all flood through on the first
        // frame after RMB releases.
        motion.clear();
        return;
    }

    let mut dx = 0.0;
    let mut dy = 0.0;
    for ev in motion.read() {
        dx += ev.delta.x;
        dy += ev.delta.y;
    }

    state.yaw -= dx * MOUSE_SENSITIVITY;
    state.pitch = (state.pitch - dy * MOUSE_SENSITIVITY).clamp(-PITCH_LIMIT, PITCH_LIMIT);

    let Ok(mut tf) = cams.single_mut() else {
        return;
    };
    tf.rotation = yaw_pitch_quat(state.yaw, state.pitch);
}

/// WASD + Q/E translation in world space, frame-rate-independent.
pub fn apply_movement(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<FreeFlyState>,
    mut cams: Query<&mut Transform, With<FreeFlyCamera>>,
    mut egui: EguiContexts,
) {
    let egui_focus = egui
        .ctx_mut()
        .map(|c| c.wants_keyboard_input())
        .unwrap_or(false);
    if egui_focus {
        return;
    }

    let mut local_axis = Vec3::ZERO;
    if keys.pressed(KeyCode::KeyW) {
        local_axis.z -= 1.0;
    }
    if keys.pressed(KeyCode::KeyS) {
        local_axis.z += 1.0;
    }
    if keys.pressed(KeyCode::KeyA) {
        local_axis.x -= 1.0;
    }
    if keys.pressed(KeyCode::KeyD) {
        local_axis.x += 1.0;
    }
    if keys.pressed(KeyCode::KeyQ) {
        local_axis.y -= 1.0;
    }
    if keys.pressed(KeyCode::KeyE) {
        local_axis.y += 1.0;
    }
    if local_axis == Vec3::ZERO {
        return;
    }

    let Ok(mut tf) = cams.single_mut() else {
        return;
    };

    // Translate in the camera's local frame for forward/strafe so look
    // direction drives motion. Vertical Q/E is world-space so pressing
    // E always lifts toward +Y regardless of pitch.
    let world_dir = tf.rotation * Vec3::new(local_axis.x, 0.0, local_axis.z);
    let world_dir = world_dir.normalize_or_zero();
    let vertical = Vec3::new(0.0, local_axis.y, 0.0);
    let dir = (world_dir + vertical).normalize_or_zero();

    let mut speed = state.speed;
    if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) {
        speed *= SPEED_BOOST;
    }

    tf.translation += dir * speed * time.delta_secs();
}

/// Mouse wheel scales `state.speed`. Multiplicative so the same number
/// of ticks moves through the speed range linearly in log space.
pub fn apply_scroll_speed(
    mut wheel: MessageReader<MouseWheel>,
    mut state: ResMut<FreeFlyState>,
    mut egui: EguiContexts,
) {
    let egui_owns = egui
        .ctx_mut()
        .map(|c| c.is_pointer_over_area())
        .unwrap_or(false);
    if egui_owns {
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

    let factor = SPEED_SCROLL_FACTOR.powf(ticks);
    state.speed = (state.speed * factor).clamp(MIN_SPEED, MAX_SPEED);
}

/// Build the camera's rotation quaternion from yaw + pitch.
/// Convention: yaw rotates around +Y (right-handed), pitch around the
/// resulting local +X.
fn yaw_pitch_quat(yaw: f32, pitch: f32) -> Quat {
    Quat::from_axis_angle(Vec3::Y, yaw) * Quat::from_axis_angle(Vec3::X, pitch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_speed_is_within_range() {
        let s = FreeFlyState::default();
        assert!(s.speed >= MIN_SPEED && s.speed <= MAX_SPEED);
    }

    #[test]
    fn yaw_pitch_zero_is_identity_within_eps() {
        let q = yaw_pitch_quat(0.0, 0.0);
        assert!(q.angle_between(Quat::IDENTITY) < 1e-5);
    }

    #[test]
    fn pitch_limit_is_under_half_pi() {
        assert!(PITCH_LIMIT < std::f32::consts::FRAC_PI_2);
    }
}
