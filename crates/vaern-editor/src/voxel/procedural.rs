//! Editor-side bridge between the voxel chunk generator
//! ([`super::store::EditorHeightfield`]) and the shared
//! `vaern_cartography::WorldTerrain`.
//!
//! On `OnEnter(Editing)` the active zone's [`ZoneTerrain`] is built
//! and cached in a process-global `OnceLock`. Per-voxel sampling
//! reads it lock-free.
//!
//! The same `WorldTerrain`/`ZoneTerrain` types power the server +
//! client runtime resolver via
//! `vaern_cartography::install_terrain_resolver`. Editor + server +
//! client therefore share one elevation source by construction.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use bevy::prelude::*;
use vaern_cartography::ZoneTerrain;
use vaern_data::{load_all_geography, load_all_landmarks, load_world, load_world_layout};

use crate::state::EditorContext;
use crate::ui::console::ConsoleLog;

/// Single active-zone heightfield. The editor only edits one zone at
/// a time so a single cell is enough; matching the shape of
/// `WorldTerrain` would be wasteful for the other 27 zones.
static HEIGHTFIELD: OnceLock<ZoneTerrain> = OnceLock::new();

fn world_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src/generated/world")
}

/// Sample the active zone's procedural heightfield at world XZ.
/// Returns `None` if the zone hasn't been loaded yet (e.g. before
/// `OnEnter(Editing)`); the caller should fall back to a flat
/// baseline so chunks generated in that window stay flat until the
/// zone finishes loading.
#[inline]
pub fn sample(world_x: f32, world_z: f32) -> Option<f32> {
    let zone = HEIGHTFIELD.get()?;
    Some(zone.final_height(world_x, world_z))
}

/// Bevy system: build the active zone's [`ZoneTerrain`] and cache it.
/// Runs on `OnEnter(EditorAppState::Editing)` after
/// `seed_context_from_boot` has populated `EditorContext.active_zone`.
pub fn load_procedural_heightfield(ctx: Res<EditorContext>, mut log: ResMut<ConsoleLog>) {
    if HEIGHTFIELD.get().is_some() {
        return; // idempotent
    }
    let root = world_root();
    let world = match load_world(&root) {
        Ok(w) => w,
        Err(e) => {
            warn!("editor: load_world failed: {e}");
            log.push(format!("procedural heightfield: load_world FAILED: {e}"));
            return;
        }
    };
    let layout = match load_world_layout(&root) {
        Ok(l) => l,
        Err(e) => {
            warn!("editor: load_world_layout failed: {e}");
            log.push(format!("procedural heightfield: load_world_layout FAILED: {e}"));
            return;
        }
    };
    let landmarks = match load_all_landmarks(&root) {
        Ok(l) => l,
        Err(e) => {
            warn!("editor: load_all_landmarks failed: {e}");
            log.push(format!("procedural heightfield: load_all_landmarks FAILED: {e}"));
            return;
        }
    };
    let geography = match load_all_geography(&root) {
        Ok(g) => g,
        Err(e) => {
            warn!("editor: load_all_geography failed: {e}");
            log.push(format!("procedural heightfield: load_all_geography FAILED: {e}"));
            return;
        }
    };

    let zone_id = ctx.active_zone.clone();
    let Some(geo) = geography.get(&zone_id) else {
        log.push(format!(
            "procedural heightfield: no geography.yaml for {zone_id} (skipping)"
        ));
        return;
    };
    let Some(zt) = ZoneTerrain::build(&zone_id, &world, &layout, &landmarks, geo, &root) else {
        log.push(format!(
            "procedural heightfield: zone {zone_id} has no world.yaml placement"
        ));
        return;
    };

    let polygon_count = zt.index.polygons.len();
    let river_count = zt.index.rivers.len();
    let stamp_count = zt.index.stamps.len();
    let edit_count = zt.elevation_edits.len();
    if HEIGHTFIELD.set(zt).is_err() {
        warn!("editor: procedural heightfield already initialised — second load ignored");
        return;
    }
    log.push(format!(
        "procedural heightfield: zone {zone_id} ({polygon_count} polygons, {river_count} rivers, {stamp_count} stamps, {edit_count} edits)"
    ));
}
