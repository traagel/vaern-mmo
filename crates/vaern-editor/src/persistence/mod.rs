//! Editor persistence — read + (V2) write zone YAML.
//!
//! V1 reads via `vaern_data::load_world` (already wired in
//! `world::load`). The save path is stubbed: `zone_io::save_zone`
//! exists but writes a `*.editor.yaml` sidecar in the user cache dir
//! and logs that in-place rewrites are not implemented.
//!
//! [`atomic`] holds a working `write_atomic` helper for V2 to reuse.
//! [`dirty`] is a placeholder for the dirty-tracking bookkeeping.

pub mod atomic;
pub mod dirty;
pub mod voxel_io;
pub mod zone_io;

use bevy::prelude::*;

pub struct EditorPersistencePlugin;

impl Plugin for EditorPersistencePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<dirty::DirtyZones>()
            .init_resource::<voxel_io::SaveVoxelEditsRequested>()
            // Load authored voxel edits as soon as the chunk store is
            // initialized. Runs after vaern-voxel's `VoxelCorePlugin`
            // (which `EditorVoxelPlugin` registers), so the resources
            // exist by the time we touch them.
            .add_systems(Startup, voxel_io::load_voxel_edits_into_store)
            // Save when the toolbar flips the request flag. Doesn't
            // need a state-gate — runs before Editing too if someone
            // somehow flips the flag during boot, which is harmless.
            .add_systems(Update, voxel_io::drain_save_requests);
    }
}
