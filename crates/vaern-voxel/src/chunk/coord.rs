//! Integer coordinates + world↔chunk↔voxel conversion helpers.
//!
//! Three coordinate systems coexist; every conversion is O(1) and
//! lives in this module so a call site never does the math inline:
//!
//! * [`ChunkCoord`] — index of a chunk in the sparse store.
//! * [`VoxelCoord`] — global voxel-grid index (across all chunks).
//! * `Vec3` — Bevy world space, in world units.

use crate::config::{CHUNK_DIM, VOXEL_SIZE};
use bevy::math::{IVec3, Vec3};

/// Sparse-store index of a chunk.
///
/// Chunk at `(cx, cy, cz)` covers voxel indices `[cx*DIM, (cx+1)*DIM)`
/// on each axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChunkCoord(pub IVec3);

impl ChunkCoord {
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self(IVec3::new(x, y, z))
    }

    /// World-space position of this chunk's (0,0,0) local sample corner.
    pub fn world_origin(self) -> Vec3 {
        self.0.as_vec3() * (CHUNK_DIM as f32 * VOXEL_SIZE)
    }

    /// Chunk that contains the given world-space point. Points on chunk
    /// boundaries round down (toward the chunk with smaller index).
    pub fn containing(world: Vec3) -> Self {
        let grid = world / (CHUNK_DIM as f32 * VOXEL_SIZE);
        Self(IVec3::new(
            grid.x.floor() as i32,
            grid.y.floor() as i32,
            grid.z.floor() as i32,
        ))
    }

    /// Chebyshev-distance neighborhood (3×3×3 = 27 entries including
    /// self) as an iterator. Useful when a brush affects more than one
    /// chunk.
    pub fn neighborhood(self) -> impl Iterator<Item = ChunkCoord> {
        let base = self.0;
        (-1..=1).flat_map(move |dz| {
            (-1..=1).flat_map(move |dy| {
                (-1..=1).map(move |dx| ChunkCoord(base + IVec3::new(dx, dy, dz)))
            })
        })
    }
}

/// Global voxel-grid index (across all chunks). `(0,0,0)` is the world
/// origin; `(1,0,0)` is one voxel along +X.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VoxelCoord(pub IVec3);

impl VoxelCoord {
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self(IVec3::new(x, y, z))
    }

    /// Voxel that contains the given world point (rounds toward −∞).
    pub fn containing(world: Vec3) -> Self {
        let grid = world / VOXEL_SIZE;
        Self(IVec3::new(
            grid.x.floor() as i32,
            grid.y.floor() as i32,
            grid.z.floor() as i32,
        ))
    }

    /// World-space position of this voxel's minimum corner.
    pub fn world_min(self) -> Vec3 {
        self.0.as_vec3() * VOXEL_SIZE
    }

    /// Chunk that owns this voxel.
    pub fn chunk(self) -> ChunkCoord {
        // Integer floor division — Rust's `/` rounds toward zero, which
        // gives wrong results for negative numbers. `div_euclid` rounds
        // toward −∞ which is what we want.
        ChunkCoord(IVec3::new(
            self.0.x.div_euclid(CHUNK_DIM as i32),
            self.0.y.div_euclid(CHUNK_DIM as i32),
            self.0.z.div_euclid(CHUNK_DIM as i32),
        ))
    }

    /// Local index within the owning chunk's sample array *ignoring
    /// padding* — the caller must add `PADDING` to each component to
    /// get an index into the actual padded sample array.
    pub fn local_unpadded(self) -> IVec3 {
        IVec3::new(
            self.0.x.rem_euclid(CHUNK_DIM as i32),
            self.0.y.rem_euclid(CHUNK_DIM as i32),
            self.0.z.rem_euclid(CHUNK_DIM as i32),
        )
    }
}

/// World-space AABB of a chunk. Handy for culling / bounding-volume
/// lookups.
pub fn chunk_aabb_world(coord: ChunkCoord) -> (Vec3, Vec3) {
    let lo = coord.world_origin();
    let hi = lo + Vec3::splat(CHUNK_DIM as f32 * VOXEL_SIZE);
    (lo, hi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_to_chunk_to_world_roundtrips_at_origin() {
        let c = ChunkCoord::containing(Vec3::ZERO);
        assert_eq!(c, ChunkCoord::new(0, 0, 0));
        assert_eq!(c.world_origin(), Vec3::ZERO);
    }

    #[test]
    fn negative_world_rounds_to_negative_chunk() {
        let p = Vec3::new(-1.0, -1.0, -1.0);
        let c = ChunkCoord::containing(p);
        assert_eq!(c, ChunkCoord::new(-1, -1, -1));
    }

    #[test]
    fn voxel_local_handles_negative() {
        let v = VoxelCoord::new(-1, 0, 0);
        let dim = CHUNK_DIM as i32;
        assert_eq!(v.chunk(), ChunkCoord::new(-1, 0, 0));
        assert_eq!(v.local_unpadded(), IVec3::new(dim - 1, 0, 0));
    }

    #[test]
    fn neighborhood_has_27_entries_including_self() {
        let base = ChunkCoord::new(3, 3, 3);
        let n: Vec<_> = base.neighborhood().collect();
        assert_eq!(n.len(), 27);
        assert!(n.contains(&base));
    }
}
