//! Deterministic 2D map rendering. The function signatures take fully
//! resolved data (no I/O) so the same inputs always produce the same
//! SVG bytes — golden-image testable.

mod svg;
mod world;
mod zone;

pub use world::render_world_svg;
pub use zone::render_zone_svg;

#[derive(Debug, Clone)]
pub struct RenderOptions {
    /// SVG canvas width in pixels. The renderer fits the zone bounds
    /// (or world placements) into this canvas while preserving aspect.
    pub canvas_width: u32,
    pub canvas_height: u32,
    pub include_legend: bool,
    pub include_compass: bool,
    pub include_scale_bar: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            canvas_width: 1600,
            canvas_height: 2000,
            include_legend: true,
            include_compass: true,
            include_scale_bar: true,
        }
    }
}
