//! Editor "world" — the lifecycle that brings a zone up for editing
//! and tears it down on switch / exit.
//!
//! `Startup` runs `load::load_active_zone` once after the boot config
//! has been read. The system reads the active `EditorContext.active_zone`,
//! pulls the world YAML, and spawns hub props + a sun light.

pub mod load;
pub mod markers;

use bevy::prelude::*;
use std::collections::HashMap;
use vaern_assets::PolyHavenCatalog;

use crate::state::EditorAppState;

/// World-space origins for every hub of the **active zone**. Populated
/// by [`load::load_active_zone`]; read by Place mode (cursor → nearest
/// hub) and the save path. Empty before a zone is loaded.
#[derive(Resource, Debug, Default)]
pub struct ActiveZoneHubs {
    /// `hub_id → (world_x, world_z)`.
    pub origins: HashMap<String, (f32, f32)>,
    /// Path to each hub's YAML file, for save write-back.
    pub yaml_paths: HashMap<String, std::path::PathBuf>,
}

impl ActiveZoneHubs {
    /// Find the hub closest (in 2D Euclidean) to a world XZ. Returns
    /// the hub id + its world origin. `None` if the table is empty.
    pub fn nearest(&self, world_x: f32, world_z: f32) -> Option<(String, (f32, f32))> {
        self.origins
            .iter()
            .min_by(|a, b| {
                let da = sqr_dist((world_x, world_z), *a.1);
                let db = sqr_dist((world_x, world_z), *b.1);
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(id, origin)| (id.clone(), *origin))
    }
}

#[inline]
fn sqr_dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    let dx = a.0 - b.0;
    let dz = a.1 - b.1;
    dx * dx + dz * dz
}

/// Plugin: registers the catalog resource (V1 owns it directly so the
/// editor binary doesn't have to wire it manually) and the
/// load-on-enter-Editing system.
pub struct EditorWorldPlugin;

impl Plugin for EditorWorldPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(PolyHavenCatalog::new())
            .init_resource::<ActiveZoneHubs>()
            .add_systems(OnEnter(EditorAppState::Editing), load::load_active_zone)
            .add_systems(Startup, spawn_sun);
    }
}

/// Single directional light tagged with `EditorSun` so the
/// `environment::apply_environment` driver can rotate / re-color it
/// from the time-of-day setting. Initial transform is overwritten on
/// the first frame of `Update`.
fn spawn_sun(mut commands: Commands) {
    commands.spawn((
        DirectionalLight {
            illuminance: 100_000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::IDENTITY.looking_to(Vec3::new(0.0, -1.0, 0.0), Vec3::Z),
        markers::EditorWorld,
        markers::EditorSun,
        Name::new("EditorSun"),
    ));
}
