//! Sparse delta overlay for hand-painted heightfield + biome edits.
//!
//! Per zone, two bincode files hold the cells the editor brush has
//! actually touched:
//!
//! - `world/zones/<zone>/elevation_edits.bin` — `(sub_x, sub_z) → cm`
//! - `world/zones/<zone>/biome_edits.bin` — `(sub_x, sub_z) → biome_id`
//!
//! Everything else is derived procedurally by [`crate::heightfield`].
//! Empty map → file is deleted on save so authors don't carry stale
//! files when nothing was painted.
//!
//! ## Why bincode (not yaml)
//!
//! These are tool-generated coordinate deltas, not editorial content.
//! There's no realistic workflow where an author hand-edits a row
//! like `(470, 80, 4.5)`. Bincode is ~3× smaller on disk, parses
//! faster at editor startup, and matches the existing
//! `voxel_edits.bin` pattern. Hand-authored content (`terrain:` on
//! hubs, lore descriptions, biome polygons) stays yaml.
//!
//! ## Format
//!
//! Versioned via an outer enum so future migrations can add `V2`
//! without breaking existing files. Mirrors the encoding of
//! `vaern-editor::voxel::overrides::OverridesFile`.
//!
//! Coordinate convention:
//! - `(x, z)` are **integer sub-cell keys** matching
//!   `crate::raster::SUB_CELL_SIZE_M = 8` m.
//! - `cm` is centimetres above Y=0 baseline (`i16`, ±327 m range).
//! - `biome_id` is the editor `BiomeKey` u8 (0..=8); see
//!   `vaern-editor::voxel::biomes::BiomeKey`.
//!
//! Determinism: entries are sorted by `(x, z)` ascending on save so
//! the file's bytes are stable across runs and machines.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ElevationEditsV1 {
    /// `(sub_x, sub_z) -> centimetres`. Sorted by key on save.
    pub entries: Vec<((i32, i32), i16)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ElevationEditsFile {
    V1(ElevationEditsV1),
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct BiomeEditsV1 {
    /// `(sub_x, sub_z) -> editor BiomeKey id`. Sorted by key on save.
    pub entries: Vec<((i32, i32), u8)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BiomeEditsFile {
    V1(BiomeEditsV1),
}

/// Path to a zone's `elevation_edits.bin`.
pub fn elevation_edits_path(world_root: &Path, zone_id: &str) -> PathBuf {
    world_root
        .join("zones")
        .join(zone_id)
        .join("elevation_edits.bin")
}

/// Path to a zone's `biome_edits.bin`.
pub fn biome_edits_path(world_root: &Path, zone_id: &str) -> PathBuf {
    world_root
        .join("zones")
        .join(zone_id)
        .join("biome_edits.bin")
}

/// Load a zone's elevation edits as a `(sub_x, sub_z) → metres` map.
/// Returns an empty map for missing or unparseable files.
pub fn load_elevation_edits(world_root: &Path, zone_id: &str) -> HashMap<(i32, i32), f32> {
    let path = elevation_edits_path(world_root, zone_id);
    let mut map = HashMap::new();
    let Ok(bytes) = fs::read(&path) else {
        return map;
    };
    let Ok(file) = bincode::deserialize::<ElevationEditsFile>(&bytes) else {
        eprintln!("warn: failed to parse {}", path.display());
        return map;
    };
    let ElevationEditsFile::V1(v1) = file;
    for ((x, z), cm) in v1.entries {
        map.insert((x, z), cm as f32 * 0.01);
    }
    map
}

/// Load a zone's biome edits as a `(sub_x, sub_z) → BiomeKey id` map.
pub fn load_biome_edits(world_root: &Path, zone_id: &str) -> HashMap<(i32, i32), u8> {
    let path = biome_edits_path(world_root, zone_id);
    let mut map = HashMap::new();
    let Ok(bytes) = fs::read(&path) else {
        return map;
    };
    let Ok(file) = bincode::deserialize::<BiomeEditsFile>(&bytes) else {
        eprintln!("warn: failed to parse {}", path.display());
        return map;
    };
    let BiomeEditsFile::V1(v1) = file;
    for ((x, z), id) in v1.entries {
        map.insert((x, z), id);
    }
    map
}

/// Persist a zone's elevation edits. Sorts by `(x, z)` ascending on
/// save. Empty map → file is deleted (or never created).
pub fn save_elevation_edits(
    world_root: &Path,
    zone_id: &str,
    edits: &HashMap<(i32, i32), f32>,
) -> std::io::Result<usize> {
    let path = elevation_edits_path(world_root, zone_id);
    if edits.is_empty() {
        if path.exists() {
            fs::remove_file(&path)?;
        }
        return Ok(0);
    }
    // Convert metres → centimetres, drop near-zero entries (within
    // ±0.5 cm of the procedural baseline they're not worth a row).
    let mut entries: Vec<((i32, i32), i16)> = edits
        .iter()
        .filter_map(|(&xz, &m)| {
            let cm = (m * 100.0).round();
            if cm.abs() < 1.0 {
                return None;
            }
            let clamped = cm.clamp(i16::MIN as f32, i16::MAX as f32);
            Some((xz, clamped as i16))
        })
        .collect();
    entries.sort_by_key(|(xz, _)| *xz);
    let count = entries.len();
    if count == 0 {
        if path.exists() {
            fs::remove_file(&path)?;
        }
        return Ok(0);
    }
    let payload = ElevationEditsFile::V1(ElevationEditsV1 { entries });
    let bytes = bincode::serialize(&payload)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, bytes)?;
    Ok(count)
}

/// Persist a zone's biome edits. Sorts by `(x, z)` ascending. Empty
/// map removes the file.
pub fn save_biome_edits(
    world_root: &Path,
    zone_id: &str,
    edits: &HashMap<(i32, i32), u8>,
) -> std::io::Result<usize> {
    let path = biome_edits_path(world_root, zone_id);
    if edits.is_empty() {
        if path.exists() {
            fs::remove_file(&path)?;
        }
        return Ok(0);
    }
    let mut entries: Vec<((i32, i32), u8)> =
        edits.iter().map(|(&xz, &id)| (xz, id)).collect();
    entries.sort_by_key(|(xz, _)| *xz);
    let count = entries.len();
    let payload = BiomeEditsFile::V1(BiomeEditsV1 { entries });
    let bytes = bincode::serialize(&payload)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, bytes)?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_root() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("zones").join("synth")).unwrap();
        dir
    }

    #[test]
    fn elevation_edits_round_trip() {
        let dir = tmp_root();
        let mut edits: HashMap<(i32, i32), f32> = HashMap::new();
        edits.insert((10, 20), 4.5);
        edits.insert((10, 21), 5.0);
        edits.insert((11, 20), 4.7);
        let n = save_elevation_edits(dir.path(), "synth", &edits).unwrap();
        assert_eq!(n, 3);
        let loaded = load_elevation_edits(dir.path(), "synth");
        assert_eq!(loaded.len(), 3);
        // Centimetre rounding may shave a fraction; allow ±0.01.
        assert!((loaded.get(&(10, 20)).unwrap() - 4.5).abs() < 0.01);
        assert!((loaded.get(&(11, 20)).unwrap() - 4.7).abs() < 0.01);
    }

    #[test]
    fn biome_edits_round_trip() {
        let dir = tmp_root();
        let mut edits: HashMap<(i32, i32), u8> = HashMap::new();
        edits.insert((10, 20), 8); // rocky
        edits.insert((10, 21), 7); // marsh
        let n = save_biome_edits(dir.path(), "synth", &edits).unwrap();
        assert_eq!(n, 2);
        let loaded = load_biome_edits(dir.path(), "synth");
        assert_eq!(loaded.get(&(10, 20)), Some(&8));
        assert_eq!(loaded.get(&(10, 21)), Some(&7));
    }

    #[test]
    fn empty_edits_removes_file() {
        let dir = tmp_root();
        let mut edits: HashMap<(i32, i32), f32> = HashMap::new();
        edits.insert((10, 20), 4.5);
        save_elevation_edits(dir.path(), "synth", &edits).unwrap();
        assert!(elevation_edits_path(dir.path(), "synth").exists());
        let empty: HashMap<(i32, i32), f32> = HashMap::new();
        save_elevation_edits(dir.path(), "synth", &empty).unwrap();
        assert!(!elevation_edits_path(dir.path(), "synth").exists());
    }

    #[test]
    fn sorted_output_is_byte_deterministic() {
        let dir = tmp_root();
        let mut a: HashMap<(i32, i32), f32> = HashMap::new();
        a.insert((10, 20), 4.5);
        a.insert((11, 20), 4.7);
        a.insert((10, 21), 5.0);
        save_elevation_edits(dir.path(), "synth", &a).unwrap();
        let bytes_a = fs::read(elevation_edits_path(dir.path(), "synth")).unwrap();

        let mut b: HashMap<(i32, i32), f32> = HashMap::new();
        // Insert in different order; serialised result must match.
        b.insert((11, 20), 4.7);
        b.insert((10, 21), 5.0);
        b.insert((10, 20), 4.5);
        save_elevation_edits(dir.path(), "synth", &b).unwrap();
        let bytes_b = fs::read(elevation_edits_path(dir.path(), "synth")).unwrap();
        assert_eq!(bytes_a, bytes_b);
    }

    #[test]
    fn missing_file_returns_empty_map() {
        let dir = tmp_root();
        let m = load_elevation_edits(dir.path(), "synth");
        assert!(m.is_empty());
    }

    #[test]
    fn near_zero_elevations_are_dropped() {
        // Heights within ±0.5 cm of baseline aren't worth storing —
        // they round to 0 cm and should be filtered before write.
        let dir = tmp_root();
        let mut edits: HashMap<(i32, i32), f32> = HashMap::new();
        edits.insert((10, 20), 0.001); // 0.1 cm — drop
        edits.insert((11, 20), 0.05); // 5 cm — keep
        let n = save_elevation_edits(dir.path(), "synth", &edits).unwrap();
        assert_eq!(n, 1, "only one cell should be persisted");
        let loaded = load_elevation_edits(dir.path(), "synth");
        assert!(loaded.contains_key(&(11, 20)));
        assert!(!loaded.contains_key(&(10, 20)));
    }
}
