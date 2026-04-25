//! Ray marching + ground probes against the stored SDF.

use crate::chunk::{ChunkCoord, ChunkStore};
use crate::config::{PADDING, VOXEL_SIZE};
use bevy::math::Vec3;

/// One ray-probe result.
#[derive(Clone, Copy, Debug)]
pub struct RayHit {
    /// World-space hit point (approximate — quantized to the step
    /// size used during the march).
    pub position: Vec3,
    /// Distance from ray origin to the hit.
    pub distance: f32,
    /// Chunk that owned the crossing sample. Handy for subsequent
    /// edits that want to queue the same chunk for re-mesh.
    pub chunk: ChunkCoord,
}

/// Find the ground Y at world column `(x, z)` by walking down from
/// `top_y` until the SDF crosses zero. `max_descent` caps how far we
/// walk (returns `None` if no surface is found in range).
///
/// Step size is `VOXEL_SIZE` — sub-voxel precision via the two
/// bracketing samples at the crossing.
pub fn ground_y(
    store: &ChunkStore,
    x: f32,
    z: f32,
    top_y: f32,
    max_descent: f32,
) -> Option<f32> {
    let step = VOXEL_SIZE;
    let steps = (max_descent / step).ceil() as i32;

    let mut prev_v: Option<f32> = None;
    let mut prev_y = top_y;
    for i in 0..=steps {
        let y = top_y - (i as f32) * step;
        let p = Vec3::new(x, y, z);
        let v = sample_point(store, p);
        if let Some(pv) = prev_v {
            if pv >= 0.0 && v < 0.0 {
                // Linear interp for sub-voxel precision.
                let t = pv / (pv - v);
                return Some(prev_y + (y - prev_y) * t);
            }
        }
        prev_v = Some(v);
        prev_y = y;
    }
    None
}

/// Walk a ray from `origin` along `dir` (unit vector) up to `max_dist`
/// world units. First negative-SDF sample encountered wins.
///
/// Fixed-step for simplicity. A later sphere-traced version can use
/// the magnitude of the current SDF sample to choose a larger step.
pub fn raycast(store: &ChunkStore, origin: Vec3, dir: Vec3, max_dist: f32) -> Option<RayHit> {
    let dir = dir.normalize_or_zero();
    if dir.length_squared() < 1e-6 {
        return None;
    }
    let step = VOXEL_SIZE * 0.5;
    let steps = (max_dist / step).ceil() as i32;

    for i in 0..steps {
        let t = i as f32 * step;
        let p = origin + dir * t;
        let v = sample_point(store, p);
        if v < 0.0 {
            return Some(RayHit {
                position: p,
                distance: t,
                chunk: ChunkCoord::containing(p),
            });
        }
    }
    None
}

/// Sample the stored SDF at an arbitrary world point by reading from
/// the owning chunk's padded array + trilinear interpolation.
///
/// If the owning chunk is not loaded, returns a large positive value
/// (treat unloaded regions as "air"). Callers that need
/// generator-on-demand behavior should seed chunks ahead of probing.
fn sample_point(store: &ChunkStore, p: Vec3) -> f32 {
    let coord = ChunkCoord::containing(p);
    let Some(chunk) = store.get(coord) else {
        return crate::config::UNINITIALIZED_SDF;
    };
    let origin = coord.world_origin();
    let local_voxels = (p - origin) / VOXEL_SIZE;
    // Translate into padded-array local coords.
    let local_padded = local_voxels + Vec3::splat(PADDING as f32);
    chunk.sample_trilinear(local_padded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::VoxelChunk;
    use crate::generator::{HeightfieldGenerator, WorldGenerator};

    #[test]
    fn ground_y_finds_heightfield_crossing() {
        let mut store = ChunkStore::new();
        let generator = HeightfieldGenerator::new();
        let coord = ChunkCoord::new(0, 0, 0);
        let mut chunk = VoxelChunk::new_air();
        generator.seed_chunk(coord, &mut chunk);
        store.insert(coord, chunk);

        let y = ground_y(&store, 5.0, 5.0, 20.0, 30.0);
        assert!(y.is_some(), "expected to find surface below y=20");
        let y = y.unwrap();
        // terrain::height(5,5) is bounded ≤ 2; the crossing should sit
        // within a handful of voxels of zero.
        assert!(y.abs() < 3.0, "unexpected ground y {y}");
    }

    #[test]
    fn raycast_hits_solid_column() {
        let mut store = ChunkStore::new();
        let mut chunk = VoxelChunk::new_air();
        // Surface at world y = SURFACE_Y — well inside the single
        // chunk we load, so a downward ray from above won't cross a
        // chunk boundary into unloaded space.
        const SURFACE_Y: f32 = 10.0;
        chunk.fill_all_padded(|[_x, y, _z]| {
            let world_y = (y as f32 - PADDING as f32) * VOXEL_SIZE;
            // SDF = distance above the surface → solid below, air above.
            world_y - SURFACE_Y
        });
        store.insert(ChunkCoord::new(0, 0, 0), chunk);

        let hit = raycast(&store, Vec3::new(5.0, 25.0, 5.0), Vec3::NEG_Y, 20.0);
        assert!(hit.is_some(), "expected a hit");
        let hit = hit.unwrap();
        assert!(
            (hit.position.y - SURFACE_Y).abs() < 1.0,
            "expected hit near surface y={SURFACE_Y}, got {:?}",
            hit
        );
    }
}
