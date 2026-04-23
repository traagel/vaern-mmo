//! Creature-type + armor-class catalog. Every mob and race inherits from a
//! `CreatureType`; every mob declares an `ArmorClass`. The Python seed scripts
//! own the authoring; these loaders are read-only.

use std::{collections::HashMap, fs, path::Path};

use serde::Deserialize;

use crate::{read_dir, LoadError};

#[derive(Debug, Clone, Deserialize)]
pub struct HpScaling {
    pub base_at_level_1: u32,
    pub per_level_multiplier: f32,
    #[serde(default)]
    pub formula: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Affinities {
    #[serde(default)]
    pub preferred: Vec<String>,
    #[serde(default)]
    pub allowed: Vec<String>,
    #[serde(default)]
    pub forbidden: Vec<String>,
}

impl Affinities {
    pub fn is_legal(&self, school: &str) -> bool {
        if self.forbidden.iter().any(|s| s == school) {
            return false;
        }
        self.preferred.iter().any(|s| s == school)
            || self.allowed.iter().any(|s| s == school)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct BehaviorDefaults {
    pub intelligence: String,
    pub social: String,
    pub flee_threshold: f32,
    pub aggro_range: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreatureType {
    pub id: String,
    pub name: String,
    pub category: String,
    #[serde(default)]
    pub description: String,
    pub hp_scaling: HpScaling,
    pub default_armor_class: String,
    #[serde(default)]
    pub resistances: HashMap<String, f32>,
    pub affinities: Affinities,
    pub behavior_defaults: BehaviorDefaults,
    #[serde(default)]
    pub tags: Vec<String>,
}

impl CreatureType {
    /// Geometric HP scaling: hp(L) = base * multiplier^(L - 1).
    /// Rarity multiplier applied by caller.
    pub fn base_hp_at_level(&self, level: u32) -> u32 {
        let base = self.hp_scaling.base_at_level_1 as f32;
        let mult = self.hp_scaling.per_level_multiplier;
        (base * mult.powi((level as i32).saturating_sub(1))).round() as u32
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ArmorClass {
    pub id: String,
    pub name: String,
    pub tier: String,
    pub physical_reduction: f32,
    pub magic_reduction: f32,
    #[serde(default)]
    pub weak_against: Vec<String>,
    #[serde(default)]
    pub strong_against: Vec<String>,
    pub mobility_penalty: f32,
    #[serde(default)]
    pub notes: String,
}

/// Combined bestiary handle. Indexed by id for O(1) lookups.
#[derive(Debug, Default)]
pub struct Bestiary {
    pub creature_types: HashMap<String, CreatureType>,
    pub armor_classes: HashMap<String, ArmorClass>,
}

impl Bestiary {
    pub fn creature_type(&self, id: &str) -> Option<&CreatureType> {
        self.creature_types.get(id)
    }

    pub fn armor_class(&self, id: &str) -> Option<&ArmorClass> {
        self.armor_classes.get(id)
    }
}

/// Load both `creature_types/*.yaml` and `armor_classes/*.yaml` under `root`
/// (expected layout: `<root>/creature_types/...`, `<root>/armor_classes/...`).
/// Files whose name starts with `_` (schema docs) are skipped.
pub fn load_bestiary(root: impl AsRef<Path>) -> Result<Bestiary, LoadError> {
    let root = root.as_ref();
    let mut out = Bestiary::default();

    for path in read_dir(&root.join("creature_types"))? {
        if !is_loadable(&path) {
            continue;
        }
        let text = fs::read_to_string(&path).map_err(|e| LoadError::Io {
            path: path.clone(),
            source: e,
        })?;
        let ct: CreatureType =
            serde_yaml::from_str(&text).map_err(|e| LoadError::Yaml {
                path: path.clone(),
                source: e,
            })?;
        out.creature_types.insert(ct.id.clone(), ct);
    }
    for path in read_dir(&root.join("armor_classes"))? {
        if !is_loadable(&path) {
            continue;
        }
        let text = fs::read_to_string(&path).map_err(|e| LoadError::Io {
            path: path.clone(),
            source: e,
        })?;
        let ac: ArmorClass =
            serde_yaml::from_str(&text).map_err(|e| LoadError::Yaml {
                path: path.clone(),
                source: e,
            })?;
        out.armor_classes.insert(ac.id.clone(), ac);
    }
    Ok(out)
}

fn is_loadable(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "yaml")
        && !path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with('_'))
}
