//! Quaternius weapon-overlay spawn pipeline.
//!
//! A [`QuaterniusWeaponOverlay`] is a spawn-request: an empty entity
//! carrying `{ prop_id, target }` that, once both the catalog and
//! the target character's [`NamedRegions`] have resolved, promotes
//! itself into a scene-root parented to the resolved hand bone with
//! the calibrated grip transform.
//!
//! Single-shot: once spawned, a [`OverlaySpawned`] marker gates
//! re-processing. To switch weapons, despawn the old overlay entity
//! and spawn a new one — mutating the [`QuaterniusWeaponOverlay`] in
//! place will **not** respawn.

use bevy::prelude::*;

use crate::meshtint::OverlaySpawned;
use crate::regions::NamedRegions;

use super::bones::{BONE_MAINHAND, BONE_OFFHAND};
use super::grips::{AttachHand, QuaterniusGrips};
use super::props::MegakitCatalog;

/// Spawn-request for a MEGAKIT prop parented to one of the character's
/// hand bones. The `target` is the Quaternius character parent entity
/// that owns `NamedRegions`; `prop_id` is the MEGAKIT basename
/// (e.g. `"Sword_Bronze"`).
///
/// Despawn + respawn this entity to switch weapons — it's a spawn
/// request, not a live reference.
#[derive(Component, Clone, Debug)]
pub struct QuaterniusWeaponOverlay {
    pub target: Entity,
    pub prop_id: String,
}

/// Resolve pending weapon overlays once the catalog + the target's
/// hand bones have been cached. Defers silently until both are ready.
pub fn spawn_quaternius_weapon_overlays(
    mut commands: Commands,
    assets: Res<AssetServer>,
    catalog: Option<Res<MegakitCatalog>>,
    grips: Option<Res<QuaterniusGrips>>,
    q: Query<(Entity, &QuaterniusWeaponOverlay), Without<OverlaySpawned>>,
    regions: Query<&NamedRegions>,
) {
    let Some(catalog) = catalog else { return };

    for (entity, overlay) in &q {
        // Grip lookup — falls back to mainhand + identity if the prop
        // isn't calibrated. Logs a one-time warn via the catalog miss
        // path below if the prop is wholly unknown to MEGAKIT.
        let (attach, spec) = grips
            .as_deref()
            .map(|g| g.lookup(&overlay.prop_id))
            .unwrap_or_default();

        let bone_name = match attach {
            AttachHand::Mainhand => BONE_MAINHAND,
            AttachHand::Offhand => BONE_OFFHAND,
        };

        // Defer until the character's NamedRegions walker has cached
        // the requested hand bone. First frames after spawn may miss
        // because the sub-scene hierarchies haven't loaded yet.
        let Ok(character_regions) = regions.get(overlay.target) else {
            continue;
        };
        let Some(bone) = character_regions.entity(bone_name) else {
            continue;
        };

        let Some(entry) = catalog.get(&overlay.prop_id) else {
            warn!("no MEGAKIT prop named {:?}", overlay.prop_id);
            commands.entity(entity).insert(OverlaySpawned);
            continue;
        };

        commands.entity(entity).insert((
            SceneRoot(assets.load(format!("{}#Scene0", entry.path))),
            spec.transform(),
            ChildOf(bone),
            OverlaySpawned,
        ));
    }
}
