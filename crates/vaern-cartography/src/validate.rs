//! Cross-file validation. Runs after `vaern-data::load_world()` /
//! `load_all_landmarks` / `load_all_geography` / `load_all_connections`
//! and produces a structured report with severities. The renderer and
//! (future) 3D terrain generator both gate on `report.is_clean()`.

use std::path::PathBuf;

use vaern_data::{
    point_in_polygon, Bounds, ConnectionsIndex, Coord2, GeographyIndex, LandmarkIndex, World,
    WorldLayout,
};

use crate::style::CartographyStyle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone)]
pub struct ValidationIssue {
    pub severity: Severity,
    pub kind: &'static str,
    pub path: Option<PathBuf>,
    pub message: String,
}

#[derive(Debug, Default, Clone)]
pub struct ValidationReport {
    pub issues: Vec<ValidationIssue>,
}

impl ValidationReport {
    pub fn errors(&self) -> impl Iterator<Item = &ValidationIssue> {
        self.issues.iter().filter(|i| i.severity == Severity::Error)
    }

    pub fn warnings(&self) -> impl Iterator<Item = &ValidationIssue> {
        self.issues.iter().filter(|i| i.severity == Severity::Warning)
    }

    pub fn is_clean(&self) -> bool {
        self.errors().next().is_none()
    }

    fn push_error(&mut self, kind: &'static str, message: impl Into<String>) {
        self.issues.push(ValidationIssue {
            severity: Severity::Error,
            kind,
            path: None,
            message: message.into(),
        });
    }

    fn push_warning(&mut self, kind: &'static str, message: impl Into<String>) {
        self.issues.push(ValidationIssue {
            severity: Severity::Warning,
            kind,
            path: None,
            message: message.into(),
        });
    }
}

/// Bundles all the data layers the validator needs. The CLI binary
/// loads them via `vaern-data::load_*` functions and passes them in.
pub struct WorldBundle<'a> {
    pub world: &'a World,
    pub landmarks: &'a LandmarkIndex,
    pub geography: &'a GeographyIndex,
    pub connections: &'a ConnectionsIndex,
    pub layout: &'a WorldLayout,
    pub style: &'a CartographyStyle,
    pub glyph_names: &'a [String],
}

