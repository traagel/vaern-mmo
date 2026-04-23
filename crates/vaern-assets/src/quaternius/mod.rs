//! Quaternius Universal Base Characters + modular outfit integration.
//!
//! Each full character is one or more `.gltf` files under
//! `assets/extracted/characters/outfits/` — Quaternius packages modular
//! parts (body / legs / arms / feet / head piece) on the UE Mannequin
//! armature. Weapons are sourced from the Fantasy Props MEGAKIT under
//! `assets/extracted/props/` and attached to `hand_r` / `hand_l` via
//! the [`overlay::QuaterniusWeaponOverlay`] pipeline.
//!
//! Because the armature matches the Universal Animation Library
//! skeleton exactly, UAL clips drive these characters natively (no
//! retargeting). See [`crate::meshtint::animation`] for the animation
//! pipeline — it handles Quaternius characters via
//! [`AnimatedRig`]`(Rig::QuaterniusModular)`.

use bevy::prelude::*;

pub mod bones;
pub mod character;
pub mod grips;
pub mod overlay;
pub mod props;
pub mod resolve;

pub use bones::{BONE_MAINHAND, BONE_OFFHAND};
pub use character::{
    spawn_quaternius_character, Beard, ColorVariant, Hair, HeadPiece, HeadSlot, HideNonHeadRegions,
    Outfit, OutfitColor, OutfitSlot, QuaterniusCharacter, QuaterniusOutfit,
};
pub use grips::{AttachHand, PropGrip, QuaterniusGrips, QuaterniusGripSpec};
pub use overlay::{spawn_quaternius_weapon_overlays, QuaterniusWeaponOverlay};
pub use props::{MegakitCatalog, PropEntry};
pub use resolve::{outfit_from_equipped, weapon_props_from_equipped, EquippedProps};

pub struct QuaterniusPlugin;

impl Plugin for QuaterniusPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                character::hide_non_head_regions,
                character::apply_outfit_color,
                overlay::spawn_quaternius_weapon_overlays,
            ),
        );
    }
}
