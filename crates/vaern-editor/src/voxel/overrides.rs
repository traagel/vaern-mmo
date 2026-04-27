//! Per-sub-cell biome override map — what the BiomePaint mode writes,
//! what `compute_blend_weights` consults.
//!
//! ## Granularity
//!
//! Storage is **sub-chunk-XZ**: each chunk's XZ footprint is divided
//! into [`SUB_CELLS_PER_CHUNK`]² square cells. With CHUNK_DIM=32 world
//! units and SUB_CELLS_PER_CHUNK=4, each sub-cell is 8 m on a side.
//! The painted biome is stored per sub-cell.
//!
//! ## Why sub-chunk
//!
//! V1 stored one biome per chunk-XZ column. The blend shader smoothed
//! transitions per-vertex but every painted unit was 32m square,
//! producing a visible "grid of full chunks" when painting an area.
//! Sub-chunk storage lets the brush write at 8m resolution; the blend
//! shader's per-vertex math now operates on a 4× finer Voronoi grid,
//! shrinking blend zones from 32m to 8m and giving the brush real
//! authoring resolution.
//!
//! ## Persistence
//!
//! On-disk format `OverridesFileV2`: header field `sub_cells_per_chunk`
//! + bincode-serialized `Vec<((i32, i32), u8)>` of sorted sub-cell
//! entries. Legacy `OverridesFileV1` (per-chunk-XZ) is detected on
//! load and upscaled — each chunk entry expands to `N²` sub-cells of
//! the same biome.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use vaern_voxel::config::CHUNK_WORLD_SIZE;

use super::biomes::BiomeKey;

/// Sub-cells per chunk axis. 4 = 8 m per cell (CHUNK_WORLD_SIZE / 4).
/// Keep as a `pub const` rather than configurable so the on-disk
/// format and the blend math stay coupled — changing this requires a
/// migration pass on existing `biome_overrides.bin` files.
pub const SUB_CELLS_PER_CHUNK: u32 = 4;

/// World-space side length of one sub-cell.
pub const SUB_CELL_SIZE_M: f32 = CHUNK_WORLD_SIZE / SUB_CELLS_PER_CHUNK as f32;

/// Live biome overrides keyed by (sub_cell_x, sub_cell_z). Sub-cell
/// coordinates are integer indices on a global grid: chunk (cx, cz)
/// owns sub-cells `[cx*N..cx*N+N, cz*N..cz*N+N)` where N =
/// `SUB_CELLS_PER_CHUNK`.
#[derive(Resource, Default, Debug, Clone)]
pub struct BiomeOverrideMap {
    pub by_sub: HashMap<(i32, i32), BiomeKey>,
}

impl BiomeOverrideMap {
    /// Read the override at sub-cell `(sub_x, sub_z)`. Returns `None`
    /// if no paint stroke has touched this sub-cell.
    pub fn get(&self, sub_x: i32, sub_z: i32) -> Option<BiomeKey> {
        self.by_sub.get(&(sub_x, sub_z)).copied()
    }

    /// Write `biome` at sub-cell `(sub_x, sub_z)`.
    pub fn set(&mut self, sub_x: i32, sub_z: i32, biome: BiomeKey) {
        self.by_sub.insert((sub_x, sub_z), biome);
    }

    /// Remove an override at sub-cell `(sub_x, sub_z)` — reverts that
    /// cell to the default biome (Marsh) at runtime. Eraser uses this.
    pub fn clear(&mut self, sub_x: i32, sub_z: i32) {
        self.by_sub.remove(&(sub_x, sub_z));
    }

    pub fn len(&self) -> usize {
        self.by_sub.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_sub.is_empty()
    }

    /// Convenience: world-XZ → sub-cell coord. Used by the brush to
    /// map a click position to a sub-cell index.
    pub fn world_to_sub(world_x: f32, world_z: f32) -> (i32, i32) {
        (
            (world_x / SUB_CELL_SIZE_M).floor() as i32,
            (world_z / SUB_CELL_SIZE_M).floor() as i32,
        )
    }

    /// Convenience: sub-cell center in world-XZ coords. Used by the
    /// blend math to compute distance from a vertex to each candidate
    /// sub-cell.
    pub fn sub_cell_center(sub_x: i32, sub_z: i32) -> (f32, f32) {
        (
            (sub_x as f32 + 0.5) * SUB_CELL_SIZE_M,
            (sub_z as f32 + 0.5) * SUB_CELL_SIZE_M,
        )
    }
}

/// Workspace-canonical disk path. Sibling of `voxel_edits.bin`.
pub fn biome_overrides_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../src/generated/world/biome_overrides.bin")
}

/// V2: sub-cell granularity. `sub_cells_per_chunk` is the header — a
/// future change to that constant would require a migration pass.
#[derive(Debug, Default, Serialize, Deserialize)]
struct OverridesFileV2 {
    sub_cells_per_chunk: u32,
    entries: Vec<((i32, i32), u8)>,
}

/// Legacy V1 format kept for migration. Per-chunk-XZ entries.
#[derive(Debug, Default, Serialize, Deserialize)]
struct OverridesFileV1 {
    entries: Vec<((i32, i32), u8)>,
}

/// Versioned save wrapper. Bincode tags the variant index, so V1 and
/// V2 are distinguishable on load. New writes always emit V2.
#[derive(Debug, Serialize, Deserialize)]
enum OverridesFile {
    V1(OverridesFileV1),
    V2(OverridesFileV2),
}

