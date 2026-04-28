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
use vaern_data::{load_world, load_world_layout, World, WorldLayout};

/// 9-biome palette. Unknown YAML keys fall through to `Grass`.
/// Mirror of `vaern-editor/src/voxel/biomes.rs`. The cartography
/// crate's richer vocabulary (`forest`, `mountain`, `cropland`, …)
/// collapse-maps into one of these 9 visual buckets — the SVG
/// renderer keeps full fidelity, only 3D rendering is reduced.
/// Shader expansion to a wider palette is a deferred follow-up.
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
    /// Parse a hub YAML `biome:` string OR a cartography
    /// `geography.yaml::biome_regions[].biome` string. Cartography
    /// keys collapse-map into the 9-slot palette; unknown keys fall
    /// through to `Grass`.
    pub fn from_yaml(s: &str) -> Self {
        match s {
            // Legacy hub-YAML keys (one-to-one).
            "grass" => Self::Grass,
            "grass_lush" => Self::GrassLush,
            "mossy" => Self::Mossy,
            "dirt" => Self::Dirt,
            "snow" => Self::Snow,
            "stone" => Self::Stone,
            "scorched" => Self::Scorched,
            "marsh" => Self::Marsh,
            "rocky" => Self::Rocky,
            // Cartography vocabulary aliases — collapse into the
            // 9-slot palette. Three shades of green
            // (Grass / Mossy / GrassLush) cover most of the
            // pastoral/forest/highland axis.
            "fields" | "river_valley" | "pasture" => Self::Grass,
            "forest" | "temperate_forest" => Self::Mossy,
            "highland" => Self::GrassLush,
            "mountain" | "mountain_rock"
            | "coastal_cliff" | "fjord" | "ridge_scrub" => Self::Rocky,
            "ashland" => Self::Scorched,
            "marshland" | "mud" => Self::Marsh,
            "ruin" | "cropland" | "tilled_soil" => Self::Dirt,
            "cobblestone" => Self::Stone,
            "sand" => Self::Grass,
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

/// Fallback ring radius — used only when `world.yaml` doesn't list a
/// zone. The canonical placements live in `src/generated/world/world.yaml`
/// and are loaded into a `WorldLayout`.
const FALLBACK_RING_RADIUS: f32 = 2800.0;

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

        let layout = load_world_layout(world_root).unwrap_or_default();
        let zone_origins = compute_zone_origins(&world, &layout);
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

/// Returns world-meter offsets for every starter zone. Reads
/// `world.yaml` zone_placements when available; falls back to a
/// deterministic ring sorted by zone id.
fn compute_zone_origins(world: &World, layout: &WorldLayout) -> HashMap<String, (f32, f32)> {
    let mut starters: Vec<&str> = world
        .zones
        .iter()
        .filter_map(|z| z.starter_race.as_deref().map(|_| z.id.as_str()))
        .collect();
    starters.sort();
    let n = starters.len().max(1) as f32;
    let mut out = HashMap::new();
    for (i, zid) in starters.iter().enumerate() {
        if let Some(p) = layout.zone_origin(zid) {
            out.insert(zid.to_string(), (p.x, p.z));
        } else {
            let angle = (i as f32 / n) * std::f32::consts::TAU;
            out.insert(
                zid.to_string(),
                (FALLBACK_RING_RADIUS * angle.cos(), FALLBACK_RING_RADIUS * angle.sin()),
            );
        }
    }
    out
}
