//! Editor mode stack.
//!
//! Exactly one mode is "active" at a time. Modes are inert by default
//! in V1 — the toolbar can switch between them but only the [`select`]
//! mode is the wired default; the rest log a "not implemented" status
//! when activated.
//!
//! Each mode is implemented as its own `Plugin` whose systems run only
//! while [`ActiveMode`] matches that mode.
//!
//! # Adding a real mode
//!
//! 1. Create `modes/<my_mode>.rs`.
//! 2. Implement a `Plugin` that gates its systems on
//!    `.run_if(active_mode_is(EditorMode::MyMode))`.
//! 3. Register the plugin under `ModeStackPlugin::build`.
//! 4. Wire a button into `ui::toolbar` and a [`bindings::EditorAction`]
//!    in `input::bindings`.

pub mod biome_paint;
pub mod place;
pub mod scatter_preview;
pub mod select;
pub mod voxel_brush;

use bevy::prelude::*;

use crate::input::bindings::{EditorAction, EditorActionState};
use crate::state::{EditorAppState, EditorContext};

/// Discriminated mode identifier. The active value lives in
/// [`ActiveMode`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum EditorMode {
    #[default]
    Select,
    Place,
    VoxelBrush,
    BiomePaint,
    ScatterPreview,
}

impl EditorMode {
    /// Short label for toolbar buttons + status line.
    pub fn label(self) -> &'static str {
        match self {
            Self::Select => "Select",
            Self::Place => "Place",
            Self::VoxelBrush => "Voxel Brush",
            Self::BiomePaint => "Biome Paint",
            Self::ScatterPreview => "Scatter Preview",
        }
    }

    /// All variants in declaration order. Used by the toolbar to render
    /// one button per mode.
    pub fn all() -> [EditorMode; 5] {
        [
            Self::Select,
            Self::Place,
            Self::VoxelBrush,
            Self::BiomePaint,
            Self::ScatterPreview,
        ]
    }

    /// True iff this mode is fully implemented in V1. The toolbar grays
    /// out unimplemented buttons + the activator falls through to a
    /// status-line "not implemented" notice.
    pub fn is_implemented(self) -> bool {
        matches!(self, Self::Select | Self::VoxelBrush | Self::Place)
    }
}

/// Active mode. Mode plugins gate their systems on this resource.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct ActiveMode(pub EditorMode);

/// Run-condition helper: true while `ActiveMode` matches `mode`.
pub fn active_mode_is(mode: EditorMode) -> impl FnMut(Res<ActiveMode>) -> bool + Clone {
    move |active: Res<ActiveMode>| active.0 == mode
}

/// Aggregator plugin — registers every mode plugin + the keybind
/// handler that swaps `ActiveMode`.
pub struct ModeStackPlugin;

impl Plugin for ModeStackPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActiveMode>()
            .add_plugins((
                select::SelectModePlugin,
                place::PlaceModePlugin,
                voxel_brush::VoxelBrushModePlugin,
                biome_paint::BiomePaintModePlugin,
                scatter_preview::ScatterPreviewModePlugin,
            ))
            .add_systems(
                Update,
                handle_mode_hotkeys.run_if(in_state(EditorAppState::Editing)),
            );
    }
}

fn handle_mode_hotkeys(
    actions: Res<EditorActionState>,
    mut active: ResMut<ActiveMode>,
    mut ctx: ResMut<EditorContext>,
) {
    let mapping = [
        (EditorAction::SelectMode, EditorMode::Select),
        (EditorAction::PlaceMode, EditorMode::Place),
        (EditorAction::VoxelBrushMode, EditorMode::VoxelBrush),
        (EditorAction::BiomePaintMode, EditorMode::BiomePaint),
        (EditorAction::ScatterPreviewMode, EditorMode::ScatterPreview),
    ];

    for (action, mode) in mapping {
        if actions.just_pressed(action) {
            switch_mode(&mut active, &mut ctx, mode);
        }
    }
}

/// Public switch helper — also called by the toolbar's mode buttons.
pub fn switch_mode(active: &mut ActiveMode, ctx: &mut EditorContext, mode: EditorMode) {
    if active.0 == mode {
        return;
    }
    active.0 = mode;
    if mode.is_implemented() {
        ctx.set_status(format!("mode: {}", mode.label()));
    } else {
        ctx.set_status(format!("mode: {} — not implemented yet", mode.label()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_mode_all_returns_each_variant() {
        let all = EditorMode::all();
        assert_eq!(all.len(), 5);
        assert!(all.contains(&EditorMode::Select));
        assert!(all.contains(&EditorMode::ScatterPreview));
    }

    #[test]
    fn select_place_voxel_brush_are_implemented() {
        assert!(EditorMode::Select.is_implemented());
        assert!(EditorMode::VoxelBrush.is_implemented());
        assert!(EditorMode::Place.is_implemented());
        for m in [EditorMode::BiomePaint, EditorMode::ScatterPreview] {
            assert!(!m.is_implemented());
        }
    }

    #[test]
    fn switch_mode_updates_status_implemented() {
        let mut active = ActiveMode::default();
        let mut ctx = EditorContext::default();
        switch_mode(&mut active, &mut ctx, EditorMode::VoxelBrush);
        assert_eq!(active.0, EditorMode::VoxelBrush);
        assert!(ctx.status.contains("Voxel Brush"));
        assert!(!ctx.status.contains("not implemented"));
    }

    #[test]
    fn switch_mode_unimplemented_flags_status() {
        let mut active = ActiveMode::default();
        let mut ctx = EditorContext::default();
        switch_mode(&mut active, &mut ctx, EditorMode::BiomePaint);
        assert_eq!(active.0, EditorMode::BiomePaint);
        assert!(ctx.status.contains("not implemented"));
    }
}
