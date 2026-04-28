//! Schema validation + deterministic 2D map rendering for the Vaern
//! world data. This crate sits on top of `vaern-data` and is the gate
//! between authored YAML and downstream pipelines (2D maps, eventually
//! 3D terrain).

pub mod edits;
pub mod heightfield;
pub mod raster;
pub mod render;
pub mod runtime_resolver;
pub mod style;
pub mod validate;
pub mod world_terrain;

pub use runtime_resolver::install_terrain_resolver;

pub use edits::{
    biome_edits_path, elevation_edits_path, load_biome_edits, load_elevation_edits,
    save_biome_edits, save_elevation_edits, BiomeEditsFile, BiomeEditsV1, ElevationEditsFile,
    ElevationEditsV1,
};
pub use heightfield::{seed_for_zone, PolygonIndex};
pub use world_terrain::{WorldTerrain, ZoneTerrain};

pub use render::{render_world_svg, render_zone_svg, RenderOptions};
pub use style::{load_cartography_style, BiomeStyle, CartographyStyle, GlyphLibrary, RoadStyle};
pub use validate::{validate, Severity, ValidationIssue, ValidationReport, WorldBundle};
