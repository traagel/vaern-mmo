//! Playable-race loader. Each race references a `CreatureType` in the
//! bestiary and adds racial modifiers (hp, resistances, school bonuses) on
//! top of that baseline.

use std::{collections::HashMap, fs, path::Path};

use serde::Deserialize;

use crate::{read_dir, LoadError};

#[derive(Debug, Clone, Deserialize)]
pub struct RacePillarAffinity {
    pub might: u8,
    pub arcana: u8,
    pub finesse: u8,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Race {
    pub id: String,
    pub archetype: String,
    pub faction: String,
    pub favored_class: String,
    #[serde(default)]
    pub cultural_traits: String,
    /// Reference into `Bestiary::creature_types`.
    pub creature_type: String,
    pub size_class: String,
    pub hp_modifier: f32,
    /// Pillar caps — mechanical constraint on position reach. Distinct from
    /// bestiary school affinities.
    pub affinity: RacePillarAffinity,
    #[serde(default)]
    pub racial_resistances: HashMap<String, f32>,
    #[serde(default)]
    pub racial_school_bonuses: Vec<String>,
    #[serde(default)]
    pub lore_hook: String,
}

/// Load every race yaml under `<root>/<race>/core.yaml`.
pub fn load_races(root: impl AsRef<Path>) -> Result<Vec<Race>, LoadError> {
    let root = root.as_ref();
    let mut races = Vec::new();

    for race_dir in read_dir(root)? {
        if !race_dir.is_dir() {
            continue;
        }
        let core = race_dir.join("core.yaml");
        if !core.exists() {
            continue;
        }
        let text = fs::read_to_string(&core).map_err(|e| LoadError::Io {
            path: core.clone(),
            source: e,
        })?;
        let race: Race = serde_yaml::from_str(&text).map_err(|e| LoadError::Yaml {
            path: core.clone(),
            source: e,
        })?;
        races.push(race);
    }
    races.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(races)
}
