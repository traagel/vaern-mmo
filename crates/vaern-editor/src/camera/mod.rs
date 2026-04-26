//! Free-fly editor camera.
//!
//! Spawns a single `Camera3d` with [`FreeFlyCamera`] + a default
//! [`FreeFlyState`] resource. Movement + look + speed-zoom live in
//! [`controller`]; ground-clamp helper in [`ground_clamp`].
//!
//! # Controls
//!
//! | input        | effect                                 |
//! |--------------|----------------------------------------|
//! | W / S        | forward / back along camera-XZ         |
//! | A / D        | strafe left / right                    |
//! | Q / E        | drop / rise vertically                 |
//! | RMB hold     | mouse-look (yaw + pitch)               |
//! | Scroll       | adjust move speed                      |
//! | LShift hold  | speed boost (× [`controller::SPEED_BOOST`]) |
//!
//! # Why free-fly, not orbit
//!
//! The orbit camera in `vaern-client/src/scene/camera.rs` is locked to
//! a player entity. The editor has no player — it's authoring tools
//! over a static zone. Free-fly mirrors the camera flow people expect
//! in DCC tools (Blender, UE, Unity scene view).

pub mod controller;
pub mod ground_clamp;

use bevy::camera::Exposure;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::render::view::Hdr;

pub use controller::FreeFlyState;

use crate::state::EditorAppState;

/// Marker on the editor's camera entity.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct FreeFlyCamera;

/// Spawns + drives the free-fly camera.
pub struct FreeFlyCameraPlugin;

impl Plugin for FreeFlyCameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FreeFlyState>()
            .add_systems(Startup, spawn_camera)
            .add_systems(
                Update,
                (
                    controller::apply_mouse_look,
                    controller::apply_movement,
                    controller::apply_scroll_speed,
                )
                    .chain()
                    .run_if(in_state(EditorAppState::Editing)),
            );
    }
}

/// One-time spawn at startup. Anchors the camera 80u above the world
/// origin so the loaded zone footprint is visible without scrolling
/// (`world::load::load_active_zone` re-positions us over the zone).
///
/// Carries the **same HDR / Atmosphere / Bloom / Tonemapping /
/// DistanceFog stack** as the runtime client camera
/// (`vaern-client/src/scene/setup.rs::setup_scene`). The `Atmosphere`
/// component is NOT inserted here — the environment driver
/// (`environment::apply_environment`) inserts it on first frame from
/// the cached `EnvAssets.atmosphere_medium` handle, so atmosphere
/// toggling can remove + re-insert without leaking medium handles.
fn spawn_camera(mut commands: Commands) {
    let pos = Vec3::new(0.0, 80.0, 80.0);
    commands.spawn((
        Camera3d::default(),
        Transform::from_translation(pos).looking_at(Vec3::ZERO, Vec3::Y),
        Hdr,
        Tonemapping::TonyMcMapface,
        Bloom::NATURAL,
        Exposure::SUNLIGHT,
        DistanceFog {
            color: Color::srgba(0.70, 0.78, 0.85, 1.0),
            directional_light_color: Color::srgba(1.0, 0.95, 0.80, 0.5),
            directional_light_exponent: 30.0,
            falloff: FogFalloff::from_visibility_squared(1500.0),
        },
        AmbientLight {
            color: Color::WHITE,
            brightness: 200.0,
            ..default()
        },
        FreeFlyCamera,
        Name::new("EditorFreeFlyCamera"),
    ));
}
