//! Disk persistence for authored voxel edits.
//!
//! The runtime world's terrain shape is **procedural** by default —
//! `HeightfieldGenerator` produces the SDF for every chunk on first
//! seed. Authored edits (editor sculpting, future scripting) diverge
//! from that procedural baseline. This module saves *only the
//! divergence*: per-chunk `ChunkDelta`s where stored samples differ
//! from the generator's fresh output.
//!
//! That keeps the on-disk file small (only edited chunks land in it)
//! and makes the procedural baseline the natural "blank canvas" for
//! authoring.
//!
//! # Pipeline
//!
//! ```text
//!   editor ChunkStore ──diff_against_generator──→ Vec<ChunkDelta>
//!                                                       │
//!                                                       ▼
//!                                            bincode + atomic write
//!                                                       │
//!                                                       ▼
//!                              src/generated/world/voxel_edits.bin
//!                                                       │
//!                              ┌────────────────────────┴────────────────────┐
//!                              ▼                                             ▼
//!                       editor on Startup                         server on Startup
//!                       apply_into_store                          apply_into_store
//!                                                                 + register in EditedChunks
//!                                                                 (so existing reconnect
//!                                                                  snapshot path broadcasts
//!                                                                  to every client on connect)
//! ```
//!
//! The disk format is `bincode`-serialized `Vec<ChunkDelta>` (the same
//! type the network layer ships, so wire and disk stay consistent).
//! `ChunkDelta::apply_to` is replay-safe via its `version` gate; load
//! sets each chunk's version to the saved version so subsequent edits
//! advance correctly.

use std::io::Write;
use std::path::Path;

use bevy::math::IVec3;

use crate::chunk::{ChunkCoord, ChunkStore, DirtyChunks, VoxelChunk};
use crate::config::CHUNK_TOTAL_SAMPLES;
use crate::generator::WorldGenerator;
use crate::replication::{encode_delta, ChunkDelta};

/// Sample-equality epsilon. Two SDF samples within this distance are
/// considered identical when diffing against the procedural baseline,
/// so floating-point churn from `seed_chunk` doesn't pollute the file
/// with no-op writes.
pub const DIVERGENCE_EPSILON: f32 = 1e-4;

/// Walk every chunk in `store`, compute its divergence from a fresh
/// generator-seeded baseline, return one `ChunkDelta` per divergent
/// chunk (untouched chunks are skipped).
///
/// The deltas use whichever encoding (sparse vs full snapshot) is
/// smaller, picked by [`encode_delta`].
pub fn diff_against_generator<G: WorldGenerator>(
    store: &ChunkStore,
    generator: &G,
) -> Vec<ChunkDelta> {
    let mut out = Vec::new();
    for (coord, chunk) in store.iter() {
        let mut baseline = VoxelChunk::new_air();
        generator.seed_chunk(*coord, &mut baseline);

        let mut writes: Vec<(u32, f32)> = Vec::new();
        for i in 0..CHUNK_TOTAL_SAMPLES {
            let stored = chunk.samples[i];
            let base = baseline.samples[i];
            if (stored - base).abs() > DIVERGENCE_EPSILON {
                writes.push((i as u32, stored));
            }
        }
        if writes.is_empty() {
            continue;
        }
        out.push(encode_delta(coord.0, chunk, &writes));
    }
    out
}

/// Apply a list of authored deltas to `store`. For each delta:
/// 1. Ensure the destination chunk is seeded from `generator`.
/// 2. `delta.apply_to(chunk)` — replay-safe (version-gated).
/// 3. Mark the chunk dirty so the mesher re-extracts.
///
/// Returns the count of chunks actually updated (some may have been
/// dropped by the version gate if the store already had newer data).
pub fn apply_into_store<G: WorldGenerator>(
    deltas: &[ChunkDelta],
    store: &mut ChunkStore,
    dirty: &mut DirtyChunks,
    generator: &G,
) -> usize {
    let mut applied = 0usize;
    for delta in deltas {
        let coord = ChunkCoord(IVec3::from_array(delta.coord));
        if !store.contains(coord) {
            let mut chunk = VoxelChunk::new_air();
            generator.seed_chunk(coord, &mut chunk);
            store.insert(coord, chunk);
        }
        if let Some(chunk) = store.get_mut(coord) {
            let prev = chunk.version;
            delta.apply_to(chunk);
            if chunk.version > prev {
                dirty.mark(coord);
                applied += 1;
            }
        }
    }
    applied
}

