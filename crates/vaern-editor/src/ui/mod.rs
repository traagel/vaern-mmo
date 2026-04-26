//! Editor egui UI.
//!
//! Layout in V1:
//!
//! ```text
//! ┌─────────────── toolbar (TopPanel) ────────────────────────┐
//! │ [Select] [Place] [Brush] [Paint] [Scatter]   zone  FPS    │
//! ├─────────┬─────────────────────────────┬─────────────────┤
//! │ palette │      camera viewport         │   inspector     │
//! │ (Left)  │       (CentralPanel)         │     (Right)     │
//! ├─────────┴─────────────────────────────┴─────────────────┤
//! │ console (BottomPanel) — last status line + scroll log    │
//! └──────────────────────────────────────────────────────────┘
//! ```
//!
//! Each panel is its own module + system so individual panels can be
//! gated independently in V2 (e.g. an `F2`-toggle for the inspector).

pub mod console;
pub mod environment_panel;
pub mod inspector;
pub mod palette;
pub mod theme;
pub mod toolbar;

use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;

use crate::state::EditorAppState;

/// Aggregator plugin — registers each panel system.
pub struct EditorUiPlugin;

impl Plugin for EditorUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<console::ConsoleLog>()
            .init_resource::<palette::SelectedPaletteSlug>()
            .add_systems(Startup, theme::apply_editor_theme)
            .add_systems(
                EguiPrimaryContextPass,
                (
                    toolbar::draw_toolbar,
                    environment_panel::draw_environment_panel,
                    palette::draw_palette,
                    inspector::draw_inspector,
                    console::draw_console,
                )
                    .run_if(in_state(EditorAppState::Editing)),
            );
    }
}
