//! Pin every cosmetic enum ⇄ tag mapping. If a variant gets renamed or
//! reordered and the `*_tag` match arm moves in lockstep but the tag text
//! drifts, this test fails loud instead of silently corrupting on-disk
//! saves the next time the server reads them.

use vaern_assets::meshtint::Gender;
use vaern_assets::quaternius::{Beard, ColorVariant, Hair, HeadPiece, HeadSlot, Outfit, OutfitSlot};
use vaern_persistence::{
    PersistedCosmetics, PersistedHeadSlot, PersistedOutfitSlot, beard_tag, color_tag, hair_tag,
    head_piece_tag, outfit_tag,
};

#[test]
fn outfit_tags_are_pinned() {
    assert_eq!(outfit_tag(Outfit::Peasant), "peasant");
    assert_eq!(outfit_tag(Outfit::Ranger), "ranger");
    assert_eq!(outfit_tag(Outfit::Noble), "noble");
    assert_eq!(outfit_tag(Outfit::Knight), "knight");
    assert_eq!(outfit_tag(Outfit::KnightCloth), "knight_cloth");
    assert_eq!(outfit_tag(Outfit::Wizard), "wizard");
}

#[test]
fn head_piece_tags_are_pinned() {
    assert_eq!(head_piece_tag(HeadPiece::KnightArmet), "knight_armet");
    assert_eq!(head_piece_tag(HeadPiece::KnightHorns), "knight_horns");
    assert_eq!(head_piece_tag(HeadPiece::NobleCrown), "noble_crown");
    assert_eq!(head_piece_tag(HeadPiece::RangerHood), "ranger_hood");
}

#[test]
fn hair_tags_are_pinned() {
    assert_eq!(hair_tag(Hair::SimpleParted), "simple_parted");
    assert_eq!(hair_tag(Hair::Long), "long");
    assert_eq!(hair_tag(Hair::Buzzed), "buzzed");
    assert_eq!(hair_tag(Hair::Buns), "buns");
}

#[test]
fn beard_tags_are_pinned() {
    assert_eq!(beard_tag(Beard::Full), "full");
}

#[test]
fn color_tags_are_pinned() {
    assert_eq!(color_tag(ColorVariant::Default), "v1");
    assert_eq!(color_tag(ColorVariant::V2), "v2");
    assert_eq!(color_tag(ColorVariant::V3), "v3");
}

#[test]
fn outfit_slot_round_trip_every_variant() {
    for &o in Outfit::ALL {
        for &c in ColorVariant::ALL {
            let slot = OutfitSlot::new(o, c);
            let persisted = PersistedOutfitSlot::from_slot(slot);
            let back = persisted.to_slot().expect("round trip");
            assert_eq!(back, slot, "outfit {:?} color {:?}", o, c);
        }
    }
}

#[test]
fn head_slot_round_trip_every_variant() {
    for &p in HeadPiece::ALL {
        for &c in ColorVariant::ALL {
            let slot = HeadSlot::new(p, c);
            let persisted = PersistedHeadSlot::from_slot(slot);
            let back = persisted.to_slot().expect("round trip");
            assert_eq!(back, slot, "piece {:?} color {:?}", p, c);
        }
    }
}

#[test]
fn unknown_tag_returns_none() {
    let bad = PersistedOutfitSlot {
        outfit: "sorcerer".into(),
        color: "v1".into(),
    };
    assert!(bad.to_slot().is_err());
    let bad = PersistedOutfitSlot {
        outfit: "peasant".into(),
        color: "rgb".into(),
    };
    assert!(bad.to_slot().is_err());
}

#[test]
fn persisted_cosmetics_from_parts_round_trips_json() {
    let original = PersistedCosmetics::from_parts(
        Gender::Female,
        Some(OutfitSlot::new(Outfit::Wizard, ColorVariant::V2)),
        Some(OutfitSlot::new(Outfit::Wizard, ColorVariant::V2)),
        Some(OutfitSlot::new(Outfit::Wizard, ColorVariant::V2)),
        Some(OutfitSlot::new(Outfit::Wizard, ColorVariant::V2)),
        None,
        Some(Hair::Long),
        None,
    );
    let json = serde_json::to_string(&original).unwrap();
    let back: PersistedCosmetics = serde_json::from_str(&json).unwrap();
    assert_eq!(back, original);
}
