//! Smooth stroke — neighbor-averaging blur over a spherical region.
//!
//! Doesn't fit the [`Brush`] trait because per-voxel evaluation needs
//! to read **neighbor samples**, not just the same-position existing
//! value. Implemented as a sibling stroke type with the same
//! halo-routing + dirty-marking contract as [`super::EditStroke`].
//!
//! Algorithm (three passes, two ping-pong buffers + one snapshot):
//!
//! 1. **Snapshot.** Walk a flat grid covering the brush AABB plus a
//!    1-voxel halo. For each grid cell, route via
//!    [`super::chunks_containing_voxel`] and read the first valid
//!    sample into `original`. Unloaded chunks fall back to
//!    [`UNINITIALIZED_SDF`]. Copy `original` → `buf_a`.
//! 2. **Iteration.** For each iteration, walk interior cells (the
//!    halo ring stays unchanged so neighbor reads always see a value).
//!    For each interior cell, average the 6 face-neighbors from
//!    `buf_a`, blend with `existing × (1 − strength) + avg × strength`,
//!    write to `buf_b`. Boundary cells just copy through. Swap
//!    `buf_a` / `buf_b` after each iteration.
//! 3. **Write-back.** Walk interior cells. For every cell **inside the
//!    sphere mask**, if `final − original` differs by more than ε,
//!    write the new value to every routed `(chunk, padded_local)`
//!    pair — that's what halo-syncs across chunk boundaries — and mark
//!    each touched chunk dirty.
//!
//! Memory at radius=32: side=64+2, three `f32` buffers of 66³ ≈ 3.4 MB.
//! Runs once per LMB click, not per frame, so the alloc is fine.

use std::collections::HashSet;

use bevy::math::{IVec3, Vec3};

use super::brush::Falloff;
use super::chunks_containing_voxel;
use crate::chunk::{ChunkCoord, ChunkStore, DirtyChunks};
use crate::config::{UNINITIALIZED_SDF, VOXEL_SIZE};

/// Per-stroke smoothing parameters + borrowed store/dirty refs.
pub struct SmoothStroke<'a> {
    center: Vec3,
    radius: f32,
    strength: f32,
    iterations: u32,
    falloff: Falloff,
    store: &'a mut ChunkStore,
    dirty: &'a mut DirtyChunks,
}

/// Cap so a runaway slider can't allocate 1000-iteration loops over
/// large grids. 10 is far past the point of perceptual diminishing
/// returns for any realistic brush size.
pub const MAX_SMOOTH_ITERATIONS: u32 = 10;

impl<'a> SmoothStroke<'a> {
    pub fn new(
        center: Vec3,
        radius: f32,
        strength: f32,
        iterations: u32,
        falloff: Falloff,
        store: &'a mut ChunkStore,
        dirty: &'a mut DirtyChunks,
    ) -> Self {
        Self {
            center,
            radius,
            strength: strength.clamp(0.0, 1.0),
            iterations: iterations.clamp(1, MAX_SMOOTH_ITERATIONS),
            falloff,
            store,
            dirty,
        }
    }

