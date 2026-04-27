//! Cartography style sheet + glyph library loaders. The style sheet
//! lives at `src/generated/world/style/cartography_style.yaml`; glyph
//! SVG fragments live under `src/generated/world/style/glyphs/<name>.svg`.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StyleLoadError {
    #[error("io at {path:?}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("yaml parse at {path:?}: {source}")]
    Yaml {
        path: PathBuf,
        source: serde_yaml::Error,
    },
    #[error("missing style file at {path:?}")]
    Missing { path: PathBuf },
}

#[derive(Debug, Clone, Deserialize)]
pub struct BiomeStyle {
    pub base_color: String,
    #[serde(default)]
    pub line_color: String,
    #[serde(default)]
    pub pattern: String,
    #[serde(default = "one")]
    pub opacity_default: f32,
}

fn one() -> f32 {
    1.0
}

#[derive(Debug, Clone, Deserialize)]
pub struct HubIcon {
    pub glyph: String,
    #[serde(default)]
    pub size: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RoadStyle {
    pub color: String,
    pub width: f32,
    #[serde(default)]
    pub dash_pattern: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PaperStyle {
    pub base_color: String,
    pub edge_color: String,
    #[serde(default = "default_edge_width")]
    pub edge_width: f32,
    #[serde(default)]
    pub inner_shadow_opacity: f32,
}

fn default_edge_width() -> f32 {
    6.0
}

#[derive(Debug, Clone, Deserialize)]
pub struct TextStyle {
    pub font: String,
    pub size: f32,
    #[serde(default)]
    pub weight: String,
    #[serde(default)]
    pub style: String,
    #[serde(default)]
    pub fill: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CartographyStyle {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub schema_version: String,
    pub biomes: HashMap<String, BiomeStyle>,
    pub hub_icons: HashMap<String, HubIcon>,
    #[serde(default)]
    pub landmark_glyphs: HashMap<String, String>,
    pub roads: HashMap<String, RoadStyle>,
    pub paper: HashMap<String, PaperStyle>,
    pub typography: HashMap<String, TextStyle>,
}

impl CartographyStyle {
    pub fn biome(&self, key: &str) -> &BiomeStyle {
        self.biomes
            .get(key)
            .unwrap_or_else(|| {
                self.biomes
                    .get("default")
                    .expect("cartography_style.yaml must define biomes.default")
            })
    }

    pub fn hub_icon(&self, role: &str) -> Option<&HubIcon> {
        self.hub_icons.get(role)
    }

    pub fn paper(&self) -> &PaperStyle {
        self.paper
            .get("parchment_warm")
            .expect("cartography_style.yaml must define paper.parchment_warm")
    }

    pub fn text(&self, key: &str) -> Option<&TextStyle> {
        self.typography.get(key)
    }
}

/// Glyph library — name → raw SVG fragment string.
#[derive(Debug, Default, Clone)]
pub struct GlyphLibrary {
    pub by_name: HashMap<String, String>,
}

impl GlyphLibrary {
    pub fn get(&self, name: &str) -> Option<&str> {
        self.by_name.get(name).map(String::as_str)
    }
}

/// Load `cartography_style.yaml` and the sibling `glyphs/` directory
/// from the world style root: `src/generated/world/style/`.
pub fn load_cartography_style(
    style_root: impl AsRef<Path>,
) -> Result<(CartographyStyle, GlyphLibrary), StyleLoadError> {
    let style_root = style_root.as_ref();
    let style_path = style_root.join("cartography_style.yaml");
    if !style_path.exists() {
        return Err(StyleLoadError::Missing { path: style_path });
    }
    let text = fs::read_to_string(&style_path).map_err(|e| StyleLoadError::Io {
        path: style_path.clone(),
        source: e,
    })?;
    let style: CartographyStyle =
        serde_yaml::from_str(&text).map_err(|e| StyleLoadError::Yaml {
            path: style_path.clone(),
            source: e,
        })?;

    let glyphs_dir = style_root.join("glyphs");
    let mut glyphs = GlyphLibrary::default();
    if glyphs_dir.exists() {
        for entry in fs::read_dir(&glyphs_dir).map_err(|e| StyleLoadError::Io {
            path: glyphs_dir.clone(),
            source: e,
        })? {
            let entry = entry.map_err(|e| StyleLoadError::Io {
                path: glyphs_dir.clone(),
                source: e,
            })?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("svg") {
                continue;
            }
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_string)
                .unwrap_or_default();
            let body = fs::read_to_string(&path).map_err(|e| StyleLoadError::Io {
                path: path.clone(),
                source: e,
            })?;
            glyphs.by_name.insert(name, body);
        }
    }

    Ok((style, glyphs))
}
