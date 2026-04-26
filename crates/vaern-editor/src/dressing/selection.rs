//! Selection state for dressing entities.
//!
//! Picking strategy (best → fallback):
//!
//! 1. **Scene mesh AABB** — for each `EditorDressingEntity`, walk its
//!    `Children` hierarchy collecting every descendant `Aabb`, transform
//!    each by its `GlobalTransform` into world space, take the union.
//!    Ray-vs-AABB picks the prop whose box the cursor ray actually
//!    enters. Handles big stretched props (castle door, iron gate,
//!    fort wall) correctly.
//! 2. **Point-radius fallback** — when scene children haven't loaded
//!    yet (glTF parses async), fall back to perpendicular distance
//!    from the ray to the prop's translation. Radius is small (3u)
//!    so small props remain selectable; once their scene loads, the
//!    AABB path takes over.

use bevy::camera::primitives::Aabb;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::EguiContexts;

use crate::camera::FreeFlyCamera;
use crate::modes::{active_mode_is, EditorMode};
use crate::state::EditorAppState;
use crate::ui::console::ConsoleLog;

use super::EditorDressingEntity;

/// Fallback radius — used only when the prop's scene children haven't
/// loaded yet (no AABB available). Small to avoid false positives.
pub const PICK_RADIUS: f32 = 3.0;
/// Max ray distance to search. Mirrors the brush mode.
pub const PICK_RAY_MAX_DIST: f32 = 500.0;

/// Currently-selected dressing entity. `None` = nothing selected.
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct SelectedProp(pub Option<Entity>);

impl SelectedProp {
    pub fn is_some(&self) -> bool {
        self.0.is_some()
    }

    pub fn clear(&mut self) {
        self.0 = None;
    }

    pub fn set(&mut self, entity: Entity) {
        self.0 = Some(entity);
    }
}

pub struct SelectionPlugin;

impl Plugin for SelectionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SelectedProp>().add_systems(
            Update,
            (
                pick_on_click
                    .run_if(in_state(EditorAppState::Editing))
                    .run_if(active_mode_is(EditorMode::Select)),
                draw_selection_gizmo.run_if(in_state(EditorAppState::Editing)),
                clear_stale_selection.run_if(in_state(EditorAppState::Editing)),
            ),
        );
    }
}

/// LMB in Select mode → cast ray, pick the prop whose mesh-AABB the
/// ray enters (or, if AABB isn't ready, the closest within
/// [`PICK_RADIUS`]). Empty hit clears selection.
#[allow(clippy::too_many_arguments)]
pub fn pick_on_click(
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<FreeFlyCamera>>,
    props: Query<(Entity, &Transform, &EditorDressingEntity)>,
    children_q: Query<&Children>,
    aabb_q: Query<(&GlobalTransform, &Aabb)>,
    mut selected: ResMut<SelectedProp>,
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
    let Ok(ray) = cam.viewport_to_world(cam_xform, cursor) else {
        return;
    };

    let pick = pick_closest_prop(ray.origin, *ray.direction, &props, &children_q, &aabb_q);

    match pick {
        Some((entity, slug, hub_id)) => {
            selected.set(entity);
            log.push(format!("selected {slug} (hub {hub_id})"));
        }
        None => {
            if selected.is_some() {
                log.push("deselected (clicked empty space)");
            }
            selected.clear();
        }
    }
}

/// Ray vs every prop → closest hit by ray-t. Returns the entity + a
/// snapshot of its slug + hub_id (cloned so the caller doesn't have to
/// keep the query borrow alive while logging / mutating).
fn pick_closest_prop(
    ray_origin: Vec3,
    ray_dir: Vec3,
    props: &Query<(Entity, &Transform, &EditorDressingEntity)>,
    children_q: &Query<&Children>,
    aabb_q: &Query<(&GlobalTransform, &Aabb)>,
) -> Option<(Entity, String, String)> {
    let mut best: Option<(Entity, &EditorDressingEntity, f32)> = None;
    let radius_sq = PICK_RADIUS * PICK_RADIUS;

    for (entity, tf, dressing) in props.iter() {
        // Try the mesh-AABB path first. World-space union of every
        // descendant's AABB.
        let aabb_hit = scene_world_aabb(entity, children_q, aabb_q)
            .and_then(|(min, max)| ray_aabb_intersection(ray_origin, ray_dir, min, max));

        // Fallback: perpendicular distance to translation point.
        let radius_hit = (|| {
            let to_prop = tf.translation - ray_origin;
            let t = to_prop.dot(ray_dir);
            if t <= 0.0 || t > PICK_RAY_MAX_DIST {
                return None;
            }
            let closest_on_ray = ray_origin + ray_dir * t;
            let perp_sq = (closest_on_ray - tf.translation).length_squared();
            (perp_sq <= radius_sq).then_some(t)
        })();

        let t = match (aabb_hit, radius_hit) {
            (Some(a), _) => a,
            (None, Some(r)) => r,
            (None, None) => continue,
        };
        if t > PICK_RAY_MAX_DIST {
            continue;
        }
        if best.map_or(true, |(_, _, bt)| t < bt) {
            best = Some((entity, dressing, t));
        }
    }
    best.map(|(e, d, _)| (e, d.slug.clone(), d.hub_id.clone()))
}

