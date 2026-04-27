//! Schema validation + deterministic 2D map rendering for the Vaern
//! world data. This crate sits on top of `vaern-data` and is the gate
//! between authored YAML and downstream pipelines (2D maps, eventually
//! 3D terrain).

pub mod render;
pub mod style;
pub mod validate;

pub use render::{render_world_svg, render_zone_svg, RenderOptions};
pub use style::{load_cartography_style, BiomeStyle, CartographyStyle, GlyphLibrary, RoadStyle};
pub use validate::{validate, Severity, ValidationIssue, ValidationReport, WorldBundle};
