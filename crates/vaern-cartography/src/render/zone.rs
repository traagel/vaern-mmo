//! Zone-level SVG renderer. Pure function over fully-resolved data;
//! same inputs → same bytes. Layered:
//!
//!   1. parchment paper background
//!   2. biome regions (filled polygons)
//!   3. rivers (stroked polylines)
//!   4. roads (dashed polylines)
//!   5. scatter dots (deterministic pseudo-random)
//!   6. landmark / feature glyphs
//!   7. hub icons + labels
//!   8. region labels + free labels
//!   9. compass + scale bar + frame

use std::fmt::Write;

use vaern_data::{
    Bounds, ConnectionsIndex, Coord2, Geography, Hub, LandmarkIndex, World, WorldLayout, Zone,
};

use crate::{
    render::{
        svg::{f, polygon_d, polyline_points, Projection},
        RenderOptions,
    },
    style::{CartographyStyle, GlyphLibrary},
};

const PADDING_PX: f32 = 60.0;

pub fn render_zone_svg(
    zone: &Zone,
    world: &World,
    landmarks: &LandmarkIndex,
    geography: Option<&Geography>,
    connections: &ConnectionsIndex,
    style: &CartographyStyle,
    glyphs: &GlyphLibrary,
    layout: &WorldLayout,
    opts: &RenderOptions,
) -> String {
    // Pull the zone's Voronoi cell from world.yaml and rebase to
    // zone-local coords (subtract the zone's world_origin). When a
    // cell exists, the parchment is the cell polygon and the
    // projection bounds become the cell's AABB so the canvas tightly
    // fits the cell. Otherwise we fall back to the rectangular bounds.
    let cell_local: Option<Vec<Coord2>> = layout
        .zone_placements
        .iter()
        .find(|p| p.zone == zone.id)
        .and_then(|p| {
            p.zone_cell.as_ref().map(|c| {
                c.points
                    .iter()
                    .map(|v| Coord2::new(v.x - p.world_origin.x, v.z - p.world_origin.z))
                    .collect()
            })
        });

    let bounds = if let Some(ref cell) = cell_local {
        aabb_of(cell)
    } else {
        zone.bounds
            .expect("render_zone_svg: zone.bounds required (run validator first)")
    };
    let proj = Projection::fit(bounds, opts.canvas_width, opts.canvas_height, PADDING_PX);

    let mut out = String::with_capacity(32 * 1024);
    let _ = write!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">\n",
        opts.canvas_width, opts.canvas_height, opts.canvas_width, opts.canvas_height
    );
    write_defs(&mut out, geography, &proj, style, opts, cell_local.as_deref());
    write_paper(&mut out, style, opts, &proj, cell_local.as_deref());

    // Content clipped to the cell shape (if we have one).
    let clipped = cell_local.is_some();
    if clipped {
        out.push_str("  <g id=\"zone-content\" clip-path=\"url(#zone-cell)\">\n");
    } else {
        out.push_str("  <g id=\"zone-content\">\n");
    }
    if let Some(geo) = geography {
        write_biome_regions(&mut out, geo, &proj, style);
        write_scatter(&mut out, geo, bounds, &proj);
        write_rivers(&mut out, geo, &proj);
        write_roads(&mut out, geo, &proj, style);
        write_features(&mut out, geo, &proj, glyphs);
    }
    write_zone_connections(&mut out, &zone.id, connections, &proj, style);
    write_landmarks(&mut out, &zone.id, landmarks, &proj, glyphs, style);
    write_hubs(&mut out, world, &zone.id, &proj, glyphs, style);
    if let Some(geo) = geography {
        write_region_labels(&mut out, geo, &proj, style);
        write_road_labels(&mut out, geo, style);
        write_free_labels(&mut out, geo, &proj, style);
    }
    out.push_str("  </g>\n");

    // UI overlays sit OUTSIDE the clip so they always render fully.
    if opts.include_compass {
        write_compass(&mut out, opts);
    }
    if opts.include_scale_bar {
        write_scale_bar(&mut out, &proj, opts);
    }
    if opts.include_legend {
        write_legend(&mut out, geography, style, opts);
    }
    write_metrics_card(&mut out, bounds, opts);
    write_title(&mut out, &zone.name, opts);
    out.push_str("</svg>\n");
    out
}

fn aabb_of(points: &[Coord2]) -> Bounds {
    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_z = f32::INFINITY;
    let mut max_z = f32::NEG_INFINITY;
    for p in points {
        min_x = min_x.min(p.x);
        max_x = max_x.max(p.x);
        min_z = min_z.min(p.z);
        max_z = max_z.max(p.z);
    }
    Bounds {
        min: Coord2::new(min_x, min_z),
        max: Coord2::new(max_x, max_z),
    }
}

