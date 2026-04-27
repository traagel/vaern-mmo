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

use crate::chunk::{ChunkCoord, ChunkShape, ChunkStore, DirtyChunks, VoxelChunk};
use crate::config::{CHUNK_DIM, CHUNK_TOTAL_SAMPLES, PADDING};
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
        // Uniform-vs-uniform fast path: if both stored and baseline are
        // uniform with equal values, no writes diverge — skip the
        // CHUNK_TOTAL_SAMPLES loop entirely. This is the common case
        // for unedited stack chunks above/below the surface band.
        if let (Some(stored_u), Some(base_u)) = (chunk.uniform_value(), baseline.uniform_value()) {
            if (stored_u - base_u).abs() <= DIVERGENCE_EPSILON {
                continue;
            }
        }
        for i in 0..CHUNK_TOTAL_SAMPLES {
            let stored = chunk.sample_at_stride(i);
            let base = baseline.sample_at_stride(i);
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
/// Every coord referenced in the deltas is marked dirty regardless of
/// whether `delta.apply_to` actually advanced the chunk's version.
/// Reason: the load path inserts fresh chunks into the store via
/// `seed_chunk`, and the streamer's "if store.contains(coord) skip"
/// check would then prevent it from ever marking those chunks dirty.
/// Without this unconditional dirty-mark, chunks loaded with no-op
/// deltas (delta version ≤ seeded version) sit in the store forever
/// without a render entity → invisible "void chunks" in the loaded
/// area. See diagnostic log "loaded N/M chunk edits" — when N ≪ M,
/// this code path is what kept the M-N chunks dirty.
///
/// Returns the count of deltas whose `apply_to` actually advanced the
/// chunk's version (a strict subset of the delta count).
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
                applied += 1;
            }
        }
        // Always mark dirty — the chunk is in the store now, the
        // mesher needs to extract a mesh for it. (See doc-comment.)
        dirty.mark(coord);
    }
    applied
}

/// Re-sync padding samples between every adjacent pair of chunks in
/// the store. Each chunk's `+X / +Y / +Z` padding row is overwritten
/// with the matching content row of the neighbor on that axis (and
/// vice versa for the `-X / -Y / -Z` padding). This is **idempotent**
/// and **order-independent**: a chunk's content is the authoritative
/// value for the world position; both neighbors' paddings derive from
/// it.
///
/// ## Why this exists
///
/// Surface Nets at a chunk boundary needs the +axis padding of chunk
/// A to equal the first content row of chunk B (they're at the same
/// world position). The brush halo-routes writes to both at edit time
/// via `chunks_containing_voxel`, so this normally holds. But the
/// save/load round-trip can break the symmetry — the most common case
/// is one of two adjacent chunks getting saved (with carved boundary
/// values) while the other isn't (because its only modification was
/// halo-only and fell below the divergence epsilon, OR it just wasn't
/// touched at all and gets streamer-seeded fresh on reload). The
/// resulting halo mismatch produces a 1-voxel-wide gap in the mesh
/// at the chunk boundary — visible as a slit you can see through.
///
/// Calling this after every load (and after the streamer creates a
/// new chunk) defensively guarantees the halos are always consistent
/// with the loaded content data.
///
/// Returns the count of (chunk-pair, axis) syncs performed (4× per
/// pair in practice — both axes' both-direction samples). Cheap:
/// ~1156 sample copies per pair per axis, ~3 axes per chunk = ~7k
/// copies per chunk × store.len(). At 5k chunks that's ~35M ops,
/// ~300ms on a typical CPU. One-time cost on load.
pub fn sync_chunk_halos(store: &mut ChunkStore) -> usize {
    let coords: Vec<ChunkCoord> = store.coords().collect();
    let mut sync_count = 0usize;
    for coord in coords {
        for axis in 0..3 {
            sync_count += sync_pair_along_axis(store, coord, axis);
        }
    }
    sync_count
}

