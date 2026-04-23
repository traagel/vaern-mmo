//! Meshtint Polygonal Fantasy Pack integration for Bevy.
//!
//! The pack ships humanoid characters as a single base FBX per gender,
//! with every costume variant baked as a sibling mesh-node under one
//! armature (Unity selectively enables one variant per slot). Rigid
//! overlay GLBs (hair, helmets, weapons, …) attach to the base's rig.
//!
//! This module owns the full rendering pipeline:
//!
//! | Concern                         | Item                           |
//! |---------------------------------|--------------------------------|
//! | Mannequin spawn                 | [`MeshtintCharacterBundle`]    |
//! | Piece-node visibility           | [`OutfitPieces`] + plugin      |
//! | Rigid body overlays             | [`BodyOverlay`] + plugin       |
//! | Bone-attached weapons / shields | [`WeaponOverlay`] + plugin     |
//! | Per-variant grip calibration    | [`WeaponGrips`] (YAML)         |
//! | Asset-file enumeration          | [`MeshtintCatalog`]            |
//! | Palette swap                    | [`PaletteOverride`]            |
//! | `Mesh3d` visibility trap fix    | [`EnsureVisibility`] + plugin  |
//!
//! # Quick start
//!
//! ```ignore
//! App::new()
//!     .add_plugins(VaernAssetsPlugin)   // includes MeshtintPlugin
//!     .add_systems(Startup, |
//!         mut commands: Commands,
//!         assets: Res<AssetServer>,
//!     | {
//!         commands.insert_resource(MeshtintCatalog::scan("assets"));
//!         commands.insert_resource(
//!             WeaponGrips::load_yaml("assets/meshtint_weapon_grips.yaml")
//!                 .unwrap_or_default(),
//!         );
//!
//!         let character = commands
//!             .spawn(MeshtintCharacterBundle::new(&assets, Gender::Male))
//!             .id();
//!
//!         commands.entity(character).insert(OutfitPieces {
//!             torso: 3,
//!             bottom: 5,
//!             ..default()
//!         });
//!
//!         commands.spawn(BodyOverlay {
//!             target: character,
//!             gender: Gender::Male,
//!             slot: BodySlot::Helmet,
//!             variant: 2,
//!         });
//!         commands.spawn(WeaponOverlay {
//!             target: character,
//!             category: "Sword".into(),
//!             variant: 1,
//!         });
//!     });
//! ```

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

pub mod animation;
pub mod catalog;
pub mod character;
pub mod grips;
pub mod outfit;
pub mod overlay;
pub mod palette;
pub mod taxonomy;
pub mod visibility;

pub use animation::{
    AnimatedRig, AnimationClipSrc, AnimationPlayerInstalled, MeshtintAnimationCatalog,
    MeshtintAnimations, Rig, ANIM_FOLDER_REL, MESHTINT_ANIM_FOLDER_REL,
};
pub use catalog::{MeshtintCatalog, Variant, WEAPON_CATEGORIES};
pub use character::{MeshtintCharacter, MeshtintCharacterBundle};
pub use grips::{AttachBone, CategoryGrip, GripSpec, WeaponGrips, WeaponGripsLoadError};
pub use outfit::{OutfitApplied, OutfitPieces};
pub use overlay::{BodyOverlay, BodySlot, OverlaySpawned, WeaponOverlay};
pub use palette::{PaletteOverride, MESHTINT_DS_PALETTES};
pub use taxonomy::{
    GenderPieces, MeshtintPieceTaxonomy, PieceCategory, PieceTaxonomy, PieceTaxonomyLoadError,
};

// --- Rig bone names -------------------------------------------------------

/// Right-hand grip bone on Meshtint's humanoid rig. Mainhand weapons
/// (swords, axes, bows, staves, tools, …) parent here.
pub const BONE_MAINHAND: &str = "RigRPalm";
/// Left-hand grip bone. Shields and other offhand items parent here.
pub const BONE_OFFHAND: &str = "RigLPalm";
/// Back / upper-spine attach point. Placeholder target for quivers —
/// not yet calibrated.
pub const BONE_BACK: &str = "RigSpine3";

// --- Per-gender piece-node variant counts --------------------------------

pub const TORSO_MAX_M: u32 = 18;
pub const TORSO_MAX_F: u32 = 20;
pub const BOTTOM_MAX_M: u32 = 20;
pub const BOTTOM_MAX_F: u32 = 21;
pub const FEET_MAX: u32 = 6;
pub const HAND_MAX: u32 = 4;
pub const BELT_MAX: u32 = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Gender {
    Male,
    Female,
}

impl Gender {
    pub const ALL: &'static [Gender] = &[Gender::Male, Gender::Female];

    /// Asset-server path for a specific base variant. E.g.
    /// `(Male, 1) → "extracted/meshtint/male/Male_01.glb"`.
    pub fn base_asset_path(self, variant: u32) -> String {
        match self {
            Gender::Male => format!("extracted/meshtint/male/Male_{:02}.glb", variant),
            Gender::Female => format!("extracted/meshtint/female/Female_{:02}.glb", variant),
        }
    }

    /// Filesystem prefix used to discover base variants in the catalog.
    pub fn base_file_prefix(self) -> &'static str {
        match self {
            Gender::Male => "Male",
            Gender::Female => "Female",
        }
    }

    pub fn folder_rel(self) -> &'static str {
        match self {
            Gender::Male => "extracted/meshtint/male",
            Gender::Female => "extracted/meshtint/female",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Gender::Male => "Male",
            Gender::Female => "Female",
        }
    }

    pub fn torso_max(self) -> u32 {
        match self {
            Gender::Male => TORSO_MAX_M,
            Gender::Female => TORSO_MAX_F,
        }
    }

    pub fn bottom_max(self) -> u32 {
        match self {
            Gender::Male => BOTTOM_MAX_M,
            Gender::Female => BOTTOM_MAX_F,
        }
    }
}

// --- Plugin ---------------------------------------------------------------

pub struct MeshtintPlugin;

impl Plugin for MeshtintPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<animation::MeshtintAnimations>()
            .add_systems(Startup, animation::load_animation_sources)
            .add_systems(
                Update,
                (
                    outfit::apply_outfit_visibility,
                    overlay::spawn_body_overlays,
                    overlay::spawn_weapon_overlays,
                    overlay::fix_mirrored_overlay_normals,
                    palette::apply_palette_override,
                    visibility::apply_visibility_fix,
                    animation::build_animation_graph,
                    animation::install_character_animation_player,
                ),
            );
    }
}

// --- Helpers --------------------------------------------------------------

/// Predicate: does this node name look like a Meshtint piece-node?
/// (two digits + space prefix; matches `"02 Torso 05"` but not bone or
/// armature nodes like `"RigSpine1"`.)
pub(crate) fn is_piece_node(name: &str) -> bool {
    let b = name.as_bytes();
    b.len() >= 3 && b[0].is_ascii_digit() && b[1].is_ascii_digit() && b[2] == b' '
}
