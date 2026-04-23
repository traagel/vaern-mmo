//! Dungeon / raid loader. Layout:
//!   `world/dungeons/<id>/{core.yaml, bosses.yaml}` + `_index.yaml` summary.

use std::{fs, path::Path};

use serde::Deserialize;

use crate::{read_dir, world::LevelRange, LoadError};

#[derive(Debug, Clone, Deserialize)]
pub struct Boss {
    pub id: String,
    pub name: String,
    pub role_tag: String,
    pub level: u32,
    pub mechanic: String,
    pub hp_tier: String,
}

#[derive(Debug, Clone, Deserialize)]
struct BossesFile {
    #[serde(default)]
    _id: Option<String>,
    #[serde(default)]
    _instance: Option<String>,
    bosses: Vec<Boss>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Dungeon {
    pub id: String,
    pub name: String,
    /// "dungeon" | "raid"
    pub kind: String,
    pub group_size: u8,
    pub zone: String,
    pub entrance_hub: String,
    pub level_range: LevelRange,
    #[serde(default)]
    pub level_band: String,
    pub tier: String,
    pub boss_count: u32,
    pub estimated_clear_minutes: u32,
    pub loot_tier: String,
    pub lockout: String,
    #[serde(default)]
    pub theme: String,
    #[serde(default)]
    pub coop_notes: String,
    /// Populated after loading `bosses.yaml`.
    #[serde(default, skip_deserializing)]
    pub bosses: Vec<Boss>,
}

pub fn load_dungeons(root: impl AsRef<Path>) -> Result<Vec<Dungeon>, LoadError> {
    let root = root.as_ref();
    let mut out = Vec::new();
    for dir in read_dir(root)? {
        if !dir.is_dir() {
            continue;
        }
        let core = dir.join("core.yaml");
        if !core.exists() {
            continue;
        }
        let text = fs::read_to_string(&core).map_err(|e| LoadError::Io {
            path: core.clone(),
            source: e,
        })?;
        let mut dungeon: Dungeon = serde_yaml::from_str(&text).map_err(|e| LoadError::Yaml {
            path: core.clone(),
            source: e,
        })?;

        let bosses_file = dir.join("bosses.yaml");
        if bosses_file.exists() {
            let text = fs::read_to_string(&bosses_file).map_err(|e| LoadError::Io {
                path: bosses_file.clone(),
                source: e,
            })?;
            let parsed: BossesFile =
                serde_yaml::from_str(&text).map_err(|e| LoadError::Yaml {
                    path: bosses_file.clone(),
                    source: e,
                })?;
            dungeon.bosses = parsed.bosses;
        }

        out.push(dungeon);
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}
