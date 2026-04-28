//! Editor environment — atmosphere / time-of-day / fog / draw distance.
//!
//! All driven by a single [`EnvSettings`] resource. The plugin runs:
//!
//! 1. [`tick_time_of_day`] — advances `time_hours` when autoplay is on.
//! 2. [`apply_environment`] — reads `EnvSettings`, writes to the editor
//!    sun (rotation/color/illuminance), the camera's `AmbientLight` +
//!    `DistanceFog`, and toggles `Atmosphere` by inserting/removing
//!    the component on the camera entity.
//!
//! Sun position math + color/illuminance/fog curves are pure helpers
//! so they unit-test cleanly.

use bevy::pbr::{Atmosphere, DistanceFog, FogFalloff, ScatteringMedium};
use bevy::prelude::*;

use crate::camera::FreeFlyCamera;
use crate::state::EditorAppState;
use crate::voxel::stream::DEFAULT_STREAM_RADIUS_XZ;
use crate::world::markers::EditorSun;

/// Snake-cased label for each falloff mode (used by the inspector).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FogFalloffMode {
    Linear,
    Exponential,
    #[default]
    ExponentialSquared,
}

impl FogFalloffMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Linear => "Linear",
            Self::Exponential => "Exp",
            Self::ExponentialSquared => "Exp²",
        }
    }

    pub const ALL: [FogFalloffMode; 3] = [
        Self::Linear,
        Self::Exponential,
        Self::ExponentialSquared,
    ];
}

/// Live environment state. Reset on launch — disk persistence deferred.
#[derive(Resource, Debug, Clone)]
pub struct EnvSettings {
    // ---- Time of day ----
    pub time_hours: f32,
    pub autoplay: bool,
    pub autoplay_seconds_per_hour: f32,

    // ---- Fog ----
    pub fog_enabled: bool,
    pub fog_visibility_u: f32,
    pub fog_falloff: FogFalloffMode,
    pub fog_color_auto: bool,
    pub fog_color_manual: [f32; 4],

    // ---- Sky ----
    pub atmosphere_enabled: bool,

    // ---- Streaming ----
    pub draw_distance_chunks: i32,
}

impl Default for EnvSettings {
    fn default() -> Self {
        Self {
            time_hours: 12.0,
            autoplay: false,
            autoplay_seconds_per_hour: 60.0,

            fog_enabled: true,
            fog_visibility_u: 1500.0,
            fog_falloff: FogFalloffMode::ExponentialSquared,
            fog_color_auto: true,
            fog_color_manual: [0.70, 0.78, 0.85, 1.0],

            atmosphere_enabled: true,

            draw_distance_chunks: DEFAULT_STREAM_RADIUS_XZ,
        }
    }
}

/// Cached `ScatteringMedium` handle so toggling `Atmosphere` on/off
/// doesn't allocate a fresh medium each time. Initialized at Startup.
#[derive(Resource)]
pub struct EnvAssets {
    pub atmosphere_medium: Handle<ScatteringMedium>,
}

pub struct EnvironmentPlugin;

impl Plugin for EnvironmentPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EnvSettings>()
            .add_systems(Startup, init_env_assets)
            .add_systems(
                Update,
                (
                    attach_atmosphere_on_camera_spawn,
                    tick_time_of_day,
                    apply_environment,
                )
                    .chain()
                    .run_if(in_state(EditorAppState::Editing)),
            );
    }
}

/// Attach `Atmosphere` to the camera once, on first frame after spawn,
/// when `EnvSettings.atmosphere_enabled` is true.
///
/// **Why not Startup, why not toggleable?** Bevy's PBR pipeline is
/// specialized at pipeline-cache time based on whether `Atmosphere`
/// is present on the view: presence selects the
/// `mesh_view_layout_multisampled_atmosphere` layout (with bindings
/// 29/30/31 for atmosphere LUTs); absence selects
/// `mesh_view_layout_multisampled`. Inserting/removing the component
/// at runtime mismatches the cached pipeline and bind group layouts,
/// crashing wgpu mid-frame ("Expected entry with binding 29 not found").
/// So atmosphere is a one-shot startup-time decision; flipping the
/// `atmosphere_enabled` bool in the Sky panel sets the resource but
/// has no runtime effect until restart.
pub fn attach_atmosphere_on_camera_spawn(
    mut commands: Commands,
    env: Res<EnvSettings>,
    env_assets: Option<Res<EnvAssets>>,
    cam_q: Query<Entity, (Added<FreeFlyCamera>, Without<Atmosphere>)>,
) {
    if !env.atmosphere_enabled {
        return;
    }
    let Some(env_assets) = env_assets else {
        return;
    };
    for entity in &cam_q {
        commands
            .entity(entity)
            .insert(Atmosphere::earthlike(env_assets.atmosphere_medium.clone()));
    }
}

fn init_env_assets(
    mut commands: Commands,
    mut mediums: ResMut<Assets<ScatteringMedium>>,
) {
    let medium = mediums.add(ScatteringMedium::earthlike(256, 256));
    commands.insert_resource(EnvAssets {
        atmosphere_medium: medium,
    });
}