fn write_paper(
    out: &mut String,
    style: &CartographyStyle,
    opts: &RenderOptions,
    proj: &Projection,
    cell: Option<&[Coord2]>,
) {
    let p = style.paper();
    if let Some(cell) = cell {
        // Sea-blue backdrop, then the cell as a parchment polygon.
        let _ = write!(
            out,
            "  <rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" fill=\"#88A2A8\"/>\n",
            opts.canvas_width, opts.canvas_height
        );
        let d = polygon_d(proj, cell);
        let _ = write!(
            out,
            "  <path d=\"{}\" fill=\"{}\" stroke=\"#3E595F\" stroke-width=\"3.0\" stroke-linejoin=\"round\"/>\n",
            d, p.base_color
        );
        // Vignette painted over parchment via clipPath (defined in <defs>)
        let _ = write!(
            out,
            "  <rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" fill=\"url(#paper-vignette)\" clip-path=\"url(#zone-cell)\"/>\n",
            opts.canvas_width, opts.canvas_height
        );
    } else {
        // Legacy rectangular parchment for zones without a cell.
        let _ = write!(
            out,
            "  <rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" fill=\"{}\"/>\n",
            opts.canvas_width, opts.canvas_height, p.base_color
        );
        let _ = write!(
            out,
            "  <rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" fill=\"url(#paper-vignette)\"/>\n",
            opts.canvas_width, opts.canvas_height
        );
        let _ = write!(
            out,
            "  <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\"/>\n",
            f(p.edge_width * 0.5),
            f(p.edge_width * 0.5),
            f(opts.canvas_width as f32 - p.edge_width),
            f(opts.canvas_height as f32 - p.edge_width),
            p.edge_color,
            f(p.edge_width)
        );
    }
}

/// Emits a `<defs>` block containing:
///   - the paper vignette gradient
///   - the zone-cell clipPath (when the zone has a Voronoi cell)
///   - one `<pattern>` per biome referenced in this zone's geography
///   - one `<path>` per road, so road labels can use `<textPath>`
fn write_defs(
    out: &mut String,
    geography: Option<&Geography>,
    proj: &Projection,
    style: &CartographyStyle,
    opts: &RenderOptions,
    cell: Option<&[Coord2]>,
) {
    out.push_str("  <defs>\n");

    // Paper vignette: subtle dark fade at the edges.
    let _ = write!(
        out,
        "    <radialGradient id=\"paper-vignette\" cx=\"50%\" cy=\"50%\" r=\"70%\">\n      <stop offset=\"0%\" stop-color=\"#000000\" stop-opacity=\"0\"/>\n      <stop offset=\"100%\" stop-color=\"#3A2C1B\" stop-opacity=\"0.18\"/>\n    </radialGradient>\n"
    );

    // Zone-cell clipPath — used to clip in-zone content (biome
    // regions, scatter, hubs, labels) to the actual cell shape.
    if let Some(cell) = cell {
        let d = polygon_d(proj, cell);
        let _ = write!(
            out,
            "    <clipPath id=\"zone-cell\"><path d=\"{}\"/></clipPath>\n",
            d
        );
    }

    if let Some(geo) = geography {
        // One pattern per unique biome used. Sorted for byte-determinism.
        let mut biomes: Vec<&str> =
            geo.biome_regions.iter().map(|r| r.biome.as_str()).collect();
        biomes.sort();
        biomes.dedup();
        for biome_id in biomes {
            let bs = style.biome(biome_id);
            emit_pattern_for_biome(out, biome_id, &bs.pattern, &bs.base_color, &bs.line_color);
        }

        // Path defs for road textPath labels — projected once here so
        // textPath can `xlink:href` them.
        for r in &geo.roads {
            if r.path.points.is_empty() {
                continue;
            }
            let mut d = String::new();
            let (x0, y0) = proj.project(r.path.points[0]);
            let _ = write!(d, "M {} {}", f(x0), f(y0));
            for p in &r.path.points[1..] {
                let (x, y) = proj.project(*p);
                let _ = write!(d, " L {} {}", f(x), f(y));
            }
            let _ = write!(
                out,
                "    <path id=\"road-{}\" d=\"{}\" fill=\"none\" stroke=\"none\"/>\n",
                xml_id(&r.id),
                d
            );
        }
    }

    // Legend background (small rounded rect) — no actual element here,
    // we just keep the defs block tidy. The legend itself is drawn in
    // body order so it composites above the map.
    let _ = opts; // reserved for future use
    out.push_str("  </defs>\n");
}

