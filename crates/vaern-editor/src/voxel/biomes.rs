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

    pub fn label(self) -> &'static str {
        match self {
            Self::Grass => "Grass",
            Self::GrassLush => "Grass (lush)",
            Self::Mossy => "Mossy",
            Self::Dirt => "Dirt",
            Self::Snow => "Snow",
            Self::Stone => "Stone",
            Self::Scorched => "Scorched",
            Self::Marsh => "Marsh",
            Self::Rocky => "Rocky",
        }
    }

    /// Stable u8 mapping for on-disk biome-override persistence. The
    /// numeric values must NEVER be reordered — existing files would
    /// silently swap biomes. Add new variants only at the end.
    pub const fn id(self) -> u8 {
        match self {
            Self::Grass => 0,
            Self::GrassLush => 1,
            Self::Mossy => 2,
            Self::Dirt => 3,
            Self::Snow => 4,
            Self::Stone => 5,
            Self::Scorched => 6,
            Self::Marsh => 7,
            Self::Rocky => 8,
        }
    }

    pub fn from_id(id: u8) -> Option<Self> {
        Some(match id {
            0 => Self::Grass,
            1 => Self::GrassLush,
            2 => Self::Mossy,
            3 => Self::Dirt,
            4 => Self::Snow,
            5 => Self::Stone,
            6 => Self::Scorched,
            7 => Self::Marsh,
            8 => Self::Rocky,
            _ => return None,
        })
    }

    pub const ALL: [BiomeKey; 9] = [
        Self::Grass,
        Self::GrassLush,
        Self::Mossy,
        Self::Dirt,
        Self::Snow,
        Self::Stone,
        Self::Scorched,
        Self::Marsh,
        Self::Rocky,
    ];

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

#[cfg(test)]
mod tests {
    use super::*;

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
