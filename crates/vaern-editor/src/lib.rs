//! Vaern map editor — in-engine authoring tool for zone YAML, hub
//! props, voxel terrain, biome painting, and scatter-rule preview.
//!
//! Standalone Bevy binary, sibling of `vaern-client`. Reuses the
//! `vaern-voxel` edit pipeline, `vaern-data` zone schemas, and
//! `vaern-assets` Poly Haven catalog. **No** networking — every edit is
//! client-local; persistence is YAML write-back.
//!
//! # V1 baseline
//!
//! The crate currently ships a viewer: free-fly camera over a chosen
//! zone with chunks streaming and authored hub props visible.
//! Editing modes are stubbed (file slots reserved, plugins inert).
//!
//! # Module map
//!
//! | module          | role                                                  |
//! |-----------------|-------------------------------------------------------|
//! | [`camera`]      | free-fly camera (WASD + Q/E + RMB-look + scroll-speed)|
//! | [`input`]       | EditorAction bindings + egui focus guard              |
//! | [`modes`]       | mode stack (select / place / brush / paint / scatter) |
//! | [`voxel`]       | client-local chunk store + streaming                  |
//! | [`dressing`]    | hub-prop spawn + selection / gizmo (stubbed)          |
//! | [`ui`]          | toolbar / palette / inspector / console               |
//! | [`persistence`] | zone YAML load + (V2) save                            |
//! | [`world`]       | bring a zone up for editing + teardown                |
//! | [`state`]       | `EditorContext` resource + active-zone tracking       |
//! | [`cli`]         | `clap` argv: `--zone`, `--window-size`                |

pub mod camera;
pub mod cli;
pub mod dressing;
pub mod input;
pub mod modes;
pub mod persistence;
pub mod state;
pub mod ui;
pub mod voxel;
pub mod world;

use bevy::prelude::*;

/// Top-level plugin. Aggregates every editor subsystem.
///
/// Add this to a Bevy `App` after `DefaultPlugins` and `EguiPlugin`.
/// The editor binary at `bin/editor.rs` is the canonical entrypoint;
/// this plugin is also valid to embed in another harness for tests.
pub struct EditorPlugin;

impl Plugin for EditorPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            state::EditorStatePlugin,
            input::EditorInputPlugin,
            camera::FreeFlyCameraPlugin,
            world::EditorWorldPlugin,
            voxel::EditorVoxelPlugin,
            dressing::EditorDressingPlugin,
            ui::EditorUiPlugin,
            persistence::EditorPersistencePlugin,
            modes::ModeStackPlugin,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke test — the plugin builds without panicking on a minimal
    /// Bevy app. Catches the "missing schedule / unregistered system
    /// param" class of bugs at `cargo test` time.
    ///
    /// `StatesPlugin` is required because `EditorStatePlugin` uses
    /// `init_state` and `MinimalPlugins` doesn't include it (Bevy
    /// `DefaultPlugins` does, but DefaultPlugins requires a windowing
    /// backend that is not available in the test harness).
    #[test]
    fn editor_plugin_builds() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(bevy::state::app::StatesPlugin);
        app.add_plugins(EditorPlugin);
    }
}
