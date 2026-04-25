//! Per-chunk biome resolution for the voxel ground.
//!
//! Replaces the overlay-mesh pipeline in `scene/hub_regions.rs`. Instead
//! of lifting textured floor patches + road ribbons above the voxel
//! surface, each voxel chunk asks this resolver "what biome owns my
//! footprint?" at seed time and picks a per-biome `StandardMaterial`
//! (CC0 ambientCG PBR set) from a cached table.
//!
//! Resolution is grid-aligned at chunk granularity (32u steps) — sharp
//! transitions between biomes. Smooth per-fragment blending would need
//! a custom triplanar shader and per-vertex biome weights; that's the
//! proper follow-up once the chunk-tiled baseline is in.
//!
//! Roads are not rendered today. The Catmull-Rom ribbon overlay from
//! `hub_regions` is gone; a "dirt corridor" biome override along road
//! paths would bake them into this same system without geometry.

use std::collections::HashMap;
use std::path::Path;

use bevy::prelude::*;
use vaern_data::{load_world, World};

/// Biome tag — one of the 9 palette entries from the hub YAMLs, plus
/// a default `Grass` fallback for chunks outside any hub's influence.
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
    /// Parse a hub YAML `biome:` string. Unknown keys fall through to
    /// `Grass`.
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

    /// PBR texture triple for this biome. Paths are relative to the
    /// client's `assets/` root. `ao` is optional — some ambientCG sets
    /// ship without AO (Snow004, PavingStones004).
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

/// PBR paths for a biome. `'static` — the table is compile-time.
pub struct BiomeTextures {
    pub color: &'static str,
    pub normal: &'static str,
    pub ao: Option<&'static str>,
}

/// Pre-computed hub positions + their declared biome. The resolver
/// answers biome queries by nearest-hub distance — coarse but correct
/// and matches what hub_regions did, just without the overlay geometry.
#[derive(Resource, Default)]
pub struct BiomeResolver {
    hubs: Vec<HubBiome>,
}

#[derive(Clone, Copy, Debug)]
struct HubBiome {
    x: f32,
    z: f32,
    biome: BiomeKey,
    /// Squared influence radius. Queries past this distance fall back
    /// to the default `Grass` biome, so we don't paint snow 3000u from
    /// a mountain hub just because it's the nearest.
    influence_sq: f32,
}

/// How far a hub's biome can reach, world units. 900u covers
/// Dalewatch's 1200×1200u playable box while staying inside a zone's
/// typical 2800u ring spacing.
const HUB_INFLUENCE_RADIUS: f32 = 900.0;

/// Zone ring radius — must match `vaern-server::data::load_game_data`.
/// Duplicated because the client doesn't link the server crate.
const ZONE_RING_RADIUS: f32 = 2800.0;

impl BiomeResolver {
    /// Load the world YAML and build the hub → biome table. On failure
    /// returns an empty resolver (every query → `Grass`).
    pub fn load(world_root: &Path) -> Self {
        let world = match load_world(world_root) {
            Ok(w) => w,
            Err(e) => {
                warn!("BiomeResolver: couldn't load world from {world_root:?}: {e}");
                return Self::default();
            }
        };

        let zone_origins = compute_zone_origins(&world);
        let mut hubs = Vec::new();

        for zone in &world.zones {
            let Some(&(ox, oz)) = zone_origins.get(&zone.id) else { continue };
            for hub in world.hubs_in_zone(&zone.id) {
                let Some(off) = hub.offset_from_zone_origin.as_ref() else { continue };
                hubs.push(HubBiome {
                    x: ox + off.x,
                    z: oz + off.z,
                    biome: BiomeKey::from_yaml(&hub.biome),
                    influence_sq: HUB_INFLUENCE_RADIUS * HUB_INFLUENCE_RADIUS,
                });
            }
        }

        info!("BiomeResolver: loaded {} hub biome sources", hubs.len());
        Self { hubs }
    }

    /// Nearest-hub biome at world `(x, z)`. Returns `Grass` if no hub
    /// is within its influence radius.
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

/// Mirror of `vaern-server::data::load_game_data`'s zone-ring layout.
/// Must stay in sync; if the server's formula changes, biomes paint in
/// the wrong world position. Same logic as `scene::hub_regions`.
fn compute_zone_origins(world: &World) -> HashMap<String, (f32, f32)> {
    let mut starters: Vec<&str> = world
        .zones
        .iter()
        .filter_map(|z| z.starter_race.as_deref().map(|_| z.id.as_str()))
        .collect();
    starters.sort();
    let n = starters.len().max(1) as f32;
    let mut out = HashMap::new();
    for (i, zid) in starters.iter().enumerate() {
        let angle = (i as f32 / n) * std::f32::consts::TAU;
        out.insert(
            zid.to_string(),
            (ZONE_RING_RADIUS * angle.cos(), ZONE_RING_RADIUS * angle.sin()),
        );
    }
    out
}