/// Walk an entity's `Children` hierarchy, collecting every descendant
/// `Aabb`, transforming each by its `GlobalTransform` into world
/// space, returning the union AABB. Returns `None` when no descendant
/// has rendered yet (scene still loading).
fn scene_world_aabb(
    root: Entity,
    children_q: &Query<&Children>,
    aabb_q: &Query<(&GlobalTransform, &Aabb)>,
) -> Option<(Vec3, Vec3)> {
    let mut union: Option<(Vec3, Vec3)> = None;
    let mut stack: Vec<Entity> = vec![root];
    while let Some(e) = stack.pop() {
        if let Ok((gtf, aabb)) = aabb_q.get(e) {
            let (lmin, lmax) = aabb_world_bounds(aabb, gtf);
            union = Some(match union {
                None => (lmin, lmax),
                Some((u_min, u_max)) => (u_min.min(lmin), u_max.max(lmax)),
            });
        }
        if let Ok(children) = children_q.get(e) {
            stack.extend(children.iter());
        }
    }
    union
}

/// Transform a local-space `Aabb` by a `GlobalTransform` into a world-
/// space (min, max) bound. Walks the 8 corners through the matrix and
/// re-bounds — the returned box may be larger than necessary when the
/// transform has rotation, which is correct for a conservative ray
/// test.
fn aabb_world_bounds(aabb: &Aabb, gtf: &GlobalTransform) -> (Vec3, Vec3) {
    let c = Vec3::from(aabb.center);
    let h = Vec3::from(aabb.half_extents);
    let mat = gtf.to_matrix();
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for sx in [-1.0, 1.0] {
        for sy in [-1.0, 1.0] {
            for sz in [-1.0, 1.0] {
                let local = c + Vec3::new(h.x * sx, h.y * sy, h.z * sz);
                let world = mat.transform_point3(local);
                min = min.min(world);
                max = max.max(world);
            }
        }
    }
    (min, max)
}

/// Slab-method ray-vs-AABB. Returns the entry-t (≥ 0) on hit, `None`
/// otherwise. `dir` should be unit-length but we don't require it —
/// the returned `t` is in `dir`-space units.
fn ray_aabb_intersection(origin: Vec3, dir: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
    let inv = Vec3::new(
        if dir.x.abs() > 1e-8 { 1.0 / dir.x } else { f32::INFINITY * dir.x.signum().max(0.0) },
        if dir.y.abs() > 1e-8 { 1.0 / dir.y } else { f32::INFINITY * dir.y.signum().max(0.0) },
        if dir.z.abs() > 1e-8 { 1.0 / dir.z } else { f32::INFINITY * dir.z.signum().max(0.0) },
    );
    let t1 = (min - origin) * inv;
    let t2 = (max - origin) * inv;
    let tmin = t1.min(t2);
    let tmax = t1.max(t2);
    let entry = tmin.x.max(tmin.y).max(tmin.z);
    let exit = tmax.x.min(tmax.y).min(tmax.z);
    if exit >= entry && exit >= 0.0 {
        Some(entry.max(0.0))
    } else {
        None
    }
}

/// Cyan wireframe AABB around the selected entity, drawn each frame.
/// Falls back to a fallback-radius sphere if the prop's scene mesh
/// hasn't loaded yet.
pub fn draw_selection_gizmo(
    selected: Res<SelectedProp>,
    props: Query<&Transform, With<EditorDressingEntity>>,
    children_q: Query<&Children>,
    aabb_q: Query<(&GlobalTransform, &Aabb)>,
    mut gizmos: Gizmos,
) {
    let Some(entity) = selected.0 else { return };
    let Ok(tf) = props.get(entity) else { return };
    let color = Color::srgb(0.35, 0.85, 0.95);
    if let Some((min, max)) = scene_world_aabb(entity, &children_q, &aabb_q) {
        let center = (min + max) * 0.5;
        let size = max - min;
        gizmos.cube(
            Transform::from_translation(center).with_scale(size),
            color,
        );
    } else {
        gizmos.sphere(tf.translation, PICK_RADIUS, color);
    }
}

/// If the selected entity got despawned (delete button) the resource
/// can hold a stale Entity id. This system clears the stale handle.
pub fn clear_stale_selection(
    mut selected: ResMut<SelectedProp>,
    props: Query<&EditorDressingEntity>,
) {
    let Some(entity) = selected.0 else { return };
    if props.get(entity).is_err() {
        selected.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_radius_is_positive() {
        assert!(PICK_RADIUS > 0.0);
    }
}

