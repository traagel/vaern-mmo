//! World-level renderer. Places every zone at its `world_origin` and
//! draws cross-zone connections as overlay edges.

use std::fmt::Write;

use vaern_data::{Bounds, ConnectionsIndex, Coord2, World, WorldLayout};

use crate::{
    render::{
        svg::{f, polygon_d, polyline_points, Projection},
        RenderOptions,
    },
    style::CartographyStyle,
};

const PADDING_PX: f32 = 80.0;

pub fn render_world_svg(
    world: &World,
    layout: &WorldLayout,
    connections: &ConnectionsIndex,
    style: &CartographyStyle,
    opts: &RenderOptions,
) -> String {
    let bounds = world_bounds(layout);
    let proj = Projection::fit(bounds, opts.canvas_width, opts.canvas_height, PADDING_PX);

    let mut out = String::with_capacity(16 * 1024);
    let _ = write!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">\n",
        opts.canvas_width, opts.canvas_height, opts.canvas_width, opts.canvas_height
    );

    // Sea — fills the entire canvas as a backdrop. Land is drawn over it.
    let _ = write!(
        out,
        "  <rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" fill=\"#88A2A8\"/>\n",
        opts.canvas_width, opts.canvas_height
    );

    // Land — coastline polygon filled with parchment color.
    let paper = style.paper();
    if let Some(coast) = &layout.coastline {
        let d = polygon_d(&proj, &coast.points);
        let _ = write!(
            out,
            "  <path id=\"coastline\" d=\"{}\" fill=\"{}\" stroke=\"#3E595F\" stroke-width=\"2.5\" stroke-linejoin=\"round\"/>\n",
            d, paper.base_color
        );
    } else {
        // Fallback if no coastline: paper covers everything.
        let _ = write!(
            out,
            "  <rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" fill=\"{}\"/>\n",
            opts.canvas_width, opts.canvas_height, paper.base_color
        );
    }

    // Outer frame
    let _ = write!(
        out,
        "  <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\"/>\n",
        f(paper.edge_width * 0.5),
        f(paper.edge_width * 0.5),
        f(opts.canvas_width as f32 - paper.edge_width),
        f(opts.canvas_height as f32 - paper.edge_width),
        paper.edge_color,
        f(paper.edge_width)
    );

    // Zones — Voronoi cells filled with biome color; fallback to scaled
    // rectangles for zones without a cell (transitional during rollout).
    out.push_str("  <g id=\"zones\">\n");
    let placements_by_zone: std::collections::HashMap<&str, &vaern_data::ZonePlacement> = layout
        .zone_placements
        .iter()
        .map(|p| (p.zone.as_str(), p))
        .collect();
    let mut zones: Vec<&vaern_data::Zone> = world.zones.iter().collect();
    zones.sort_by(|a, b| a.id.cmp(&b.id));
    for zone in &zones {
        let Some(placement) = placements_by_zone.get(zone.id.as_str()) else {
            continue;
        };
        let bs = style.biome(&zone.biome);
        let stroke_w = if zone.starter_race.is_some() { 2.0 } else { 1.4 };
        if let Some(cell) = &placement.zone_cell {
            let d = polygon_d(&proj, &cell.points);
            let _ = write!(
                out,
                "    <path d=\"{}\" fill=\"{}\" fill-opacity=\"0.92\" stroke=\"{}\" stroke-width=\"{}\" stroke-linejoin=\"round\"/>\n",
                d, bs.base_color, bs.line_color, f(stroke_w)
            );
        } else if let Some(b) = zone.bounds {
            // Fallback rectangle
            let (cx, cy) = proj.project(placement.world_origin);
            let w_px = proj.px(b.width());
            let h_px = proj.px(b.height());
            let _ = write!(
                out,
                "    <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"3\" ry=\"3\" fill=\"{}\" fill-opacity=\"0.85\" stroke=\"{}\" stroke-width=\"{}\"/>\n",
                f(cx - w_px * 0.5),
                f(cy - h_px * 0.5),
                f(w_px),
                f(h_px),
                bs.base_color,
                bs.line_color,
                f(stroke_w)
            );
        }
    }
    out.push_str("  </g>\n");

    // Connections (drawn over cells so edges read clearly).
    out.push_str("  <g id=\"connections\" stroke=\"#3A2C1B\" stroke-width=\"1.4\" fill=\"none\" stroke-dasharray=\"4 3\" stroke-opacity=\"0.7\">\n");
    let mut drawn = std::collections::HashSet::new();
    for (from_zone, e) in connections.all_edges() {
        let key = if from_zone < e.to_zone.as_str() {
            (from_zone, e.to_zone.as_str())
        } else {
            (e.to_zone.as_str(), from_zone)
        };
        if !drawn.insert(key) {
            continue;
        }
        let Some(a) = layout.zone_origin(from_zone) else { continue };
        let Some(b) = layout.zone_origin(&e.to_zone) else { continue };
        let pts = polyline_points(&proj, &[a, b]);
        let _ = write!(out, "    <polyline points=\"{}\"/>\n", pts);
    }
    out.push_str("  </g>\n");

    // Zone labels — placed at world_origin so they sit roughly on
    // each cell's centroid (which equals the anchor by construction
    // for Voronoi cells of single-anchor sites).
    out.push_str("  <g id=\"zone-labels\">\n");
    for zone in &zones {
        let Some(placement) = placements_by_zone.get(zone.id.as_str()) else {
            continue;
        };
        let (cx, cy) = proj.project(placement.world_origin);
        let label = style
            .text("outpost_label")
            .map(|t| (t.font.as_str(), t.size, t.fill.as_str()))
            .unwrap_or(("Georgia, serif", 11.0, "#3A2C1B"));
        let _ = write!(
            out,
            "    <text x=\"{}\" y=\"{}\" font-family=\"{}\" font-size=\"{}\" fill=\"{}\" text-anchor=\"middle\">{}</text>\n",
            f(cx),
            f(cy + 4.0),
            label.0,
            f(label.1),
            label.2,
            xml_escape(&zone.name)
        );
    }
    out.push_str("  </g>\n");

    let _ = write!(
        out,
        "  <text x=\"{}\" y=\"{}\" font-family=\"Georgia, serif\" font-size=\"24\" font-weight=\"bold\" fill=\"#3A2C1B\" text-anchor=\"middle\">{}</text>\n",
        f(opts.canvas_width as f32 * 0.5),
        f(40.0),
        "The World of Vaern"
    );

    out.push_str("</svg>\n");
    out
}

fn world_bounds(layout: &WorldLayout) -> Bounds {
    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_z = f32::INFINITY;
    let mut max_z = f32::NEG_INFINITY;
    let mut any = false;
    // Coastline (if present) defines the visible world extent.
    if let Some(coast) = &layout.coastline {
        for p in &coast.points {
            min_x = min_x.min(p.x);
            max_x = max_x.max(p.x);
            min_z = min_z.min(p.z);
            max_z = max_z.max(p.z);
            any = true;
        }
    }
    // Anchors as a fallback if no coastline.
    for p in &layout.zone_placements {
        min_x = min_x.min(p.world_origin.x);
        max_x = max_x.max(p.world_origin.x);
        min_z = min_z.min(p.world_origin.z);
        max_z = max_z.max(p.world_origin.z);
        any = true;
    }
    if !any {
        return Bounds {
            min: Coord2::new(-3000.0, -3000.0),
            max: Coord2::new(3000.0, 3000.0),
        };
    }
    let pad = 800.0;
    Bounds {
        min: Coord2::new(min_x - pad, min_z - pad),
        max: Coord2::new(max_x + pad, max_z + pad),
    }
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}
