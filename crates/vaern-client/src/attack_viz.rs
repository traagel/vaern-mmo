//! Visual feedback for the shape system:
//!   - in-flight projectiles render as a glowing colored sphere (via the
//!     replicated `ProjectileVisual` marker + `Transform`)
//!   - own-player casts spawn a transient gizmo telegraph in the right shape
//!     (cone / line / ring) at the moment the key is pressed
//!
//! Gizmo-based flashes are immediate-mode — no entity lifetime to worry about
//! past a small fade timer. The projectile mesh uses Bevy's standard PBR
//! pipeline so it reads against the 3D world lighting.

use bevy::prelude::*;
use lightyear::prelude::*;
use vaern_combat::{ProjectileVisual, Target};

use crate::hotbar_ui::{CastAttempted, Hotbar};
use crate::menu::AppState;
use crate::shared::{GameWorld, Player, school_color};

pub struct AttackVizPlugin;

impl Plugin for AttackVizPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                attach_projectile_meshes,
                spawn_cast_flashes,
                draw_cast_flashes,
            )
                .run_if(in_state(AppState::InGame)),
        );
    }
}

// ─── projectile meshes ─────────────────────────────────────────────────────

/// Marker for "we already attached a mesh child to this projectile".
#[derive(Component)]
struct ProjectileMeshAttached;

/// Whenever a new `ProjectileVisual` shows up (server replicated it), attach
/// a glowing sphere child mesh colored by school. Position comes from the
/// replicated Transform — Bevy handles interpolation via lightyear.
fn attach_projectile_meshes(
    fresh: Query<
        (Entity, &ProjectileVisual),
        (
            With<Replicated>,
            Without<ProjectileMeshAttached>,
        ),
    >,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    for (entity, vis) in &fresh {
        let color = school_color(&vis.school);
        let linear = color.to_linear();
        let mesh = meshes.add(Sphere::new(0.3));
        let material = materials.add(StandardMaterial {
            base_color: color,
            emissive: LinearRgba::new(
                linear.red * 4.0,
                linear.green * 4.0,
                linear.blue * 4.0,
                1.0,
            ),
            unlit: true,
            ..default()
        });
        commands.entity(entity).insert((
            Mesh3d(mesh),
            MeshMaterial3d(material),
            ProjectileMeshAttached,
            // Tagged as GameWorld so teardown-on-logout sweeps it.
            GameWorld,
        ));
    }
}

// ─── own-cast shape telegraph ──────────────────────────────────────────────

/// Transient shape flash spawned on a hotbar keypress. Lives for ~0.35s,
/// drawn each frame by `draw_cast_flashes` via gizmos, then despawns.
#[derive(Component, Debug, Clone)]
struct CastFlash {
    origin: Vec3,
    /// Horizontal unit direction (zero for self-AoE).
    aim: Vec3,
    shape: String,
    range: f32,
    aoe_radius: f32,
    cone_half_angle_deg: f32,
    line_width: f32,
    remaining: f32,
    total: f32,
    school: String,
}

/// Detect own-player hotbar keypresses and spawn a matching shape flash at
/// the caster's current transform, aimed toward the current target.
fn spawn_cast_flashes(
    mut attempts: MessageReader<CastAttempted>,
    hotbar: Res<Hotbar>,
    player: Query<(&Transform, Option<&Target>), With<Player>>,
    transforms: Query<&Transform>,
    mut commands: Commands,
) {
    let Ok((ptf, target)) = player.single() else {
        attempts.clear();
        return;
    };
    let Some(attempt) = attempts.read().last().copied() else { return };
    let Some(slot) = hotbar.slots.get(attempt.slot_idx as usize) else { return };
    let info = &slot.info;

    // Compute aim: horizontal unit vector from caster → target (if any).
    let aim = target
        .and_then(|t| transforms.get(t.0).ok().map(|ttf| ttf.translation))
        .map(|tp| {
            let mut d = tp - ptf.translation;
            d.y = 0.0;
            if d.length_squared() > 1e-4 { d.normalize() } else { Vec3::ZERO }
        })
        .unwrap_or(Vec3::ZERO);

    commands.spawn(CastFlash {
        origin: ptf.translation,
        aim,
        shape: info.shape.clone(),
        range: info.range,
        aoe_radius: info.aoe_radius,
        cone_half_angle_deg: info.cone_half_angle_deg,
        line_width: info.line_width,
        remaining: 0.35,
        total: 0.35,
        school: info.school.clone(),
    });
}

