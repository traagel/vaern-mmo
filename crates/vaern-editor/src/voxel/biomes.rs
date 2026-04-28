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
//!
//! ## 9-slot palette
//!
//! BiomeKey is **9 variants** because the entire blend pipeline is
//! hardcoded to 9 slots — `compute_blend_weights() -> [f32; 9]`,
//! the WGSL fragment shader's `array<f32, 9>` weights, the 3 vec4
//! vertex attributes (`w_lo`, `w_hi`, `w_8`), and `biome_debug_color`
//! switch arms 0..=8. Adding more variants without expanding all of
//! those would index out of bounds at runtime.
//!
//! The cartography crate's richer biome vocabulary (`forest`, `mountain`,
//! `cropland`, etc.) collapses into these 9 slots via `from_yaml` —
//! the SVG renderer keeps full fidelity, only the 3D editor's render
//! is reduced. Shader expansion to a wider palette is a deferred
//! follow-up.

use std::collections::HashMap;

/// 9-biome palette. Unknown YAML keys fall through to `Grass`.
/// `from_yaml` accepts both legacy hub-YAML keys (`grass_lush`,
/// `mossy`, …) and the cartography vocabulary (`fields`, `forest`,
/// `mountain`, `cobblestone`, …) — the latter collapse-map into one
/// of these 9 visual buckets.
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
            // 9-slot palette. Goal: each cartography region renders
            // as a *visually distinct* biome in the editor while
            // staying within the 9-slot pipeline. Three shades of
            // green (Grass / Mossy / GrassLush) cover most of the
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

    #[test]
    fn cartography_keys_collapse_into_palette() {
        // Each cartography biome string maps to a sensible 9-slot
        // visual. Three shades of green for fields/forest/highland.
        assert_eq!(BiomeKey::from_yaml("fields"), BiomeKey::Grass);
        assert_eq!(BiomeKey::from_yaml("river_valley"), BiomeKey::Grass);
        assert_eq!(BiomeKey::from_yaml("forest"), BiomeKey::Mossy);
        assert_eq!(BiomeKey::from_yaml("temperate_forest"), BiomeKey::Mossy);
        assert_eq!(BiomeKey::from_yaml("highland"), BiomeKey::GrassLush);
        assert_eq!(BiomeKey::from_yaml("mountain"), BiomeKey::Rocky);
        assert_eq!(BiomeKey::from_yaml("coastal_cliff"), BiomeKey::Rocky);
        assert_eq!(BiomeKey::from_yaml("fjord"), BiomeKey::Rocky);
        assert_eq!(BiomeKey::from_yaml("ridge_scrub"), BiomeKey::Rocky);
        assert_eq!(BiomeKey::from_yaml("ashland"), BiomeKey::Scorched);
        assert_eq!(BiomeKey::from_yaml("marshland"), BiomeKey::Marsh);
        assert_eq!(BiomeKey::from_yaml("ruin"), BiomeKey::Dirt);
        assert_eq!(BiomeKey::from_yaml("cropland"), BiomeKey::Dirt);
        assert_eq!(BiomeKey::from_yaml("cobblestone"), BiomeKey::Stone);
        assert_eq!(BiomeKey::from_yaml("pasture"), BiomeKey::Grass);
    }

    #[test]
    fn all_ids_below_palette_size() {
        // Guard the 9-slot blend pipeline. If a future variant lands
        // with id ≥ 9, `compute_blend_weights` would panic at
        // `weights[id]` (the array is `[f32; 9]`).
        for b in BiomeKey::ALL {
            assert!(
                (b.id() as usize) < 9,
                "BiomeKey {:?} has id {} ≥ 9 — would OOB the blend weights",
                b,
                b.id()
            );
        }
    }
}

// keep import live across feature flags
#[allow(dead_code)]
fn _hashmap_marker() -> HashMap<(), ()> {
    HashMap::new()
}
