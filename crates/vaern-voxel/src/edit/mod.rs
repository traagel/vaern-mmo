//! Authoritative edit layer — brushes + strokes that mutate stored
//! chunk samples permanently.
//!
//! Edits always flow through this module. That guarantees three
//! invariants:
//!
//! 1. **Dirty tracking.** Every chunk whose samples change is added to
//!    [`crate::DirtyChunks`] so the mesher + replicator re-runs on it
//!    next tick.
//! 2. **Halo sync.** Neighbor chunks share samples in the padding
//!    rows; an edit at a chunk boundary writes into every affected
//!    chunk so the meshed seams stay watertight.
//! 3. **Version bump.** [`crate::VoxelChunk::version`] increments on
//!    every write so asynchronous observers (network, LOD) notice.
//!
//! Edits are composed from two layers:
//!
//! * [`Brush`] — the *shape* of the edit (sphere, box, capsule, …).
//!   Defines the region of space affected plus how the new SDF value
//!   combines with the existing one.
//! * [`EditStroke`] — the *act* of applying one brush to the
//!   [`crate::ChunkStore`]. Returns the set of chunks that actually
//!   changed so the caller can queue re-meshing / network deltas.
//!
//! Adding a new brush = one new file in `brush/` + an `impl Brush`.

pub mod brush;

pub use brush::{AddSphereBrush, BoxBrush, Brush, BrushMode, SphereBrush};

use crate::chunk::{ChunkCoord, ChunkStore, DirtyChunks};
use crate::config::{CHUNK_DIM, PADDING, VOXEL_SIZE};
use bevy::math::{IVec3, Vec3};
use std::collections::HashSet;

/// One application of a single [`Brush`] against a [`ChunkStore`].
///
/// Run an edit by building a stroke with the brush + store + dirty-set
/// references, then calling [`Self::apply`]. Returns the coord list for
/// logging / replication.
pub struct EditStroke<'a, B: Brush> {
    brush: B,
    store: &'a mut ChunkStore,
    dirty: &'a mut DirtyChunks,
}

impl<'a, B: Brush> EditStroke<'a, B> {
    pub fn new(brush: B, store: &'a mut ChunkStore, dirty: &'a mut DirtyChunks) -> Self {
        Self { brush, store, dirty }
    }

    /// Apply the brush. Walks every sample in the brush's world-space
    /// AABB, updates the owning chunk's stored SDF per
    /// [`Brush::blend`], marks affected chunks as dirty.
    pub fn apply(self) -> HashSet<ChunkCoord> {
        let (aabb_min, aabb_max) = self.brush.aabb();

        // Expand the AABB by one voxel so boundary sampling sees the
        // neighbor-halo. Any sample within [min-1vx, max+1vx] may need
        // updating in at least one chunk's padding ring.
        let halo = Vec3::splat(VOXEL_SIZE);
        let world_min = aabb_min - halo;
        let world_max = aabb_max + halo;

        // Convert to inclusive voxel-grid index range.
        let ivmin = IVec3::new(
            (world_min.x / VOXEL_SIZE).floor() as i32,
            (world_min.y / VOXEL_SIZE).floor() as i32,
            (world_min.z / VOXEL_SIZE).floor() as i32,
        );
        let ivmax = IVec3::new(
            (world_max.x / VOXEL_SIZE).ceil() as i32,
            (world_max.y / VOXEL_SIZE).ceil() as i32,
            (world_max.z / VOXEL_SIZE).ceil() as i32,
        );

        let mut touched: HashSet<ChunkCoord> = HashSet::new();

        for z in ivmin.z..=ivmax.z {
            for y in ivmin.y..=ivmax.y {
                for x in ivmin.x..=ivmax.x {
                    let world = IVec3::new(x, y, z).as_vec3() * VOXEL_SIZE;

                    // One voxel coord can live in up to 8 chunks
                    // (via the shared-edge padding), so we route it
                    // to every chunk whose padded array would
                    // contain a sample at this position.
                    for (coord, padded_local) in chunks_containing_voxel(x, y, z) {
                        let chunk = self.store.get_or_insert_air(coord);
                        let existing = chunk.get(padded_local);
                        let candidate = self.brush.sample(world);
                        let blended = self.brush.blend(existing, candidate);
                        if (blended - existing).abs() > 1e-6 {
                            chunk.set(padded_local, blended);
                            touched.insert(coord);
                        }
                    }
                }
            }
        }

        for &c in &touched {
            self.dirty.mark(c);
        }
        touched
    }
}

