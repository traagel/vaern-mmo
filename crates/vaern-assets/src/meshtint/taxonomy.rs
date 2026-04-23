//! Human-readable classification of Meshtint base-character piece-node
//! variants.
//!
//! Each base glTF ships costume variants as anonymous numbered mesh-nodes
//! (`02 Torso 13`, `03 Bottom 04`, …). This registry attaches a display
//! name + optional `kind` archetype + free-form tags to each variant, so:
//!
//! - The museum can label its outfit sliders with readable names.
//! - The game's item system can later map item archetypes onto specific
//!   piece-node variants via `kind` / `tags` filters (e.g. find all
//!   `plate` torsos or all `sleeves + cloth`).
//!
//! Load once at startup with [`MeshtintPieceTaxonomy::load_yaml`] and
//! insert as a Bevy resource. See
//! `assets/meshtint_piece_taxonomy.yaml` for the canonical file.

use std::collections::HashMap;
use std::path::Path;

use bevy::prelude::*;
use serde::Deserialize;
use thiserror::Error;

use super::{BodySlot, Gender};

/// One variant's classification.
#[derive(Clone, Debug, Deserialize)]
pub struct PieceTaxonomy {
    pub name: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Piece-node category on the Meshtint base character. Matches the
/// piece-node name prefixes (`02 Torso`, `03 Bottom`, `04 Feet`,
/// `05 Hand`, `06 Belt`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PieceCategory {
    Torso,
    Bottom,
    Feet,
    Hand,
    Belt,
}

#[derive(Default, Debug, Deserialize)]
pub struct GenderPieces {
    // --- Base piece-node categories (torso/bottom/feet/hand/belt variants
    //     live on the base character and toggle via Visibility). ---
    #[serde(default)]
    pub torso: HashMap<u32, PieceTaxonomy>,
    #[serde(default)]
    pub bottom: HashMap<u32, PieceTaxonomy>,
    #[serde(default)]
    pub feet: HashMap<u32, PieceTaxonomy>,
    #[serde(default)]
    pub hand: HashMap<u32, PieceTaxonomy>,
    #[serde(default)]
    pub belt: HashMap<u32, PieceTaxonomy>,

    // --- Body-overlay slots (separate GLB per variant, spawn as child
    //     entity of the character). ---
    #[serde(default)]
    pub hair: HashMap<u32, PieceTaxonomy>,
    #[serde(default)]
    pub beard: HashMap<u32, PieceTaxonomy>,
    #[serde(default)]
    pub brow: HashMap<u32, PieceTaxonomy>,
    #[serde(default)]
    pub eyes: HashMap<u32, PieceTaxonomy>,
    #[serde(default)]
    pub mouth: HashMap<u32, PieceTaxonomy>,
    #[serde(default)]
    pub helmet: HashMap<u32, PieceTaxonomy>,
    #[serde(default)]
    pub hat: HashMap<u32, PieceTaxonomy>,
    #[serde(default)]
    pub headband: HashMap<u32, PieceTaxonomy>,
    #[serde(default)]
    pub earring: HashMap<u32, PieceTaxonomy>,
    #[serde(default)]
    pub necklace: HashMap<u32, PieceTaxonomy>,
    #[serde(default)]
    pub pauldron: HashMap<u32, PieceTaxonomy>,
    #[serde(default)]
    pub bracer: HashMap<u32, PieceTaxonomy>,
    #[serde(default)]
    pub poleyn: HashMap<u32, PieceTaxonomy>,
}

impl GenderPieces {
    fn get(&self, category: PieceCategory, variant: u32) -> Option<&PieceTaxonomy> {
        let map = match category {
            PieceCategory::Torso => &self.torso,
            PieceCategory::Bottom => &self.bottom,
            PieceCategory::Feet => &self.feet,
            PieceCategory::Hand => &self.hand,
            PieceCategory::Belt => &self.belt,
        };
        map.get(&variant)
    }

    fn overlay_map(&self, slot: BodySlot) -> &HashMap<u32, PieceTaxonomy> {
        match slot {
            BodySlot::Hair => &self.hair,
            BodySlot::Beard => &self.beard,
            BodySlot::Brow => &self.brow,
            BodySlot::Eyes => &self.eyes,
            BodySlot::Mouth => &self.mouth,
            BodySlot::Helmet => &self.helmet,
            BodySlot::Hat => &self.hat,
            BodySlot::Headband => &self.headband,
            BodySlot::Earring => &self.earring,
            BodySlot::Necklace => &self.necklace,
            BodySlot::Pauldron => &self.pauldron,
            BodySlot::Bracer => &self.bracer,
            BodySlot::Poleyn => &self.poleyn,
        }
    }
}

#[derive(Default, Debug, Deserialize, Resource)]
pub struct MeshtintPieceTaxonomy {
    #[serde(default)]
    pub male: GenderPieces,
    #[serde(default)]
    pub female: GenderPieces,
}

impl MeshtintPieceTaxonomy {
    pub fn get(
        &self,
        gender: Gender,
        category: PieceCategory,
        variant: u32,
    ) -> Option<&PieceTaxonomy> {
        match gender {
            Gender::Male => self.male.get(category, variant),
            Gender::Female => self.female.get(category, variant),
        }
    }

    /// Display name for a base piece-node variant. Returns the
    /// YAML-authored name if present, `"None"` for variant 0 (the
    /// "hide this piece" slot), otherwise `"Variant NN"`.
    pub fn label(&self, gender: Gender, category: PieceCategory, variant: u32) -> String {
        match self.get(gender, category, variant) {
            Some(p) => p.name.clone(),
            None if variant == 0 => "None".to_string(),
            None => format!("Variant {:02}", variant),
        }
    }

    /// Overlay taxonomy for a `(gender, slot, variant)` lookup, or
    /// `None` if no YAML entry exists for it.
    pub fn overlay(
        &self,
        gender: Gender,
        slot: BodySlot,
        variant: u32,
    ) -> Option<&PieceTaxonomy> {
        let pieces = match gender {
            Gender::Male => &self.male,
            Gender::Female => &self.female,
        };
        pieces.overlay_map(slot).get(&variant)
    }

    /// Display name for an overlay variant. Same fallback semantics as
    /// [`Self::label`].
    pub fn overlay_label(&self, gender: Gender, slot: BodySlot, variant: u32) -> String {
        match self.overlay(gender, slot, variant) {
            Some(p) => p.name.clone(),
            None if variant == 0 => "None".to_string(),
            None => format!("Variant {:02}", variant),
        }
    }

    pub fn load_yaml<P: AsRef<Path>>(path: P) -> Result<Self, PieceTaxonomyLoadError> {
        let bytes = std::fs::read(path.as_ref())?;
        let tax: MeshtintPieceTaxonomy = serde_yaml::from_slice(&bytes)?;
        Ok(tax)
    }
}

#[derive(Debug, Error)]
pub enum PieceTaxonomyLoadError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("yaml: {0}")]
    Yaml(#[from] serde_yaml::Error),
}
