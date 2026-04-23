//! Zone / Hub / Mob loaders. The world layout lives under
//! `src/generated/world/zones/<zone>/{core.yaml, hubs/*.yaml, mobs/*.yaml}`.

use std::{collections::HashMap, fs, path::Path};

use serde::Deserialize;

use crate::{read_dir, LoadError};

// ─── shared ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct LevelRange {
    pub min: u32,
    pub max: u32,
}

// ─── biome + continent ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct Biome {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub climate: String,
    #[serde(default)]
    pub hazards: Vec<String>,
    #[serde(default)]
    pub typical_flora: Vec<String>,
    #[serde(default)]
    pub typical_fauna: Vec<String>,
    #[serde(default)]
    pub faction_affinity: String,
}

pub fn load_biomes(root: impl AsRef<Path>) -> Result<Vec<Biome>, LoadError> {
    let root = root.as_ref();
    let mut out = Vec::new();
    for path in read_dir(root)? {
        if !is_loadable_yaml(&path) {
            continue;
        }
        let text = fs::read_to_string(&path).map_err(|e| LoadError::Io {
            path: path.clone(),
            source: e,
        })?;
        out.push(serde_yaml::from_str(&text).map_err(|e| LoadError::Yaml {
            path: path.clone(),
            source: e,
        })?);
    }
    out.sort_by(|a: &Biome, b| a.id.cmp(&b.id));
    Ok(out)
}

// ─── zone ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct ZoneBudgetHours {
    pub solo: f32,
    pub duo: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ZoneBudget {
    pub quest_count_target: u32,
    pub unique_mob_types: u32,
    pub mob_kills_to_complete: u32,
    pub estimated_hours_to_complete: ZoneBudgetHours,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Zone {
    pub id: String,
    pub name: String,
    pub faction_control: String,
    pub biome: String,
    pub region: String,
    pub tier: String,
    pub level_range: LevelRange,
    #[serde(default)]
    pub starter_race: Option<String>,
    pub hub_count: u32,
    pub budget: ZoneBudget,
    #[serde(default)]
    pub notes: String,
}

// ─── hub ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct HubOffset {
    pub x: f32,
    pub z: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Hub {
    pub id: String,
    pub zone: String,
    pub name: String,
    pub role: String,
    #[serde(default)]
    pub amenities: Vec<String>,
    pub quest_givers: u32,
    /// Position relative to the zone's world origin. When `None`, the
    /// server falls back to a tiny radial layout (legacy behavior).
    /// Expressed as `(x, z)` — Y is sampled from the shared terrain.
    #[serde(default)]
    pub offset_from_zone_origin: Option<HubOffset>,
}

// ─── mob ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct MobDamage {
    pub primary_school: String,
    pub attack_range: String,
    pub dps_tier: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MobBehavior {
    pub aggro_range: String,
    pub social_radius: u32,
    pub flee_threshold: f32,
    pub calls_allies: bool,
    pub patrol: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MobDrops {
    pub gold_copper_avg: u32,
    pub item_hint: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Mob {
    pub id: String,
    pub name: String,
    pub zone: String,
    pub level: u32,
    /// Reference into `Bestiary::creature_types`.
    pub creature_type: String,
    /// Reference into `Bestiary::armor_classes`.
    pub armor_class: String,
    pub rarity: String,
    pub role: String,
    pub faction_alignment: String,
    pub hp_tier: String,
    pub damage: MobDamage,
    pub behavior: MobBehavior,
    pub loot_tier: String,
    pub drops: MobDrops,
    #[serde(default)]
    pub biome_context: String,
    #[serde(default)]
    pub chain_target: bool,
}

impl Mob {
    /// Rarity multiplier applied on top of creature_type base hp, per
    /// `bestiary/_schema.yaml`.
    pub fn rarity_hp_multiplier(&self) -> f32 {
        match self.rarity.as_str() {
            "common" => 1.00,
            "elite" => 2.75,
            "rare" => 3.50,
            "named" => 5.00,
            _ => 1.00,
        }
    }
}

// ─── aggregate ───────────────────────────────────────────────────────────────

/// Complete world snapshot loaded from `src/generated/world/`.
#[derive(Debug, Default)]
pub struct World {
    pub biomes: Vec<Biome>,
    pub zones: Vec<Zone>,
    pub hubs: Vec<Hub>,
    pub mobs: Vec<Mob>,
}

impl World {
    pub fn zone(&self, id: &str) -> Option<&Zone> {
        self.zones.iter().find(|z| z.id == id)
    }

    pub fn hub(&self, id: &str) -> Option<&Hub> {
        self.hubs.iter().find(|h| h.id == id)
    }

    pub fn mobs_in_zone(&self, zone_id: &str) -> impl Iterator<Item = &Mob> {
        self.mobs.iter().filter(move |m| m.zone == zone_id)
    }

    pub fn hubs_in_zone(&self, zone_id: &str) -> impl Iterator<Item = &Hub> {
        self.hubs.iter().filter(move |h| h.zone == zone_id)
    }

    pub fn by_zone_index(&self) -> HashMap<&str, &Zone> {
        self.zones.iter().map(|z| (z.id.as_str(), z)).collect()
    }
}

pub fn load_world(world_root: impl AsRef<Path>) -> Result<World, LoadError> {
    let root = world_root.as_ref();

    let biomes = load_biomes(root.join("biomes"))?;
    let mut zones: Vec<Zone> = Vec::new();
    let mut hubs: Vec<Hub> = Vec::new();
    let mut mobs: Vec<Mob> = Vec::new();

    let zones_dir = root.join("zones");
    for zone_dir in read_dir(&zones_dir)? {
        if !zone_dir.is_dir() {
            continue;
        }
        // zone core
        let zone_core = zone_dir.join("core.yaml");
        if zone_core.exists() {
            let text = fs::read_to_string(&zone_core).map_err(|e| LoadError::Io {
                path: zone_core.clone(),
                source: e,
            })?;
            let zone: Zone = serde_yaml::from_str(&text).map_err(|e| LoadError::Yaml {
                path: zone_core.clone(),
                source: e,
            })?;
            zones.push(zone);
        }
        // hubs
        let hubs_dir = zone_dir.join("hubs");
        if hubs_dir.exists() {
            for path in read_dir(&hubs_dir)? {
                if !is_loadable_yaml(&path) {
                    continue;
                }
                let text = fs::read_to_string(&path).map_err(|e| LoadError::Io {
                    path: path.clone(),
                    source: e,
                })?;
                let hub: Hub = serde_yaml::from_str(&text).map_err(|e| LoadError::Yaml {
                    path: path.clone(),
                    source: e,
                })?;
                hubs.push(hub);
            }
        }
        // mobs
        let mobs_dir = zone_dir.join("mobs");
        if mobs_dir.exists() {
            for path in read_dir(&mobs_dir)? {
                if !is_loadable_yaml(&path) {
                    continue;
                }
                let text = fs::read_to_string(&path).map_err(|e| LoadError::Io {
                    path: path.clone(),
                    source: e,
                })?;
                let mob: Mob = serde_yaml::from_str(&text).map_err(|e| LoadError::Yaml {
                    path: path.clone(),
                    source: e,
                })?;
                mobs.push(mob);
            }
        }
    }

    zones.sort_by(|a, b| a.id.cmp(&b.id));
    hubs.sort_by(|a, b| a.id.cmp(&b.id));
    mobs.sort_by(|a, b| a.id.cmp(&b.id));

    Ok(World {
        biomes,
        zones,
        hubs,
        mobs,
    })
}

fn is_loadable_yaml(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "yaml")
        && !path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with('_'))
}
