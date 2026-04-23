//! Outfit-piece selection and per-frame visibility toggling.
//!
//! Meshtint bases ship every costume variant as a sibling mesh-node
//! under one armature (`"02 Torso 01"` … `"02 Torso 18"` for the male
//! base, etc). At runtime we toggle `Visibility` on each piece-node so
//! that exactly one variant per category (Torso / Bottom / Feet / Hand /
//! Belt) is visible. The head node is always on.
//!
//! [`OutfitPieces`] on a [`MeshtintCharacter`] entity drives the choice;
//! the [`apply_outfit_visibility`] system applies it whenever the value
//! differs from the last successful apply recorded in [`OutfitApplied`].

use bevy::prelude::*;

use super::{is_piece_node, BELT_MAX, FEET_MAX, HAND_MAX, MeshtintCharacter};

/// Which numbered variant (1-based) to display for each piece-node
/// category. Clamped to the gender's available range on apply.
///
/// Defaults to all `1`s, which is a valid combination for both genders.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutfitPieces {
    pub torso: u32,
    pub bottom: u32,
    pub feet: u32,
    pub hand: u32,
    pub belt: u32,
}

impl Default for OutfitPieces {
    fn default() -> Self {
        Self { torso: 1, bottom: 1, feet: 1, hand: 1, belt: 1 }
    }
}

/// Records the most recently *successfully applied* outfit so the system
/// can distinguish "scene still loading, retry" from "no change, skip".
/// Inserted automatically by [`apply_outfit_visibility`].
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutfitApplied(pub OutfitPieces);

pub(super) fn apply_outfit_visibility(
    mut commands: Commands,
    chars: Query<(
        Entity,
        &MeshtintCharacter,
        &OutfitPieces,
        Option<&OutfitApplied>,
    )>,
    children: Query<&Children>,
    names: Query<&Name>,
    meshes: Query<(), With<Mesh3d>>,
) {
    for (root, character, wanted, applied) in &chars {
        if applied.map_or(false, |a| a.0 == *wanted) {
            continue;
        }
        let g = character.gender;
        let allowed = [
            "01 Head 01".to_string(),
            format!("02 Torso {:02}", wanted.torso.min(g.torso_max())),
            format!("03 Bottom {:02}", wanted.bottom.min(g.bottom_max())),
            format!("04 Feet {:02}", wanted.feet.min(FEET_MAX)),
            format!("05 Hand {:02}", wanted.hand.min(HAND_MAX)),
            format!("06 Belt {:02}", wanted.belt.min(BELT_MAX)),
        ];

        let mut stack = vec![root];
        let mut touched_pieces = 0usize;
        let mut touched_meshes = 0usize;
        while let Some(e) = stack.pop() {
            if let Ok(name) = names.get(e) {
                if is_piece_node(name.as_str()) {
                    let keep = allowed.iter().any(|w| w == name.as_str());
                    commands.entity(e).insert(if keep {
                        Visibility::Visible
                    } else {
                        Visibility::Hidden
                    });
                    touched_pieces += 1;
                }
            }
            // Force every `Mesh3d` descendant to `Visibility::Inherited`.
            // Unconditional — Bevy's glTF loader may spawn primitives
            // with `Visibility::Hidden` baked in (or with no Visibility
            // at all); either way we want them to inherit from the
            // piece-node parent we just toggled. This is the load-bearing
            // fix: without it, meshes that the glTF loader created with
            // an explicit Visibility would be stuck at their authored
            // value and ignore our parent-level toggle.
            if meshes.contains(e) {
                commands.entity(e).insert(Visibility::Inherited);
                touched_meshes += 1;
            }
            if let Ok(kids) = children.get(e) {
                for &c in kids {
                    stack.push(c);
                }
            }
        }

        if touched_pieces > 0 {
            debug!(
                "outfit applied {:?}: {} piece-nodes, {} mesh primitives",
                wanted, touched_pieces, touched_meshes
            );
            commands.entity(root).insert(OutfitApplied(*wanted));
        }
    }
}
