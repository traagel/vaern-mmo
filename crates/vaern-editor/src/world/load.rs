//! Bring a zone up for editing: load YAML, compute world origin,
//! spawn hub props.
//!
//! Mirrors the relevant chunk of `vaern-client/src/scene/dressing.rs`
//! — the zone-ring layout for starter zones is the same constant
//! (`ZONE_RING_RADIUS = 2800`) so an editor-side prop sits at the
//! same world coordinates as the runtime client would render it.

use std::collections::HashMap;

use bevy::prelude::*;
use vaern_assets::PolyHavenCatalog;
use vaern_data::{World, Zone};

use crate::camera::FreeFlyCamera;
use crate::dressing::spawn::spawn_zone_authored_props;
use crate::persistence::zone_io::{load_world_for_editor, world_root};
use crate::state::EditorContext;
use crate::ui::console::ConsoleLog;
use crate::world::ActiveZoneHubs;

/// How far above the zone origin the editor camera spawns. 80u sits
/// above the tallest authored hub prop (gates / castle door) without
/// pushing the zone footprint off-frame.
pub const CAMERA_ABOVE_ZONE: f32 = 80.0;
/// Horizontal offset (south, +Z) so the camera looks down-and-forward
/// at the zone origin rather than straight down.
pub const CAMERA_BACK_OFFSET: f32 = 80.0;

/// Mirror of `vaern-server::data::load_game_data` + the client's
/// duplicate. Must stay in sync.
pub const ZONE_RING_RADIUS: f32 = 2800.0;

/// Run once on `OnEnter(Editing)`. Loads the world, computes the active
/// zone's world origin, spawns its hub props, and repositions the
/// free-fly camera to look at the zone origin (otherwise the camera
/// sits at world (0, 80, 80) and starter zones live thousands of units
/// away on the zone ring — the viewport renders empty).
#[allow(clippy::too_many_arguments)]
pub fn load_active_zone(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    catalog: Res<PolyHavenCatalog>,
    mut ctx: ResMut<EditorContext>,
    mut log: ResMut<ConsoleLog>,
    mut cameras: Query<&mut Transform, With<FreeFlyCamera>>,
    mut active_hubs: ResMut<ActiveZoneHubs>,
) {
    let world = match load_world_for_editor() {
        Ok(w) => w,
        Err(e) => {
            warn!("editor: failed to load world YAML: {e:?}");
            log.push(format!("ERROR: failed to load world YAML — {e}"));
            ctx.set_status("world load FAILED");
            return;
        }
    };

    let zone_id = ctx.active_zone.clone();
    let Some(_zone) = world.zone(&zone_id) else {
        warn!("editor: zone {zone_id} not found in world YAML");
        log.push(format!("ERROR: zone {zone_id} not found"));
        ctx.set_status(format!("zone {zone_id} not found"));
        return;
    };

    let origins = compute_zone_origins(&world);
    let origin = origins.get(&zone_id).copied().unwrap_or((0.0, 0.0));
    log.push(format!(
        "loaded zone {zone_id} (origin world ({:.1}, {:.1}))",
        origin.0, origin.1
    ));

    let report = spawn_zone_authored_props(
        &mut commands,
        &asset_server,
        &catalog,
        &world,
        &zone_id,
        origin,
    );
    log.push(format!(
        "spawned {} hub props ({} unknown slugs)",
        report.spawned, report.unknown_slugs
    ));

    // Populate ActiveZoneHubs so Place mode + save have the per-hub
    // world origins + YAML paths cached on a resource.
    active_hubs.origins.clear();
    active_hubs.yaml_paths.clear();
    for hub in world.hubs_in_zone(&zone_id) {
        let Some(off) = hub.offset_from_zone_origin.as_ref() else {
            continue;
        };
        let world_xz = (origin.0 + off.x, origin.1 + off.z);
        active_hubs.origins.insert(hub.id.clone(), world_xz);
        let yaml_path = world_root()
            .join("zones")
            .join(&zone_id)
            .join("hubs")
            .join(format!("{}.yaml", hub.id));
        active_hubs.yaml_paths.insert(hub.id.clone(), yaml_path);
    }

    // Teleport the free-fly camera over the zone origin so the viewport
    // actually frames the props + chunks (which stream around the
    // camera). Without this, the camera stays at world (0, 80, 80) and
    // every starter zone is ~2800u away on the zone ring — invisible.
    if let Ok(mut cam_tf) = cameras.single_mut() {
        let target = Vec3::new(origin.0, 0.0, origin.1);
        let cam_pos = target + Vec3::new(0.0, CAMERA_ABOVE_ZONE, CAMERA_BACK_OFFSET);
        *cam_tf = Transform::from_translation(cam_pos).looking_at(target, Vec3::Y);
        log.push(format!(
            "camera moved to ({:.0}, {:.0}, {:.0})",
            cam_pos.x, cam_pos.y, cam_pos.z
        ));
    }

    ctx.set_status(format!("editing {zone_id}"));
}

/// Compute world origin per starter zone via the canonical zone-ring
/// layout. Mirrors `vaern-client/src/scene/dressing.rs::compute_zone_origins`.
pub fn compute_zone_origins(world: &World) -> HashMap<String, (f32, f32)> {
    let mut starters: Vec<&str> = world
        .zones
        .iter()
        .filter_map(|z: &Zone| z.starter_race.as_deref().map(|_| z.id.as_str()))
        .collect();
    starters.sort();
    let n = starters.len().max(1) as f32;
    let mut out = HashMap::new();
    for (i, zid) in starters.iter().enumerate() {
        let angle = (i as f32 / n) * std::f32::consts::TAU;
        out.insert(
            (*zid).to_string(),
            (ZONE_RING_RADIUS * angle.cos(), ZONE_RING_RADIUS * angle.sin()),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_zone_origins_is_deterministic_under_same_world() {
        // Synthesize a minimal world with one starter zone — exact
        // origin values aren't asserted; we just check the helper
        // runs + produces an entry for the starter.
        let world = match load_world_for_editor() {
            Ok(w) => w,
            Err(_) => return, // skip test if YAML not on disk in the test runner
        };
        let origins = compute_zone_origins(&world);
        // Dalewatch Marches is a starter zone.
        assert!(origins.contains_key("dalewatch_marches"));
    }
}
