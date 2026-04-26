//! Editor mode stack.

pub mod biome_paint;
pub mod place;
pub mod scatter_preview;
pub mod select;
pub mod voxel_brush;

use bevy::prelude::*;

use crate::input::bindings::{EditorAction, EditorActionState};
use crate::state::{EditorAppState, EditorContext};

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
    pub fn label(self) -> &'static str {
        match self {
            Self::Select => "Select",
            Self::Place => "Place",
            Self::VoxelBrush => "Voxel Brush",
            Self::BiomePaint => "Biome Paint",
            Self::ScatterPreview => "Scatter Preview",
        }
    }

    pub fn all() -> [EditorMode; 5] {
        [
            Self::Select,
            Self::Place,
            Self::VoxelBrush,
            Self::BiomePaint,
            Self::ScatterPreview,
        ]
    }

    pub fn is_implemented(self) -> bool {
        matches!(
            self,
            Self::Select | Self::VoxelBrush | Self::Place | Self::BiomePaint
        )
    }
}

#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct ActiveMode(pub EditorMode);

pub fn active_mode_is(mode: EditorMode) -> impl FnMut(Res<ActiveMode>) -> bool + Clone {
    move |active: Res<ActiveMode>| active.0 == mode
}

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
    fn select_place_voxel_brush_biome_paint_are_implemented() {
        assert!(EditorMode::Select.is_implemented());
        assert!(EditorMode::VoxelBrush.is_implemented());
        assert!(EditorMode::Place.is_implemented());
        assert!(EditorMode::BiomePaint.is_implemented());
        assert!(!EditorMode::ScatterPreview.is_implemented());
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
        switch_mode(&mut active, &mut ctx, EditorMode::ScatterPreview);
        assert_eq!(active.0, EditorMode::ScatterPreview);
        assert!(ctx.status.contains("not implemented"));
    }
}
