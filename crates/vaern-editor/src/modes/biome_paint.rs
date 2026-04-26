//! Biome-paint mode — paint biome-tag values onto chunk-aligned tiles
//! so the voxel ground picks up new materials on next remesh.
//!
//! V1: stub. Open questions before this becomes real:
//!
//! * Today, biome assignment is global per chunk via
//!   `BiomeResolver::biome_at(cx, cz)` — a nearest-hub lookup. To
//!   "paint" a biome, the editor either (a) overrides the resolver via
//!   a `BiomeOverrideMap` keyed on chunk coord, or (b) writes back a
//!   per-zone biome-mask file. Option (a) keeps the runtime resolver
//!   intact; option (b) is the one that survives the editor exit.
//! * Brush size is pixel-perfect at chunk granularity (32u). A
//!   sub-chunk paint would need a per-fragment biome shader (not in
//!   today's pipeline).
//!
//! Decision deferred to the slice that actually ships this mode.

use bevy::prelude::*;

use super::{active_mode_is, EditorMode};
use crate::state::EditorAppState;

pub struct BiomePaintModePlugin;

impl Plugin for BiomePaintModePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            tick_stub
                .run_if(in_state(EditorAppState::Editing))
                .run_if(active_mode_is(EditorMode::BiomePaint)),
        );
    }
}

fn tick_stub() {
    // TODO(editor): chunk-tile painter, per-zone biome-override map.
}
