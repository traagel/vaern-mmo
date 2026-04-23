//! Quest-chain loader. Main storyline only for now — one yaml per chain
//! under `world/zones/<zone>/quests/chains/<chain_id>.yaml`. Side + filler
//! quests are a later pass.

use std::{collections::HashMap, fs, path::Path};

use serde::Deserialize;

use crate::{read_dir, LoadError};

#[derive(Debug, Clone, Deserialize)]
pub struct QuestObjective {
    pub kind: String,
    #[serde(default)]
    pub target_hint: String,
    #[serde(default)]
    pub count: u32,
    #[serde(default)]
    pub group_suggested: bool,
    /// When the step's `kind` is `talk` / `deliver` / `escort`, `npc` is the
    /// authoritative reference into the chain's `npcs` registry. Display
    /// name + hub come from that entry. `target_hint` stays as a fallback
    /// for chains that haven't been hand-curated yet.
    #[serde(default)]
    pub npc: Option<String>,
    /// For `kill` steps: id of the target mob (matches NpcSpawnSlot display
    /// naming on the server). Enables kill-counter objective matching.
    #[serde(default)]
    pub mob_id: Option<String>,
    /// For `investigate` / `explore` steps: waypoint id in the zone.
    #[serde(default)]
    pub location: Option<String>,
}

/// Named NPC entry in a chain's registry. Authoritative source for display
/// name, hub placement, and greeting dialogue. Server spawns one NPC per
/// entry; client dialogue prefers `dialogue` over the generic greeting.
#[derive(Debug, Clone, Deserialize)]
pub struct QuestNpc {
    pub id: String,
    pub display_name: String,
    #[serde(default)]
    pub title: Option<String>,
    /// Which hub in the zone hosts this NPC (e.g. "dalewatch_keep",
    /// "miller_crossing"). If unset, the NPC clusters at the chain's
    /// capital hub.
    #[serde(default)]
    pub hub_id: Option<String>,
    /// Greeting line shown when the player opens dialogue with this NPC.
    #[serde(default)]
    pub dialogue: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QuestStep {
    pub step: u32,
    pub id: String,
    pub name: String,
    pub level: u32,
    pub objective: QuestObjective,
    pub xp_reward: u32,
    pub gold_reward_copper: u32,
    #[serde(default)]
    pub prerequisite: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QuestChainFinalReward {
    pub xp_bonus: u32,
    pub gold_bonus_copper: u32,
    #[serde(default)]
    pub item_hint: String,
    #[serde(default)]
    pub title_hint: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QuestChain {
    pub id: String,
    pub zone: String,
    pub title: String,
    pub premise: String,
    pub total_steps: u32,
    pub final_reward: QuestChainFinalReward,
    #[serde(default)]
    pub breadcrumb_from: String,
    #[serde(default)]
    pub final_boss_hint: Option<String>,
    pub steps: Vec<QuestStep>,
    /// Explicit NPC registry. Populated for hand-curated chains; empty for
    /// procedurally-seeded ones (which still work via target_hint parsing).
    #[serde(default)]
    pub npcs: Vec<QuestNpc>,
}

impl QuestChain {
    pub fn step(&self, idx: u32) -> Option<&QuestStep> {
        self.steps.iter().find(|s| s.step == idx + 1)
    }

    /// Look up an NPC by id from the chain's registry.
    pub fn npc(&self, id: &str) -> Option<&QuestNpc> {
        self.npcs.iter().find(|n| n.id == id)
    }
}

/// Keyed by `chain.id`. Flat index across every zone.
#[derive(Debug, Default, Clone)]
pub struct QuestIndex {
    pub chains: HashMap<String, QuestChain>,
    /// zone_id -> list of chain_ids in that zone (ordered by file name).
    pub by_zone: HashMap<String, Vec<String>>,
}

impl QuestIndex {
    pub fn chain(&self, id: &str) -> Option<&QuestChain> {
        self.chains.get(id)
    }

    pub fn zone_chains(&self, zone_id: &str) -> impl Iterator<Item = &QuestChain> {
        self.by_zone
            .get(zone_id)
            .into_iter()
            .flat_map(move |ids| ids.iter().filter_map(move |id| self.chains.get(id)))
    }
}

/// Walk `world_root/zones/<zone>/quests/chains/*.yaml` for every zone.
pub fn load_all_chains(world_root: impl AsRef<Path>) -> Result<QuestIndex, LoadError> {
    let world_root = world_root.as_ref();
    let zones_dir = world_root.join("zones");
    let mut out = QuestIndex::default();
    for zone_dir in read_dir(&zones_dir)? {
        if !zone_dir.is_dir() {
            continue;
        }
        let zone_name = zone_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        let chains_dir = zone_dir.join("quests").join("chains");
        if !chains_dir.exists() {
            continue;
        }
        let mut zone_chain_ids = Vec::new();
        for path in read_dir(&chains_dir)? {
            if path.extension().is_none_or(|e| e != "yaml") {
                continue;
            }
            let text = fs::read_to_string(&path).map_err(|e| LoadError::Io {
                path: path.clone(),
                source: e,
            })?;
            let chain: QuestChain =
                serde_yaml::from_str(&text).map_err(|e| LoadError::Yaml {
                    path: path.clone(),
                    source: e,
                })?;
            zone_chain_ids.push(chain.id.clone());
            out.chains.insert(chain.id.clone(), chain);
        }
        if !zone_chain_ids.is_empty() {
            out.by_zone.insert(zone_name, zone_chain_ids);
        }
    }
    Ok(out)
}
