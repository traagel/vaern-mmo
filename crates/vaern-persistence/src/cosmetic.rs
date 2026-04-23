//! Cosmetic enum ⇄ string-tag converters + on-disk cosmetic struct.
//!
//! The Quaternius picker enums (`Outfit`, `HeadPiece`, `Hair`, `Beard`,
//! `ColorVariant`) are defined in `vaern-assets` without serde derives on
//! purpose — we don't want a variant reorder to silently re-map on-disk
//! values. Tags here are stable lowercase snake_case strings. Adding a new
//! enum variant forces a compile error in the forward `*_tag` match; adding
//! a new tag on disk that matches no variant parses to `None` and the load
//! path logs + drops it.

use serde::{Deserialize, Serialize};
use vaern_assets::meshtint::Gender;
use vaern_assets::quaternius::{
    Beard, ColorVariant, Hair, HeadPiece, HeadSlot, Outfit, OutfitSlot, QuaterniusOutfit,
};

// ---------------------------------------------------------------------------
// Forward conversion (enum → tag)
// ---------------------------------------------------------------------------

pub fn outfit_tag(o: Outfit) -> &'static str {
    match o {
        Outfit::Peasant => "peasant",
        Outfit::Ranger => "ranger",
        Outfit::Noble => "noble",
        Outfit::Knight => "knight",
        Outfit::KnightCloth => "knight_cloth",
        Outfit::Wizard => "wizard",
    }
}

pub fn head_piece_tag(h: HeadPiece) -> &'static str {
    match h {
        HeadPiece::KnightArmet => "knight_armet",
        HeadPiece::KnightHorns => "knight_horns",
        HeadPiece::NobleCrown => "noble_crown",
        HeadPiece::RangerHood => "ranger_hood",
    }
}

pub fn hair_tag(h: Hair) -> &'static str {
    match h {
        Hair::SimpleParted => "simple_parted",
        Hair::Long => "long",
        Hair::Buzzed => "buzzed",
        Hair::Buns => "buns",
    }
}

pub fn beard_tag(b: Beard) -> &'static str {
    match b {
        Beard::Full => "full",
    }
}

pub fn color_tag(c: ColorVariant) -> &'static str {
    match c {
        ColorVariant::Default => "v1",
        ColorVariant::V2 => "v2",
        ColorVariant::V3 => "v3",
    }
}

// ---------------------------------------------------------------------------
// Reverse conversion (tag → enum). None = unknown tag; caller decides
// whether to drop, warn, or fall back.
// ---------------------------------------------------------------------------

pub fn outfit_from_tag(tag: &str) -> Option<Outfit> {
    Some(match tag {
        "peasant" => Outfit::Peasant,
        "ranger" => Outfit::Ranger,
        "noble" => Outfit::Noble,
        "knight" => Outfit::Knight,
        "knight_cloth" => Outfit::KnightCloth,
        "wizard" => Outfit::Wizard,
        _ => return None,
    })
}

pub fn head_piece_from_tag(tag: &str) -> Option<HeadPiece> {
    Some(match tag {
        "knight_armet" => HeadPiece::KnightArmet,
        "knight_horns" => HeadPiece::KnightHorns,
        "noble_crown" => HeadPiece::NobleCrown,
        "ranger_hood" => HeadPiece::RangerHood,
        _ => return None,
    })
}

pub fn hair_from_tag(tag: &str) -> Option<Hair> {
    Some(match tag {
        "simple_parted" => Hair::SimpleParted,
        "long" => Hair::Long,
        "buzzed" => Hair::Buzzed,
        "buns" => Hair::Buns,
        _ => return None,
    })
}

pub fn beard_from_tag(tag: &str) -> Option<Beard> {
    Some(match tag {
        "full" => Beard::Full,
        _ => return None,
    })
}