fn draw_cast_flashes(
    time: Res<Time>,
    mut gizmos: Gizmos,
    mut flashes: Query<(Entity, &mut CastFlash)>,
    mut commands: Commands,
    mut seen_any: Local<bool>,
) {
    let dt = time.delta_secs();
    for (entity, mut flash) in &mut flashes {
        if !*seen_any {
            info!(
                "[attack_viz] drawing first flash shape={} aim=({:.2},{:.2}) origin=({:.1},{:.1},{:.1})",
                flash.shape, flash.aim.x, flash.aim.z,
                flash.origin.x, flash.origin.y, flash.origin.z,
            );
            *seen_any = true;
        }
        flash.remaining -= dt;
        if flash.remaining <= 0.0 {
            if let Ok(mut ec) = commands.get_entity(entity) {
                ec.despawn();
            }
            continue;
        }

        // Fade alpha from 0.9 → 0.0 over the lifetime.
        let t = (flash.remaining / flash.total).clamp(0.0, 1.0);
        let base = school_color(&flash.school);
        let color = base.with_alpha(0.9 * t);
        let ground_y = flash.origin.y + 0.05;

        match flash.shape.as_str() {
            "aoe_on_self" => {
                let center = Vec3::new(flash.origin.x, ground_y, flash.origin.z);
                gizmos.circle(
                    Isometry3d::new(
                        center,
                        Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
                    ),
                    flash.aoe_radius,
                    color,
                );
            }
            "aoe_on_target" => {
                // Origin is aimed — step `range` along aim on the ground
                // plane (approx target position, enough for a telegraph).
                if flash.aim.length_squared() < 1e-4 {
                    continue;
                }
                let center = Vec3::new(flash.origin.x, ground_y, flash.origin.z)
                    + flash.aim * flash.range;
                gizmos.circle(
                    Isometry3d::new(
                        center,
                        Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
                    ),
                    flash.aoe_radius,
                    color,
                );
            }
            "cone" => {
                if flash.aim.length_squared() < 1e-4 {
                    continue;
                }
                draw_cone(
                    &mut gizmos,
                    Vec3::new(flash.origin.x, ground_y, flash.origin.z),
                    flash.aim,
                    flash.range,
                    flash.cone_half_angle_deg.to_radians(),
                    color,
                );
            }
            "line" => {
                if flash.aim.length_squared() < 1e-4 {
                    continue;
                }
                draw_line_rect(
                    &mut gizmos,
                    Vec3::new(flash.origin.x, ground_y, flash.origin.z),
                    flash.aim,
                    flash.range,
                    flash.line_width * 0.5,
                    color,
                );
            }
            // Target / projectile: no shape telegraph — impact flash + the
            // projectile mesh itself are the player feedback.
            _ => {}
        }
    }
}

/// Wedge on the ground: two radial edges + connecting arc. 8 arc segments.
fn draw_cone(
    gizmos: &mut Gizmos,
    origin: Vec3,
    aim: Vec3,
    range: f32,
    half_angle_rad: f32,
    color: Color,
) {
    let rotate_y = |v: Vec3, angle: f32| -> Vec3 {
        let (s, c) = angle.sin_cos();
        Vec3::new(v.x * c - v.z * s, v.y, v.x * s + v.z * c)
    };
    let left = rotate_y(aim, -half_angle_rad) * range;
    let right = rotate_y(aim, half_angle_rad) * range;

    gizmos.line(origin, origin + left, color);
    gizmos.line(origin, origin + right, color);

    // Arc approximation.
    let segments = 10;
    let mut prev = origin + left;
    for i in 1..=segments {
        let t = i as f32 / segments as f32;
        let ang = -half_angle_rad + 2.0 * half_angle_rad * t;
        let pt = origin + rotate_y(aim, ang) * range;
        gizmos.line(prev, pt, color);
        prev = pt;
    }
}

/// Rectangle on the ground, length `length` along `aim`, total width
/// `half_w * 2`.
fn draw_line_rect(
    gizmos: &mut Gizmos,
    origin: Vec3,
    aim: Vec3,
    length: f32,
    half_w: f32,
    color: Color,
) {
    // Perpendicular in the XZ plane.
    let perp = Vec3::new(-aim.z, 0.0, aim.x);
    let p0 = origin + perp * half_w;
    let p1 = origin - perp * half_w;
    let p2 = p1 + aim * length;
    let p3 = p0 + aim * length;
    gizmos.line(p0, p1, color);
    gizmos.line(p1, p2, color);
    gizmos.line(p2, p3, color);
    gizmos.line(p3, p0, color);
}
