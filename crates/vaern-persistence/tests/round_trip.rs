//! Structural round-trip: build a PersistedCharacter with nontrivial
//! fields, serialize to JSON, deserialize, reserialize, and assert the
//! two JSON values are deeply equal. Catches any field we forgot to flow
//! through serde.
//!
//! We compare via `serde_json::Value` equality (map-order-independent)
//! rather than byte-string equality because `Equipped` is a `HashMap` and
//! its JSON object emits keys in nondeterministic order.

use serde_json::json;
use vaern_assets::meshtint::Gender;
use vaern_assets::quaternius::{ColorVariant, Hair, HeadPiece, HeadSlot, Outfit, OutfitSlot};
use vaern_character::Experience;
use vaern_core::pillar::Pillar;
use vaern_equipment::Equipped;
use vaern_inventory::{ConsumableBelt, PlayerInventory};
use vaern_items::ItemInstance;
use vaern_persistence::{
    PersistedCharacter, PersistedCosmetics, PersistedQuestEntry, PersistedQuestLog, SCHEMA_VERSION,
};
use vaern_professions::{Profession, ProfessionSkills};
use vaern_stats::{PillarCaps, PillarScores, PillarXp};

fn sample_cosmetics() -> PersistedCosmetics {
    PersistedCosmetics::from_parts(
        Gender::Male,
        Some(OutfitSlot::new(Outfit::Knight, ColorVariant::V2)),
        Some(OutfitSlot::new(Outfit::Knight, ColorVariant::V2)),
        Some(OutfitSlot::new(Outfit::Knight, ColorVariant::V2)),
        Some(OutfitSlot::new(Outfit::Knight, ColorVariant::V2)),
        Some(HeadSlot::new(HeadPiece::KnightArmet, ColorVariant::V2)),
        Some(Hair::Buzzed),
        None,
    )
}

fn sample_character() -> PersistedCharacter {
    let mut professions = ProfessionSkills::default();
    professions.set(Profession::Mining, 42);
    professions.set(Profession::Herbalism, 17);

    let mut belt = ConsumableBelt::default();
    belt.bind(
        0,
        ItemInstance {
            base_id: "potion_healing_minor".into(),
            material_id: None,
            quality_id: "common".into(),
            affixes: vec![],
        },
    );
    belt.bind(
        2,
        ItemInstance {
            base_id: "potion_mana_major".into(),
            material_id: None,
            quality_id: "fine".into(),
            affixes: vec![],
        },
    );

    PersistedCharacter {
        schema_version: SCHEMA_VERSION,
        character_id: "5ae1a3f4-8c7b-4f2e-9d1a-3b0e9c2a1f70".into(),
        name: "Brenn".into(),
        race_id: "mannin".into(),
        core_pillar: Pillar::Might,
        cosmetics: sample_cosmetics(),
        experience: Experience { current: 420, level: 7 },
        pillar_scores: PillarScores { might: 35, finesse: 10, arcana: 8 },
        pillar_caps: PillarCaps { might: 100, finesse: 50, arcana: 50 },
        pillar_xp: PillarXp { might: 120, finesse: 5, arcana: 0 },
        inventory: PlayerInventory::default(),
        equipped: Equipped::default(),
        belt,
        professions,
        position: Some([123.5, 0.0, -45.2]),
        yaw_rad: Some(1.5),
        quest_log: PersistedQuestLog {
            entries: vec![
                PersistedQuestEntry {
                    chain_id: "dalewatch_marches".into(),
                    current_step: 3,
                    total_steps: 5,
                    completed: false,
                },
                PersistedQuestEntry {
                    chain_id: "old_brenn_shepherd".into(),
                    current_step: 2,
                    total_steps: 2,
                    completed: true,
                },
            ],
        },
        created_at: 1_714_000_000,
        updated_at: 1_714_000_500,
    }
}

#[test]
fn persisted_character_round_trips_through_json() {
    let original = sample_character();

    let json_a = serde_json::to_string(&original).expect("serialize");
    let decoded: PersistedCharacter = serde_json::from_str(&json_a).expect("deserialize");
    let json_b = serde_json::to_string(&decoded).expect("re-serialize");

    let value_a: serde_json::Value = serde_json::from_str(&json_a).unwrap();
    let value_b: serde_json::Value = serde_json::from_str(&json_b).unwrap();

    assert_eq!(value_a, value_b, "round-trip altered the structure");
}

#[test]
fn schema_version_field_is_present_and_pinned() {
    let json = serde_json::to_value(&sample_character()).unwrap();
    assert_eq!(json["schema_version"], json!(SCHEMA_VERSION));
    assert_eq!(json["schema_version"], json!(1));
}

#[test]
fn cosmetic_tags_land_in_output_json() {
    let json = serde_json::to_value(&sample_character()).unwrap();
    assert_eq!(json["cosmetics"]["gender"], json!("male"));
    assert_eq!(json["cosmetics"]["body"]["outfit"], json!("knight"));
    assert_eq!(json["cosmetics"]["body"]["color"], json!("v2"));
    assert_eq!(json["cosmetics"]["head_piece"]["piece"], json!("knight_armet"));
    assert_eq!(json["cosmetics"]["hair"], json!("buzzed"));
    assert_eq!(json["cosmetics"]["beard"], json!(null));
}

#[test]
fn missing_optional_cosmetic_fields_default_to_none() {
    let minimal = json!({
        "gender": "female",
    });
    let cosmetics: PersistedCosmetics = serde_json::from_value(minimal).unwrap();
    assert!(cosmetics.body.is_none());
    assert!(cosmetics.legs.is_none());
    assert!(cosmetics.head_piece.is_none());
    assert!(cosmetics.hair.is_none());
    assert!(cosmetics.beard.is_none());
    assert_eq!(cosmetics.gender, Gender::Female);
}