/// Save the override map. Sorts entries for deterministic output.
/// Always writes V2 format.
pub fn save_biome_overrides(
    path: &Path,
    overrides: &BiomeOverrideMap,
) -> std::io::Result<usize> {
    let mut entries: Vec<((i32, i32), u8)> = overrides
        .by_sub
        .iter()
        .map(|(&xz, &biome)| (xz, biome.id()))
        .collect();
    entries.sort_by_key(|(xz, _)| *xz);
    let count = entries.len();
    let payload = OverridesFile::V2(OverridesFileV2 {
        sub_cells_per_chunk: SUB_CELLS_PER_CHUNK,
        entries,
    });
    let bytes = bincode::serialize(&payload)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, bytes)?;
    Ok(count)
}

/// Load the override map. Tries V2 first; on failure falls back to
/// V1 and upscales each chunk entry to N×N sub-cells of the same
/// biome. Missing file returns an empty map.
pub fn load_biome_overrides(path: &Path) -> std::io::Result<BiomeOverrideMap> {
    if !path.exists() {
        return Ok(BiomeOverrideMap::default());
    }
    let bytes = std::fs::read(path)?;
    let mut map = BiomeOverrideMap::default();

    // Try the versioned wrapper (V1 or V2 enum variant).
    if let Ok(file) = bincode::deserialize::<OverridesFile>(&bytes) {
        match file {
            OverridesFile::V2(v2) => {
                if v2.sub_cells_per_chunk != SUB_CELLS_PER_CHUNK {
                    warn!(
                        "biome overrides V2: file has {} sub-cells/chunk, runtime expects {}; loading anyway",
                        v2.sub_cells_per_chunk, SUB_CELLS_PER_CHUNK
                    );
                }
                for (xz, id) in v2.entries {
                    if let Some(biome) = BiomeKey::from_id(id) {
                        map.by_sub.insert(xz, biome);
                    }
                }
            }
            OverridesFile::V1(v1) => {
                upscale_v1_into(&v1, &mut map);
            }
        }
        return Ok(map);
    }

    // Bare V1 (no enum wrapper) — older files written before the
    // versioned format. Try deserializing directly.
    if let Ok(v1) = bincode::deserialize::<OverridesFileV1>(&bytes) {
        upscale_v1_into(&v1, &mut map);
        return Ok(map);
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "biome_overrides.bin: not V1 or V2 format",
    ))
}

/// Upscale a V1 (per-chunk) map into a V2 (per-sub-cell) map by
/// expanding each chunk entry into N×N sub-cells of the same biome.
/// Deterministic — same input always produces same output.
fn upscale_v1_into(v1: &OverridesFileV1, dst: &mut BiomeOverrideMap) {
    let n = SUB_CELLS_PER_CHUNK as i32;
    for &((cx, cz), id) in &v1.entries {
        let Some(biome) = BiomeKey::from_id(id) else {
            continue;
        };
        for dz in 0..n {
            for dx in 0..n {
                dst.by_sub.insert((cx * n + dx, cz * n + dz), biome);
            }
        }
    }
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
                log.push(format!("loaded {n} biome overrides (sub-cells)"));
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
    fn world_to_sub_round_trip_at_centers() {
        // Sub-cell (3, 5) center is at world ((3.5)*8, (5.5)*8) = (28, 44).
        let (cx, cz) = BiomeOverrideMap::sub_cell_center(3, 5);
        assert_eq!(cx, 3.5 * SUB_CELL_SIZE_M);
        assert_eq!(cz, 5.5 * SUB_CELL_SIZE_M);
        assert_eq!(BiomeOverrideMap::world_to_sub(cx, cz), (3, 5));
    }

    #[test]
    fn save_and_load_round_trip_v2() {
        let tmp = std::env::temp_dir().join("vaern-editor-biome-overrides-v2-test.bin");
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

    #[test]
    fn v1_legacy_file_upscales_to_n_squared_sub_cells() {
        let tmp = std::env::temp_dir().join("vaern-editor-biome-overrides-v1-legacy.bin");
        let _ = std::fs::remove_file(&tmp);

        // Hand-write a V1 file: one chunk (5, 7) painted Snow.
        let v1 = OverridesFileV1 {
            entries: vec![((5, 7), BiomeKey::Snow.id())],
        };
        let bytes = bincode::serialize(&OverridesFile::V1(v1)).unwrap();
        std::fs::write(&tmp, bytes).unwrap();

        let loaded = load_biome_overrides(&tmp).unwrap();
        let n = SUB_CELLS_PER_CHUNK as i32;
        // Should upscale to N×N sub-cells, all Snow.
        for dz in 0..n {
            for dx in 0..n {
                assert_eq!(
                    loaded.get(5 * n + dx, 7 * n + dz),
                    Some(BiomeKey::Snow),
                    "missing sub-cell ({dx},{dz}) of chunk (5,7) after V1 upscale"
                );
            }
        }
        // Outside the chunk, no overrides.
        assert_eq!(loaded.get(5 * n - 1, 7 * n), None);
        assert_eq!(loaded.get(5 * n, 7 * n + n), None);
        assert_eq!(loaded.len(), (n * n) as usize);

        let _ = std::fs::remove_file(&tmp);
    }
}
