//! Composed terrain: per-zone procedural heightfield + sparse paint
//! deltas + world-level zone routing.
//!
//! [`ZoneTerrain`] is one zone's full elevation source — wraps its
//! [`PolygonIndex`] (biome SDF blend + river carve + Gaussian terrain
//! stamps + FBM noise) plus the per-cell `elevation_edits.bin`
//! overlay. Sample priority: hand-painted edit cell wins; otherwise
//! the procedural heightfield.
//!
//! [`WorldTerrain`] holds one [`ZoneTerrain`] per zone and routes
//! `(world_x, world_z)` queries to the owning zone via
//! `ZonePlacement::zone_cell` containment. A small per-thread
//! last-zone cache avoids the linear scan when the next sample lands
//! in the same zone (the common case during voxel chunk gen).
//!
//! Used by:
//! - `vaern-editor` voxel generator (one zone in flight at a time —
//!   the cache hits 100%)
//! - `vaern-server` + `vaern-client` voxel generators after
//!   `vaern_core::terrain::register_resolver` is called at startup.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use vaern_data::{
    build_zone_stamps, load_all_geography, load_all_landmarks, load_world, load_world_layout,
    point_in_polygon, Coord2, LandmarkIndex, World, WorldLayout,
};

use crate::edits::load_elevation_edits;
use crate::heightfield::PolygonIndex;
use crate::raster::SUB_CELL_SIZE_M;

/// One zone's composed terrain — procedural heightfield + sparse
/// elevation paint deltas.
#[derive(Debug, Clone)]
pub struct ZoneTerrain {
    pub zone_id: String,
    pub index: PolygonIndex,
    /// `(sub_x, sub_z) → metres`. Sparse: empty for unpainted zones.
    pub elevation_edits: HashMap<(i32, i32), f32>,
    /// World-space points of the zone's Voronoi cell (for routing in
    /// [`WorldTerrain`]). `None` for zones not placed in `world.yaml`.
    pub zone_cell: Option<Vec<Coord2>>,
}

impl ZoneTerrain {
    /// Build a [`ZoneTerrain`] for `zone_id`. Returns `None` if the
    /// zone has no `world.yaml` placement or no `geography.yaml`.
    pub fn build(
        zone_id: &str,
        world: &World,
        layout: &WorldLayout,
        landmarks: &LandmarkIndex,
        geography: &vaern_data::Geography,
        world_root: &Path,
    ) -> Option<Self> {
        let placement = layout
            .zone_placements
            .iter()
            .find(|p| p.zone == zone_id)?;
        let origin = placement.world_origin;
        let stamps = build_zone_stamps(zone_id, layout, world, landmarks);
        let index = PolygonIndex::build(zone_id, origin, geography, stamps);
        let elevation_edits = load_elevation_edits(world_root, zone_id);
        // Rebase the zone_cell into world coords (it's stored in
        // world coords already in `world.yaml::zone_cell`, so just
        // clone the points).
        let zone_cell = placement
            .zone_cell
            .as_ref()
            .map(|poly| poly.points.clone());
        Some(Self {
            zone_id: zone_id.to_string(),
            index,
            elevation_edits,
            zone_cell,
        })
    }

    /// Sample the final elevation at world `(x, z)` — paint delta wins
    /// over procedural.
    #[inline]
    pub fn final_height(&self, world_x: f32, world_z: f32) -> f32 {
        let sx = (world_x / SUB_CELL_SIZE_M).floor() as i32;
        let sz = (world_z / SUB_CELL_SIZE_M).floor() as i32;
        if let Some(&edited) = self.elevation_edits.get(&(sx, sz)) {
            return edited;
        }
        self.index.sample(world_x, world_z)
    }
}

/// World-level terrain: one [`ZoneTerrain`] per placed zone + a
/// routing helper that picks the right zone for a `(world_x, world_z)`
/// query.
#[derive(Debug, Clone, Default)]
pub struct WorldTerrain {
    /// Sorted by zone_id for stable iteration. `Arc` so consumers can
    /// hand the same zone out to multiple threads cheaply.
    pub zones: Vec<Arc<ZoneTerrain>>,
}

impl WorldTerrain {
    /// Load every placed zone in the world. Zones missing
    /// `geography.yaml` or `world.yaml` placement are silently
    /// skipped.
    pub fn build(world_root: &Path) -> Result<Self, vaern_data::LoadError> {
        let world = load_world(world_root)?;
        let layout = load_world_layout(world_root)?;
        let landmarks = load_all_landmarks(world_root)?;
        let geography = load_all_geography(world_root)?;

        let mut zone_ids: Vec<&str> = world.zones.iter().map(|z| z.id.as_str()).collect();
        zone_ids.sort();

        let mut zones: Vec<Arc<ZoneTerrain>> = Vec::with_capacity(zone_ids.len());
        for zone_id in zone_ids {
            let Some(geo) = geography.get(zone_id) else {
                continue;
            };
            if let Some(zt) = ZoneTerrain::build(zone_id, &world, &layout, &landmarks, geo, world_root)
            {
                zones.push(Arc::new(zt));
            }
        }
        Ok(Self { zones })
    }

