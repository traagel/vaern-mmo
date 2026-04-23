//! NPC display-name → render-spec lookup.
//!
//! Loaded once from `assets/npc_mesh_map.yaml` at server startup.
//! Two render paths fall out of the same lookup:
//!
//! - **Beast path.** `entries["<name>"].mesh = "<Species>"` →
//!   replicated [`NpcMesh`] → client spawns an EverythingLibrary GLB.
//! - **Humanoid path.** `entries["<name>"].humanoid = "<archetype>"` →
//!   replicated [`NpcAppearance`] → client spawns a Quaternius modular
//!   character. Archetypes are defined once in the
//!   `humanoid_archetypes:` top-level table and referenced by key.
//!
//! Entries with neither (or unlisted names) fall through to the blue
//! cuboid.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;

use bevy::prelude::*;
use serde::Deserialize;
use thiserror::Error;
use vaern_assets::Gender;
use vaern_persistence::{HumanoidArchetype, PersistedCosmetics};
use vaern_protocol::{NpcAppearance, NpcMesh};

/// One per-NPC-name entry. Either `mesh` (beast) or `humanoid`
/// (archetype key) is set — never both. `scale` applies to either
/// render path (mainly used on beasts; humanoids usually render at 1.0).
#[derive(Clone, Debug, Default, Deserialize)]
pub struct NpcMeshEntry {
    /// EverythingLibrary species basename for the beast render path.
    #[serde(default)]
    pub mesh: Option<String>,
    /// Archetype key for the humanoid render path.
    #[serde(default)]
    pub humanoid: Option<String>,
    /// Uniform scale factor applied on either render path. Defaults
    /// to 1.0 when omitted.
    #[serde(default = "one")]
    pub scale: f32,
}

fn one() -> f32 {
    1.0
}

/// Deserialized root of `assets/npc_mesh_map.yaml`.
#[derive(Clone, Debug, Default, Deserialize, Resource)]
pub struct NpcMeshMap {
    #[serde(default)]
    entries: HashMap<String, NpcMeshEntry>,
    #[serde(default)]
    humanoid_archetypes: HashMap<String, HumanoidArchetype>,
}

/// Resolved render spec for one NPC. The spawn site picks the matching
/// component variant to attach; unmapped / cuboid-fallback NPCs get
/// `None` here.
#[derive(Clone, Debug)]
pub enum NpcVisual {
    Beast(NpcMesh),
    Humanoid(NpcAppearance),
}

impl NpcMeshMap {
    pub fn load_yaml<P: AsRef<Path>>(path: P) -> Result<Self, NpcMeshMapLoadError> {
        let bytes = std::fs::read(path.as_ref())?;
        let map: NpcMeshMap = serde_yaml::from_slice(&bytes)?;
        let beast = map.entries.values().filter(|e| e.mesh.is_some()).count();
        let humanoid = map
            .entries
            .values()
            .filter(|e| e.humanoid.is_some())
            .count();
        let cuboid = map.entries.len() - beast - humanoid;
        info!(
            "NpcMeshMap loaded — {} entries ({} beast, {} humanoid, {} cuboid), \
             {} humanoid archetypes",
            map.entries.len(),
            beast,
            humanoid,
            cuboid,
            map.humanoid_archetypes.len(),
        );
        Ok(map)
    }

    /// Resolve an NPC display-name to its replicated render component.
    /// `None` means: no entry, explicit `{mesh: null, humanoid: null}`,
    /// or referenced archetype key missing — caller falls through to
    /// the cuboid path.
    pub fn lookup(&self, display_name: &str) -> Option<NpcVisual> {
        let entry = self.entries.get(display_name)?;
        if let Some(species) = entry.mesh.as_ref() {
            return Some(NpcVisual::Beast(NpcMesh {
                species: species.clone(),
                scale: entry.scale,
            }));
        }
        if let Some(archetype_key) = entry.humanoid.as_ref() {
            if self.humanoid_archetypes.contains_key(archetype_key) {
                return Some(NpcVisual::Humanoid(NpcAppearance::new(
                    archetype_key.clone(),
                    entry.scale,
                )));
            }
            warn!(
                "NpcMeshMap: {display_name:?} references unknown archetype {archetype_key:?}"
            );
        }
        None
    }

    /// Resolve an archetype key to its cosmetics bundle. Client uses
    /// this at render time to expand the wire-efficient
    /// `NpcAppearance.archetype` key back into a full
    /// `PersistedCosmetics`.
    pub fn resolve_archetype(&self, key: &str) -> Option<PersistedCosmetics> {
        self.humanoid_archetypes.get(key).map(|a| a.to_cosmetics())
    }

