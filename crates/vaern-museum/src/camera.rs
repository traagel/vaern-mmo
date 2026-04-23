//! Right-drag orbit + scroll zoom camera. Input is suppressed while egui
//! wants the pointer.

use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;
use bevy_egui::EguiContexts;

#[derive(Component)]
pub(crate) struct OrbitCamera {
    pub(crate) focus: Vec3,
    pub(crate) yaw: f32,
    pub(crate) pitch: f32,
    pub(crate) distance: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            focus: Vec3::new(0.0, 1.0, 0.0),
            yaw: 0.5,
            pitch: 0.35,
            distance: 4.5,
        }
    }
}

impl OrbitCamera {
    pub(crate) fn write_transform(&self, tf: &mut Transform) {
        let cos_p = self.pitch.cos();
        let offset = Vec3::new(
            self.distance * cos_p * self.yaw.sin(),
            self.distance * self.pitch.sin(),
            self.distance * cos_p * self.yaw.cos(),
        );
        *tf = Transform::from_translation(self.focus + offset).looking_at(self.focus, Vec3::Y);
    }
}

pub(crate) fn orbit_camera_input(
    mut contexts: EguiContexts,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    mut cams: Query<&mut OrbitCamera>,
) {
    let egui_has_pointer = contexts
        .ctx_mut()
        .map(|ctx| ctx.wants_pointer_input() || ctx.is_pointer_over_area())
        .unwrap_or(false);

    let Ok(mut cam) = cams.single_mut() else {
        return;
    };

    if !egui_has_pointer && mouse_buttons.pressed(MouseButton::Right) {
        cam.yaw -= motion.delta.x * 0.005;
        cam.pitch = (cam.pitch - motion.delta.y * 0.005)
            .clamp(-std::f32::consts::FRAC_PI_2 + 0.05, std::f32::consts::FRAC_PI_2 - 0.05);
    }

    if !egui_has_pointer && scroll.delta.y != 0.0 {
        cam.distance = (cam.distance * (1.0 - scroll.delta.y * 0.1)).clamp(0.8, 40.0);
    }
}

pub(crate) fn orbit_camera_apply(mut q: Query<(&OrbitCamera, &mut Transform)>) {
    for (cam, mut tf) in &mut q {
        cam.write_transform(&mut tf);
    }
}