/// Save deltas atomically. Writes to `<path>.tmp` first, then renames
/// over the destination so a crash mid-write can't corrupt the file.
pub fn save_to_disk(path: &Path, deltas: &[ChunkDelta]) -> Result<(), PersistenceError> {
    let bytes = bincode::serialize(deltas).map_err(|source| PersistenceError::Encode { source })?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| PersistenceError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp_path: std::path::PathBuf = tmp.into();
    {
        let mut file = std::fs::File::create(&tmp_path).map_err(|source| PersistenceError::Io {
            path: tmp_path.clone(),
            source,
        })?;
        file.write_all(&bytes).map_err(|source| PersistenceError::Io {
            path: tmp_path.clone(),
            source,
        })?;
        file.sync_all().map_err(|source| PersistenceError::Io {
            path: tmp_path.clone(),
            source,
        })?;
    }
    std::fs::rename(&tmp_path, path).map_err(|source| PersistenceError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(())
}

/// Load deltas from disk. Returns an empty Vec if the file does not
/// exist (treat "no edits authored yet" as a normal state, not an
/// error).
pub fn load_from_disk(path: &Path) -> Result<Vec<ChunkDelta>, PersistenceError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let bytes = std::fs::read(path).map_err(|source| PersistenceError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    bincode::deserialize::<Vec<ChunkDelta>>(&bytes)
        .map_err(|source| PersistenceError::Decode { source })
}

/// Errors emitted by [`save_to_disk`] / [`load_from_disk`].
#[derive(Debug, thiserror::Error)]
pub enum PersistenceError {
    #[error("io at {path:?}: {source}")]
    Io {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
    #[error("bincode encode: {source}")]
    Encode {
        source: bincode::Error,
    },
    #[error("bincode decode: {source}")]
    Decode {
        source: bincode::Error,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::ChunkShape;
    use crate::generator::HeightfieldGenerator;

    #[test]
    fn pristine_chunk_diff_is_empty() {
        let mut store = ChunkStore::new();
        let generator = HeightfieldGenerator::new();
        let coord = ChunkCoord::new(0, 0, 0);
        let mut chunk = VoxelChunk::new_air();
        generator.seed_chunk(coord, &mut chunk);
        store.insert(coord, chunk);

        let deltas = diff_against_generator(&store, &generator);
        assert!(deltas.is_empty(), "fresh-seeded chunk should match baseline");
    }

    #[test]
    fn edited_chunk_appears_in_diff() {
        let mut store = ChunkStore::new();
        let generator = HeightfieldGenerator::new();
        let coord = ChunkCoord::new(0, 0, 0);
        let mut chunk = VoxelChunk::new_air();
        generator.seed_chunk(coord, &mut chunk);
        // Hand-edit a sample: write a value that the generator
        // definitely didn't produce.
        chunk.set([5, 5, 5], -100.0);
        store.insert(coord, chunk);

        let deltas = diff_against_generator(&store, &generator);
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].coord, [0, 0, 0]);
    }

    #[test]
    fn diff_then_apply_round_trips() {
        // Edit a chunk, diff, apply to a fresh store, verify the
        // edited sample matches.
        let mut src_store = ChunkStore::new();
        let generator = HeightfieldGenerator::new();
        let coord = ChunkCoord::new(2, 0, -1);
        let mut chunk = VoxelChunk::new_air();
        generator.seed_chunk(coord, &mut chunk);
        chunk.set([10, 10, 10], -42.0);
        src_store.insert(coord, chunk);

        let deltas = diff_against_generator(&src_store, &generator);
        assert_eq!(deltas.len(), 1);

        let mut dst_store = ChunkStore::new();
        let mut dirty = DirtyChunks::new();
        let n = apply_into_store(&deltas, &mut dst_store, &mut dirty, &generator);
        assert_eq!(n, 1);

        let restored = dst_store.get(coord).unwrap();
        let v = restored.get([10, 10, 10]);
        assert!(
            (v - (-42.0)).abs() < 1e-3,
            "expected -42.0 after round-trip, got {v}"
        );
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = std::env::temp_dir().join(format!("vaern_voxel_persistence_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("voxel_edits.bin");

        let coord = ChunkCoord::new(1, 0, 1);
        let mut chunk = VoxelChunk::new_air();
        let idx = ChunkShape::linearize([3, 3, 3]);
        // Apply a sparse write directly so the version increments.
        chunk.set([3, 3, 3], -7.0);
        let _ = idx;
        let delta = ChunkDelta::full_snapshot(coord.0, &chunk);
        let original = vec![delta];

        save_to_disk(&path, &original).unwrap();
        let loaded = load_from_disk(&path).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].coord, [1, 0, 1]);

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn load_missing_file_returns_empty() {
        let path = std::path::Path::new("/tmp/definitely_does_not_exist_voxel_edits.bin");
        let loaded = load_from_disk(path).unwrap();
        assert!(loaded.is_empty());
    }
}
