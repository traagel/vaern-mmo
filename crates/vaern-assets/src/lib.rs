//! Shared asset helpers for Bevy-based Vaern crates.
//!
//! Modules:
//!
//! - [`regions`] — [`NamedRegions`] component caches `Name → Entity`
//!   lookups after a scene spawns.
//!
//! - [`meshtint`] — full rendering pipeline for Meshtint Polygonal
//!   Fantasy Pack characters: base spawn, outfit piece-node visibility,
//!   rigid body overlays, bone-attached weapon overlays with
//!   YAML-calibrated grips, palette swap. Also hosts the shared
//!   animation pipeline (UAL clip catalog, graph, player installer).
//!
//! - [`quaternius`] — Quaternius Universal Base Characters + modular
//!   outfit integration. Characters spawn from pre-combined
//!   `{Gender}_{Outfit}.gltf` files and play UAL clips natively via
//!   the shared animation pipeline.
//!
//! Add [`VaernAssetsPlugin`] once at app init to register every
//! subsystem's `Update` schedule.

use bevy::prelude::*;

pub mod animals;
pub mod meshtint;
pub mod quaternius;
pub mod regions;

pub use animals::{AnimalCatalog, AnimalEntry};

pub use meshtint::*;
pub use quaternius::{
    outfit_from_equipped, spawn_quaternius_character, spawn_quaternius_weapon_overlays,
    weapon_props_from_equipped, AttachHand, Beard, ColorVariant as QuaterniusColor,
    EquippedProps, Hair as QuaterniusHair, HeadPiece, HeadSlot, MegakitCatalog, Outfit,
    OutfitColor, OutfitSlot, PropEntry, PropGrip, QuaterniusCharacter, QuaterniusGripSpec,
    QuaterniusGrips, QuaterniusOutfit, QuaterniusPlugin, QuaterniusWeaponOverlay,
};
pub use regions::{NamedRegions, RegionPlugin};

pub struct VaernAssetsPlugin;

impl Plugin for VaernAssetsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((RegionPlugin, MeshtintPlugin, QuaterniusPlugin));
    }
}
