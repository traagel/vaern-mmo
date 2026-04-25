//! 3D gameplay scene — cameras, world setup, mesh rendering, and
//! animation driving.
//!
//! Split across focused submodules, each exposing its own `Plugin`:
//!
//! - [`setup`]    — menu + gameplay camera spawn, sun light,
//!   `teardown_game` tied to `AppState::InGame`, menu-camera background.
//! - [`camera`]   — orbit/follow camera, mouse-look input, cursor
//!   lock/release. Owns [`CameraController`].
//! - [`render`]   — entity → visual mesh mapping. NPCs get a simple
//!   blue cuboid; predicted + interpolated players get a Quaternius
//!   modular character, kept in sync with gear + cosmetic state.
//! - [`animation`] — `AnimState` → UAL clip driving for own + remote
//!   players, plus the [`CastFiredLocal`] message relay that many other
//!   modules subscribe to.
//!
//! The ground itself is provided by [`crate::voxel_demo`] — a chunked
//! SDF voxel world streamed around the camera, replacing the legacy
//! tessellated grass plane.
//!
//! [`hub_regions`] (Voronoi biome floor patches + wiggly roads) was
//! the original overlay on top of the flat ground plane. It's fully
//! superseded by `crate::voxel_biomes` + `crate::voxel_demo`, which
//! bake biome materials directly onto voxel chunks via per-biome
//! `StandardMaterial` + world-XZ UVs. The file is kept as source-level
//! reference for the Catmull-Rom road work (not yet ported).
//!
//! External consumers of scene-internal types use `crate::scene::*`.
//! Only [`CameraController`], [`CameraSet`] and [`CastFiredLocal`] leak
//! outside the module — everything else is intra-scene plumbing.

mod animation;
mod camera;
mod dressing;
#[allow(dead_code)]
mod hub_regions;
mod render;
mod setup;

use bevy::prelude::*;

pub use animation::CastFiredLocal;
pub use camera::{CameraController, CameraSet};

/// Composite plugin that registers every scene subplugin. Installed once
/// from `main.rs`; each submodule owns its own system registration.
pub struct ScenePlugin;

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            setup::SetupPlugin,
            camera::CameraPlugin,
            render::RenderPlugin,
            animation::AnimationPlugin,
            dressing::DressingPlugin,
        ));
    }
}
