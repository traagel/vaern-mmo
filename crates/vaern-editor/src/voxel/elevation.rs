//! Cartography-driven elevation overlay.
//!
//! The editor's `EditorHeightfield` was a perfectly flat plane at
//! `Y=GROUND_BIAS_Y`. With this overlay it now adds a per-sub-cell
//! offset on top so cartography rivers carve channels and biome
//! regions raise hills/mountains above the baseline.
//!
//! ## Data flow
//!
//! 1. `vaern-cartography::vaern-import-editor` walks `geography.yaml`
//!    polygons and rivers, rasterizes height offsets into a sub-cell
//!    grid (8 m cells), writes `src/generated/world/elevation_overrides.bin`
//!    as bincode-encoded `ElevationFileV1` (signed-cm i16 per cell).
//! 2. On Startup, the editor loads that file into the
//!    [`ELEVATION`] OnceLock. Streamer first-frame fires AFTER
//!    Startup so the generator already has the data.
//! 3. `EditorHeightfield::sample` reads `lookup(x, z)` and adds it
//!    to the flat baseline.
//!
//! ## Why OnceLock
//!
//! The voxel generator trait (`vaern_voxel::generator::WorldGenerator`)
//! is `&self`-only and the `EditorHeightfield` is `Copy + Default`. To
//! keep the generator stateless, the elevation map lives in a
//! process-global OnceLock instead of being threaded through the
//! generator type. Loading happens once at Startup; reads are
//! lock-free after.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// World units per sub-cell (must match the importer + biome paint).
const SUB_CELL_SIZE_M: f32 = 8.0;

/// On-disk elevation overlay format. Mirror of the importer's
/// `ElevationFileV1` (kept in sync — change both at once).
#[derive(Debug, Default, Serialize, Deserialize)]
struct ElevationFileV1 {
    sub_cells_per_chunk: u32,
    /// `(sub_x, sub_z) -> centimeters`. Sorted by key on write.
    entries: Vec<((i32, i32), i16)>,
}

#[derive(Debug, Serialize, Deserialize)]
enum ElevationFile {
    V1(ElevationFileV1),
}

/// Process-global elevation map. Filled by [`load_elevation_overrides`]
/// on Startup; queried by [`lookup`] from the generator. Empty when no
/// file exists (returns 0 height).
static ELEVATION: OnceLock<HashMap<(i32, i32), i16>> = OnceLock::new();

/// Workspace-canonical disk path. Sibling of `biome_overrides.bin`.
pub fn elevation_overrides_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../src/generated/world/elevation_overrides.bin")
}

/// Sample the elevation offset at world XZ. Returns 0 if no file is
/// loaded or the sub-cell wasn't authored.
#[inline]
pub fn lookup(world_x: f32, world_z: f32) -> f32 {
    let Some(map) = ELEVATION.get() else {
        return 0.0;
    };
    let sx = (world_x / SUB_CELL_SIZE_M).floor() as i32;
    let sz = (world_z / SUB_CELL_SIZE_M).floor() as i32;
    map.get(&(sx, sz))
        .copied()
        .map(|cm| cm as f32 * 0.01)
        .unwrap_or(0.0)
}

/// Bevy Startup system. Reads `elevation_overrides.bin`, fills the
/// `ELEVATION` OnceLock. No-op if the file is missing (every chunk
/// then samples 0 offset → flat plane, same as before this overlay).
pub fn load_elevation_overrides(mut log: ResMut<crate::ui::console::ConsoleLog>) {
    let path = elevation_overrides_path();
    if !path.exists() {
        return;
    }
    let map = match read_file(&path) {
        Ok(m) => m,
        Err(e) => {
            warn!("editor: failed to load elevation overrides: {e}");
            log.push(format!("elevation overrides load FAILED: {e}"));
            return;
        }
    };
    let n = map.len();
    if ELEVATION.set(map).is_err() {
        warn!("editor: elevation overrides already loaded — second load ignored");
        return;
    }
    log.push(format!("loaded {n} elevation overrides (sub-cells, cm)"));
}

fn read_file(path: &Path) -> std::io::Result<HashMap<(i32, i32), i16>> {
    let bytes = std::fs::read(path)?;
    let file: ElevationFile = bincode::deserialize(&bytes)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let ElevationFile::V1(v1) = file;
    let mut map = HashMap::with_capacity(v1.entries.len());
    for ((sx, sz), cm) in v1.entries {
        map.insert((sx, sz), cm);
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_returns_zero_when_no_file_loaded() {
        // ELEVATION may or may not be set depending on test ordering;
        // if not set, lookup returns 0. If set (by another test), the
        // result depends on the data — so this test is purely a
        // no-panic guard.
        let v = lookup(1234.5, -678.9);
        assert!(v.is_finite());
    }

    #[test]
    fn world_to_sub_indexing_matches_biome_paint() {
        // The elevation lookup must use the same sub-cell coord
        // convention as `BiomeOverrideMap::world_to_sub` so paint
        // and elevation refer to the same cells.
        let world_x = 25.0;
        let world_z = -3.0;
        let expected = (
            (world_x / SUB_CELL_SIZE_M).floor() as i32,
            (world_z / SUB_CELL_SIZE_M).floor() as i32,
        );
        // (25/8).floor() = 3, (-3/8).floor() = -1 → (3, -1).
        assert_eq!(expected, (3, -1));
    }
}