fn emit_pattern_for_biome(
    out: &mut String,
    biome_id: &str,
    pattern_kind: &str,
    base_color: &str,
    line_color: &str,
) {
    let pid = xml_id(biome_id);
    match pattern_kind {
        "hatched_diagonal" => {
            let _ = write!(
                out,
                "    <pattern id=\"biome-{}\" x=\"0\" y=\"0\" width=\"8\" height=\"8\" patternUnits=\"userSpaceOnUse\" patternTransform=\"rotate(45)\">\n      <rect width=\"8\" height=\"8\" fill=\"{}\"/>\n      <line x1=\"0\" y1=\"0\" x2=\"0\" y2=\"8\" stroke=\"{}\" stroke-width=\"0.6\" stroke-opacity=\"0.55\"/>\n    </pattern>\n",
                pid, base_color, line_color
            );
        }
        "hatched_cross" => {
            let _ = write!(
                out,
                "    <pattern id=\"biome-{}\" x=\"0\" y=\"0\" width=\"10\" height=\"10\" patternUnits=\"userSpaceOnUse\">\n      <rect width=\"10\" height=\"10\" fill=\"{}\"/>\n      <line x1=\"0\" y1=\"0\" x2=\"10\" y2=\"10\" stroke=\"{}\" stroke-width=\"0.5\" stroke-opacity=\"0.45\"/>\n      <line x1=\"10\" y1=\"0\" x2=\"0\" y2=\"10\" stroke=\"{}\" stroke-width=\"0.5\" stroke-opacity=\"0.45\"/>\n    </pattern>\n",
                pid, base_color, line_color, line_color
            );
        }
        "stippled_pines" => {
            let _ = write!(
                out,
                "    <pattern id=\"biome-{}\" x=\"0\" y=\"0\" width=\"14\" height=\"14\" patternUnits=\"userSpaceOnUse\">\n      <rect width=\"14\" height=\"14\" fill=\"{}\"/>\n      <polygon points=\"7,4 9,9 5,9\" fill=\"{}\" fill-opacity=\"0.7\"/>\n      <polygon points=\"2,11 3.2,13.5 0.8,13.5\" fill=\"{}\" fill-opacity=\"0.55\"/>\n      <polygon points=\"12,11 13.2,13.5 10.8,13.5\" fill=\"{}\" fill-opacity=\"0.55\"/>\n    </pattern>\n",
                pid, base_color, line_color, line_color, line_color
            );
        }
        "stippled_circles" => {
            let _ = write!(
                out,
                "    <pattern id=\"biome-{}\" x=\"0\" y=\"0\" width=\"12\" height=\"12\" patternUnits=\"userSpaceOnUse\">\n      <rect width=\"12\" height=\"12\" fill=\"{}\"/>\n      <circle cx=\"3\" cy=\"3\" r=\"0.9\" fill=\"{}\" fill-opacity=\"0.5\"/>\n      <circle cx=\"9\" cy=\"7\" r=\"0.9\" fill=\"{}\" fill-opacity=\"0.5\"/>\n      <circle cx=\"6\" cy=\"10\" r=\"0.7\" fill=\"{}\" fill-opacity=\"0.5\"/>\n    </pattern>\n",
                pid, base_color, line_color, line_color, line_color
            );
        }
        "wavy_horizontal" => {
            let _ = write!(
                out,
                "    <pattern id=\"biome-{}\" x=\"0\" y=\"0\" width=\"22\" height=\"10\" patternUnits=\"userSpaceOnUse\">\n      <rect width=\"22\" height=\"10\" fill=\"{}\"/>\n      <path d=\"M 0 5 q 5.5 -3 11 0 t 11 0\" stroke=\"{}\" fill=\"none\" stroke-width=\"0.7\" stroke-opacity=\"0.55\"/>\n    </pattern>\n",
                pid, base_color, line_color
            );
        }
        "chevron_peaks" => {
            let _ = write!(
                out,
                "    <pattern id=\"biome-{}\" x=\"0\" y=\"0\" width=\"16\" height=\"12\" patternUnits=\"userSpaceOnUse\">\n      <rect width=\"16\" height=\"12\" fill=\"{}\"/>\n      <path d=\"M 0 8 l 4 -4 l 4 4 m 0 0 l 4 -4 l 4 4\" stroke=\"{}\" fill=\"none\" stroke-width=\"0.7\" stroke-opacity=\"0.6\"/>\n    </pattern>\n",
                pid, base_color, line_color
            );
        }
        // "solid" or anything else: emit a trivial pattern that's just
        // a flat rect, so the regions can still reference url(#biome-...)
        // uniformly without a branch in write_biome_regions.
        _ => {
            let _ = write!(
                out,
                "    <pattern id=\"biome-{}\" x=\"0\" y=\"0\" width=\"4\" height=\"4\" patternUnits=\"userSpaceOnUse\">\n      <rect width=\"4\" height=\"4\" fill=\"{}\"/>\n    </pattern>\n",
                pid, base_color
            );
        }
    }
}