pub fn tick_time_of_day(time: Res<Time>, mut env: ResMut<EnvSettings>) {
    if !env.autoplay {
        return;
    }
    let secs_per_hour = env.autoplay_seconds_per_hour.max(1.0);
    let dt_hours = time.delta_secs() / secs_per_hour;
    env.time_hours = (env.time_hours + dt_hours).rem_euclid(24.0);
}

#[allow(clippy::too_many_arguments)]
pub fn apply_environment(
    env: Res<EnvSettings>,
    mut sun_q: Query<
        (&mut Transform, &mut DirectionalLight),
        (With<EditorSun>, Without<FreeFlyCamera>),
    >,
    mut cam_ambient: Query<&mut AmbientLight, With<FreeFlyCamera>>,
    mut cam_fog: Query<&mut DistanceFog, With<FreeFlyCamera>>,
) {
    // Sun.
    let dir = sun_direction_at_time(env.time_hours);
    let color = sun_color_at_time(env.time_hours);
    let lux = sun_illuminance_at_time(env.time_hours);
    if let Ok((mut tf, mut light)) = sun_q.single_mut() {
        // Build a transform that has its forward (-Z) pointing along
        // `dir`. `looking_to` orients the entity's -Z toward the given
        // direction.
        *tf = Transform::IDENTITY.looking_to(dir, Vec3::Y);
        light.color = color;
        light.illuminance = lux;
    }

    // Ambient floor.
    if let Ok(mut amb) = cam_ambient.single_mut() {
        amb.brightness = ambient_brightness_at_time(env.time_hours);
    }

    // Fog.
    if let Ok(mut fog) = cam_fog.single_mut() {
        if env.fog_enabled {
            fog.falloff = match env.fog_falloff {
                FogFalloffMode::Linear => FogFalloff::Linear {
                    start: env.fog_visibility_u * 0.25,
                    end: env.fog_visibility_u,
                },
                FogFalloffMode::Exponential => {
                    FogFalloff::from_visibility(env.fog_visibility_u)
                }
                FogFalloffMode::ExponentialSquared => {
                    FogFalloff::from_visibility_squared(env.fog_visibility_u)
                }
            };
            fog.color = if env.fog_color_auto {
                fog_color_at_time(env.time_hours)
            } else {
                Color::srgba(
                    env.fog_color_manual[0],
                    env.fog_color_manual[1],
                    env.fog_color_manual[2],
                    env.fog_color_manual[3],
                )
            };
        } else {
            // Push visibility to effectively infinite — equivalent to
            // disabling fog without removing the component.
            fog.falloff = FogFalloff::from_visibility_squared(1.0e9);
        }
    }

    // Atmosphere is attached once at camera-spawn by
    // `attach_atmosphere_on_camera_spawn` — runtime toggling crashes
    // wgpu (pipeline-cache vs bind-group layout mismatch). The Sky
    // panel toggle still flips `env.atmosphere_enabled` for telemetry
    // but takes effect on next launch.
}

// ---- Pure helpers (testable) ----------------------------------------

/// Sun direction (world-space unit vector pointing FROM sun TOWARD
/// scene). Light's transform uses `looking_to(dir, Vec3::Y)` so its
/// -Z aligns with this direction.
///
/// At noon (t=12) → (0, -1, 0) (straight down).
/// At midnight (t=0/24) → (0, +1, 0) (below horizon, pointing up).
/// At dawn (t=6) → roughly horizontal, azimuth quarter-turn.
pub fn sun_direction_at_time(t_hours: f32) -> Vec3 {
    let t = t_hours.rem_euclid(24.0);
    // Elevation: −π/2 at midnight, 0 at dawn/dusk, +π/2 at noon.
    let elev = ((t / 24.0 * std::f32::consts::TAU) - std::f32::consts::FRAC_PI_2).sin()
        * std::f32::consts::FRAC_PI_2;
    // Azimuth — full 360° orbit over the day.
    let azim = (t / 24.0) * std::f32::consts::TAU;
    Vec3::new(
        elev.cos() * azim.cos(),
        -elev.sin(),
        elev.cos() * azim.sin(),
    )
    .normalize()
}

/// Sun color across the day. Smooth lerp between hand-picked stops.
pub fn sun_color_at_time(t: f32) -> Color {
    let stops: &[(f32, [f32; 3])] = &[
        (0.0, [0.10, 0.12, 0.20]),  // midnight
        (5.0, [0.30, 0.20, 0.30]),  // pre-dawn
        (6.5, [1.00, 0.55, 0.30]),  // dawn
        (8.0, [1.00, 0.90, 0.75]),  // morning
        (12.0, [1.00, 1.00, 1.00]), // noon
        (16.0, [1.00, 0.90, 0.75]), // afternoon
        (18.0, [1.00, 0.50, 0.20]), // sunset
        (20.0, [0.30, 0.20, 0.40]), // twilight
        (24.0, [0.10, 0.12, 0.20]), // midnight
    ];
    let rgb = lerp_stops(stops, t.rem_euclid(24.0));
    Color::srgb(rgb[0], rgb[1], rgb[2])
}

