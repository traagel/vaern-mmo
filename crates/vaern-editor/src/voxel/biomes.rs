//! Per-chunk biome resolution.
//!
//! Mirror of `vaern-client/src/voxel_biomes.rs` — duplicated rather than
//! imported because the editor crate intentionally avoids depending on
//! the client (which drags networking + AppState gating). If the
//! client's resolver gets a richer model, port the change here too.
//!
//! Resolution is grid-aligned at chunk granularity: each chunk picks
//! its biome via nearest-hub lookup at the chunk footprint center.
//! Biome → ambientCG PBR texture set table is the same as the client's.

use std::collections::HashMap;
use std::path::Path;

use bevy::prelude::*;
use vaern_data::{load_world, World};

use crate::world::load::compute_zone_origins;

/// Same 9-biome palette as the runtime client. Unknown YAML keys fall
/// through to `Grass`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BiomeKey {
    Grass,
    GrassLush,
    Mossy,
    Dirt,
    Snow,
    Stone,
    Scorched,
    Marsh,
    Rocky,
}

impl BiomeKey {
    pub fn from_yaml(s: &str) -> Self {
        match s {
            "grass_lush" => Self::GrassLush,
            "mossy" => Self::Mossy,
            "dirt" => Self::Dirt,
            "snow" => Self::Snow,
            "stone" => Self::Stone,
            "scorched" => Self::Scorched,
            "marsh" => Self::Marsh,
            "rocky" => Self::Rocky,
            _ => Self::Grass,
        }
    }

    /// PBR triple for this biome. Paths are relative to the
    /// `vaern-editor` binary's `AssetServer` root (the workspace
    /// `assets/` folder, set in `bin/editor.rs`).
    pub fn textures(self) -> BiomeTextures {
        match self {
            Self::Grass => BiomeTextures {
                color: "extracted/terrain/grass002/Grass002_2K-JPG_Color.jpg",
                normal: "extracted/terrain/grass002/Grass002_2K-JPG_NormalGL.jpg",
                ao: Some("extracted/terrain/grass002/Grass002_2K-JPG_AmbientOcclusion.jpg"),
            },
            Self::GrassLush => BiomeTextures {
                color: "extracted/terrain/grass004/Grass004_2K-JPG_Color.jpg",
                normal: "extracted/terrain/grass004/Grass004_2K-JPG_NormalGL.jpg",
                ao: Some("extracted/terrain/grass004/Grass004_2K-JPG_AmbientOcclusion.jpg"),
            },
            Self::Mossy => BiomeTextures {
                color: "extracted/terrain/ground037/Ground037_2K-JPG_Color.jpg",
                normal: "extracted/terrain/ground037/Ground037_2K-JPG_NormalGL.jpg",
                ao: Some("extracted/terrain/ground037/Ground037_2K-JPG_AmbientOcclusion.jpg"),
            },
            Self::Dirt => BiomeTextures {
                color: "extracted/terrain/ground048/Ground048_2K-JPG_Color.jpg",
                normal: "extracted/terrain/ground048/Ground048_2K-JPG_NormalGL.jpg",
                ao: Some("extracted/terrain/ground048/Ground048_2K-JPG_AmbientOcclusion.jpg"),
            },
            Self::Snow => BiomeTextures {
                color: "extracted/terrain/snow/Snow004_2K-JPG_Color.jpg",
                normal: "extracted/terrain/snow/Snow004_2K-JPG_NormalGL.jpg",
                ao: None,
            },
            Self::Stone => BiomeTextures {
                color: "extracted/terrain/stone/PavingStones004_2K-JPG_Color.jpg",
                normal: "extracted/terrain/stone/PavingStones004_2K-JPG_NormalGL.jpg",
                ao: None,
            },
            Self::Scorched => BiomeTextures {
                color: "extracted/terrain/scorched/Ground063_2K-JPG_Color.jpg",
                normal: "extracted/terrain/scorched/Ground063_2K-JPG_NormalGL.jpg",
                ao: Some("extracted/terrain/scorched/Ground063_2K-JPG_AmbientOcclusion.jpg"),
            },
            Self::Marsh => BiomeTextures {
                color: "extracted/terrain/marsh/Ground059_2K-JPG_Color.jpg",
                normal: "extracted/terrain/marsh/Ground059_2K-JPG_NormalGL.jpg",
                ao: Some("extracted/terrain/marsh/Ground059_2K-JPG_AmbientOcclusion.jpg"),
            },
            Self::Rocky => BiomeTextures {
                color: "extracted/terrain/rocky/Rocks023_2K-JPG_Color.jpg",
                normal: "extracted/terrain/rocky/Rocks023_2K-JPG_NormalGL.jpg",
                ao: Some("extracted/terrain/rocky/Rocks023_2K-JPG_AmbientOcclusion.jpg"),
            },
        }
    }
}