fn xml_id(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

fn write_biome_regions(
    out: &mut String,
    geo: &Geography,
    proj: &Projection,
    style: &CartographyStyle,
) {
    out.push_str("  <g id=\"biome-regions\">\n");
    for r in &geo.biome_regions {
        let bs = style.biome(&r.biome);
        let d = polygon_d(proj, &r.polygon.points);
        let _ = write!(
            out,
            "    <path d=\"{}\" fill=\"url(#biome-{})\" fill-opacity=\"{:.2}\" stroke=\"{}\" stroke-width=\"1.0\" stroke-opacity=\"0.7\"/>\n",
            d, xml_id(&r.biome), r.opacity, bs.line_color
        );
    }
    out.push_str("  </g>\n");
}

fn write_rivers(out: &mut String, geo: &Geography, proj: &Projection) {
    if geo.rivers.is_empty() {
        return;
    }
    out.push_str("  <g id=\"rivers\" fill=\"none\" stroke=\"#5C7A92\" stroke-linecap=\"round\" stroke-linejoin=\"round\">\n");
    for r in &geo.rivers {
        let pts = polyline_points(proj, &r.path.points);
        let w = proj.px(r.width_units).max(2.0);
        let _ = write!(
            out,
            "    <polyline points=\"{}\" stroke-width=\"{}\"/>\n",
            pts,
            f(w)
        );
        for trib in &r.tributaries {
            let tpts = polyline_points(proj, &trib.path.points);
            let tw = proj.px(trib.width_units).max(1.5);
            let _ = write!(
                out,
                "    <polyline points=\"{}\" stroke-width=\"{}\"/>\n",
                tpts,
                f(tw)
            );
        }
    }
    out.push_str("  </g>\n");
}

fn write_roads(out: &mut String, geo: &Geography, proj: &Projection, style: &CartographyStyle) {
    if geo.roads.is_empty() {
        return;
    }
    out.push_str("  <g id=\"roads\" fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\">\n");
    for r in &geo.roads {
        let Some(rs) = style.roads.get(&r.type_) else { continue };
        let pts = polyline_points(proj, &r.path.points);
        let _ = write!(
            out,
            "    <polyline points=\"{}\" stroke=\"{}\" stroke-width=\"{}\" stroke-dasharray=\"{}\"/>\n",
            pts, rs.color, f(rs.width), rs.dash_pattern
        );
    }
    out.push_str("  </g>\n");
}

fn write_features(
    out: &mut String,
    geo: &Geography,
    proj: &Projection,
    glyphs: &GlyphLibrary,
) {
    if geo.features.is_empty() {
        return;
    }
    // Split features by type so ambient lived-in markers (farmhouses,
    // cabins, etc.) render smaller and dimmer than the named-landmark
    // glyphs they dot the backdrop around.
    let mut ambient: Vec<&vaern_data::Feature> = Vec::new();
    let mut named: Vec<&vaern_data::Feature> = Vec::new();
    for fi in &geo.features {
        if fi.type_.starts_with("ambient") {
            ambient.push(fi);
        } else {
            named.push(fi);
        }
    }
    if !ambient.is_empty() {
        out.push_str("  <g id=\"ambient-features\" color=\"#5A4632\" opacity=\"0.85\">\n");
        for fi in &ambient {
            let (cx, cy) = proj.project(fi.position);
            let glyph_inner = inline_glyph(glyphs, &fi.glyph);
            let _ = write!(
                out,
                "    <g transform=\"translate({},{}) scale(1.3)\">{}</g>\n",
                f(cx),
                f(cy),
                glyph_inner
            );
        }
        out.push_str("  </g>\n");
    }
    if !named.is_empty() {
        out.push_str("  <g id=\"features\" color=\"#3A2C1B\">\n");
        for fi in &named {
            let (cx, cy) = proj.project(fi.position);
            let glyph_inner = inline_glyph(glyphs, &fi.glyph);
            let _ = write!(
                out,
                "    <g transform=\"translate({},{}) scale(2.4)\">{}</g>\n",
                f(cx),
                f(cy),
                glyph_inner
            );
        }
        out.push_str("  </g>\n");
    }
}

fn write_zone_connections(
    out: &mut String,
    zone_id: &str,
    connections: &ConnectionsIndex,
    proj: &Projection,
    style: &CartographyStyle,
) {
    let edges = connections.get(zone_id);
    if edges.is_empty() {
        return;
    }
    out.push_str("  <g id=\"connections\">\n");
    for e in edges {
        let (cx, cy) = proj.project(e.border_position);
        let rs = style.roads.get(&e.type_);
        let stroke = rs.map(|s| s.color.as_str()).unwrap_or("#7A674A");
        let _ = write!(
            out,
            "    <circle cx=\"{}\" cy=\"{}\" r=\"6\" fill=\"#F2E3BC\" stroke=\"{}\" stroke-width=\"2\"/>\n",
            f(cx),
            f(cy),
            stroke
        );
        if let Some(t) = style.text("road_label") {
            let _ = write!(
                out,
                "    <text x=\"{}\" y=\"{}\" font-family=\"{}\" font-size=\"{}\" fill=\"{}\" font-style=\"italic\" text-anchor=\"middle\">{}</text>\n",
                f(cx),
                f(cy + 18.0),
                t.font,
                t.size,
                t.fill,
                xml_escape(&e.border_label)
            );
        }
    }
    out.push_str("  </g>\n");
}

fn write_landmarks(
    out: &mut String,
    zone_id: &str,
    landmarks: &LandmarkIndex,
    proj: &Projection,
    glyphs: &GlyphLibrary,
    style: &CartographyStyle,
) {
    out.push_str("  <g id=\"landmarks\" color=\"#3A2C1B\">\n");
    for lm in landmarks.iter_zone(zone_id) {
        let (cx, cy) = proj.project(lm.offset_from_zone_origin);
        let glyph_name = pick_landmark_glyph(&lm.id, &lm.name, style);
        let body = inline_glyph(glyphs, glyph_name);
        let _ = write!(
            out,
            "    <g transform=\"translate({},{}) scale(2.0)\">{}</g>\n",
            f(cx),
            f(cy),
            body
        );
        if let Some(t) = style.text("landmark_label") {
            let _ = write!(
                out,
                "    <text x=\"{}\" y=\"{}\" font-family=\"{}\" font-size=\"{}\" fill=\"{}\" text-anchor=\"middle\">{}</text>\n",
                f(cx),
                f(cy + 22.0),
                t.font,
                t.size,
                t.fill,
                xml_escape(&lm.name)
            );
        }
    }
    out.push_str("  </g>\n");
}

fn write_hubs(
    out: &mut String,
    world: &World,
    zone_id: &str,
    proj: &Projection,
    glyphs: &GlyphLibrary,
    style: &CartographyStyle,
) {
    out.push_str("  <g id=\"hubs\" color=\"#1F1208\">\n");
    let mut hubs: Vec<&Hub> = world.hubs_in_zone(zone_id).collect();
    hubs.sort_by(|a, b| a.id.cmp(&b.id));
    for hub in hubs {
        let Some(off) = hub.offset_from_zone_origin else {
            continue;
        };
        let (cx, cy) = proj.project(off);
        let icon = style.hub_icon(&hub.role);
        let glyph_name = icon.map(|i| i.glyph.as_str()).unwrap_or("small_house");
        let scale = match icon.map(|i| i.size.as_str()).unwrap_or("medium") {
            "large" => 3.4,
            "small" => 2.4,
            _ => 2.8,
        };
        let body = inline_glyph(glyphs, glyph_name);
        let _ = write!(
            out,
            "    <g transform=\"translate({},{}) scale({})\">{}</g>\n",
            f(cx),
            f(cy),
            f(scale),
            body
        );
        let label_key = if hub.role == "capital" {
            "capital_label"
        } else {
            "outpost_label"
        };
        if let Some(t) = style.text(label_key) {
            let _ = write!(
                out,
                "    <text x=\"{}\" y=\"{}\" font-family=\"{}\" font-size=\"{}\" font-weight=\"{}\" fill=\"{}\" text-anchor=\"middle\">{}</text>\n",
                f(cx),
                f(cy + 28.0),
                t.font,
                t.size,
                t.weight,
                t.fill,
                xml_escape(&hub.name)
            );
        }
    }
    out.push_str("  </g>\n");
}

fn write_region_labels(
    out: &mut String,
    geo: &Geography,
    proj: &Projection,
    style: &CartographyStyle,
) {
    let Some(t) = style.text("region_label") else {
        return;
    };
    out.push_str("  <g id=\"region-labels\">\n");
    for r in &geo.biome_regions {
        let Some(pos) = r.label_position else { continue };
        if r.label.is_empty() {
            continue;
        }
        let (x, y) = proj.project(pos);
        let _ = write!(
            out,
            "    <text x=\"{}\" y=\"{}\" font-family=\"{}\" font-size=\"{}\" font-style=\"{}\" fill=\"{}\" text-anchor=\"middle\">{}</text>\n",
            f(x),
            f(y),
            t.font,
            t.size,
            t.style,
            t.fill,
            xml_escape(&r.label)
        );
    }
    out.push_str("  </g>\n");
}

fn write_free_labels(
    out: &mut String,
    geo: &Geography,
    proj: &Projection,
    style: &CartographyStyle,
) {
    if geo.free_labels.is_empty() {
        return;
    }
    let Some(t) = style.text("region_label") else {
        return;
    };
    out.push_str("  <g id=\"free-labels\">\n");
    for fl in &geo.free_labels {
        let (x, y) = proj.project(fl.position);
        let _ = write!(
            out,
            "    <text x=\"{}\" y=\"{}\" font-family=\"{}\" font-size=\"{}\" fill=\"{}\" transform=\"rotate({} {} {})\">{}</text>\n",
            f(x),
            f(y),
            t.font,
            t.size,
            t.fill,
            f(fl.rotation_deg),
            f(x),
            f(y),
            xml_escape(&fl.text)
        );
    }
    out.push_str("  </g>\n");
}

fn write_compass(out: &mut String, opts: &RenderOptions) {
    let cx = opts.canvas_width as f32 - 80.0;
    let cy = 80.0;
    let _ = write!(
        out,
        "  <g id=\"compass\" transform=\"translate({},{})\" stroke=\"#3A2C1B\" fill=\"#3A2C1B\">\n    <circle r=\"30\" fill=\"#F2E3BC\" stroke-width=\"1.5\"/>\n    <polygon points=\"0,-25 5,0 0,25 -5,0\" fill=\"#3A2C1B\"/>\n    <text x=\"0\" y=\"-32\" font-family=\"Georgia, serif\" font-size=\"11\" text-anchor=\"middle\" stroke=\"none\">N</text>\n  </g>\n",
        f(cx),
        f(cy)
    );
}

fn write_scale_bar(out: &mut String, proj: &Projection, opts: &RenderOptions) {
    // Two-segment alternating bar: 0 → 500 m (filled), 500 m → 1 km (open).
    // Mirrors a classic cartographic ruler.
    let half_units = 500.0_f32;
    let full_units = 1000.0_f32;
    let half_px = proj.px(half_units);
    if half_px < 20.0 {
        return;
    }
    let full_px = proj.px(full_units);
    let x = 60.0;
    let y = opts.canvas_height as f32 - 60.0;
    let bar_h = 8.0;

    let _ = write!(
        out,
        "  <g id=\"scale-bar\" stroke=\"#3A2C1B\" stroke-width=\"1\" fill=\"none\" font-family=\"Georgia, serif\" font-size=\"10\">\n"
    );
    // Filled left segment (0–500 m)
    let _ = write!(
        out,
        "    <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#3A2C1B\" stroke=\"#3A2C1B\"/>\n",
        f(x),
        f(y),
        f(half_px),
        f(bar_h)
    );
    // Open right segment (500 m – 1 km)
    let _ = write!(
        out,
        "    <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#F2E3BC\" stroke=\"#3A2C1B\"/>\n",
        f(x + half_px),
        f(y),
        f(full_px - half_px),
        f(bar_h)
    );
    // Tick labels
    for (i, label) in [(0.0_f32, "0"), (half_units, "500 m"), (full_units, "1 km")] {
        let xt = x + proj.px(i);
        let _ = write!(
            out,
            "    <line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#3A2C1B\"/>\n    <text x=\"{}\" y=\"{}\" stroke=\"none\" fill=\"#3A2C1B\" text-anchor=\"middle\">{}</text>\n",
            f(xt),
            f(y - 3.0),
            f(xt),
            f(y + bar_h + 3.0),
            f(xt),
            f(y + bar_h + 14.0),
            label
        );
    }
    // Caption
    let _ = write!(
        out,
        "    <text x=\"{}\" y=\"{}\" stroke=\"none\" fill=\"#3A2C1B\" font-style=\"italic\">scale (metric)</text>\n",
        f(x),
        f(y - 8.0)
    );
    out.push_str("  </g>\n");
}

/// Bottom-left annotation card showing total zone extent, diagonal,
/// and an estimated traversal time at a reference run speed.
fn write_metrics_card(out: &mut String, bounds: Bounds, opts: &RenderOptions) {
    const RUN_SPEED_M_PER_S: f32 = 6.0;
    let w_m = bounds.width();
    let h_m = bounds.height();
    let diag_m = (w_m * w_m + h_m * h_m).sqrt();
    let traversal_min = diag_m / RUN_SPEED_M_PER_S / 60.0;

    let card_w = 260.0;
    let card_h = 80.0;
    let x = 30.0;
    let y = opts.canvas_height as f32 - card_h - 100.0;

    let extent = format!("{:.2} × {:.2} km", w_m / 1000.0, h_m / 1000.0);
    let diag = format!("{:.2} km diagonal", diag_m / 1000.0);
    let run = format!(
        "≈ {:.1} min corner→corner @ {:.0} m/s run",
        traversal_min, RUN_SPEED_M_PER_S
    );

    let _ = write!(
        out,
        "  <g id=\"metrics-card\">\n    <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"4\" ry=\"4\" fill=\"#F2E3BC\" fill-opacity=\"0.92\" stroke=\"#8E6C2E\" stroke-width=\"1\"/>\n    <text x=\"{}\" y=\"{}\" font-family=\"Georgia, serif\" font-size=\"12\" font-weight=\"bold\" fill=\"#3A2C1B\">Zone Metrics</text>\n    <text x=\"{}\" y=\"{}\" font-family=\"Georgia, serif\" font-size=\"11\" fill=\"#3A2C1B\">{}</text>\n    <text x=\"{}\" y=\"{}\" font-family=\"Georgia, serif\" font-size=\"11\" fill=\"#3A2C1B\">{}</text>\n    <text x=\"{}\" y=\"{}\" font-family=\"Georgia, serif\" font-size=\"11\" fill=\"#3A2C1B\">{}</text>\n  </g>\n",
        f(x), f(y), f(card_w), f(card_h),
        f(x + 12.0), f(y + 18.0),
        f(x + 12.0), f(y + 38.0), xml_escape(&extent),
        f(x + 12.0), f(y + 54.0), xml_escape(&diag),
        f(x + 12.0), f(y + 70.0), xml_escape(&run)
    );
}

fn write_title(out: &mut String, name: &str, opts: &RenderOptions) {
    let cx = opts.canvas_width as f32 * 0.5;
    let _ = write!(
        out,
        "  <text x=\"{}\" y=\"{}\" font-family=\"Georgia, serif\" font-size=\"22\" font-weight=\"bold\" fill=\"#3A2C1B\" text-anchor=\"middle\">{}</text>\n",
        f(cx),
        f(36.0),
        xml_escape(name)
    );
}

fn pick_landmark_glyph<'a>(id: &str, name: &str, style: &'a CartographyStyle) -> &'a str {
    let key = id.to_ascii_lowercase();
    let nkey = name.to_ascii_lowercase();
    for (descriptor, glyph) in &style.landmark_glyphs {
        if key.contains(descriptor) || nkey.contains(descriptor) {
            return glyph.as_str();
        }
    }
    style
        .landmark_glyphs
        .get("default")
        .map(String::as_str)
        .unwrap_or("small_house")
}

