//! Humanoid archetype table — maps short keys (`"peasant_male"`,
//! `"knight_plate_male"`) to full [`PersistedCosmetics`] bundles.
//!
//! Authored once in `assets/npc_mesh_map.yaml` under the
//! `humanoid_archetypes:` top-level key. Both server and client load
//! the same YAML so the wire can ship just the archetype key; each
//! side expands it locally at render time. Keeps replicated NPC
//! payloads tiny (20B instead of ~200B) and prevents per-NPC
//! inserts / per-packet fragmentation at crowded spawn points.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use crate::cosmetic::{PersistedCosmetics, PersistedHeadSlot, PersistedOutfitSlot};
use vaern_assets::Gender;

fn default_color() -> String {
    "v1".to_string()
}

fn default_gender() -> Gender {
    Gender::Male
}

/// One archetype template — Quaternius outfit slots as raw string
/// tags + a color variant. `to_cosmetics()` folds this into a
/// [`PersistedCosmetics`] ready for the client's rendering path.
#[derive(Clone, Debug, Deserialize)]
pub struct HumanoidArchetype {
    #[serde(default = "default_gender")]
    pub gender: Gender,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub legs: Option<String>,
    #[serde(default)]
    pub arms: Option<String>,
    #[serde(default)]
    pub feet: Option<String>,
    #[serde(default)]
    pub head_piece: Option<String>,
    #[serde(default)]
    pub hair: Option<String>,
    #[serde(default)]
    pub beard: Option<String>,
    /// Color variant applied to every outfit-bearing slot — `v1`, `v2`, or `v3`.
    #[serde(default = "default_color")]
    pub color: String,
}

impl HumanoidArchetype {
    /// Expand the archetype's string tags into a `PersistedCosmetics`.
    /// The client's `to_outfit()` converts tags to Quaternius enum
    /// values at spawn time; unknown tags silently render as an
    /// empty slot.
    pub fn to_cosmetics(&self) -> PersistedCosmetics {
        let color = self.color.clone();
        let slot = |o: &Option<String>| {
            o.as_ref().map(|tag| PersistedOutfitSlot {
                outfit: tag.clone(),
                color: color.clone(),
            })
        };
        PersistedCosmetics {
            gender: self.gender,
            body: slot(&self.body),
            legs: slot(&self.legs),
            arms: slot(&self.arms),
            feet: slot(&self.feet),
            head_piece: self.head_piece.as_ref().map(|p| PersistedHeadSlot {
                piece: p.clone(),
                color: color.clone(),
            }),
            hair: self.hair.clone(),
            beard: self.beard.clone(),
        }
    }
}

/// Key → archetype lookup table. Loaded at startup on both server
/// (for resolving `humanoid: <key>` entries) and client (for
/// expanding `NpcAppearance.archetype` back to cosmetics).
#[derive(Clone, Debug, Default)]
pub struct HumanoidArchetypeTable {
    entries: HashMap<String, HumanoidArchetype>,
}

impl HumanoidArchetypeTable {
    /// Load the archetype table from `assets/npc_mesh_map.yaml`.
    /// Only the `humanoid_archetypes:` section is read — the
    /// `entries:` section is ignored via `#[serde(other)]`-ish
    /// projection (the outer struct just doesn't declare `entries`).
    pub fn load_yaml<P: AsRef<Path>>(path: P) -> Result<Self, LoadError> {
        let bytes = std::fs::read(path.as_ref())?;
        let wrap: ArchetypesOnlyWrap = serde_yaml::from_slice(&bytes)?;
        Ok(Self {
            entries: wrap.humanoid_archetypes,
        })
    }

    /// Resolve a key to its cosmetics bundle, or `None` for unknown
    /// keys — caller typically logs + falls back to cuboid or a
    /// default outfit.
    pub fn resolve(&self, key: &str) -> Option<PersistedCosmetics> {
        self.entries.get(key).map(|a| a.to_cosmetics())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Direct archetype access — needed when the server wants the
    /// raw template for its own `NpcMeshMap::lookup` logic without
    /// round-tripping through `PersistedCosmetics`.
    pub fn get(&self, key: &str) -> Option<&HumanoidArchetype> {
        self.entries.get(key)
    }

    /// Iterate every `(key, archetype)` pair — used by server-side
    /// validation and museum tooling.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &HumanoidArchetype)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v))
    }
}

/// Deserialize-only wrapper that reads just the `humanoid_archetypes:`
/// key out of `assets/npc_mesh_map.yaml` and ignores `entries:`.
#[derive(Deserialize)]
struct ArchetypesOnlyWrap {
    #[serde(default)]
    humanoid_archetypes: HashMap<String, HumanoidArchetype>,
}

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("yaml: {0}")]
    Yaml(#[from] serde_yaml::Error),
}
