//! The Meshtint mannequin itself: base GLB + skeleton + region cache.

use bevy::prelude::*;

use crate::regions::NamedRegions;

use super::animation::{AnimatedRig, Rig};
use super::outfit::OutfitPieces;
use super::{Gender, BONE_BACK, BONE_MAINHAND, BONE_OFFHAND};

/// Character root marker. Carries gender + base-variant number so queries
/// can distinguish mannequins.
#[derive(Component, Clone, Copy, Debug)]
pub struct MeshtintCharacter {
    pub gender: Gender,
    /// `NN` in `{Gender}_NN.glb`. The Polygonal Fantasy Pack 1.4 ships
    /// `_01` only; future base-variant packs extend this.
    pub base_variant: u32,
}

/// Spawn a Meshtint mannequin. Includes:
/// - [`MeshtintCharacter`] with gender + base variant
/// - Default [`OutfitPieces`] (every piece-node variant = 1)
/// - [`SceneRoot`] loading the gendered base GLB
/// - [`NamedRegions`] expecting the three hand/back bones
///
/// The Bevy-0.18 `Mesh3d` visibility fix runs globally via
/// [`super::visibility::apply_visibility_fix`] — no per-character marker
/// needed.
///
/// ```ignore
/// let char = commands
///     .spawn(MeshtintCharacterBundle::new(&assets, Gender::Male, 1))
///     .id();
/// commands.entity(char).insert(OutfitPieces { torso: 5, ..default() });
/// ```
#[derive(Bundle)]
pub struct MeshtintCharacterBundle {
    pub character: MeshtintCharacter,
    pub outfit: OutfitPieces,
    pub scene: SceneRoot,
    pub transform: Transform,
    pub regions: NamedRegions,
    pub rig: AnimatedRig,
}

impl MeshtintCharacterBundle {
    pub fn new(assets: &AssetServer, gender: Gender, base_variant: u32) -> Self {
        Self {
            character: MeshtintCharacter { gender, base_variant },
            outfit: OutfitPieces::default(),
            scene: SceneRoot(
                assets.load(format!("{}#Scene0", gender.base_asset_path(base_variant))),
            ),
            transform: Transform::default(),
            regions: NamedRegions::expect(&[BONE_MAINHAND, BONE_OFFHAND, BONE_BACK]),
            rig: AnimatedRig(Rig::Meshtint),
        }
    }
}