fn inline_glyph(glyphs: &GlyphLibrary, name: &str) -> String {
    let Some(raw) = glyphs.get(name) else {
        // fallback: a plain dot so missing glyphs don't crash
        return "<circle r=\"3\" fill=\"currentColor\"/>".to_string();
    };
    // Strip the outer <svg> wrapper, keep only the inner contents.
    extract_svg_inner(raw)
}

fn extract_svg_inner(svg: &str) -> String {
    if let Some(start) = svg.find("<svg") {
        if let Some(open_end) = svg[start..].find('>') {
            let inner_start = start + open_end + 1;
            if let Some(close) = svg.rfind("</svg>") {
                if close > inner_start {
                    return svg[inner_start..close].trim().to_string();
                }
            }
        }
    }
    svg.to_string()
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

// ─── scatter (deterministic) ─────────────────────────────────────────

/// Tiny LCG for byte-deterministic scatter sampling. Same seed → same
/// output, identical across machines (no float-derived state).
struct LcgRng(u64);

impl LcgRng {
    fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }
    fn next_u32(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 32) as u32
    }
    fn next_f32(&mut self) -> f32 {
        // Map u32 → [0, 1). Stable rounding.
        (self.next_u32() as f32) * (1.0 / 4_294_967_296.0)
    }
}

