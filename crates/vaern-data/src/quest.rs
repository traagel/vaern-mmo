//! Quest-chain loader. Main storyline only for now — one yaml per chain
//! under `world/zones/<zone>/quests/chains/<chain_id>.yaml`. Side + filler
//! quests are a later pass.

use std::{collections::HashMap, fs, path::Path};

use serde::Deserialize;
use vaern_core::Pillar;

use crate::{read_dir, LoadError};

/// One armor / item piece a quest hands out as a reward. Optional
/// `pillar` makes the entry pillar-gated (None = grant to everyone;
/// `Some(p)` = only grant when the player's `core_pillar == p`).
///
/// `material` is `None` for materialless bases (consumables / runes /
/// scrolls). `quality` defaults to `"regular"` and `count` to `1`.
#[derive(Debug, Clone, Deserialize)]
pub struct ItemReward {
    pub base: String,
    #[serde(default)]
    pub material: Option<String>,
    #[serde(default = "default_quality")]
    pub quality: String,
    #[serde(default = "default_count")]
    pub count: u32,
    #[serde(default)]
    pub pillar: Option<Pillar>,
}

fn default_quality() -> String {
    "regular".to_string()
}
fn default_count() -> u32 {
    1
}

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
    /// Optional list of items handed out when the step advances. Pillar
    /// filtering is applied per-entry at grant time on the server.
    #[serde(default)]
    pub item_reward: Vec<ItemReward>,
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
    /// Optional list of items handed out on chain completion. Same
    /// pillar-filter rules as `QuestStep::item_reward`.
    #[serde(default)]
    pub item_reward: Vec<ItemReward>,
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

// ─── side quests ─────────────────────────────────────────────────────────────

/// One side quest in a hub's `side/<hub_id>.yaml`. Single objective; no
/// chain progression. Only `kill` and `collect` are auto-progressing
/// today; talk/deliver/investigate are pre-alpha follow-up.
#[derive(Debug, Clone, Deserialize)]
pub struct SideQuest {
    pub id: String,
    pub name: String,
    pub hub: String,
    #[serde(default = "default_side_type")]
    #[serde(rename = "type")]
    pub kind: String,
    pub level: u32,
    pub objective: QuestObjective,
    pub xp_reward: u32,
    pub gold_reward_copper: u32,
    #[serde(default)]
    pub repeatable: bool,
}

fn default_side_type() -> String {
    "side".to_string()
}

/// One hub's side-quest bundle. Each hub gets a single giver NPC who
/// hands out every side quest in this file.
#[derive(Debug, Clone, Deserialize)]
pub struct HubSideQuests {
    pub id: String,
    pub hub: String,
    pub hub_role: String,
    pub zone: String,
    #[serde(default)]
    pub biome: String,
    /// Authored quest-giver NPC. When present, server spawns one NPC at
    /// the hub with this id. When absent, the side quests are still
    /// loaded but no giver appears in-world (legacy behavior).
    #[serde(default)]
    pub giver: Option<QuestNpc>,
    #[serde(default)]
    pub quests: Vec<SideQuest>,
}

#[derive(Debug, Default, Clone)]
pub struct SideQuestIndex {
    /// hub_id → bundle (one file per hub).
    pub by_hub: HashMap<String, HubSideQuests>,
    /// zone_id → list of hub_ids that have side-quest bundles.
    pub by_zone: HashMap<String, Vec<String>>,
}

impl SideQuestIndex {
    pub fn for_hub(&self, hub_id: &str) -> Option<&HubSideQuests> {
        self.by_hub.get(hub_id)
    }

    pub fn hubs_in_zone(&self, zone_id: &str) -> impl Iterator<Item = &HubSideQuests> {
        self.by_zone
            .get(zone_id)
            .into_iter()
            .flat_map(move |ids| ids.iter().filter_map(move |id| self.by_hub.get(id)))
    }
}

/// Walk `world_root/zones/<zone>/quests/side/<hub_id>.yaml` for every zone.
pub fn load_all_side_quests(
    world_root: impl AsRef<Path>,
) -> Result<SideQuestIndex, LoadError> {
    let world_root = world_root.as_ref();
    let zones_dir = world_root.join("zones");
    let mut out = SideQuestIndex::default();
    for zone_dir in read_dir(&zones_dir)? {
        if !zone_dir.is_dir() {
            continue;
        }
        let zone_name = zone_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        let side_dir = zone_dir.join("quests").join("side");
        if !side_dir.exists() {
            continue;
        }
        let mut hubs_in_zone: Vec<String> = Vec::new();
        for path in read_dir(&side_dir)? {
            if path.extension().is_none_or(|e| e != "yaml") {
                continue;
            }
            // Skip leading-underscore files (schema / readme / summary).
            let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if fname.starts_with('_') {
                continue;
            }
            let text = fs::read_to_string(&path).map_err(|e| LoadError::Io {
                path: path.clone(),
                source: e,
            })?;
            let bundle: HubSideQuests =
                serde_yaml::from_str(&text).map_err(|e| LoadError::Yaml {
                    path: path.clone(),
                    source: e,
                })?;
            hubs_in_zone.push(bundle.hub.clone());
            out.by_hub.insert(bundle.hub.clone(), bundle);
        }
        if !hubs_in_zone.is_empty() {
            out.by_zone.insert(zone_name, hubs_in_zone);
        }
    }
    Ok(out)
}
