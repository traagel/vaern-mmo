//! Per-zone spatial layer: biome polygons, rivers, roads, discrete
//! features, and density-driven scatter. Loaded from
//! `world/zones/<zone>/geography.yaml`. Zones without a geography file
//! are silently skipped — the renderer will fall back to a plain
//! `bounds`-rectangle for those zones until they're authored.

use std::{collections::HashMap, fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::{read_dir, Coord2, LoadError, PolyPath, Polygon};

/// One painted biome region. The polygon is in zone-local meters.
/// `biome` keys into `cartography_style.yaml::biomes`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BiomeRegion {
    pub id: String,
    #[serde(default)]
    pub label: String,
    pub biome: String,
    pub polygon: Polygon,
    #[serde(default)]
    pub label_position: Option<Coord2>,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
}

fn default_opacity() -> f32 {
    1.0
}

/// One river. `path` is the centerline; `width_units` is rendered
/// width in zone meters. Tributaries branch off the main path.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct River {
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub path: PolyPath,
    #[serde(default = "default_river_width")]
    pub width_units: f32,
    #[serde(default)]
    pub tributaries: Vec<RiverTributary>,
    #[serde(default)]
    pub label_position: Option<Coord2>,
}

fn default_river_width() -> f32 {
    40.0
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RiverTributary {
    pub id: String,
    pub path: PolyPath,
    #[serde(default = "default_tributary_width")]
    pub width_units: f32,
}

fn default_tributary_width() -> f32 {
    20.0
}

/// One road. `type_` keys into `cartography_style.yaml::roads`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Road {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub path: PolyPath,
    #[serde(default)]
    pub label_position: Option<Coord2>,
    #[serde(default)]
    pub label_rotation_deg: f32,
}

/// One discrete glyph (fauna sigil, monument, point feature). The
/// `glyph` keys into `style/glyphs/<glyph>.svg`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Feature {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub glyph: String,
    #[serde(default = "default_count")]
    pub count: u32,
    pub position: Coord2,
}

fn default_count() -> u32 {
    1
}

/// Density-driven dressing scatter (trees, hills, rocks). The
/// renderer seeds an LcgRng from `seed` (or a hash of zone+rule_id if
/// `seed` is None) so output is byte-deterministic.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct GeographyScatter {
    #[serde(default)]
    pub trees: Option<ScatterLayer>,
    #[serde(default)]
    pub hills: Option<ScatterLayer>,
    #[serde(default)]
    pub rocks: Option<ScatterLayer>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScatterLayer {
    /// `low` / `medium` / `high`. Mapped to per-cell probabilities by
    /// the renderer.
    pub density: String,
    #[serde(default)]
    pub biomes_allowed: Vec<String>,
    /// Optional explicit positions. When set, the renderer uses these
    /// in addition to procedurally-scattered positions.
    #[serde(default)]
    pub explicit_positions: Vec<Coord2>,
    /// Deterministic seed. When `None`, hashed from `(zone_id, layer_kind)`.
    #[serde(default)]
    pub seed: Option<u64>,
}

/// Free-floating atmospheric label not bound to any region.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FreeLabel {
    pub text: String,
    pub position: Coord2,
    #[serde(default)]
    pub rotation_deg: f32,
    #[serde(default)]
    pub style: String,
}

/// One zone's spatial layer. File: `geography.yaml`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Geography {
    pub id: String,
    pub zone: String,
    #[serde(default)]
    pub schema_version: String,
    #[serde(default)]
    pub biome_regions: Vec<BiomeRegion>,
    #[serde(default)]
    pub rivers: Vec<River>,
    #[serde(default)]
    pub roads: Vec<Road>,
    #[serde(default)]
    pub features: Vec<Feature>,
    #[serde(default)]
    pub scatter: GeographyScatter,
    #[serde(default)]
    pub free_labels: Vec<FreeLabel>,
}

/// Geography per zone, keyed by zone id.
#[derive(Debug, Default, Clone)]
pub struct GeographyIndex {
    pub by_zone: HashMap<String, Geography>,
}

impl GeographyIndex {
    pub fn get(&self, zone_id: &str) -> Option<&Geography> {
        self.by_zone.get(zone_id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Geography> {
        self.by_zone.values()
    }
}

/// Walk `world_root/zones/<zone>/geography.yaml`. Zones without the
/// file are silently skipped.
pub fn load_all_geography(world_root: impl AsRef<Path>) -> Result<GeographyIndex, LoadError> {
    let world_root = world_root.as_ref();
    let zones_dir = world_root.join("zones");
    let mut out = GeographyIndex::default();
    if !zones_dir.exists() {
        return Ok(out);
    }
    for zone_dir in read_dir(&zones_dir)? {
        if !zone_dir.is_dir() {
            continue;
        }
        let path = zone_dir.join("geography.yaml");
        if !path.exists() {
            continue;
        }
        let text = fs::read_to_string(&path).map_err(|e| LoadError::Io {
            path: path.clone(),
            source: e,
        })?;
        let geo: Geography = serde_yaml::from_str(&text).map_err(|e| LoadError::Yaml {
            path: path.clone(),
            source: e,
        })?;
        out.by_zone.insert(geo.zone.clone(), geo);
    }
    Ok(out)
}