fn density_p(s: &str) -> f32 {
    match s {
        "low" => 0.10,
        "medium" => 0.25,
        "high" => 0.50,
        _ => 0.20,
    }
}

use vaern_data::point_in_polygon;

fn write_scatter(out: &mut String, geo: &Geography, bounds: Bounds, proj: &Projection) {
    let layers = [
        (geo.scatter.trees.as_ref(), "trees", 60.0_f32, "#3E5C2E"),
        (geo.scatter.hills.as_ref(), "hills", 90.0_f32, "#7A6F44"),
        (geo.scatter.rocks.as_ref(), "rocks", 100.0_f32, "#5E574E"),
    ];
    let mut any = false;
    for (layer_opt, _, _, _) in &layers {
        if layer_opt.is_some() {
            any = true;
        }
    }
    if !any {
        return;
    }
    out.push_str("  <g id=\"scatter\">\n");
    for (layer_opt, kind, step_units, color) in layers {
        let Some(layer) = layer_opt else { continue };
        let seed = layer.seed.unwrap_or_else(|| seed_from(kind));
        let p = density_p(&layer.density);
        let mut rng = LcgRng::new(seed);

        // Walk a grid over the bounds, jitter sample points, test
        // membership against allowed regions.
        let mut z = bounds.min.z + step_units * 0.5;
        while z < bounds.max.z {
            let mut x = bounds.min.x + step_units * 0.5;
            while x < bounds.max.x {
                let jx = rng.next_f32() - 0.5;
                let jz = rng.next_f32() - 0.5;
                let probe = Coord2::new(x + jx * step_units * 0.6, z + jz * step_units * 0.6);
                let take = rng.next_f32() < p;
                if take {
                    if let Some(_region) = first_matching_region(geo, &layer.biomes_allowed, probe) {
                        emit_scatter_glyph(out, kind, color, proj, probe);
                    }
                }
                x += step_units;
            }
            z += step_units;
        }

        // Explicit positions also rendered.
        for pt in &layer.explicit_positions {
            emit_scatter_glyph(out, kind, color, proj, *pt);
        }
    }
    out.push_str("  </g>\n");
}

