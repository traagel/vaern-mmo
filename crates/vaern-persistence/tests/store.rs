//! Integration tests for `ServerCharacterStore`. Each test owns a
//! tempdir so nothing touches the real `~/.config/vaern/server/`.

use tempfile::tempdir;
use uuid::Uuid;
use vaern_character::Experience;
use vaern_core::pillar::Pillar;
use vaern_equipment::Equipped;
use vaern_inventory::{ConsumableBelt, PlayerInventory};
use vaern_persistence::{
    CorruptReason, PersistedCharacter, PersistedCosmetics, PersistedQuestLog, SCHEMA_VERSION,
    ServerCharacterStore, StoreError,
};
use vaern_professions::ProfessionSkills;
use vaern_stats::{PillarCaps, PillarScores, PillarXp};

fn sample_character(uuid: Uuid) -> PersistedCharacter {
    PersistedCharacter {
        schema_version: SCHEMA_VERSION,
        character_id: uuid.to_string(),
        name: "Brenn".into(),
        race_id: "mannin".into(),
        core_pillar: Pillar::Might,
        cosmetics: PersistedCosmetics::default(),
        experience: Experience { current: 50, level: 2 },
        pillar_scores: PillarScores { might: 20, finesse: 5, arcana: 5 },
        pillar_caps: PillarCaps { might: 100, finesse: 50, arcana: 50 },
        pillar_xp: PillarXp::default(),
        inventory: PlayerInventory::default(),
        equipped: Equipped::default(),
        belt: ConsumableBelt::default(),
        professions: ProfessionSkills::default(),
        wallet_copper: 0,
        quest_log: PersistedQuestLog::default(),
        position: None,
        yaw_rad: None,
        created_at: 1_714_000_000,
        updated_at: 1_714_000_000,
    }
}

#[test]
fn save_then_load_round_trips_identity_fields() {
    let dir = tempdir().unwrap();
    let store = ServerCharacterStore::open(dir.path().into()).unwrap();
    let uuid = Uuid::new_v4();
    let original = sample_character(uuid);

    store.save(uuid, &original).expect("save");
    let loaded = store.load(uuid).expect("load");

    assert_eq!(loaded.character_id, original.character_id);
    assert_eq!(loaded.name, original.name);
    assert_eq!(loaded.race_id, original.race_id);
    assert_eq!(loaded.core_pillar, original.core_pillar);
    assert_eq!(loaded.experience, original.experience);
    assert_eq!(loaded.pillar_scores, original.pillar_scores);
    assert_eq!(loaded.pillar_caps, original.pillar_caps);
    assert_eq!(loaded.pillar_xp, original.pillar_xp);
    assert_eq!(loaded.schema_version, SCHEMA_VERSION);
}

#[test]
fn list_returns_every_saved_uuid() {
    let dir = tempdir().unwrap();
    let store = ServerCharacterStore::open(dir.path().into()).unwrap();
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    store.save(a, &sample_character(a)).unwrap();
    store.save(b, &sample_character(b)).unwrap();

    let mut uuids = store.list();
    uuids.sort();
    let mut expected = vec![a, b];
    expected.sort();
    assert_eq!(uuids, expected);
}

#[test]
fn list_skips_stray_non_uuid_files() {
    let dir = tempdir().unwrap();
    let store = ServerCharacterStore::open(dir.path().into()).unwrap();
    let u = Uuid::new_v4();
    store.save(u, &sample_character(u)).unwrap();
    std::fs::write(dir.path().join("notes.txt"), "ignored").unwrap();
    std::fs::write(dir.path().join("stray.json"), "ignored").unwrap();

    assert_eq!(store.list(), vec![u]);
}

#[test]
fn load_missing_file_returns_io_error() {
    let dir = tempdir().unwrap();
    let store = ServerCharacterStore::open(dir.path().into()).unwrap();
    let u = Uuid::new_v4();
    let err = store.load(u).unwrap_err();
    assert!(matches!(err, StoreError::Io(_)));
}

#[test]
fn load_quarantines_wrong_schema_version() {
    let dir = tempdir().unwrap();
    let store = ServerCharacterStore::open(dir.path().into()).unwrap();
    let u = Uuid::new_v4();
    let path = dir.path().join(format!("{u}.json"));

    // Write a file that parses but claims a future schema.
    let body = serde_json::json!({
        "schema_version": 99,
        "character_id": u.to_string(),
    });
    std::fs::write(&path, body.to_string()).unwrap();

    let err = store.load(u).unwrap_err();
    match err {
        StoreError::Corrupt {
            reason: CorruptReason::UnsupportedVersion { found, expected },
            quarantined_to,
        } => {
            assert_eq!(found, 99);
            assert_eq!(expected, SCHEMA_VERSION);
            assert!(quarantined_to.exists(), "quarantine target was created");
        }
        other => panic!("expected UnsupportedVersion, got {other:?}"),
    }
    assert!(!path.exists(), "original path was vacated by rename");
}

#[test]
fn load_quarantines_id_mismatch() {
    let dir = tempdir().unwrap();
    let store = ServerCharacterStore::open(dir.path().into()).unwrap();
    let filename_uuid = Uuid::new_v4();
    let rogue_field = Uuid::new_v4().to_string();

    let mut body = sample_character(filename_uuid);
    body.character_id = rogue_field.clone();
    store.save(filename_uuid, &body).unwrap();

    let err = store.load(filename_uuid).unwrap_err();
    match err {
        StoreError::Corrupt {
            reason: CorruptReason::IdMismatch { file_uuid, field_id },
            ..
        } => {
            assert_eq!(file_uuid, filename_uuid);
            assert_eq!(field_id, rogue_field);
        }
        other => panic!("expected IdMismatch, got {other:?}"),
    }
}

#[test]
fn load_quarantines_unparseable_json() {
    let dir = tempdir().unwrap();
    let store = ServerCharacterStore::open(dir.path().into()).unwrap();
    let u = Uuid::new_v4();
    let path = dir.path().join(format!("{u}.json"));
    std::fs::write(&path, "{ this isn't json :: }").unwrap();

    let err = store.load(u).unwrap_err();
    assert!(matches!(
        err,
        StoreError::Corrupt { reason: CorruptReason::ParseFailed(_), .. }
    ));
}

#[test]
fn save_is_atomic_against_concurrent_reader() {
    // Pseudo-test: after save, no `.tmp` file remains — caller never
    // observes a half-written file.
    let dir = tempdir().unwrap();
    let store = ServerCharacterStore::open(dir.path().into()).unwrap();
    let u = Uuid::new_v4();
    store.save(u, &sample_character(u)).unwrap();

    let stray_tmp = dir.path().join(format!("{u}.json.tmp"));
    assert!(!stray_tmp.exists(), "tmp file should have been renamed");
    assert!(dir.path().join(format!("{u}.json")).exists());
}

#[test]
fn resaving_overwrites_existing_file() {
    let dir = tempdir().unwrap();
    let store = ServerCharacterStore::open(dir.path().into()).unwrap();
    let u = Uuid::new_v4();
    let mut ch = sample_character(u);
    store.save(u, &ch).unwrap();
    ch.experience.level = 42;
    ch.name = "Brenn the Elder".into();
    store.save(u, &ch).unwrap();

    let loaded = store.load(u).unwrap();
    assert_eq!(loaded.experience.level, 42);
    assert_eq!(loaded.name, "Brenn the Elder");
}
