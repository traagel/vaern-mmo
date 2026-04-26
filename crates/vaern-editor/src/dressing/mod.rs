//! Hub-prop dressing — load + render `AuthoredProp` lists for the
//! active zone, mirroring `vaern-client/src/scene/dressing.rs`.
//!
//! V1 implements the **read** path: at world-load time, every hub's
//! `props:` array spawns Poly Haven scenes at the right world position
//! / rotation / scale. Selection, transform gizmos, and snap remain
//! stubbed.
//!
//! Editor entities tagged with [`EditorDressingEntity`] so a future
//! teardown / zone-switch system can despawn them by query.

pub mod selection;
pub mod snap;
pub mod spawn;
pub mod transform_gizmo;

use bevy::prelude::*;
use vaern_data::AuthoredProp;

/// Component carried by every spawned hub-prop entity. Mirrors the
/// authored YAML data so save can reconstruct the hub's `props:` list
/// directly from the live entity world without going back to disk.
///
/// Mutating editor systems (place / move / rotate / scale) update both
/// the entity's `Transform` *and* this component's `authored` field so
/// they stay in lockstep.
#[derive(Component, Debug, Clone)]
pub struct EditorDressingEntity {
    /// Poly Haven catalog slug — duplicated from `authored.slug` for
    /// fast queries that don't need the full prop record.
    pub slug: String,
    /// Hub id this prop belongs to. Stable across moves: a prop you
    /// created in Dalewatch Keep stays a "Keep prop" even if you drag
    /// it across the zone — it just reads as far-away dressing.
    pub hub_id: String,
    /// Authored prop record. Mirror of the YAML row.
    pub authored: AuthoredProp,
}

/// Plugin — registers selection state + the gizmo (stubbed) systems.
/// Spawn happens via `world::load`, not via this plugin's startup,
/// because spawn needs the world YAML which is only known after the
/// `world` module loads it.
pub struct EditorDressingPlugin;

impl Plugin for EditorDressingPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            selection::SelectionPlugin,
            transform_gizmo::TransformGizmoPlugin,
        ));
    }
}
