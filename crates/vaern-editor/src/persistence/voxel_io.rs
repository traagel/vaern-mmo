//! Editor-side wrappers around `vaern_voxel::persistence`.
//!
//! Editor save / load both target the workspace canonical path
//! `<workspace>/src/generated/world/voxel_edits.bin` so the runtime
//! server reads exactly the file the editor produces — the editor IS
//! the source of truth for terrain divergence from the heightfield.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use vaern_voxel::chunk::{ChunkStore, DirtyChunks};
use vaern_voxel::generator::HeightfieldGenerator;
use vaern_voxel::persistence::{
    apply_into_store, diff_against_generator, load_from_disk, save_to_disk,
};

use crate::ui::console::ConsoleLog;

/// Toolbar Save button writes `requested = true` here; the
/// [`drain_save_requests`] system performs the actual save next frame
/// when it has mutable access to the `ChunkStore`. Drained back to
/// `false` after the save runs.
#[derive(Resource, Debug, Default)]
pub struct SaveVoxelEditsRequested {
    pub requested: bool,
}

/// Workspace-canonical path. Lives next to `world.yaml` under
/// `src/generated/world/` so it ships in the same content tree the
/// runtime reads from.
pub fn voxel_edits_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../src/generated/world/voxel_edits.bin")
}

/// Save the editor's current chunk store to disk. Returns the on-disk
/// path on success and the count of saved chunks (those that diverged
/// from the heightfield baseline).
pub fn save_voxel_edits(store: &ChunkStore) -> anyhow::Result<(PathBuf, usize)> {
    let generator = HeightfieldGenerator::new();
    let deltas = diff_against_generator(store, &generator);
    let path = voxel_edits_path();
    save_to_disk(&path, &deltas)?;
    Ok((path, deltas.len()))
}

/// Bevy Startup system: read `voxel_edits.bin` (if present) and
/// replay its deltas into the editor's `ChunkStore` + `DirtyChunks`.
///
/// This runs after `VoxelCorePlugin` has inserted the resources and
/// before the streamer kicks in (Update schedule). The streamer's
/// `if store.contains(coord)` guard ensures we don't double-seed
/// chunks the load path has already populated.
pub fn load_voxel_edits_into_store(
    mut store: ResMut<ChunkStore>,
    mut dirty: ResMut<DirtyChunks>,
    mut log: ResMut<ConsoleLog>,
) {
    let path = voxel_edits_path();
    let deltas = match load_from_disk(&path) {
        Ok(d) => d,
        Err(e) => {
            warn!("editor: failed to load voxel_edits.bin: {e}");
            log.push(format!("voxel-edits load FAILED: {e}"));
            return;
        }
    };
    if deltas.is_empty() {
        info!("editor: no authored voxel edits on disk (file missing or empty)");
        return;
    }
    let generator = HeightfieldGenerator::new();
    let applied = apply_into_store(&deltas, &mut store, &mut dirty, &generator);
    info!("editor: replayed {applied} authored chunk edits from {path:?}");
    log.push(format!("loaded {applied} authored chunk edits"));
}

/// Drain the toolbar's save-button flag. Runs every frame in Update;
/// returns immediately when nothing's been requested.
///
/// Saves both:
/// 1. **Voxel deltas** to `world/voxel_edits.bin` (terrain changes).
/// 2. **Hub YAMLs** under `world/zones/<zone>/hubs/<hub>.yaml` (every
///    placed / moved / deleted dressing prop is written back to the
///    hub it belongs to). Hub YAMLs not represented in the live entity
///    world are left untouched, so an editor session that touched only
///    Dalewatch Keep won't disturb Harrier's Rest.
#[allow(clippy::too_many_arguments)]
pub fn drain_save_requests(
    mut req: ResMut<SaveVoxelEditsRequested>,
    store: Res<ChunkStore>,
    hubs: Res<crate::world::ActiveZoneHubs>,
    dressing_q: Query<&crate::dressing::EditorDressingEntity>,
    overrides: Res<crate::voxel::overrides::BiomeOverrideMap>,
    mut log: ResMut<ConsoleLog>,
) {
    if !req.requested {
        return;
    }
    req.requested = false;

    // 1. Voxel deltas.
    match save_voxel_edits(&store) {
        Ok((path, count)) => {
            info!("editor: saved {count} chunk deltas to {path:?}");
            log.push(format!(
                "saved {count} chunk deltas → {}",
                path.display()
            ));
        }
        Err(e) => {
            warn!("editor: voxel save failed: {e}");
            log.push(format!("VOXEL SAVE FAILED: {e}"));
        }
    }

    // 2. Hub YAMLs.
    match save_active_zone_hubs(&hubs, &dressing_q) {
        Ok(touched) => {
            info!("editor: saved {touched} hub yaml files");
            log.push(format!("saved {touched} hub yaml files"));
        }
        Err(e) => {
            warn!("editor: hub yaml save failed: {e}");
            log.push(format!("HUB YAML SAVE FAILED: {e}"));
        }
    }

    // 3. Biome overrides.
    let overrides_path = crate::voxel::overrides::biome_overrides_path();
    match crate::voxel::overrides::save_biome_overrides(&overrides_path, &overrides) {
        Ok(count) => {
            info!(
                "editor: saved {count} biome overrides to {overrides_path:?}"
            );
            log.push(format!("saved {count} biome overrides"));
        }
        Err(e) => {
            warn!("editor: biome override save failed: {e}");
            log.push(format!("BIOME OVERRIDE SAVE FAILED: {e}"));
        }
    }
}

/// Group every live dressing entity by its `hub_id`, write each hub's
/// `props:` array back to the source YAML. Returns the count of files
/// written.
fn save_active_zone_hubs(
    hubs: &crate::world::ActiveZoneHubs,
    dressing_q: &Query<&crate::dressing::EditorDressingEntity>,
) -> anyhow::Result<usize> {
    use std::collections::HashMap;
    use vaern_data::AuthoredProp;

    // Build hub_id → Vec<AuthoredProp> from the live world.
    let mut by_hub: HashMap<String, Vec<AuthoredProp>> = HashMap::new();
    for d in dressing_q.iter() {
        by_hub
            .entry(d.hub_id.clone())
            .or_default()
            .push(d.authored.clone());
    }

    // Also include hubs in `hubs.yaml_paths` that have no live entities,
    // so deletion-of-last-prop survives save: an empty `props: []` lands
    // in the YAML rather than the deleted props re-appearing on reload.
    for hub_id in hubs.yaml_paths.keys() {
        by_hub.entry(hub_id.clone()).or_default();
    }

    let mut touched = 0usize;
    for (hub_id, props) in by_hub.iter() {
        let Some(path) = hubs.yaml_paths.get(hub_id) else {
            // Prop was placed in a hub the editor doesn't have a path
            // for (shouldn't happen — Place mode only chooses from
            // hubs.origins, populated alongside yaml_paths). Skip.
            warn!("editor: no yaml path for hub {hub_id}; skipping save");
            continue;
        };
        super::zone_io::save_hub_props(path, props)?;
        touched += 1;
    }
    Ok(touched)
}
