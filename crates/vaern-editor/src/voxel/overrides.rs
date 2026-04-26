//! Per-chunk biome override map — what the BiomePaint mode writes,
//! what the streamer + material attach consult.
//!
//! Granularity: **chunk-XZ column**. A painted biome applies to every
//! Y-stacked chunk at that XZ — biome is conceptually a 2D surface
//! property in the current pipeline (texture choice only, never
//! affects height). Storing as `(i32, i32) → BiomeKey` (XZ chunk
//! coord) instead of full `ChunkCoord` avoids redundant per-Y entries.
//!
//! Persistence: bincode-serialized `Vec<((i32, i32), u8)>` at
//! `src/generated/world/biome_overrides.bin`. The u8 mapping is the
//! stable [`BiomeKey::id`] discriminant.
//!
//! Loaded on Startup. Saved alongside voxel edits when the toolbar
//! Save button fires.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::biomes::BiomeKey;

/// Live biome overrides keyed by (chunk_x, chunk_z).
#[derive(Resource, Default, Debug, Clone)]
pub struct BiomeOverrideMap {
    pub by_xz: HashMap<(i32, i32), BiomeKey>,
}

impl BiomeOverrideMap {
    pub fn get(&self, x: i32, z: i32) -> Option<BiomeKey> {
        self.by_xz.get(&(x, z)).copied()
    }

    pub fn set(&mut self, x: i32, z: i32, biome: BiomeKey) {
        self.by_xz.insert((x, z), biome);
    }

    pub fn len(&self) -> usize {
        self.by_xz.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_xz.is_empty()
    }
}

/// Workspace-canonical disk path. Sibling of `voxel_edits.bin`.
pub fn biome_overrides_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../src/generated/world/biome_overrides.bin")
}

/// Bincode wire format. `Vec<((i32, i32), u8)>` sorted for determinism
/// — re-saves of an unchanged map produce byte-identical files.
#[derive(Debug, Default, Serialize, Deserialize)]
struct OverridesFileV1 {
    entries: Vec<((i32, i32), u8)>,
}

/// Save the override map. Sorts entries for deterministic output.
pub fn save_biome_overrides(
    path: &Path,
    overrides: &BiomeOverrideMap,
) -> std::io::Result<usize> {
    let mut entries: Vec<((i32, i32), u8)> = overrides
        .by_xz
        .iter()
        .map(|(&xz, &biome)| (xz, biome.id()))
        .collect();
    entries.sort_by_key(|(xz, _)| *xz);
    let count = entries.len();
    let payload = OverridesFileV1 { entries };
    let bytes = bincode::serialize(&payload)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, bytes)?;
    Ok(count)
}

/// Load the override map. Missing file returns an empty map (the
/// default state — no painting has happened yet).
pub fn load_biome_overrides(path: &Path) -> std::io::Result<BiomeOverrideMap> {
    if !path.exists() {
        return Ok(BiomeOverrideMap::default());
    }
    let bytes = std::fs::read(path)?;
    let payload: OverridesFileV1 = bincode::deserialize(&bytes)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    let mut map = BiomeOverrideMap::default();
    for (xz, id) in payload.entries {
        if let Some(biome) = BiomeKey::from_id(id) {
            map.by_xz.insert(xz, biome);
        }
    }
    Ok(map)
}

/// Bevy Startup system: load overrides into the resource.
pub fn load_biome_overrides_into_resource(
    mut overrides: ResMut<BiomeOverrideMap>,
    mut log: ResMut<crate::ui::console::ConsoleLog>,
) {
    let path = biome_overrides_path();
    match load_biome_overrides(&path) {
        Ok(map) => {
            if !map.is_empty() {
                let n = map.len();
                *overrides = map;
                log.push(format!("loaded {n} biome overrides"));
            }
        }
        Err(e) => {
            warn!("editor: failed to load biome overrides: {e}");
            log.push(format!("biome overrides load FAILED: {e}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_round_trips_for_every_biome() {
        for b in BiomeKey::ALL {
            let id = b.id();
            assert_eq!(BiomeKey::from_id(id), Some(b));
        }
    }

    #[test]
    fn unknown_id_returns_none() {
        assert!(BiomeKey::from_id(255).is_none());
    }

    #[test]
    fn save_and_load_round_trip() {
        let tmp = std::env::temp_dir().join("vaern-editor-biome-overrides-test.bin");
        let _ = std::fs::remove_file(&tmp);

        let mut map = BiomeOverrideMap::default();
        map.set(1, 2, BiomeKey::Snow);
        map.set(-3, 5, BiomeKey::Marsh);
        save_biome_overrides(&tmp, &map).unwrap();

        let loaded = load_biome_overrides(&tmp).unwrap();
        assert_eq!(loaded.get(1, 2), Some(BiomeKey::Snow));
        assert_eq!(loaded.get(-3, 5), Some(BiomeKey::Marsh));
        assert_eq!(loaded.get(0, 0), None);

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn load_missing_file_returns_empty() {
        let path = std::env::temp_dir().join("vaern-editor-nonexistent-biome-overrides.bin");
        let _ = std::fs::remove_file(&path);
        let loaded = load_biome_overrides(&path).unwrap();
        assert!(loaded.is_empty());
    }

    #[test]
    fn save_is_deterministic_across_runs() {
        let tmp1 = std::env::temp_dir().join("vaern-editor-biome-determinism-1.bin");
        let tmp2 = std::env::temp_dir().join("vaern-editor-biome-determinism-2.bin");
        let _ = std::fs::remove_file(&tmp1);
        let _ = std::fs::remove_file(&tmp2);

        let mut a = BiomeOverrideMap::default();
        a.set(1, 1, BiomeKey::Snow);
        a.set(2, 2, BiomeKey::Marsh);
        a.set(0, 0, BiomeKey::Stone);

        let mut b = BiomeOverrideMap::default();
        b.set(0, 0, BiomeKey::Stone);
        b.set(2, 2, BiomeKey::Marsh);
        b.set(1, 1, BiomeKey::Snow);

        save_biome_overrides(&tmp1, &a).unwrap();
        save_biome_overrides(&tmp2, &b).unwrap();
        assert_eq!(std::fs::read(&tmp1).unwrap(), std::fs::read(&tmp2).unwrap());

        let _ = std::fs::remove_file(&tmp1);
        let _ = std::fs::remove_file(&tmp2);
    }
}
