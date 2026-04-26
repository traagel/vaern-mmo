//! Spawn hub props from `AuthoredProp` lists.
//!
//! Pure helper — invoked by `world::load` when a zone is brought up.
//! Mirrors `vaern-client/src/scene/dressing.rs::spawn_one` but tagged
//! with [`EditorDressingEntity`] for editor-side teardown queries.
//!
//! Y-snap policy: prop's authored `absolute_y` wins; otherwise sample
//! `vaern_core::terrain::height` at the prop's XZ. The voxel store
//! probably hasn't been seeded by the time spawn runs (chunks stream
//! around the camera over multiple frames), so we use the analytical
//! heightmap deliberately.

use bevy::prelude::*;
use vaern_assets::{PolyHavenCatalog, PolyHavenEntry};
use vaern_core::terrain;
use vaern_data::{AuthoredProp, Hub, World};

use super::EditorDressingEntity;
use crate::world::markers::EditorWorld;

/// Spawn every authored prop for every hub in the active zone. Returns
/// the count spawned + the count of unknown-slug warnings emitted.
pub fn spawn_zone_authored_props(
    commands: &mut Commands,
    asset_server: &AssetServer,
    catalog: &PolyHavenCatalog,
    world: &World,
    zone_id: &str,
    zone_origin: (f32, f32),
) -> SpawnReport {
    let mut report = SpawnReport::default();
    for hub in world.hubs_in_zone(zone_id) {
        spawn_hub_props(
            commands,
            asset_server,
            catalog,
            hub,
            zone_origin,
            &mut report,
        );
    }
    info!(
        "editor dressing: zone {zone_id} spawned {} hub-prop instances ({} unknown slugs)",
        report.spawned, report.unknown_slugs
    );
    report
}

fn spawn_hub_props(
    commands: &mut Commands,
    asset_server: &AssetServer,
    catalog: &PolyHavenCatalog,
    hub: &Hub,
    zone_origin: (f32, f32),
    report: &mut SpawnReport,
) {
    let Some(off) = hub.offset_from_zone_origin.as_ref() else {
        return;
    };
    let hub_x = zone_origin.0 + off.x;
    let hub_z = zone_origin.1 + off.z;
    for prop in &hub.props {
        let Some(entry) = catalog.get(&prop.slug) else {
            warn!(
                "editor dressing: hub {} references unknown slug {:?}",
                hub.id, prop.slug
            );
            report.unknown_slugs += 1;
            continue;
        };
        spawn_one_prop(commands, asset_server, entry, prop, &hub.id, hub_x, hub_z);
        report.spawned += 1;
    }
}

/// Spawn a single prop. Public so place-mode can call it directly with
/// an ad-hoc `AuthoredProp` constructed from a cursor click.
///
/// `hub_id` + `(hub_x, hub_z)` parameterize where the prop "belongs" —
/// the world position lands at `(hub_x + prop.offset.x, …, hub_z +
/// prop.offset.z)`, and the hub id is recorded on the
/// `EditorDressingEntity` component so save can write it back to the
/// right hub YAML.
pub fn spawn_one_prop(
    commands: &mut Commands,
    asset_server: &AssetServer,
    entry: &PolyHavenEntry,
    prop: &AuthoredProp,
    hub_id: &str,
    hub_x: f32,
    hub_z: f32,
) -> Entity {
    let world_x = hub_x + prop.offset.x;
    let world_z = hub_z + prop.offset.z;
    let y = prop
        .absolute_y
        .unwrap_or_else(|| terrain::height(world_x, world_z));

    let mut transform = Transform::from_translation(Vec3::new(world_x, y, world_z))
        .with_scale(Vec3::splat(prop.scale));
    transform.rotation = Quat::from_rotation_y(prop.rotation_y_deg.to_radians());

    commands
        .spawn((
            SceneRoot(asset_server.load(entry.scene_path())),
            transform,
            EditorDressingEntity {
                slug: entry.slug.clone(),
                hub_id: hub_id.to_string(),
                authored: prop.clone(),
            },
            EditorWorld,
            Name::new(format!("EditorDressing:{}", entry.slug)),
        ))
        .id()
}

/// Counts emitted by `spawn_zone_authored_props`.
#[derive(Debug, Default, Clone, Copy)]
pub struct SpawnReport {
    pub spawned: usize,
    pub unknown_slugs: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_report_default_is_zero() {
        let r = SpawnReport::default();
        assert_eq!(r.spawned, 0);
        assert_eq!(r.unknown_slugs, 0);
    }
}
