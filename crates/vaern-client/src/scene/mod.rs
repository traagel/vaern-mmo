//! 3D gameplay scene — cameras, world setup, mesh rendering, and
//! animation driving.
//!
//! Split across five focused submodules, each exposing its own
//! `Plugin`:
//!
//! - [`setup`]    — menu + gameplay camera spawn, sun light,
//!   `teardown_game` tied to `AppState::InGame`, menu-camera background.
//! - [`ground`]   — large flat ground plane + player-centered gizmo
//!   grid overlay for spatial reference.
//! - [`camera`]   — orbit/follow camera, mouse-look input, cursor
//!   lock/release. Owns [`CameraController`].
//! - [`render`]   — entity → visual mesh mapping. NPCs get a simple
//!   blue cuboid; predicted + interpolated players get a Quaternius
//!   modular character, kept in sync with gear + cosmetic state.
//! - [`animation`] — `AnimState` → UAL clip driving for own + remote
//!   players, plus the [`CastFiredLocal`] message relay that many other
//!   modules subscribe to.
//!
//! External consumers of scene-internal types use `crate::scene::*`.
//! Only [`CameraController`], [`CameraSet`] and [`CastFiredLocal`] leak
//! outside the module — everything else is intra-scene plumbing.

mod animation;
mod camera;
mod ground;
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
            ground::GroundPlugin,
            camera::CameraPlugin,
            render::RenderPlugin,
            animation::AnimationPlugin,
        ));
    }
}