fn first_matching_region<'a>(
    geo: &'a Geography,
    allowed: &[String],
    p: Coord2,
) -> Option<&'a vaern_data::BiomeRegion> {
    for r in &geo.biome_regions {
        if !allowed.iter().any(|b| b == &r.biome) {
            continue;
        }
        if point_in_polygon(p, &r.polygon.points) {
            return Some(r);
        }
    }
    None
}

fn emit_scatter_glyph(out: &mut String, kind: &str, color: &str, proj: &Projection, p: Coord2) {
    let (cx, cy) = proj.project(p);
    match kind {
        "trees" => {
            let _ = write!(
                out,
                "    <polygon points=\"{},{} {},{} {},{}\" fill=\"{}\" fill-opacity=\"0.85\"/>\n",
                f(cx),
                f(cy - 3.5),
                f(cx + 2.0),
                f(cy + 2.0),
                f(cx - 2.0),
                f(cy + 2.0),
                color
            );
        }
        "hills" => {
            let _ = write!(
                out,
                "    <path d=\"M {} {} q 2.5 -2.5 5 0\" stroke=\"{}\" stroke-width=\"0.9\" fill=\"none\" stroke-opacity=\"0.75\"/>\n",
                f(cx - 2.5),
                f(cy + 1.0),
                color
            );
        }
        _ => {
            let _ = write!(
                out,
                "    <circle cx=\"{}\" cy=\"{}\" r=\"1.2\" fill=\"{}\" fill-opacity=\"0.7\"/>\n",
                f(cx),
                f(cy),
                color
            );
        }
    }
}

