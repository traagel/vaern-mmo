//! Runtime material swap — replace the Meshtint glTF's embedded
//! `DS Blue Gold.png` material with any of the 14 DS palette PNGs
//! shipped alongside the pack.

use bevy::prelude::*;

use super::MeshtintCharacter;

/// DS palette texture names shipped in the Polygonal Fantasy Pack
/// (without extension or path prefix). Each file lives at
/// `extracted/meshtint/palettes/{name}.png`.
pub const MESHTINT_DS_PALETTES: &[&str] = &[
    "blue_gold",
    "blue_silver",
    "brown_gold",
    "brown_silver",
    "green_gold",
    "green_silver",
    "grey_gold",
    "grey_silver",
    "purple_gold",
    "purple_silver",
    "red_gold",
    "red_silver",
    "white_gold",
    "white_silver",
];

/// Attach to a [`MeshtintCharacter`] entity to swap every mesh
/// primitive's material to the given [`StandardMaterial`].
///
/// Change-detected — reassigning the handle re-walks the character.
/// Removing the component does **not** revert to the original glTF
/// material (you'd need to respawn the scene); treat the override as
/// sticky once applied.
#[derive(Component, Clone, Debug)]
pub struct PaletteOverride(pub Handle<StandardMaterial>);

#[derive(Component)]
pub struct PaletteApplied;

pub fn apply_palette_override(
    mut commands: Commands,
    chars: Query<
        (Entity, &PaletteOverride),
        (
            With<MeshtintCharacter>,
            Or<(Changed<PaletteOverride>, Without<PaletteApplied>)>,
        ),
    >,
    children: Query<&Children>,
    meshes: Query<(), With<Mesh3d>>,
) {
    for (root, palette) in &chars {
        let mut stack = vec![root];
        let mut saw = false;
        while let Some(e) = stack.pop() {
            if meshes.contains(e) {
                commands
                    .entity(e)
                    .insert(MeshMaterial3d(palette.0.clone()));
                saw = true;
            }
            if let Ok(kids) = children.get(e) {
                for &c in kids {
                    stack.push(c);
                }
            }
        }
        if saw {
            commands.entity(root).insert(PaletteApplied);
        }
    }
}
