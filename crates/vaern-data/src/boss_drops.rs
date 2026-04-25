//! Boss-drop ladder loader. Reads `world/dungeons/<id>/loot.yaml` and builds
//! a `BossDrops` registry keyed by mob slot id (the same canonical id carried
//! by `MobSourceId` on combat mobs — e.g. `mob_dalewatch_marches_named_drifter_valenn`).
//!
//! Each entry lists the guaranteed `ItemReward`s a Named-tier mob drops on
//! kill. Resolution to `ItemInstance` happens at runtime against the
//! `ContentRegistry` (mirrors `quests.rs::grant_item_rewards`); ill-formed
//! entries log + skip rather than panicking the server.
//!
//! Open Need (Slice 6 user decision): the per-entry `pillar` tag is
//! advisory only. The roll layer (Phase C) does not gate Need votes by
//! pillar — any party member can roll Need on any drop.

use std::{collections::HashMap, fs, path::Path};

use serde::Deserialize;

use crate::{quest::ItemReward, read_dir, LoadError};

#[derive(Debug, Clone, Deserialize)]
pub struct BossDropEntry {
    pub mob_id: String,
    pub drops: Vec<ItemReward>,
}

#[derive(Debug, Clone, Deserialize)]
struct LootFile {
    #[serde(default)]
    _id: Option<String>,
    #[serde(default)]
    _instance: Option<String>,
    boss_drops: Vec<BossDropEntry>,
}

#[derive(Debug, Clone, Default)]
pub struct BossDrops {
    by_mob_id: HashMap<String, Vec<ItemReward>>,
}

impl BossDrops {
    pub fn drops_for_mob(&self, mob_id: &str) -> Option<&[ItemReward]> {
        self.by_mob_id.get(mob_id).map(|v| v.as_slice())
    }

    pub fn len(&self) -> usize {
        self.by_mob_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_mob_id.is_empty()
    }
}

/// Walk every `world/dungeons/<id>/loot.yaml` and merge into a single
/// registry. Dungeons without a `loot.yaml` are silently skipped — most
/// dungeon entries are metadata-only today.
pub fn load_all_boss_drops(dungeons_root: impl AsRef<Path>) -> Result<BossDrops, LoadError> {
    let dungeons_root = dungeons_root.as_ref();
    let mut by_mob_id: HashMap<String, Vec<ItemReward>> = HashMap::new();

    for dir in read_dir(dungeons_root)? {
        if !dir.is_dir() {
            continue;
        }
        let loot_path = dir.join("loot.yaml");
        if !loot_path.exists() {
            continue;
        }
        let text = fs::read_to_string(&loot_path).map_err(|e| LoadError::Io {
            path: loot_path.clone(),
            source: e,
        })?;
        let parsed: LootFile = serde_yaml::from_str(&text).map_err(|e| LoadError::Yaml {
            path: loot_path.clone(),
            source: e,
        })?;
        for entry in parsed.boss_drops {
            by_mob_id.insert(entry.mob_id, entry.drops);
        }
    }

    Ok(BossDrops { by_mob_id })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dungeons_root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../src/generated/world/dungeons")
    }

    #[test]
    fn loads_drifters_lair_boss_drops() {
        let drops = load_all_boss_drops(dungeons_root()).unwrap();
        assert!(
            drops.drops_for_mob("mob_dalewatch_marches_named_drifter_valenn").is_some(),
            "Valenn should have authored boss drops"
        );
        let valenn = drops
            .drops_for_mob("mob_dalewatch_marches_named_drifter_valenn")
            .unwrap();
        assert_eq!(valenn.len(), 12, "Valenn drops 4-piece × 3 pillars");
        // Spot-check material tier — mithril/dragonscale/shadowsilk are T5/T6.
        assert!(valenn.iter().any(|r| r.material.as_deref() == Some("mithril")));
        assert!(valenn.iter().any(|r| r.material.as_deref() == Some("dragonscale")));
        assert!(valenn.iter().any(|r| r.material.as_deref() == Some("shadowsilk")));
        assert!(valenn.iter().all(|r| r.quality == "exceptional"));
    }

    #[test]
    fn loads_halen_boss_drops_smaller_set() {
        let drops = load_all_boss_drops(dungeons_root()).unwrap();
        let halen = drops
            .drops_for_mob("mob_dalewatch_marches_named_drifter_master")
            .expect("Halen should have authored boss drops");
        assert_eq!(halen.len(), 3, "Halen drops one chest piece per pillar");
        assert!(halen.iter().all(|r| r.quality == "exceptional"));
    }

    #[test]
    fn unknown_mob_returns_none() {
        let drops = load_all_boss_drops(dungeons_root()).unwrap();
        assert!(drops.drops_for_mob("not_a_real_mob").is_none());
    }
}