pub fn color_from_tag(tag: &str) -> Option<ColorVariant> {
    Some(match tag {
        "v1" => ColorVariant::Default,
        "v2" => ColorVariant::V2,
        "v3" => ColorVariant::V3,
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// On-disk / on-the-wire cosmetic pick
// ---------------------------------------------------------------------------

/// Per-slot outfit pick (body / legs / arms / feet).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PersistedOutfitSlot {
    pub outfit: String,
    pub color: String,
}

impl PersistedOutfitSlot {
    pub fn from_slot(slot: OutfitSlot) -> Self {
        Self {
            outfit: outfit_tag(slot.outfit).to_string(),
            color: color_tag(slot.color).to_string(),
        }
    }

    /// `Err(&str)` is the tag that failed to resolve.
    pub fn to_slot(&self) -> Result<OutfitSlot, &str> {
        let outfit = outfit_from_tag(&self.outfit).ok_or(self.outfit.as_str())?;
        let color = color_from_tag(&self.color).ok_or(self.color.as_str())?;
        Ok(OutfitSlot::new(outfit, color))
    }
}

/// Head-piece pick.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PersistedHeadSlot {
    pub piece: String,
    pub color: String,
}

impl PersistedHeadSlot {
    pub fn from_slot(slot: HeadSlot) -> Self {
        Self {
            piece: head_piece_tag(slot.piece).to_string(),
            color: color_tag(slot.color).to_string(),
        }
    }

    pub fn to_slot(&self) -> Result<HeadSlot, &str> {
        let piece = head_piece_from_tag(&self.piece).ok_or(self.piece.as_str())?;
        let color = color_from_tag(&self.color).ok_or(self.color.as_str())?;
        Ok(HeadSlot::new(piece, color))
    }
}

/// Full cosmetic snapshot — everything char-create picks. Rides on
/// `ClientHello` and lands in `PersistedCharacter` on disk. The server's
/// saved copy wins on subsequent logins (client-supplied cosmetics are
/// only consulted on first-time character creation).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PersistedCosmetics {
    pub gender: Gender,
    #[serde(default)]
    pub body: Option<PersistedOutfitSlot>,
    #[serde(default)]
    pub legs: Option<PersistedOutfitSlot>,
    #[serde(default)]
    pub arms: Option<PersistedOutfitSlot>,
    #[serde(default)]
    pub feet: Option<PersistedOutfitSlot>,
    #[serde(default)]
    pub head_piece: Option<PersistedHeadSlot>,
    /// Hair style tag; `None` = bald.
    #[serde(default)]
    pub hair: Option<String>,
    /// Beard style tag; `None` = clean-shaven (or female).
    #[serde(default)]
    pub beard: Option<String>,
}

impl PersistedCosmetics {
    pub fn from_parts(
        gender: Gender,
        body: Option<OutfitSlot>,
        legs: Option<OutfitSlot>,
        arms: Option<OutfitSlot>,
        feet: Option<OutfitSlot>,
        head_piece: Option<HeadSlot>,
        hair: Option<Hair>,
        beard: Option<Beard>,
    ) -> Self {
        Self {
            gender,
            body: body.map(PersistedOutfitSlot::from_slot),
            legs: legs.map(PersistedOutfitSlot::from_slot),
            arms: arms.map(PersistedOutfitSlot::from_slot),
            feet: feet.map(PersistedOutfitSlot::from_slot),
            head_piece: head_piece.map(PersistedHeadSlot::from_slot),
            hair: hair.map(|h| hair_tag(h).to_string()),
            beard: beard.map(|b| beard_tag(b).to_string()),
        }
    }

    pub fn hair_enum(&self) -> Option<Hair> {
        self.hair.as_deref().and_then(hair_from_tag)
    }

    pub fn beard_enum(&self) -> Option<Beard> {
        self.beard.as_deref().and_then(beard_from_tag)
    }

    /// Re-hydrate a `QuaterniusOutfit` for character-mesh rendering.
    /// Unknown tags on any field resolve to `None` on that field only —
    /// the rest of the outfit still renders. Client-side render path
    /// consumes this for remote players (own player still builds its
    /// outfit from `OwnEquipped` directly).
    pub fn to_outfit(&self) -> QuaterniusOutfit {
        QuaterniusOutfit {
            body: self.body.as_ref().and_then(|s| s.to_slot().ok()),
            legs: self.legs.as_ref().and_then(|s| s.to_slot().ok()),
            arms: self.arms.as_ref().and_then(|s| s.to_slot().ok()),
            feet: self.feet.as_ref().and_then(|s| s.to_slot().ok()),
            head_piece: self.head_piece.as_ref().and_then(|s| s.to_slot().ok()),
            hair: self.hair_enum(),
            beard: self.beard_enum(),
        }
    }
}

impl Default for PersistedCosmetics {
    fn default() -> Self {
        Self {
            gender: Gender::Male,
            body: None,
            legs: None,
            arms: None,
            feet: None,
            head_piece: None,
            hair: None,
            beard: None,
        }
    }
}