    /// Fallback humanoid visual for NPCs (typically quest givers) that
    /// aren't individually mapped in `entries`. Picks an archetype by
    /// hashing the display name, so each giver gets a stable look across
    /// respawns and server restarts — and different givers in the same
    /// hub visually distinct from each other. Returns `None` only when
    /// no humanoid archetypes are defined in the YAML at all.
    pub fn quest_giver_visual(&self, display_name: &str) -> Option<NpcVisual> {
        if self.humanoid_archetypes.is_empty() {
            return None;
        }
        let mut keys: Vec<&str> =
            self.humanoid_archetypes.keys().map(String::as_str).collect();
        keys.sort_unstable();
        let mut hasher = DefaultHasher::new();
        display_name.hash(&mut hasher);
        let idx = (hasher.finish() as usize) % keys.len();
        Some(NpcVisual::Humanoid(NpcAppearance::new(
            keys[idx].to_string(),
            1.0,
        )))
    }
}

#[derive(Debug, Error)]
pub enum NpcMeshMapLoadError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("yaml: {0}")]
    Yaml(#[from] serde_yaml::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> NpcMeshMap {
        let yaml = r#"
humanoid_archetypes:
  peasant_male:
    gender: male
    body: peasant
    legs: peasant
    arms: peasant
    feet: peasant
    hair: simple_parted
    beard: full
    color: v1
  knight_plate_male:
    gender: male
    body: knight
    legs: knight
    arms: knight
    feet: knight
    head_piece: knight_armet
    hair: buzzed
    color: v2
entries:
  "Grey Wolf":
    mesh: GrayWolf
    scale: 1.0
  "Juvenile Grey Wolf":
    mesh: GrayWolf
    scale: 0.7
  "Bandit Recruit":
    humanoid: peasant_male
  "Concord Refuser-Paladin":
    humanoid: knight_plate_male
  "Abbey Cultist":
    mesh: null
  "Missing Archetype Referrer":
    humanoid: nonexistent_key
"#;
        serde_yaml::from_str(yaml).unwrap()
    }

    #[test]
    fn beast_returns_mesh_variant() {
        match sample().lookup("Grey Wolf") {
            Some(NpcVisual::Beast(m)) => {
                assert_eq!(m.species, "GrayWolf");
                assert!((m.scale - 1.0).abs() < 1e-5);
            }
            other => panic!("expected Beast, got {}", other.is_some()),
        }
    }

    #[test]
    fn juvenile_scale_carries() {
        let v = sample().lookup("Juvenile Grey Wolf").unwrap();
        match v {
            NpcVisual::Beast(m) => assert!((m.scale - 0.7).abs() < 1e-5),
            _ => panic!("expected Beast"),
        }
    }

    #[test]
    fn humanoid_ships_archetype_key_only() {
        let v = sample().lookup("Bandit Recruit").unwrap();
        match v {
            NpcVisual::Humanoid(app) => {
                assert_eq!(app.archetype, "peasant_male");
                assert!((app.scale() - 1.0).abs() < 1e-3);
            }
            _ => panic!("expected Humanoid"),
        }
    }

    #[test]
    fn archetype_resolves_to_cosmetics_on_demand() {
        let map = sample();
        let cos = map.resolve_archetype("peasant_male").unwrap();
        assert_eq!(cos.gender, Gender::Male);
        assert_eq!(cos.body.as_ref().map(|s| s.outfit.as_str()), Some("peasant"));
        assert_eq!(cos.hair.as_deref(), Some("simple_parted"));
        assert_eq!(cos.beard.as_deref(), Some("full"));

        let knight = map.resolve_archetype("knight_plate_male").unwrap();
        assert_eq!(
            knight.head_piece.as_ref().map(|s| s.piece.as_str()),
            Some("knight_armet")
        );
    }

    #[test]
    fn explicit_null_returns_none() {
        assert!(sample().lookup("Abbey Cultist").is_none());
    }

    #[test]
    fn unlisted_returns_none() {
        assert!(sample().lookup("Some Unknown Mob").is_none());
    }

    #[test]
    fn missing_archetype_falls_through_to_none() {
        // Warns + returns None rather than panicking; caller uses the
        // cuboid path.
        assert!(sample().lookup("Missing Archetype Referrer").is_none());
    }
}