fn seed_from(kind: &str) -> u64 {
    // FNV-1a 64-bit; deterministic per kind name.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in kind.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

// ─── road labels (textPath) ──────────────────────────────────────────

fn write_road_labels(out: &mut String, geo: &Geography, style: &CartographyStyle) {
    let Some(t) = style.text("road_label") else { return };
    if geo.roads.is_empty() {
        return;
    }
    out.push_str("  <g id=\"road-labels\">\n");
    for r in &geo.roads {
        // Use the road id as label fallback; geography schema doesn't
        // currently carry a road `name` field.
        let label = pretty_road_label(&r.id);
        let _ = write!(
            out,
            "    <text font-family=\"{}\" font-size=\"{}\" font-style=\"italic\" fill=\"{}\" letter-spacing=\"1\">\n      <textPath xlink:href=\"#road-{}\" startOffset=\"30%\">{}</textPath>\n    </text>\n",
            t.font,
            t.size,
            t.fill,
            xml_id(&r.id),
            xml_escape(&label)
        );
    }
    out.push_str("  </g>\n");
}

fn pretty_road_label(id: &str) -> String {
    // "kingsroad" → "the Kingsroad", "drifters_track" → "Drifters' Track"
    let words: Vec<String> = id
        .split('_')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect();
    words.join(" ")
}

// ─── legend ──────────────────────────────────────────────────────────

fn write_legend(
    out: &mut String,
    geography: Option<&Geography>,
    style: &CartographyStyle,
    opts: &RenderOptions,
) {
    let Some(geo) = geography else { return };
    if geo.biome_regions.is_empty() {
        return;
    }
    let mut biomes: Vec<&str> = geo.biome_regions.iter().map(|r| r.biome.as_str()).collect();
    biomes.sort();
    biomes.dedup();

    let row_h = 18.0;
    let pad = 10.0;
    let title_h = 22.0;
    let swatch_w = 22.0;
    let box_w = 200.0;
    let box_h = title_h + pad + row_h * biomes.len() as f32 + pad;
    let x0 = opts.canvas_width as f32 - box_w - 30.0;
    let y0 = opts.canvas_height as f32 - box_h - 30.0;

    let _ = write!(
        out,
        "  <g id=\"legend\">\n    <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"4\" ry=\"4\" fill=\"#F2E3BC\" fill-opacity=\"0.9\" stroke=\"#8E6C2E\" stroke-width=\"1\"/>\n    <text x=\"{}\" y=\"{}\" font-family=\"Georgia, serif\" font-size=\"13\" font-weight=\"bold\" fill=\"#3A2C1B\">Legend</text>\n",
        f(x0),
        f(y0),
        f(box_w),
        f(box_h),
        f(x0 + pad),
        f(y0 + 16.0)
    );
    for (i, biome) in biomes.iter().enumerate() {
        let yr = y0 + title_h + pad + i as f32 * row_h;
        let _ = write!(
            out,
            "    <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"url(#biome-{})\" stroke=\"{}\" stroke-width=\"0.6\"/>\n    <text x=\"{}\" y=\"{}\" font-family=\"Georgia, serif\" font-size=\"11\" fill=\"#3A2C1B\">{}</text>\n",
            f(x0 + pad),
            f(yr),
            f(swatch_w),
            f(row_h - 4.0),
            xml_id(biome),
            style.biome(biome).line_color,
            f(x0 + pad + swatch_w + 8.0),
            f(yr + 11.0),
            xml_escape(&pretty_biome_label(biome))
        );
    }
    out.push_str("  </g>\n");
}

fn pretty_biome_label(id: &str) -> String {
    let mut s = id.replace('_', " ");
    if let Some(c) = s.chars().next() {
        s.replace_range(0..c.len_utf8(), &c.to_uppercase().collect::<String>());
    }
    s
}
