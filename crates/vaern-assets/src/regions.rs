//! Named-region caching for spawned scenes.
//!
//! Attach a [`NamedRegions`] component to a scene root (or any entity with a
//! loading scene as a descendant) together with the list of node names you
//! want to find. The [`RegionPlugin`] walks the entity tree each frame until
//! every requested name is cached; consumers then use [`NamedRegions::entity`]
//! to grab the mesh entity for visibility / material overrides / child spawns.
//!
//! ```ignore
//! commands.spawn((
//!     SceneRoot(asset_server.load("character_split.gltf#Scene0")),
//!     Transform::default(),
//!     NamedRegions::expect(&[
//!         "Region_Head", "Region_Torso",
//!         "Region_LeftArm", "Region_RightArm",
//!         "Region_LeftLeg", "Region_RightLeg",
//!     ]),
//! ));
//!
//! // Later:
//! if let Some(e) = regions.entity("Region_Torso") {
//!     commands.entity(e).insert(Visibility::Hidden);
//! }
//! ```

use bevy::prelude::*;
use std::collections::HashMap;

pub struct RegionPlugin;

impl Plugin for RegionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, resolve_region_bindings);
    }
}

/// List of expected node names + lazily-populated cache of the entities
/// Bevy spawned for each match.
#[derive(Component, Default)]
pub struct NamedRegions {
    names: Vec<&'static str>,
    cache: HashMap<&'static str, Entity>,
}

impl NamedRegions {
    /// Ask the plugin to locate each of these node names inside the attached
    /// entity's descendant tree. Names must be `&'static` — they identify
    /// glTF nodes authored at build time.
    pub fn expect(names: &[&'static str]) -> Self {
        Self {
            names: names.to_vec(),
            cache: HashMap::new(),
        }
    }

    /// True once every expected name has been resolved.
    pub fn is_ready(&self) -> bool {
        self.cache.len() == self.names.len()
    }

    /// Resolved entity for `name`, if the plugin has found it yet.
    pub fn entity(&self, name: &str) -> Option<Entity> {
        self.cache.get(name).copied()
    }

    /// Iterate `(name, entity)` pairs for every resolved region.
    pub fn iter(&self) -> impl Iterator<Item = (&'static str, Entity)> + '_ {
        self.cache.iter().map(|(n, e)| (*n, *e))
    }
}

fn resolve_region_bindings(
    mut q: Query<(Entity, &mut NamedRegions)>,
    children: Query<&Children>,
    names: Query<&Name>,
) {
    for (root, mut regions) in &mut q {
        if regions.is_ready() {
            continue;
        }
        // Collect names still missing; walk the tree once, bucket hits.
        let missing: Vec<&'static str> = regions
            .names
            .iter()
            .copied()
            .filter(|n| !regions.cache.contains_key(n))
            .collect();
        if missing.is_empty() {
            continue;
        }
        walk(root, &missing, &children, &names, &mut regions.cache);
    }
}

fn walk(
    entity: Entity,
    missing: &[&'static str],
    children: &Query<&Children>,
    names: &Query<&Name>,
    cache: &mut HashMap<&'static str, Entity>,
) {
    if let Ok(name) = names.get(entity) {
        let s = name.as_str();
        for &want in missing {
            if s == want {
                cache.insert(want, entity);
                break;
            }
        }
    }
    if let Ok(kids) = children.get(entity) {
        for &c in kids {
            walk(c, missing, children, names, cache);
        }
    }
}