/// Run every cross-file invariant. Returns a report; caller decides
/// whether warnings count as failure.
pub fn validate(b: &WorldBundle<'_>) -> ValidationReport {
    let mut r = ValidationReport::default();

    // ─── per-zone: bounds, coord system, hub offsets ────────────────
    for zone in &b.world.zones {
        let Some(bounds) = zone.bounds else {
            r.push_error(
                "zone_missing_bounds",
                format!("zone {:?} has no `bounds` declared", zone.id),
            );
            continue;
        };

        if bounds.width() <= 0.0 || bounds.height() <= 0.0 {
            r.push_error(
                "zone_bounds_degenerate",
                format!(
                    "zone {:?} bounds have zero/negative extent: w={} h={}",
                    zone.id,
                    bounds.width(),
                    bounds.height()
                ),
            );
        }

        let Some(cs) = &zone.coordinate_system else {
            r.push_error(
                "zone_missing_coordinate_system",
                format!("zone {:?} has no `coordinate_system` declared", zone.id),
            );
            continue;
        };

        if !b.world.hubs_in_zone(&zone.id).any(|h| h.id == cs.origin) {
            r.push_error(
                "coord_origin_unknown_hub",
                format!(
                    "zone {:?} coordinate_system.origin = {:?} is not a hub in this zone",
                    zone.id, cs.origin
                ),
            );
        }

        // Hub offset checks
        for hub in b.world.hubs_in_zone(&zone.id) {
            let Some(off) = hub.offset_from_zone_origin else {
                r.push_error(
                    "hub_missing_offset",
                    format!("hub {:?} (zone {:?}) has no offset_from_zone_origin", hub.id, zone.id),
                );
                continue;
            };
            if !bounds.contains(off) {
                r.push_error(
                    "hub_offset_out_of_bounds",
                    format!(
                        "hub {:?} (zone {:?}) at ({}, {}) is outside zone bounds [{:?} → {:?}]",
                        hub.id, zone.id, off.x, off.z, bounds.min, bounds.max
                    ),
                );
            }

            // Hub role must resolve in the style sheet
            if b.style.hub_icon(&hub.role).is_none() {
                r.push_warning(
                    "hub_role_no_style",
                    format!(
                        "hub {:?} role {:?} has no entry in cartography_style.yaml::hub_icons",
                        hub.id, hub.role
                    ),
                );
            }
        }

        // When the zone has a Voronoi cell, the cell is the
        // authoritative spatial extent. The rectangular `bounds`
        // field is an authored playspace reference and may be
        // smaller — skip bounds-based checks for content positions
        // when a cell is present.
        let has_cell = b
            .layout
            .zone_placements
            .iter()
            .any(|p| p.zone == zone.id && p.zone_cell.is_some());

        // Landmark coords must lie inside bounds (or cell if present).
        if !has_cell {
            for lm in b.landmarks.iter_zone(&zone.id) {
                if !bounds.contains(lm.offset_from_zone_origin) {
                    r.push_warning(
                        "landmark_out_of_bounds",
                        format!(
                            "landmark {:?} (zone {:?}) at ({}, {}) is outside zone bounds",
                            lm.id, zone.id, lm.offset_from_zone_origin.x, lm.offset_from_zone_origin.z
                        ),
                    );
                }
            }
        }

        // Geography coverage (only zones with geography.yaml)
        if let Some(geo) = b.geography.get(&zone.id) {
            for region in &geo.biome_regions {
                if !b.style.biomes.contains_key(&region.biome) {
                    r.push_warning(
                        "biome_unknown_in_style",
                        format!(
                            "zone {:?} region {:?} biome {:?} not defined in cartography_style.yaml",
                            zone.id, region.id, region.biome
                        ),
                    );
                }
                if has_cell {
                    continue;  // cell is the constraint, not bounds
                }
                for p in &region.polygon.points {
                    if !bounds.contains(*p) {
                        r.push_warning(
                            "polygon_vertex_out_of_bounds",
                            format!(
                                "zone {:?} region {:?} vertex ({}, {}) outside bounds",
                                zone.id, region.id, p.x, p.z
                            ),
                        );
                        break;
                    }
                }
            }
            for river in &geo.rivers {
                if !has_cell {
                    check_path_inside(&river.path.points, bounds, &zone.id, &river.id, "river", &mut r);
                }
            }
            for road in &geo.roads {
                if !has_cell {
                    check_path_inside(&road.path.points, bounds, &zone.id, &road.id, "road", &mut r);
                }
                if !b.style.roads.contains_key(&road.type_) {
                    r.push_warning(
                        "road_type_unknown",
                        format!(
                            "zone {:?} road {:?} type {:?} not in cartography_style.yaml::roads",
                            zone.id, road.id, road.type_
                        ),
                    );
                }
            }
            for f in &geo.features {
                let glyph_known = b.glyph_names.iter().any(|n| n == &f.glyph);
                if !glyph_known {
                    r.push_warning(
                        "glyph_missing",
                        format!(
                            "zone {:?} feature {:?} glyph {:?} not present in style/glyphs/",
                            zone.id, f.id, f.glyph
                        ),
                    );
                }
                if !has_cell && !bounds.contains(f.position) {
                    r.push_warning(
                        "feature_out_of_bounds",
                        format!(
                            "zone {:?} feature {:?} at ({}, {}) outside bounds",
                            zone.id, f.id, f.position.x, f.position.z
                        ),
                    );
                }
            }
        }
    }

    // ─── connections ────────────────────────────────────────────────
    let zone_ids: std::collections::HashSet<&str> =
        b.world.zones.iter().map(|z| z.id.as_str()).collect();

    for (zone_id, conn) in b.connections.all_edges() {
        if !zone_ids.contains(conn.to_zone.as_str()) {
            r.push_error(
                "connection_unknown_zone",
                format!(
                    "zone {:?} declares connection to unknown zone {:?}",
                    zone_id, conn.to_zone
                ),
            );
        }
        if !b.connections.get(&conn.to_zone).iter().any(|back| back.to_zone == zone_id) {
            r.push_warning(
                "connection_unidirectional",
                format!(
                    "zone {:?} → {:?} has no return edge {:?} → {:?}",
                    zone_id, conn.to_zone, conn.to_zone, zone_id
                ),
            );
        }
        if !b.style.roads.contains_key(&conn.type_) {
            r.push_warning(
                "connection_type_unknown",
                format!(
                    "zone {:?} connection to {:?} type {:?} not in cartography_style.yaml::roads",
                    zone_id, conn.to_zone, conn.type_
                ),
            );
        }
    }

    // ─── world layout ───────────────────────────────────────────────
    for placement in &b.layout.zone_placements {
        if !zone_ids.contains(placement.zone.as_str()) {
            r.push_error(
                "layout_unknown_zone",
                format!(
                    "world.yaml places unknown zone {:?}",
                    placement.zone
                ),
            );
        }
    }
    for zone in &b.world.zones {
        if b.layout.zone_origin(&zone.id).is_none() {
            r.push_warning(
                "zone_not_placed_in_world",
                format!(
                    "zone {:?} has no entry in world.yaml zone_placements",
                    zone.id
                ),
            );
        }
    }

    // ─── Graph rules (§4.6 of zone-map brief) ───────────────────────
    // Build adjacency: zone -> set of neighbor zones via connections.
    let mut adj: std::collections::HashMap<&str, std::collections::HashSet<&str>> =
        std::collections::HashMap::new();
    for (from_zone, edge) in b.connections.all_edges() {
        adj.entry(from_zone).or_default().insert(edge.to_zone.as_str());
    }
    let zone_by_id: std::collections::HashMap<&str, &vaern_data::Zone> =
        b.world.zones.iter().map(|z| (z.id.as_str(), z)).collect();

    // Rule 1: every non-endgame zone has ≥ 2 outgoing edges.
    for zone in &b.world.zones {
        if is_endgame_tier(&zone.tier) {
            continue;
        }
        let n_out = adj.get(zone.id.as_str()).map(|s| s.len()).unwrap_or(0);
        if n_out < 2 {
            r.push_warning(
                "zone_too_few_exits",
                format!(
                    "zone {:?} (tier {}) has {} outgoing connection(s); funnels are bad UX",
                    zone.id, zone.tier, n_out
                ),
            );
        }
    }

    // Rule 2: every endgame zone reachable from at least one starter.
    let starters: Vec<&str> = b
        .world
        .zones
        .iter()
        .filter(|z| z.starter_race.is_some())
        .map(|z| z.id.as_str())
        .collect();
    let mut reachable_from_any: std::collections::HashSet<&str> =
        std::collections::HashSet::new();
    for start in &starters {
        let mut stack = vec![*start];
        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        while let Some(node) = stack.pop() {
            if !seen.insert(node) {
                continue;
            }
            reachable_from_any.insert(node);
            if let Some(neighbors) = adj.get(node) {
                for n in neighbors {
                    stack.push(*n);
                }
            }
        }
    }
    for zone in &b.world.zones {
        if !is_endgame_tier(&zone.tier) {
            continue;
        }
        if !reachable_from_any.contains(zone.id.as_str()) {
            r.push_warning(
                "endgame_unreachable",
                format!(
                    "endgame zone {:?} is not reachable from any starter via connections",
                    zone.id
                ),
            );
        }
    }

    // Rule 3: connected `level_continuous: true` edges must not jump
    // more than 5 levels (asymmetric — only flag the gap direction).
    const LEVEL_GAP: u32 = 5;
    for (from_zone, edge) in b.connections.all_edges() {
        if !edge.level_continuous {
            continue;
        }
        let Some(a) = zone_by_id.get(from_zone) else { continue };
        let Some(c) = zone_by_id.get(edge.to_zone.as_str()) else { continue };
        if a.level_range.max + LEVEL_GAP < c.level_range.min {
            r.push_warning(
                "level_band_gap",
                format!(
                    "level_continuous edge {:?}→{:?}: {} (max L{}) → {} (min L{}) jumps {} levels",
                    from_zone, edge.to_zone,
                    from_zone, a.level_range.max,
                    edge.to_zone, c.level_range.min,
                    c.level_range.min as i32 - a.level_range.max as i32
                ),
            );
        }
    }

    // ─── Voronoi-cell sanity (warnings) ─────────────────────────────
    // 1. Each anchor must lie inside its own cell.
    // 2. Each cell must lie inside the coastline (sample a few cell
    //    vertices for speed; any vertex outside is a fail).
    for placement in &b.layout.zone_placements {
        let Some(cell) = &placement.zone_cell else { continue };
        if !point_in_polygon(placement.world_origin, &cell.points) {
            r.push_warning(
                "world_origin_outside_cell",
                format!(
                    "zone {:?} world_origin ({}, {}) is not inside its zone_cell",
                    placement.zone,
                    placement.world_origin.x,
                    placement.world_origin.z
                ),
            );
        }
        if let Some(coast) = &b.layout.coastline {
            // Centroid test — robust to boundary-coincident vertices
            // produced by polygon clipping. If the cell's centroid is
            // inside the coastline, the cell is broadly on land.
            let n = cell.points.len() as f32;
            if n > 0.0 {
                let cx = cell.points.iter().map(|p| p.x).sum::<f32>() / n;
                let cz = cell.points.iter().map(|p| p.z).sum::<f32>() / n;
                let centroid = Coord2::new(cx, cz);
                if !point_in_polygon(centroid, &coast.points) {
                    r.push_warning(
                        "cell_outside_coastline",
                        format!(
                            "zone {:?} cell centroid ({}, {}) is outside the coastline",
                            placement.zone, cx, cz
                        ),
                    );
                }
            }
        }
    }

    r
}

fn is_endgame_tier(tier: &str) -> bool {
    matches!(tier, "endgame")
}

fn check_path_inside(
    pts: &[Coord2],
    bounds: Bounds,
    zone_id: &str,
    item_id: &str,
    kind: &str,
    r: &mut ValidationReport,
) {
    for p in pts {
        if !bounds.contains(*p) {
            r.push_warning(
                "path_vertex_out_of_bounds",
                format!(
                    "zone {:?} {} {:?} vertex ({}, {}) outside bounds",
                    zone_id, kind, item_id, p.x, p.z
                ),
            );
            break;
        }
    }
    for w in pts.windows(2) {
        let a = w[0];
        let b = w[1];
        let dx = b.x - a.x;
        let dz = b.z - a.z;
        if dx.abs() < f32::EPSILON && dz.abs() < f32::EPSILON {
            r.push_warning(
                "path_zero_length_segment",
                format!("zone {:?} {} {:?} has a zero-length segment", zone_id, kind, item_id),
            );
            break;
        }
    }
}