/// Sin-clamped illuminance — peaks at noon, zero from dusk to dawn.
pub fn sun_illuminance_at_time(t: f32) -> f32 {
    let t = t.rem_euclid(24.0);
    let s = ((t - 6.0) / 12.0 * std::f32::consts::PI).sin();
    s.max(0.0) * 100_000.0
}

/// Ambient floor — 12 at midnight, 200 at noon. Linear in illuminance.
pub fn ambient_brightness_at_time(t: f32) -> f32 {
    let frac = (sun_illuminance_at_time(t) / 100_000.0).clamp(0.0, 1.0);
    12.0 + 188.0 * frac
}

/// Fog color tracks sun, biased toward sky-blue at zenith.
pub fn fog_color_at_time(t: f32) -> Color {
    let sun = color_to_rgb(sun_color_at_time(t));
    let zenith = [0.70, 0.78, 0.85];
    let elev_frac = (sun_illuminance_at_time(t) / 100_000.0).clamp(0.0, 1.0);
    Color::srgb(
        sun[0] + (zenith[0] - sun[0]) * elev_frac,
        sun[1] + (zenith[1] - sun[1]) * elev_frac,
        sun[2] + (zenith[2] - sun[2]) * elev_frac,
    )
}

fn lerp_stops(stops: &[(f32, [f32; 3])], t: f32) -> [f32; 3] {
    if stops.is_empty() {
        return [1.0, 1.0, 1.0];
    }
    if t <= stops[0].0 {
        return stops[0].1;
    }
    for w in stops.windows(2) {
        let (t0, a) = (w[0].0, w[0].1);
        let (t1, b) = (w[1].0, w[1].1);
        if t >= t0 && t <= t1 {
            let f = if (t1 - t0).abs() < 1e-6 {
                0.0
            } else {
                (t - t0) / (t1 - t0)
            };
            return [
                a[0] + (b[0] - a[0]) * f,
                a[1] + (b[1] - a[1]) * f,
                a[2] + (b[2] - a[2]) * f,
            ];
        }
    }
    stops.last().map(|s| s.1).unwrap_or([1.0, 1.0, 1.0])
}

fn color_to_rgb(c: Color) -> [f32; 3] {
    let s = c.to_srgba();
    [s.red, s.green, s.blue]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sun_direction_at_noon_points_down() {
        let d = sun_direction_at_time(12.0);
        assert!(d.y < -0.99, "expected down, got {d:?}");
    }

    #[test]
    fn sun_direction_at_midnight_points_up() {
        let d = sun_direction_at_time(0.0);
        assert!(d.y > 0.99, "expected up, got {d:?}");
    }

    #[test]
    fn sun_direction_at_dawn_horizontal() {
        let d = sun_direction_at_time(6.0);
        assert!(d.y.abs() < 0.05, "expected horizontal, got {d:?}");
    }

    #[test]
    fn sun_illuminance_zero_at_night() {
        assert!(sun_illuminance_at_time(0.0) < 1.0);
        assert!(sun_illuminance_at_time(2.0) < 1.0);
        assert!(sun_illuminance_at_time(22.0) < 1.0);
        assert!(sun_illuminance_at_time(24.0) < 1.0);
    }

    #[test]
    fn sun_illuminance_peaks_at_noon() {
        assert!(sun_illuminance_at_time(12.0) > 99_000.0);
    }

    #[test]
    fn sun_illuminance_zero_at_dawn_and_dusk() {
        assert!(sun_illuminance_at_time(6.0).abs() < 1.0);
        assert!(sun_illuminance_at_time(18.0).abs() < 1.0);
    }

    #[test]
    fn ambient_floor_at_night_is_positive() {
        assert!(ambient_brightness_at_time(0.0) >= 12.0);
    }

    #[test]
    fn ambient_peaks_at_noon() {
        let n = ambient_brightness_at_time(12.0);
        assert!(n > 199.0 && n <= 200.0, "got {n}");
    }

    #[test]
    fn env_settings_default_is_noon_with_atmosphere_on() {
        let e = EnvSettings::default();
        assert!((e.time_hours - 12.0).abs() < 1e-6);
        assert!(e.atmosphere_enabled);
        assert!(e.fog_enabled);
        assert!(!e.autoplay);
    }

    #[test]
    fn draw_distance_default_matches_legacy_const() {
        assert_eq!(
            EnvSettings::default().draw_distance_chunks,
            DEFAULT_STREAM_RADIUS_XZ
        );
    }

    #[test]
    fn lerp_stops_returns_first_when_below_range() {
        let stops = &[(5.0_f32, [1.0, 0.0, 0.0]), (10.0, [0.0, 1.0, 0.0])];
        let r = lerp_stops(stops, 2.0);
        assert_eq!(r, [1.0, 0.0, 0.0]);
    }

    #[test]
    fn lerp_stops_interpolates_midpoint() {
        let stops = &[(0.0_f32, [0.0, 0.0, 0.0]), (10.0, [1.0, 1.0, 1.0])];
        let r = lerp_stops(stops, 5.0);
        for c in &r {
            assert!((c - 0.5).abs() < 1e-6);
        }
    }
}