    /// Find the zone whose `zone_cell` contains `(x, z)`. Linear scan
    /// — `O(zones)` — acceptable because there are <30 zones and the
    /// caller (typically a voxel chunk generator) clusters samples in
    /// space; pair with `WorldTerrainSampler::sample` for a
    /// last-zone cache.
    pub fn zone_at(&self, x: f32, z: f32) -> Option<&Arc<ZoneTerrain>> {
        let p = Coord2::new(x, z);
        for zone in &self.zones {
            if let Some(cell) = &zone.zone_cell {
                if point_in_polygon(p, cell) {
                    return Some(zone);
                }
            }
        }
        None
    }

    /// Sample world elevation at `(x, z)`. Routes to the zone whose
    /// `zone_cell` contains the point. Returns 0 for ocean / unzoned
    /// space (between zone cells, outside the coastline).
    pub fn final_height(&self, x: f32, z: f32) -> f32 {
        match self.zone_at(x, z) {
            Some(zone) => zone.final_height(x, z),
            None => 0.0,
        }
    }
}

/// Per-thread sampler with a single-zone hint cache. Voxel chunk
/// generation samples thousands of clustered XZ points in a row; the
/// cache turns the linear scan into a single membership test for ~99%
/// of samples after the first hit.
///
/// Use in the per-thread chunk-gen task: `let mut s =
/// WorldTerrainSampler::new(arc); for each voxel: s.sample(x, z);`
pub struct WorldTerrainSampler {
    pub world: Arc<WorldTerrain>,
    cached: Option<Arc<ZoneTerrain>>,
}

impl WorldTerrainSampler {
    pub fn new(world: Arc<WorldTerrain>) -> Self {
        Self {
            world,
            cached: None,
        }
    }

    pub fn sample(&mut self, x: f32, z: f32) -> f32 {
        let p = Coord2::new(x, z);
        if let Some(cached) = &self.cached {
            if let Some(cell) = &cached.zone_cell {
                if point_in_polygon(p, cell) {
                    return cached.final_height(x, z);
                }
            }
        }
        // Cache miss — fall through to the linear scan.
        if let Some(zone) = self.world.zone_at(x, z) {
            let h = zone.final_height(x, z);
            self.cached = Some(Arc::clone(zone));
            return h;
        }
        // No owning zone (ocean / between zones). Don't cache — keeps
        // the previous cache valid for nearby in-zone samples.
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn world_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../src/generated/world")
            .canonicalize()
            .unwrap()
    }

    #[test]
    fn world_terrain_loads_every_placed_zone() {
        let wt = WorldTerrain::build(&world_root()).unwrap();
        // 28 zones in the corpus, all placed in world.yaml as of
        // the cartography work in `project_cartography.md`.
        assert!(
            wt.zones.len() >= 20,
            "expected ~28 zones, got {}",
            wt.zones.len()
        );
    }

    #[test]
    fn dalewatch_keep_world_position_samples_lifted_height() {
        // Dalewatch Keep is at zone-local (0, 0) → world origin from
        // world.yaml. Auto-derives BigHill (+18m stamp). Sample at
        // that world point should be > 10m.
        let wt = WorldTerrain::build(&world_root()).unwrap();
        let layout = load_world_layout(&world_root()).unwrap();
        let origin = layout
            .zone_origin("dalewatch_marches")
            .expect("dalewatch placement");
        let h = wt.final_height(origin.x, origin.z);
        assert!(
            h > 10.0,
            "Dalewatch Keep should sit on a +18m mound, got {h}"
        );
    }

    #[test]
    fn ocean_returns_zero() {
        // Far from any zone cell — should hit None and return 0.
        let wt = WorldTerrain::build(&world_root()).unwrap();
        let h = wt.final_height(1_000_000.0, 1_000_000.0);
        assert_eq!(h, 0.0);
    }

    #[test]
    fn sampler_cache_returns_same_value_as_world_lookup() {
        let wt = Arc::new(WorldTerrain::build(&world_root()).unwrap());
        let layout = load_world_layout(&world_root()).unwrap();
        let origin = layout
            .zone_origin("dalewatch_marches")
            .expect("dalewatch placement");
        let mut s = WorldTerrainSampler::new(Arc::clone(&wt));
        for k in 0..50 {
            let x = origin.x + ((k * 17) % 400) as f32 - 200.0;
            let z = origin.z + ((k * 31 + 7) % 400) as f32 - 200.0;
            let direct = wt.final_height(x, z);
            let cached = s.sample(x, z);
            assert_eq!(
                direct.to_bits(),
                cached.to_bits(),
                "sampler cache diverged at ({x}, {z})"
            );
        }
    }
}
