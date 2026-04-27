//! Per-zone landmark registry. Loaded from
//! `world/zones/<zone>/landmarks.yaml`. Each landmark is a named point of
//! interest with an offset from the zone origin — used by quest
//! `investigate` / `explore` steps to anchor a turn-in interaction.

use std::{collections::HashMap, fs, path::Path};

use serde::Deserialize;

use crate::{read_dir, Coord2, LoadError};

#[derive(Debug, Clone, Deserialize)]
pub struct Landmark {
    pub id: String,
    pub name: String,
    pub offset_from_zone_origin: Coord2,
}

#[derive(Debug, Clone, Deserialize)]
struct LandmarksFile {
    zone: String,
    landmarks: Vec<Landmark>,
}

/// All landmarks across every zone, keyed by `(zone_id, landmark_id)`.
#[derive(Debug, Default, Clone)]
pub struct LandmarkIndex {
    pub by_id: HashMap<(String, String), Landmark>,
    pub by_zone: HashMap<String, Vec<String>>,
}

impl LandmarkIndex {
    pub fn get(&self, zone_id: &str, landmark_id: &str) -> Option<&Landmark> {
        self.by_id
            .get(&(zone_id.to_string(), landmark_id.to_string()))
    }

    pub fn iter_zone(&self, zone_id: &str) -> impl Iterator<Item = &Landmark> {
        self.by_zone
            .get(zone_id)
            .into_iter()
            .flat_map(move |ids| {
                ids.iter()
                    .filter_map(move |id| self.by_id.get(&(zone_id.to_string(), id.clone())))
            })
    }
}

/// Walk `world_root/zones/<zone>/landmarks.yaml`. Zones without a landmarks
/// file are silently skipped.
pub fn load_all_landmarks(world_root: impl AsRef<Path>) -> Result<LandmarkIndex, LoadError> {
    let world_root = world_root.as_ref();
    let zones_dir = world_root.join("zones");
    let mut out = LandmarkIndex::default();
    for zone_dir in read_dir(&zones_dir)? {
        if !zone_dir.is_dir() {
            continue;
        }
        let zone_name = zone_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        let landmarks_path = zone_dir.join("landmarks.yaml");
        if !landmarks_path.exists() {
            continue;
        }
        let text = fs::read_to_string(&landmarks_path).map_err(|e| LoadError::Io {
            path: landmarks_path.clone(),
            source: e,
        })?;
        let file: LandmarksFile = serde_yaml::from_str(&text).map_err(|e| LoadError::Yaml {
            path: landmarks_path.clone(),
            source: e,
        })?;
        let mut ids = Vec::with_capacity(file.landmarks.len());
        for lm in file.landmarks {
            ids.push(lm.id.clone());
            out.by_id
                .insert((zone_name.clone(), lm.id.clone()), lm);
        }
        if !ids.is_empty() {
            out.by_zone.insert(file.zone, ids);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn generated_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../src/generated")
            .canonicalize()
            .unwrap()
    }

    #[test]
    fn dalewatch_landmarks_load_and_index() {
        let idx = load_all_landmarks(generated_root().join("world")).unwrap();
        let reed = idx
            .get("dalewatch_marches", "dalewatch_reed_brake")
            .expect("reed-brake landmark must load");
        assert_eq!(reed.name, "The Reed-Brake");
        // Landmark referenced by chain_dalewatch_first_ride step 9.
        assert!(idx.get("dalewatch_marches", "blackwash_fens").is_some());
    }
}
