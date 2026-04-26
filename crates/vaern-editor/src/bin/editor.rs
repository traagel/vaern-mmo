//! Standalone Vaern editor binary entrypoint.
//!
//! Parses CLI args via `clap`, builds a Bevy `App` with `DefaultPlugins`
//! + `EguiPlugin` + `EditorPlugin`, and runs.

use std::path::Path;

use bevy::prelude::*;
use bevy_egui::EguiPlugin;
use clap::Parser;
use vaern_editor::cli::EditorCli;
use vaern_editor::state::EditorBootConfig;
use vaern_editor::EditorPlugin;

fn main() {
    let cli = EditorCli::parse();
    let (width, height) = cli.window_size();
    let zone_id = cli.zone.clone();

    // Point the AssetServer at the workspace `assets/` folder so glTFs
    // (Poly Haven props, future Quaternius placeholders) resolve via
    // their relative paths — same convention as `vaern-client`.
    let asset_root_abs = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets")
        .canonicalize()
        .expect("workspace assets/ folder must exist");
    let asset_root = asset_root_abs.to_string_lossy().into_owned();

    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: format!("Vaern Editor — {zone_id}"),
                        resolution: (width, height).into(),
                        ..default()
                    }),
                    ..default()
                })
                .set(AssetPlugin {
                    file_path: asset_root,
                    ..default()
                }),
        )
        .add_plugins(EguiPlugin::default())
        .insert_resource(EditorBootConfig { zone_id })
        .insert_resource(ClearColor(Color::srgb(0.05, 0.07, 0.10)))
        .add_plugins(EditorPlugin)
        .run();
}