    /// Apply the stroke. Returns the set of chunks whose samples
    /// actually changed (caller queues re-mesh / network deltas).
    pub fn apply(self) -> HashSet<ChunkCoord> {
        // Brush AABB → inclusive voxel grid range.
        let r = self.radius;
        let lo_world = self.center - Vec3::splat(r);
        let hi_world = self.center + Vec3::splat(r);
        let v_lo = IVec3::new(
            (lo_world.x / VOXEL_SIZE).floor() as i32,
            (lo_world.y / VOXEL_SIZE).floor() as i32,
            (lo_world.z / VOXEL_SIZE).floor() as i32,
        );
        let v_hi = IVec3::new(
            (hi_world.x / VOXEL_SIZE).ceil() as i32,
            (hi_world.y / VOXEL_SIZE).ceil() as i32,
            (hi_world.z / VOXEL_SIZE).ceil() as i32,
        );

        // Halo'd grid bounds (1 voxel pad on each side for neighbor
        // reads at the brush boundary).
        let g_lo = v_lo - IVec3::splat(1);
        let g_hi = v_hi + IVec3::splat(1);
        let g_size = g_hi - g_lo + IVec3::splat(1);
        debug_assert!(g_size.x > 0 && g_size.y > 0 && g_size.z > 0);

        let stride_y = g_size.x as usize;
        let stride_z = (g_size.x * g_size.y) as usize;
        let total = (g_size.x * g_size.y * g_size.z) as usize;

        // Snapshot pass: read every grid cell from the store.
        let mut original = vec![UNINITIALIZED_SDF; total];
        for k in 0..g_size.z {
            for j in 0..g_size.y {
                for i in 0..g_size.x {
                    let idx = i as usize + j as usize * stride_y + k as usize * stride_z;
                    let wx = g_lo.x + i;
                    let wy = g_lo.y + j;
                    let wz = g_lo.z + k;
                    original[idx] = sample_at_world_voxel(self.store, wx, wy, wz);
                }
            }
        }
        let mut buf_a = original.clone();
        let mut buf_b = original.clone();

        // Iteration pass: ping-pong blur. Boundary ring stays at its
        // snapshot value so interior reads always see a defined
        // neighbor.
        for _ in 0..self.iterations {
            blur_one_pass(&buf_a, &mut buf_b, g_size, stride_y, stride_z, self.strength);
            std::mem::swap(&mut buf_a, &mut buf_b);
        }

        // Write-back pass: walk interior cells, sphere-mask, write
        // changes through every routed (chunk, padded_local) pair.
        let mut touched: HashSet<ChunkCoord> = HashSet::new();
        let r2 = self.radius * self.radius;
        for k in 1..(g_size.z - 1) {
            for j in 1..(g_size.y - 1) {
                for i in 1..(g_size.x - 1) {
                    let idx = i as usize + j as usize * stride_y + k as usize * stride_z;
                    let wx = g_lo.x + i;
                    let wy = g_lo.y + j;
                    let wz = g_lo.z + k;
                    let world = IVec3::new(wx, wy, wz).as_vec3() * VOXEL_SIZE;
                    if (world - self.center).length_squared() > r2 {
                        continue;
                    }
                    let nd = (world - self.center).length() / self.radius;
                    let weight = self.falloff.weight(nd);
                    if weight <= 0.0 {
                        continue;
                    }
                    let new = buf_a[idx];
                    let old = original[idx];
                    let blended = old * (1.0 - weight) + new * weight;
                    if (blended - old).abs() <= 1e-6 {
                        continue;
                    }
                    for (coord, padded_local) in chunks_containing_voxel(wx, wy, wz) {
                        let chunk = self.store.get_or_insert_air(coord);
                        chunk.set(padded_local, blended);
                        touched.insert(coord);
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

/// Read the sample at world voxel `(wx, wy, wz)`. Picks the first
/// loaded chunk that owns this voxel via `chunks_containing_voxel`. If
/// no owning chunk is loaded, returns [`UNINITIALIZED_SDF`].
fn sample_at_world_voxel(store: &ChunkStore, wx: i32, wy: i32, wz: i32) -> f32 {
    for (coord, padded_local) in chunks_containing_voxel(wx, wy, wz) {
        if let Some(chunk) = store.get(coord) {
            return chunk.get(padded_local);
        }
    }
    UNINITIALIZED_SDF
}

/// One blur pass: read interior cells from `read`, write blended
/// average to `write`. Boundary cells just copy through unchanged.
fn blur_one_pass(
    read: &[f32],
    write: &mut [f32],
    g_size: IVec3,
    stride_y: usize,
    stride_z: usize,
    strength: f32,
) {
    // Boundary ring: passthrough (so swap leaves them defined for the
    // next iteration's neighbor reads).
    write.copy_from_slice(read);

    let inv_strength = 1.0 - strength;
    for k in 1..(g_size.z - 1) {
        for j in 1..(g_size.y - 1) {
            for i in 1..(g_size.x - 1) {
                let idx = i as usize + j as usize * stride_y + k as usize * stride_z;
                let existing = read[idx];
                let avg = (read[idx - 1]
                    + read[idx + 1]
                    + read[idx - stride_y]
                    + read[idx + stride_y]
                    + read[idx - stride_z]
                    + read[idx + stride_z])
                    / 6.0;
                write[idx] = existing * inv_strength + avg * strength;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::VoxelChunk;
    use crate::config::PADDING;

    /// Pre-fill chunk (0,0,0) with a half-and-half SDF: -10 below
    /// world Y=8, +10 above. After smoothing, the sample at Y=8
    /// (the discontinuity) should move noticeably toward zero.
    #[test]
    fn smooth_stroke_blurs_step_function() {
        let mut store = ChunkStore::new();
        let mut dirty = DirtyChunks::new();

        let mut chunk = VoxelChunk::new_air();
        chunk.fill_all_padded(|[_x, y, _z]| {
            let world_y = (y as f32 - PADDING as f32) * VOXEL_SIZE;
            if world_y < 8.0 {
                -10.0
            } else {
                10.0
            }
        });
        store.insert(ChunkCoord::new(0, 0, 0), chunk);

        // Capture pre-stroke sample at the step boundary.
        let pre = store.get(ChunkCoord::new(0, 0, 0)).unwrap().get([
            10 + PADDING,
            8 + PADDING,
            10 + PADDING,
        ]);
        assert_eq!(pre, 10.0);

        SmoothStroke::new(
            Vec3::new(10.0, 8.0, 10.0),
            5.0,
            1.0,
            3,
            Falloff::Hard,
            &mut store,
            &mut dirty,
        )
        .apply();

        let post = store.get(ChunkCoord::new(0, 0, 0)).unwrap().get([
            10 + PADDING,
            8 + PADDING,
            10 + PADDING,
        ]);
        // Strength=1, 3 iterations → step boundary should pull strongly
        // toward zero. Don't pin an exact value (depends on neighbor
        // mix) but require a clear move from +10.
        assert!(
            post.abs() < 8.0,
            "expected step boundary to blur toward zero, got {post}"
        );
    }

    #[test]
    fn smooth_stroke_marks_dirty_chunks() {
        let mut store = ChunkStore::new();
        let mut dirty = DirtyChunks::new();

        // A pure linear gradient is invariant under neighbor-averaging
        // (avg(±1) = self) — smoothing would be a no-op. Use a single
        // negative spike at the brush center so the smoothing has
        // something to spread.
        let mut chunk = VoxelChunk::new_air();
        chunk.fill_all_padded(|[x, y, z]| {
            if x == 15 + PADDING && y == 15 + PADDING && z == 15 + PADDING {
                -50.0
            } else {
                10.0
            }
        });
        store.insert(ChunkCoord::new(0, 0, 0), chunk);

        let touched = SmoothStroke::new(
            Vec3::new(15.0, 15.0, 15.0),
            4.0,
            0.5,
            2,
            Falloff::Hard,
            &mut store,
            &mut dirty,
        )
        .apply();

        assert!(!touched.is_empty(), "expected at least one chunk touched");
        assert!(!dirty.is_empty(), "expected dirty set non-empty");
    }

    /// A voxel far outside the brush sphere must be byte-identical
    /// before and after.
    #[test]
    fn smooth_stroke_respects_sphere_mask() {
        let mut store = ChunkStore::new();
        let mut dirty = DirtyChunks::new();

        let mut chunk = VoxelChunk::new_air();
        // Steep gradient everywhere so neighbors differ.
        chunk.fill_all_padded(|[x, y, z]| (x as f32) + (y as f32) * 0.5 + (z as f32) * 0.25);
        store.insert(ChunkCoord::new(0, 0, 0), chunk);

        // Sample far outside the brush sphere (radius 2 at origin).
        let probe = [25 + PADDING, 25 + PADDING, 25 + PADDING];
        let pre = store.get(ChunkCoord::new(0, 0, 0)).unwrap().get(probe);

        SmoothStroke::new(Vec3::ZERO, 2.0, 1.0, 5, Falloff::Hard, &mut store, &mut dirty)
            .apply();

        let post = store.get(ChunkCoord::new(0, 0, 0)).unwrap().get(probe);
        assert_eq!(pre, post, "voxel outside sphere mask was modified");
    }
}