pub struct BiomeTextures {
    pub color: &'static str,
    pub normal: &'static str,
    pub ao: Option<&'static str>,
}

/// Pre-computed hub anchors for nearest-hub biome resolution.
#[derive(Resource, Default)]
pub struct BiomeResolver {
    hubs: Vec<HubBiome>,
}

#[derive(Clone, Copy, Debug)]
struct HubBiome {
    x: f32,
    z: f32,
    biome: BiomeKey,
    influence_sq: f32,
}

/// Same influence radius as the client: 900u covers Dalewatch's
/// 1200×1200u playable box without bleeding into a neighboring zone.
const HUB_INFLUENCE_RADIUS: f32 = 900.0;

impl BiomeResolver {
    pub fn load(world_root: &Path) -> Self {
        let world = match load_world(world_root) {
            Ok(w) => w,
            Err(e) => {
                warn!("editor BiomeResolver: load_world failed: {e}");
                return Self::default();
            }
        };
        Self::from_world(&world)
    }

    pub fn from_world(world: &World) -> Self {
        let zone_origins = compute_zone_origins(world);
        let mut hubs = Vec::new();
        for zone in &world.zones {
            let Some(&(ox, oz)) = zone_origins.get(&zone.id) else {
                continue;
            };
            for hub in world.hubs_in_zone(&zone.id) {
                let Some(off) = hub.offset_from_zone_origin.as_ref() else {
                    continue;
                };
                hubs.push(HubBiome {
                    x: ox + off.x,
                    z: oz + off.z,
                    biome: BiomeKey::from_yaml(&hub.biome),
                    influence_sq: HUB_INFLUENCE_RADIUS * HUB_INFLUENCE_RADIUS,
                });
            }
        }
        info!("editor BiomeResolver: {} hub anchors", hubs.len());
        Self { hubs }
    }

    pub fn biome_at(&self, x: f32, z: f32) -> BiomeKey {
        let mut best: Option<(f32, BiomeKey)> = None;
        for h in &self.hubs {
            let dx = h.x - x;
            let dz = h.z - z;
            let d2 = dx * dx + dz * dz;
            if d2 > h.influence_sq {
                continue;
            }
            if best.map_or(true, |(b, _)| d2 < b) {
                best = Some((d2, h.biome));
            }
        }
        best.map(|(_, k)| k).unwrap_or(BiomeKey::Grass)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_resolver_falls_back_to_grass() {
        let r = BiomeResolver::default();
        assert_eq!(r.biome_at(100.0, 100.0), BiomeKey::Grass);
    }

    #[test]
    fn from_yaml_maps_known_keys() {
        assert_eq!(BiomeKey::from_yaml("grass"), BiomeKey::Grass);
        assert_eq!(BiomeKey::from_yaml("grass_lush"), BiomeKey::GrassLush);
        assert_eq!(BiomeKey::from_yaml("mossy"), BiomeKey::Mossy);
        assert_eq!(BiomeKey::from_yaml("snow"), BiomeKey::Snow);
        // Unknown → fallback.
        assert_eq!(BiomeKey::from_yaml("not_a_biome"), BiomeKey::Grass);
    }
}

// keep import live across feature flags
#[allow(dead_code)]
fn _hashmap_marker() -> HashMap<(), ()> {
    HashMap::new()
}
