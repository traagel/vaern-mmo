//! Place mode — drop the palette-selected prop at the cursor's
//! ground-snapped XZ.
//!
//! Flow on LMB click:
//! 1. Read [`SelectedPaletteSlug`] — bail if none.
//! 2. Cast cursor ray, intersect with the voxel/heightfield surface.
//! 3. Resolve the **nearest hub** in the active zone via
//!    [`ActiveZoneHubs::nearest`]. The new prop's hub_id is whichever
//!    capital/outpost is closest in 2D — it's the natural "you placed
//!    this near the keep" intent.
//! 4. Compute hub-local offset (world XZ minus hub world XZ) and
//!    spawn an `AuthoredProp` with that offset.
//!
//! Persisted by save: the spawned `EditorDressingEntity` carries
//! `hub_id` + the authored data, so the next Save serializes the new
//! prop into the right hub YAML.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::EguiContexts;
use vaern_assets::PolyHavenCatalog;
use vaern_data::{AuthoredProp, PropOffset};
use vaern_voxel::chunk::ChunkStore;

use super::{active_mode_is, EditorMode};
use crate::camera::ground_clamp::sample_ground_y;
use crate::camera::FreeFlyCamera;
use crate::dressing::spawn::spawn_one_prop;
use crate::state::EditorAppState;
use crate::ui::console::ConsoleLog;
use crate::ui::palette::SelectedPaletteSlug;
use crate::world::ActiveZoneHubs;

pub struct PlaceModePlugin;

impl Plugin for PlaceModePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            place_on_click
                .run_if(in_state(EditorAppState::Editing))
                .run_if(active_mode_is(EditorMode::Place)),
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub fn place_on_click(
    mut commands: Commands,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<FreeFlyCamera>>,
    asset_server: Res<AssetServer>,
    catalog: Res<PolyHavenCatalog>,
    palette: Res<SelectedPaletteSlug>,
    hubs: Res<ActiveZoneHubs>,
    store: Res<ChunkStore>,
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

    let Some(slug) = palette.slug() else {
        log.push("place: no palette slug selected");
        return;
    };
    let Some(entry) = catalog.get(slug) else {
        log.push(format!("place: catalog has no slug {slug}"));
        return;
    };

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

    // Project ray to ground-XZ via the analytic horizontal-plane trick:
    // step the ray downward until ground_y at that XZ matches the ray's
    // Y. Cheap binary intersection.
    let Some(world_pos) = ray_to_ground(&store, ray.origin, *ray.direction) else {
        log.push("place: ray missed ground");
        return;
    };

    let Some((hub_id, (hub_x, hub_z))) = hubs.nearest(world_pos.x, world_pos.z) else {
        log.push("place: no hub origins available (zone not loaded?)");
        return;
    };

    let prop = AuthoredProp {
        slug: slug.to_string(),
        offset: PropOffset {
            x: world_pos.x - hub_x,
            z: world_pos.z - hub_z,
        },
        rotation_y_deg: 0.0,
        scale: 1.0,
        absolute_y: None,
    };

    let entity = spawn_one_prop(
        &mut commands,
        &asset_server,
        entry,
        &prop,
        &hub_id,
        hub_x,
        hub_z,
    );
    log.push(format!(
        "placed {slug} in {hub_id} at offset ({:.1}, {:.1}) — entity {entity:?}",
        prop.offset.x, prop.offset.z
    ));
}

/// Bisection-style ray → ground-XZ intersection.
///
/// Walks the ray forward, comparing ray-Y to the terrain Y at each
/// step's XZ. When ray-Y crosses below terrain-Y we've passed the
/// surface; bisect the last bracket for sub-step precision.
///
/// Cheap and good enough for prop placement (props cluster around hub
/// centers; the heightmap is shallow in those regions). Doesn't need
/// to handle overhangs or carved caves.
fn ray_to_ground(store: &ChunkStore, origin: Vec3, dir: Vec3) -> Option<Vec3> {
    let dir = dir.normalize_or_zero();
    if dir.length_squared() < 1e-6 {
        return None;
    }
    let max_t = 600.0;
    let step = 2.0;
    let mut prev_t = 0.0;
    let mut prev_above: Option<bool> = None;
    let mut t = 0.0;
    while t <= max_t {
        let p = origin + dir * t;
        let ground = sample_ground_y(store, p.x, p.z);
        let above = p.y > ground;
        if let Some(prev) = prev_above {
            if prev && !above {
                // Crossed surface between prev_t and t; bisect.
                let mut lo = prev_t;
                let mut hi = t;
                for _ in 0..16 {
                    let mid = (lo + hi) * 0.5;
                    let pm = origin + dir * mid;
                    let g = sample_ground_y(store, pm.x, pm.z);
                    if pm.y > g {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                let p_final = origin + dir * hi;
                let ground_final = sample_ground_y(store, p_final.x, p_final.z);
                return Some(Vec3::new(p_final.x, ground_final, p_final.z));
            }
        }
        prev_above = Some(above);
        prev_t = t;
        t += step;
    }
    None
}
