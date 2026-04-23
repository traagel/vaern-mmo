//! Spell VFX: impact flash spheres at cast resolution, fade-out scaling,
//! and gizmo beams from each active caster to its target.

use bevy::prelude::*;
use vaern_combat::{Casting, Target};

use crate::menu::AppState;
use crate::scene::CastFiredLocal;
use crate::shared::{Player, school_color};

pub struct VfxPlugin;

impl Plugin for VfxPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                spawn_impact_effects,
                fade_out_effects,
                draw_cast_beams,
                draw_target_ring,
            )
                .run_if(in_state(AppState::InGame)),
        );
    }
}

#[derive(Component)]
struct FadeOut {
    remaining: f32,
    total: f32,
    start_scale: f32,
    end_scale: f32,
}

/// Drain the client-local broadcast of CastFired and spawn an impact
/// flash at the resolved target. `CastFiredLocal` is re-emitted by
/// `scene::relay_cast_fired` so multiple consumers (this system,
/// nameplate damage numbers, animation flash driver, diagnostics) can
/// each iterate without competing over the raw lightyear receiver.
fn spawn_impact_effects(
    mut reader: MessageReader<CastFiredLocal>,
    transforms: Query<&Transform>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    for CastFiredLocal(ev) in reader.read() {
        let Ok(target_tf) = transforms.get(ev.target) else {
            continue;
        };
        let color = school_color(&ev.school);
        let linear = color.to_linear();
        commands.spawn((
            Mesh3d(meshes.add(Sphere::new(0.35))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: color.with_alpha(0.85),
                emissive: LinearRgba::new(
                    linear.red * 6.0,
                    linear.green * 6.0,
                    linear.blue * 6.0,
                    1.0,
                ),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..default()
            })),
            Transform::from_translation(target_tf.translation + Vec3::Y * 1.7),
            FadeOut {
                remaining: 0.35,
                total: 0.35,
                start_scale: 1.0,
                end_scale: 3.5,
            },
        ));
    }
}

fn fade_out_effects(
    time: Res<Time>,
    mut query: Query<(
        Entity,
        &mut FadeOut,
        &mut Transform,
        &MeshMaterial3d<StandardMaterial>,
    )>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();
    for (entity, mut fade, mut tf, mat_handle) in &mut query {
        fade.remaining -= dt;
        if fade.remaining <= 0.0 {
            if let Ok(mut ec) = commands.get_entity(entity) {
                ec.despawn();
            }
            continue;
        }
        let progress = 1.0 - (fade.remaining / fade.total).clamp(0.0, 1.0);
        let scale = fade.start_scale + (fade.end_scale - fade.start_scale) * progress;
        tf.scale = Vec3::splat(scale);
        if let Some(mat) = materials.get_mut(&mat_handle.0) {
            let alpha = (1.0 - progress).max(0.0);
            mat.base_color = mat.base_color.with_alpha(alpha * 0.85);
        }
    }
}

/// Gizmo beam from each active caster to its target, colored by school.
fn draw_cast_beams(
    mut gizmos: Gizmos,
    casters: Query<(&Transform, &Casting)>,
    targets: Query<&Transform>,
) {
    for (caster_tf, casting) in &casters {
        let Ok(target_tf) = targets.get(casting.target) else {
            continue;
        };
        let color = school_color(&casting.school);
        gizmos.line(
            caster_tf.translation + Vec3::Y * 1.4,
            target_tf.translation + Vec3::Y * 1.4,
            color,
        );
    }
}

/// Flat gold ring on the ground under our current target. Uses gizmos so it's
/// free to draw — no entity or material bookkeeping. Pulses gently so it reads
/// as "alive" rather than baked-in scenery.
fn draw_target_ring(
    mut gizmos: Gizmos,
    time: Res<Time>,
    player_target: Query<&Target, With<Player>>,
    transforms: Query<&Transform>,
) {
    let Ok(target) = player_target.single() else { return };
    let Ok(tf) = transforms.get(target.0) else { return };

    // Pulse radius between 0.9 and 1.15 ~1Hz.
    let pulse = 0.5 + 0.5 * (time.elapsed_secs() * std::f32::consts::TAU * 1.2).sin();
    let radius = 0.9 + 0.25 * pulse;
    let center = Vec3::new(tf.translation.x, tf.translation.y + 0.05, tf.translation.z);

    // Outer ring.
    gizmos.circle(
        Isometry3d::new(center, Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
        radius,
        Color::srgb(1.0, 0.85, 0.25),
    );
    // Inner thin ring for a double-ring readable-from-distance look.
    gizmos.circle(
        Isometry3d::new(center, Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
        radius * 0.7,
        Color::srgba(1.0, 0.85, 0.25, 0.55),
    );
}
