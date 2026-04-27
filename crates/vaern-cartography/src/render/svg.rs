//! Tiny SVG-string helpers. We emit raw strings rather than going
//! through an `svg` crate so byte-output is fully under our control —
//! the same inputs always produce the same bytes.

use std::fmt::Write;

use vaern_data::{Bounds, Coord2};

/// Maps zone-meter coordinates to SVG-pixel coordinates. The mapping
/// is uniform-scale, fitting `bounds` into the canvas while preserving
/// aspect ratio. North (smallest z) appears at the top of the canvas.
#[derive(Debug, Clone, Copy)]
pub struct Projection {
    pub scale: f32,
    pub offset_x: f32,
    pub offset_y: f32,
}

impl Projection {
    pub fn fit(bounds: Bounds, canvas_w: u32, canvas_h: u32, padding_px: f32) -> Self {
        let cw = canvas_w as f32;
        let ch = canvas_h as f32;
        let bw = bounds.width().max(1.0);
        let bh = bounds.height().max(1.0);
        let avail_w = (cw - 2.0 * padding_px).max(1.0);
        let avail_h = (ch - 2.0 * padding_px).max(1.0);
        let scale = (avail_w / bw).min(avail_h / bh);
        let used_w = bw * scale;
        let used_h = bh * scale;
        let offset_x = (cw - used_w) * 0.5 - bounds.min.x * scale;
        let offset_y = (ch - used_h) * 0.5 - bounds.min.z * scale;
        Self {
            scale,
            offset_x,
            offset_y,
        }
    }

    pub fn project(&self, p: Coord2) -> (f32, f32) {
        (p.x * self.scale + self.offset_x, p.z * self.scale + self.offset_y)
    }

    pub fn project_pair(&self, p: Coord2) -> String {
        let (x, y) = self.project(p);
        format!("{:.2},{:.2}", x, y)
    }

    /// Convert a zone-meter length to SVG pixels.
    pub fn px(&self, units: f32) -> f32 {
        units * self.scale
    }
}

/// Round-trip-stable f32 formatting for SVG attributes. Two-decimal
/// precision is plenty for a parchment map and keeps output
/// byte-identical across runs.
pub fn f(v: f32) -> String {
    format!("{:.2}", v)
}

pub fn polyline_points(proj: &Projection, pts: &[Coord2]) -> String {
    let mut s = String::with_capacity(pts.len() * 12);
    for (i, p) in pts.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        let _ = write!(s, "{}", proj.project_pair(*p));
    }
    s
}

pub fn polygon_d(proj: &Projection, pts: &[Coord2]) -> String {
    if pts.is_empty() {
        return String::new();
    }
    let mut s = String::with_capacity(pts.len() * 14);
    let (x0, y0) = proj.project(pts[0]);
    let _ = write!(s, "M {} {}", f(x0), f(y0));
    for p in &pts[1..] {
        let (x, y) = proj.project(*p);
        let _ = write!(s, " L {} {}", f(x), f(y));
    }
    s.push_str(" Z");
    s
}
