//! Scene bootstrap + teardown. Spawns the menu overlay camera once at
//! startup and the gameplay camera / ground / sun on `AppState::InGame`
//! entry; tears everything down on exit.

use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::pbr::{Atmosphere, DistanceFog, FogFalloff, ScatteringMedium};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::camera::Exposure;
use bevy::render::view::Hdr;
use lightyear::prelude::client::*;
use lightyear::prelude::*;

use crate::menu::AppState;
use crate::shared::{GameWorld, MainCamera, MenuCamera};

pub struct SetupPlugin;

impl Plugin for SetupPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_menu_camera)
            .add_systems(OnEnter(AppState::InGame), setup_scene)
            .add_systems(OnExit(AppState::InGame), teardown_game)
            .add_systems(Update, menu_camera_bg_clear);
    }
}

fn setup_menu_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        // Order 10 puts this camera AFTER the gameplay Camera3d (order 0),
        // so egui (attached to this camera's surface) draws on top of the
        // 3D scene. Clear=None so it doesn't wipe the 3D render. In the
        // main menu this camera is the only one alive, so the BG color
        // still shows via menu_camera_bg_clear below.
        Camera {
            order: 10,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        MenuCamera,
    ));
}

/// While we're in the main-menu states (no 3D camera yet), the menu
/// camera with Clear=None leaves the window uncleared — it'd flicker
/// whatever was in the framebuffer. This system toggles the clear color
/// based on state: opaque dark gray in menu/connecting, transparent
/// in-game.
fn menu_camera_bg_clear(
    state: Res<State<AppState>>,
    mut cam: Query<&mut Camera, With<MenuCamera>>,
) {
    let Ok(mut c) = cam.single_mut() else { return };
    let wanted = match state.get() {
        AppState::MainMenu | AppState::Connecting => {
            ClearColorConfig::Custom(Color::srgb(0.06, 0.08, 0.11))
        }
        AppState::InGame => ClearColorConfig::None,
    };
    // Simple discriminant-based equality check to avoid thrashing.
    let same = matches!(
        (&c.clear_color, &wanted),
        (ClearColorConfig::None, ClearColorConfig::None)
            | (ClearColorConfig::Custom(_), ClearColorConfig::Custom(_))
    );
    if !same {
        c.clear_color = wanted;
    }
}

fn setup_scene(
    mut commands: Commands,
    mut mediums: ResMut<Assets<ScatteringMedium>>,
) {
    // Ground + grid live in `ground::GroundPlugin`. Here we spawn the
    // sun, gameplay camera, and atmosphere scattering.

    // Earth-like scattering medium — drives the procedural sky color.
    // Default 256/256 resolution is plenty for a single atmosphere.
    let medium = mediums.add(ScatteringMedium::earthlike(256, 256));

    // Directional sun. Physically-plausible illuminance (~100k lux) so
    // the atmospheric scattering integrates correctly. The previous
    // 8_000 value was dim relative to the new HDR / Atmosphere pipeline
    // and would render the world in perpetual overcast.
    commands.spawn((
        DirectionalLight {
            illuminance: 100_000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(
            EulerRot::XYZ,
            -std::f32::consts::FRAC_PI_3,
            std::f32::consts::FRAC_PI_4,
            0.0,
        )),
        GameWorld,
    ));

    // Camera — follow_camera positions it each frame.
    //
    // HDR pipeline: Hdr + Tonemapping + Bloom + Atmosphere form a
    // cohesive stack. Atmosphere renders a physically-based procedural
    // sky that reads the DirectionalLight as the sun, so no skybox
    // cubemap or HDRI bake is needed to get a believable horizon.
    // DistanceFog softens faraway silhouettes and hides the edge of
    // the 2200u ground plane. Exposure is set to outdoor physical
    // sunlight so the tonemapped image doesn't blow out.
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 12.0, 12.0).looking_at(Vec3::ZERO, Vec3::Y),
        Hdr,
        Tonemapping::TonyMcMapface,
        Bloom::NATURAL,
        Exposure::SUNLIGHT,
        Atmosphere::earthlike(medium),
        DistanceFog {
            color: Color::srgba(0.70, 0.78, 0.85, 1.0),
            directional_light_color: Color::srgba(1.0, 0.95, 0.80, 0.5),
            directional_light_exponent: 30.0,
            // Exp-squared keeps the near field crisp and only the
            // distant horizon softens into haze. 1500u visibility
            // still hides the plane edge at 1100u but doesn't milk
            // out everything past the nearest NPCs.
            falloff: FogFalloff::from_visibility_squared(1500.0),
        },
        AmbientLight {
            color: Color::WHITE,
            brightness: 20.0,
            ..default()
        },
        MainCamera,
        GameWorld,
    ));

    info!(
        "Vaern scaffold ready. WASD: move · 1-6: hotbar · Tab: cycle target · \
         Esc: clear target · I: inventory · LeftAlt: free cursor"
    );
}

fn teardown_game(
    world_entities: Query<Entity, With<GameWorld>>,
    ui_roots: Query<Entity, (With<Node>, Without<ChildOf>)>,
    clients: Query<Entity, With<Client>>,
    replicated: Query<Entity, With<Replicated>>,
    predicted: Query<Entity, With<Predicted>>,
    interpolated: Query<Entity, With<Interpolated>>,
    mut commands: Commands,
) {
    for e in &clients {
        commands.entity(e).despawn();
    }
    for e in &replicated {
        commands.entity(e).despawn();
    }
    for e in &predicted {
        commands.entity(e).despawn();
    }
    for e in &interpolated {
        commands.entity(e).despawn();
    }
    for e in &world_entities {
        commands.entity(e).despawn();
    }
    for e in &ui_roots {
        commands.entity(e).despawn();
    }
    info!("teardown complete — returned to main menu");
}