/// Sync `coord` with all 6 axis-neighbors (where they exist). Use
/// after the streamer seeds a single new chunk so its padding rows
/// inherit any already-loaded neighbor's edited content.
///
/// Returns the count of neighbor pairs synced (0..=6).
pub fn sync_chunk_halos_for_one(store: &mut ChunkStore, coord: ChunkCoord) -> usize {
    if !store.contains(coord) {
        return 0;
    }
    let mut synced = 0usize;
    for axis in 0..3 {
        // +axis neighbor (we are the "lower" coord).
        synced += sync_pair_along_axis(store, coord, axis);
        // -axis neighbor (we are the "upper" coord).
        let mut neighbor = coord;
        match axis {
            0 => neighbor.0.x -= 1,
            1 => neighbor.0.y -= 1,
            2 => neighbor.0.z -= 1,
            _ => unreachable!(),
        }
        synced += sync_pair_along_axis(store, neighbor, axis);
    }
    synced
}

/// Sync the (coord, coord + axis_unit) pair along one axis. Copies
/// each chunk's content boundary into the other's padding. Returns 1
/// if the pair was synced, 0 if the +axis neighbor wasn't in the
/// store.
fn sync_pair_along_axis(store: &mut ChunkStore, coord_a: ChunkCoord, axis: usize) -> usize {
    let mut coord_b = coord_a;
    match axis {
        0 => coord_b.0.x += 1,
        1 => coord_b.0.y += 1,
        2 => coord_b.0.z += 1,
        _ => unreachable!(),
    }
    if !store.contains(coord_b) {
        return 0;
    }
    // Gather the boundary values from each chunk (read-only).
    // chunk_a contributes its last content row → chunk_b's -axis pad
    // chunk_b contributes its first content row → chunk_a's +axis pad
    let axis_pad_pos = ChunkShape::AXIS - 1; // = PADDING + CHUNK_DIM
    let axis_content_pos = PADDING + CHUNK_DIM - 1; // = AXIS - 2 = last content
    let axis_pad_neg = 0;
    let axis_content_neg = PADDING; // = first content

    let plane: Vec<(u32, u32)> = (0..ChunkShape::AXIS)
        .flat_map(|u| (0..ChunkShape::AXIS).map(move |v| (u, v)))
        .collect();

    let chunk_a = store.get(coord_a).unwrap();
    let a_content_values: Vec<f32> = plane
        .iter()
        .map(|&(u, v)| chunk_a.get(padded_for_axis(axis, axis_content_pos, u, v)))
        .collect();

    let chunk_b = store.get(coord_b).unwrap();
    let b_content_values: Vec<f32> = plane
        .iter()
        .map(|&(u, v)| chunk_b.get(padded_for_axis(axis, axis_content_neg, u, v)))
        .collect();

    // Write A's content to B's -axis padding.
    let chunk_b = store.get_mut(coord_b).unwrap();
    let dense_b = chunk_b.make_dense();
    for (i, &(u, v)) in plane.iter().enumerate() {
        let idx = ChunkShape::linearize(padded_for_axis(axis, axis_pad_neg, u, v)) as usize;
        dense_b[idx] = a_content_values[i];
    }

    // Write B's content to A's +axis padding.
    let chunk_a = store.get_mut(coord_a).unwrap();
    let dense_a = chunk_a.make_dense();
    for (i, &(u, v)) in plane.iter().enumerate() {
        let idx = ChunkShape::linearize(padded_for_axis(axis, axis_pad_pos, u, v)) as usize;
        dense_a[idx] = b_content_values[i];
    }

    1
}

/// Build a padded sample coord with `axis_value` on the chosen axis
/// and `(u, v)` on the other two axes (in `(other1, other2)` order
/// where other1 < other2 in the standard XYZ ordering).
fn padded_for_axis(axis: usize, axis_value: u32, u: u32, v: u32) -> [u32; 3] {
    match axis {
        0 => [axis_value, u, v],
        1 => [u, axis_value, v],
        2 => [u, v, axis_value],
        _ => unreachable!(),
    }
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