/// Yields every `(chunk_coord, padded_local_idx)` pair that references
/// the given world-voxel position. A voxel on a chunk boundary lives
/// in up to 8 chunks via the shared padding rings (one voxel can sit
/// in both a chunk's +side padding *and* the neighbor's -side
/// padding), so the enumeration has to consider neighbor chunks in
/// both directions on each axis, not just one.
fn chunks_containing_voxel(x: i32, y: i32, z: i32) -> impl Iterator<Item = (ChunkCoord, [u32; 3])> {
    let mut out: Vec<(ChunkCoord, [u32; 3])> = Vec::new();

    let dim = CHUNK_DIM as i32;
    let pad = PADDING as i32;
    let axis = (CHUNK_DIM + 2 * PADDING) as i32;

    // Try `dx/dy/dz ∈ {-1, 0, +1}` relative to the voxel's owning
    // content chunk. The in-range check then keeps only chunks whose
    // padded array actually covers this voxel. Earlier versions
    // restricted this to `{-1, 0}`, which silently dropped writes to
    // the -side-padding slot of the +1 neighbor — meshes on the +side
    // of the edit rendered with stale pre-edit geometry on their
    // boundary cubes (visible as a textured cap over carved craters).
    for dz in -1..=1 {
        for dy in -1..=1 {
            for dx in -1..=1 {
                let cx = x.div_euclid(dim) + dx;
                let cy = y.div_euclid(dim) + dy;
                let cz = z.div_euclid(dim) + dz;

                let lx = x - cx * dim + pad;
                let ly = y - cy * dim + pad;
                let lz = z - cz * dim + pad;
                if lx >= 0 && lx < axis && ly >= 0 && ly < axis && lz >= 0 && lz < axis {
                    out.push((
                        ChunkCoord(IVec3::new(cx, cy, cz)),
                        [lx as u32, ly as u32, lz as u32],
                    ));
                }
            }
        }
    }
    out.into_iter()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::VoxelChunk;

    #[test]
    fn voxel_on_chunk_boundary_routes_to_both_chunks() {
        // Voxel at world index (0, 0, 0) is the first content sample
        // of chunk (0,0,0) AND the +side padding of chunk (-1,-1,-1).
        let chunks: Vec<_> = chunks_containing_voxel(0, 0, 0).collect();
        assert!(chunks.len() >= 2);
    }

    #[test]
    fn sphere_brush_carves_a_hole_in_solid_ground() {
        let mut store = ChunkStore::new();
        let mut dirty = DirtyChunks::new();

        // Pre-fill chunk (0,0,0) with solid samples.
        let mut c = VoxelChunk::new_air();
        c.fill_all_padded(|_| -10.0);
        store.insert(ChunkCoord::new(0, 0, 0), c);

        // Carve a 5-unit sphere at the chunk origin.
        let brush = brush::SphereBrush {
            center: Vec3::new(5.0, 5.0, 5.0),
            radius: 3.0,
            mode: BrushMode::Subtract,
        };
        let stroke = EditStroke::new(brush, &mut store, &mut dirty);
        let touched = stroke.apply();
        assert!(!touched.is_empty());
        assert!(!dirty.is_empty());

        // Sample at the center should now be positive (air).
        let chunk = store.get(ChunkCoord::new(0, 0, 0)).unwrap();
        let at_center = chunk.get([5 + PADDING, 5 + PADDING, 5 + PADDING]);
        assert!(at_center > 0.0);
    }
}
