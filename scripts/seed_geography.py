#!/usr/bin/env -S uv run --quiet --with pyyaml --with scipy --with shapely --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyyaml", "scipy", "shapely"]
# ///
"""Generate / refresh `geography.yaml` for every zone.

MMO-style biome rendering:

  - One main biome backdrop covering the whole zone-cell, painted with
    the zone's primary biome (river_valley → fields, etc.). Most of
    the map reads as this single biome, like a real MMO zone.
  - One small pocket (~400-600 m radius circle, clipped to the cell)
    around each landmark whose name-derived biome DIFFERS from the
    primary. e.g. dalewatch (fields default) gets marsh pockets at
    Reed-Brake / Blackwash Fens, a forest pocket at Thornroot Grove,
    a ruin pocket at Sidlow Cairn, etc.
  - Landmarks whose derived biome matches the default are silent —
    no pocket region — they only appear as glyph markers in features.

Hand-authored extras on existing geography.yaml files (rivers, roads,
scatter rules, free_labels) are preserved verbatim — only
`biome_regions` and `features` are regenerated.

Run from repo root:

    ./scripts/seed_geography.py
"""

from __future__ import annotations

import math
import pathlib
import sys

import numpy as np
import shapely.geometry as sg
import yaml
from scipy.spatial import Voronoi

ROOT = pathlib.Path(__file__).resolve().parent.parent
ZONES_ROOT = ROOT / "src" / "generated" / "world" / "zones"
WORLD_YAML = ROOT / "src" / "generated" / "world" / "world.yaml"

# Map zone-level biome name (from core.yaml) to the cartography style
# biome key. The style sheet defines fields/forest/marsh/ridge_scrub/
# grass/etc.
BIOME_KEY = {
    "river_valley":     "fields",
    "temperate_forest": "forest",
    "highland":         "highland",
    "mountain":         "mountain",
    "marshland":        "marsh",
    "ashland":          "ashland",
    "ruin":             "ruin",
    "coastal_cliff":    "coastal_cliff",
    "fjord":            "fjord",
}

# Landmark-name keyword → biome. First match wins. Anything unmatched
# falls back to the zone's primary biome.
LANDMARK_BIOME_RULES: list[tuple[list[str], str]] = [
    # Only LANDSCAPE-defining names produce a biome pocket. Cairns,
    # towers, milestones, watchposts, crofts, etc. are stone markers
    # or buildings sitting ON the backdrop — they show up as glyph
    # features but don't change the local biome. This keeps the map
    # reading as "fields + named terrain pockets" rather than
    # "ruins everywhere."
    #
    # Order matters — first match wins.
    (["lair", "cave"],                                     "ashland"),
    (["ashen", "ashland", "cinder"],                       "ashland"),
    (["fen", "marsh", "brake", "bog", "swamp", "wash"], "marsh"),
    (["wood", "grove", "glen", "forest", "thicket", "leaf"], "forest"),
    (["mine", "vault", "drift", "deep", "scrap"],         "ridge_scrub"),
    (["ridge", "scarp", "crag", "rocks"],                  "ridge_scrub"),
    (["fjord", "cliff", "shore", "coast"],                "coastal_cliff"),
    (["mountain", "peak", "spine", "frost"],              "mountain"),
    (["pasture", "downs", "moor"],                         "grass"),
    (["highland", "reach", "fells"],                       "highland"),
    # Note: cairn / barrow / tower / watchtower / milestone /
    # waystone / standing / croft / mill / field — DO NOT produce
    # biome pockets. These are markers/buildings, not terrain.
]

DEFAULT_LANDMARK_GLYPH = "small_house"

# Ambient feature glyph per zone biome. Used by the
# generate_ambient_features pass to scatter small "lived-in" markers
# (farmhouses, cabins, etc.) across the zone's dominant biome.
AMBIENT_GLYPH_BY_BIOME = {
    "river_valley":      "small_house",
    "fields":            "small_house",
    "grass":             "small_house",
    "temperate_forest":  "small_cabin",
    "forest":            "small_cabin",
    "highland":          "small_house",
    "mountain":          "small_cabin",
    "marshland":         "marsh_tuft",
    "marsh":             "marsh_tuft",
    "ashland":           "cave_mouth",
    "ruin":              "ruined_tower",
    "coastal_cliff":     "small_house",
    "fjord":             "small_house",
    "ridge_scrub":       "stone_slab",
}

AMBIENT_DENSITY_BY_TIER = {
    "starter":   1.4,  # per km² — pastoral, lived-in
    "mid":       1.0,
    "contested": 0.6,  # less settled
    "endgame":   0.4,  # frontier wilderness
}

SCATTER_BY_BIOME = {
    "forest":           {"trees": {"density": "medium", "biomes_allowed": ["forest"], "seed": 5101}},
    "temperate_forest": {"trees": {"density": "medium", "biomes_allowed": ["forest"], "seed": 5101}},
    "marshland":        {},
    "highland":         {"hills": {"density": "low", "biomes_allowed": ["highland"], "seed": 5102}},
    "mountain":         {"hills": {"density": "medium", "biomes_allowed": ["mountain"], "seed": 5103}},
    "ashland":          {},
    "ruin":             {},
    "coastal_cliff":    {},
    "fjord":            {},
    "river_valley":     {},
}


# ─── helpers ────────────────────────────────────────────────────────

def load_cells_zone_local() -> dict[str, list[tuple[float, float]]]:
    """Returns {zone_id: [(x_local, z_local), ...]} from world.yaml,
    rebased relative to the zone's world_origin."""
    if not WORLD_YAML.exists():
        return {}
    layout = yaml.safe_load(WORLD_YAML.read_text())
    out: dict[str, list[tuple[float, float]]] = {}
    for p in layout.get("zone_placements", []):
        cell = p.get("zone_cell")
        if not cell:
            continue
        ox = p["world_origin"]["x"]
        oz = p["world_origin"]["z"]
        out[p["zone"]] = [(v["x"] - ox, v["z"] - oz) for v in cell["points"]]
    return out


def biome_for_landmark(landmark_id: str, landmark_name: str) -> str | None:
    s = (landmark_id + " " + landmark_name).lower()
    for keywords, biome in LANDMARK_BIOME_RULES:
        if any(k in s for k in keywords):
            return biome
    return None


def landmark_glyph_for(landmark_id: str, landmark_name: str) -> str:
    s = (landmark_id + " " + landmark_name).lower()
    for descriptor, glyph in [
        ("lair",       "cave_mouth"),    # check before "cave"
        ("cave",       "cave_mouth"),
        ("croft",      "small_house"),
        ("cottage",    "small_house"),
        ("cache",      "small_house"),
        ("camp",       "small_house"),
        ("mill",       "small_house"),
        ("grove",      "tree_ring"),
        ("glen",       "tree_ring"),
        ("wood",       "tree_ring"),
        ("thicket",    "tree_ring"),
        ("cairn",      "stone_slab"),
        ("barrow",     "stone_slab"),
        ("milestone",  "stone_slab"),
        ("waystone",   "stone_slab"),
        ("standing",   "stone_slab"),
        ("mine",       "mine_entrance"),
        ("vault",      "mine_entrance"),
        ("watchtower", "ruined_tower"),
        ("tower",      "ruined_tower"),
        ("watchpost",  "ruined_tower"),
        ("lookout",    "ruined_tower"),
        ("ruin",       "ruined_tower"),
        ("ford",       "bridge"),
        ("crossing",   "bridge"),
        ("fen",        "marsh_tuft"),
        ("marsh",      "marsh_tuft"),
        ("brake",      "marsh_tuft"),
        ("downs",      "small_house"),  # pasture / herder mound
        ("pasture",    "small_house"),
    ]:
        if descriptor in s:
            return glyph
    return DEFAULT_LANDMARK_GLYPH


def inset_polygon(poly: list[tuple[float, float]], factor: float = 0.97) -> list[tuple[float, float]]:
    """Shrink a polygon toward its centroid by `factor` so the
    backdrop biome doesn't kiss the cell border (cosmetic)."""
    if not poly:
        return poly
    cx = sum(p[0] for p in poly) / len(poly)
    cz = sum(p[1] for p in poly) / len(poly)
    return [(cx + (p[0] - cx) * factor, cz + (p[1] - cz) * factor) for p in poly]


def generate_biome_regions(
    zone_id: str,
    zone_name: str,
    cell_local: list[tuple[float, float]],
    landmarks: list[dict],
    default_biome: str,
) -> list[dict]:
    """Returns biome_regions list:
      - 1 main backdrop = full cell (slightly inset), painted default_biome.
      - 1 pocket per landmark whose derived biome != default_biome,
        computed as Voronoi-cell ∩ disc-around-landmark ∩ zone-cell so
        adjacent pockets share clean borders and never overlap.
    Landmarks whose biome matches the default produce NO pocket.
    """
    cx = sum(p[0] for p in cell_local) / len(cell_local)
    cz = sum(p[1] for p in cell_local) / len(cell_local)

    # Main backdrop — slight inset so it doesn't kiss the cell edge.
    backdrop_pts = inset_polygon(cell_local, 0.97)
    regions: list[dict] = [{
        "id": f"{zone_id}_main",
        "label": zone_name,
        "biome": default_biome,
        "polygon": {
            "points": [{"x": round(x, 1), "z": round(z, 1)} for x, z in backdrop_pts],
        },
        "opacity": 0.85,
        "label_position": {"x": round(cx, 1), "z": round(cz, 1)},
    }]

    # Distinct-biome landmarks (those whose pocket would actually
    # differ visually from the backdrop).
    distinct: list[tuple[dict, str]] = []
    for lm in sorted(landmarks, key=lambda l: l["id"]):
        biome = biome_for_landmark(lm["id"], lm.get("name", ""))
        if biome is None or biome == default_biome:
            continue
        distinct.append((lm, biome))
    if not distinct:
        return regions

    # Pocket radius scales with cell size, but capped so pockets stay
    # distinct named places — not biome floods.
    bb_w = max(p[0] for p in cell_local) - min(p[0] for p in cell_local)
    bb_h = max(p[1] for p in cell_local) - min(p[1] for p in cell_local)
    pocket_radius = max(280.0, min(min(bb_w, bb_h) / 6.0, 600.0))

    cell_poly = sg.Polygon(cell_local)
    if not cell_poly.is_valid:
        cell_poly = cell_poly.buffer(0)

    # If only one distinct landmark, just clip a disc to the cell.
    if len(distinct) == 1:
        lm, biome = distinct[0]
        off = lm["offset_from_zone_origin"]
        lx = float(off["x"]); lz = float(off["z"])
        ring = _clip_to_cell(
            sg.Point(lx, lz).buffer(pocket_radius, resolution=10),
            cell_poly,
        )
        if ring:
            regions.append(_pocket_dict(zone_id, lm, biome, ring, lx, lz))
        return regions

    # Multi-landmark: Voronoi over all distinct anchors, intersect each
    # cell with (disc ∩ zone-cell). Sentinel padding so all real cells
    # are finite.
    pts = np.array([
        (float(lm["offset_from_zone_origin"]["x"]),
         float(lm["offset_from_zone_origin"]["z"]))
        for lm, _ in distinct
    ])
    minx, miny, maxx, maxy = cell_poly.bounds
    span = max(maxx - minx, maxy - miny)
    sentinels = np.array([
        (minx - 5 * span, miny - 5 * span),
        (maxx + 5 * span, miny - 5 * span),
        (minx - 5 * span, maxy + 5 * span),
        (maxx + 5 * span, maxy + 5 * span),
    ])
    pts_padded = np.vstack([pts, sentinels])
    vor = Voronoi(pts_padded)

    for i, (lm, biome) in enumerate(distinct):
        off = lm["offset_from_zone_origin"]
        lx = float(off["x"]); lz = float(off["z"])
        region_idx = vor.point_region[i]
        verts = vor.regions[region_idx]
        if -1 in verts or not verts:
            continue
        sub = sg.Polygon([vor.vertices[v] for v in verts])
        if not sub.is_valid:
            sub = sub.buffer(0)
        disc = sg.Point(lx, lz).buffer(pocket_radius, resolution=10)
        clipped = sub.intersection(disc).intersection(cell_poly)
        ring = _shape_to_ring(clipped)
        if ring:
            regions.append(_pocket_dict(zone_id, lm, biome, ring, lx, lz))

    return regions


def _clip_to_cell(geom, cell_poly):
    return _shape_to_ring(geom.intersection(cell_poly))


def _shape_to_ring(geom) -> list[tuple[float, float]] | None:
    if geom.is_empty:
        return None
    if geom.geom_type == "MultiPolygon":
        geom = max(geom.geoms, key=lambda g: g.area)
    if geom.geom_type != "Polygon":
        return None
    ring = list(geom.exterior.coords)
    if ring and ring[0] == ring[-1]:
        ring = ring[:-1]
    return ring


def generate_spur_paths(
    landmarks: list[dict],
    hubs: list[tuple[str, float, float]],
    roads: list[dict],
    min_spur_length: float = 100.0,
) -> list[dict]:
    """For every landmark + hub not already on a road, draw a small
    dirt-path spur from the nearest road point to the place. Min 100m
    threshold skips landmarks/hubs that already sit on an existing
    path segment.

    Returns a list of new road dicts to append to the existing
    `roads` list.
    """
    road_points: list[tuple[float, float]] = []
    for road in roads:
        for p in road.get("path", {}).get("points", []) or []:
            road_points.append((float(p["x"]), float(p["z"])))
    if not road_points:
        return []

    targets: list[tuple[str, float, float]] = []
    for hub_id, hx, hz in sorted(hubs, key=lambda t: t[0]):
        targets.append((hub_id, hx, hz))
    for lm in sorted(landmarks, key=lambda l: l["id"]):
        off = lm["offset_from_zone_origin"]
        targets.append((lm["id"], float(off["x"]), float(off["z"])))

    spurs: list[dict] = []
    for tid, x, z in targets:
        nearest = min(road_points, key=lambda p: math.hypot(x - p[0], z - p[1]))
        d = math.hypot(x - nearest[0], z - nearest[1])
        if d < min_spur_length:
            continue
        spurs.append({
            "id": f"spur_to_{tid}",
            "type": "dirt_path",
            "path": {"points": [
                {"x": round(nearest[0], 1), "z": round(nearest[1], 1)},
                {"x": round(x, 1), "z": round(z, 1)},
            ]},
        })
    return spurs


def generate_ambient_features(
    zone_id: str,
    cell_local: list[tuple[float, float]],
    zone_biome: str,
    tier: str,
    occupied: list[tuple[float, float]],
    path_points: list[tuple[float, float]],
) -> list[dict]:
    """Deterministically scatter small lived-in glyphs (farmhouses /
    cabins / fishing huts depending on zone biome) across the zone
    cell. Constraints:
      - inside the cell polygon
      - ≥220 m from any landmark/hub
      - ≥320 m from any other ambient placement
      - ≤MAX_DIST_FROM_PATH (700 m) from any road/river point
        if such paths exist (so farmhouses cluster along the kingsroad
        and river, not in random wilderness pockets)
    """
    glyph = AMBIENT_GLYPH_BY_BIOME.get(zone_biome, "small_house")
    density = AMBIENT_DENSITY_BY_TIER.get(tier, 1.0)

    cell_poly = sg.Polygon(cell_local)
    if not cell_poly.is_valid:
        cell_poly = cell_poly.buffer(0)
    area_km2 = cell_poly.area / 1_000_000
    target_count = max(4, int(round(area_km2 * density)))

    minx, miny, maxx, maxy = cell_poly.bounds

    # Tiny LCG seeded from zone_id — same across runs.
    seed = 0xcbf29ce484222325
    for c in zone_id.encode():
        seed ^= c
        seed = (seed * 0x100000001b3) & 0xFFFFFFFFFFFFFFFF

    state = max(seed, 1)

    def rng_f32() -> float:
        nonlocal state
        state = (state * 6364136223846793005 + 1442695040888963407) & 0xFFFFFFFFFFFFFFFF
        return ((state >> 32) & 0xFFFFFFFF) / 0xFFFFFFFF

    MAX_DIST_FROM_PATH = 700.0
    have_paths = bool(path_points)

    def near_a_path(x: float, z: float) -> bool:
        if not have_paths:
            return True
        return min(math.hypot(x - px, z - pz) for px, pz in path_points) <= MAX_DIST_FROM_PATH

    placed: list[tuple[float, float]] = []
    avoid = list(occupied)
    min_self_spacing = 320.0
    min_landmark_spacing = 220.0

    attempts = 0
    while len(placed) < target_count and attempts < target_count * 60:
        attempts += 1
        x = minx + rng_f32() * (maxx - minx)
        z = miny + rng_f32() * (maxy - miny)
        if not cell_poly.contains(sg.Point(x, z)):
            continue
        if not near_a_path(x, z):
            continue
        too_close = False
        for ax, az in avoid:
            if math.hypot(x - ax, z - az) < min_landmark_spacing:
                too_close = True
                break
        if too_close:
            continue
        for px, pz in placed:
            if math.hypot(x - px, z - pz) < min_self_spacing:
                too_close = True
                break
        if too_close:
            continue
        placed.append((x, z))

    out: list[dict] = []
    for i, (x, z) in enumerate(placed):
        out.append({
            "id": f"{zone_id}_ambient_{i:02d}",
            "type": "ambient_glyph",
            "glyph": glyph,
            "position": {"x": round(x, 1), "z": round(z, 1)},
        })
    return out


def _pocket_dict(zone_id, lm, biome, ring, lx, lz):
    return {
        "id": f"{zone_id}_pocket_{lm['id']}",
        "label": lm.get("name", ""),
        "biome": biome,
        "polygon": {
            "points": [{"x": round(x, 1), "z": round(z, 1)} for x, z in ring],
        },
        "opacity": 0.85,
        "label_position": {"x": round(lx, 1), "z": round(lz, 1)},
    }


# ─── main ───────────────────────────────────────────────────────────

def main() -> int:
    cells_by_zone = load_cells_zone_local()
    written = 0
    skipped_no_cell = 0
    region_counts: list[int] = []

    for zd in sorted(p for p in ZONES_ROOT.iterdir() if p.is_dir()):
        zone_id = zd.name
        core_p = zd / "core.yaml"
        if not core_p.exists():
            continue
        core = yaml.safe_load(core_p.read_text())

        if zone_id not in cells_by_zone:
            print(f"  skip: {zone_id} (no zone_cell in world.yaml)", file=sys.stderr)
            skipped_no_cell += 1
            continue

        cell_local = cells_by_zone[zone_id]
        zone_biome = core.get("biome", "fields")
        default_biome = BIOME_KEY.get(zone_biome, zone_biome)

        # Load landmarks
        landmarks: list[dict] = []
        landmarks_p = zd / "landmarks.yaml"
        if landmarks_p.exists():
            landmarks = yaml.safe_load(landmarks_p.read_text()).get("landmarks", []) or []

        biome_regions = generate_biome_regions(
            zone_id,
            core.get("name", zone_id),
            cell_local,
            landmarks,
            default_biome,
        )
        region_counts.append(len(biome_regions))

        # Hub positions (avoid placing ambient features on top of them,
        # and used to derive spur paths to any hub off the road network).
        hub_positions: list[tuple[float, float]] = []
        hub_id_positions: list[tuple[str, float, float]] = []
        hubs_dir = zd / "hubs"
        if hubs_dir.is_dir():
            for hp in sorted(hubs_dir.glob("*.yaml")):
                hub_doc = yaml.safe_load(hp.read_text()) or {}
                off = hub_doc.get("offset_from_zone_origin")
                if off and "id" in hub_doc:
                    hx = float(off["x"])
                    hz = float(off["z"])
                    hub_positions.append((hx, hz))
                    hub_id_positions.append((hub_doc["id"], hx, hz))

        # Preserve hand-authored extras when geography.yaml already exists.
        # Read these BEFORE the ambient pass so it can bias farmhouse
        # placement toward existing roads/rivers.
        out_path = zd / "geography.yaml"
        existing_rivers: list = []
        existing_roads: list = []
        existing_scatter: dict = {}
        existing_free_labels: list = []
        if out_path.exists():
            existing = yaml.safe_load(out_path.read_text()) or {}
            existing_rivers = existing.get("rivers", []) or []
            existing_roads = existing.get("roads", []) or []
            existing_scatter = existing.get("scatter", {}) or {}
            existing_free_labels = existing.get("free_labels", []) or []

        scatter = existing_scatter or SCATTER_BY_BIOME.get(zone_biome, {})

        if not existing_roads:
            xs = [p[0] for p in cell_local]
            zs = [p[1] for p in cell_local]
            cx_road = (min(xs) + max(xs)) / 2
            cz_road = (min(zs) + max(zs)) / 2
            roads_out = [{
                "id": f"{zone_id}_main_road",
                "type": "dirt_path",
                "path": {
                    "points": [
                        {"x": round(min(xs) + 30, 1), "z": round(cz_road, 1)},
                        {"x": round(cx_road, 1),     "z": round(cz_road, 1)},
                        {"x": round(max(xs) - 30, 1), "z": round(cz_road, 1)},
                    ],
                },
            }]
        else:
            roads_out = existing_roads

        # Spur paths: every hub/landmark not on a road gets a dirt
        # connector to the nearest road point.
        spurs = generate_spur_paths(landmarks, hub_id_positions, roads_out)
        roads_out = roads_out + spurs

        # Pull all road + river path points for the ambient placement
        # bias (so farmhouses cluster along roads, spurs included).
        path_points: list[tuple[float, float]] = []
        for road in roads_out:
            for p in road.get("path", {}).get("points", []) or []:
                path_points.append((float(p["x"]), float(p["z"])))
        for river in existing_rivers:
            for p in river.get("path", {}).get("points", []) or []:
                path_points.append((float(p["x"]), float(p["z"])))

        # Features for landmarks (idempotent — recompute every run).
        features: list[dict] = []
        landmark_positions: list[tuple[float, float]] = []
        for lm in sorted(landmarks, key=lambda l: l["id"]):
            off = lm["offset_from_zone_origin"]
            x = float(off["x"])
            z = float(off["z"])
            landmark_positions.append((x, z))
            features.append({
                "id": lm["id"],
                "type": "landmark_glyph",
                "glyph": landmark_glyph_for(lm["id"], lm.get("name", "")),
                "position": {"x": x, "z": z},
            })

        # Procedural ambient features — small farmhouses / cabins /
        # fishing huts scattered along roads + rivers in the zone's
        # dominant biome.
        tier = core.get("tier", "starter")
        ambient = generate_ambient_features(
            zone_id, cell_local, zone_biome, tier,
            landmark_positions + hub_positions,
            path_points,
        )
        features.extend(ambient)

        # Default free-labels from connections (only if none preserved).
        if existing_free_labels:
            free_labels = existing_free_labels
        else:
            free_labels = []
            conn_p = zd / "connections.yaml"
            if conn_p.exists():
                conn_data = yaml.safe_load(conn_p.read_text()) or {}
                for c in conn_data.get("connections", []):
                    bp = c["border_position"]
                    arrow = {"e": "⟶", "se": "↘", "s": "↓", "sw": "↙",
                             "w": "⟵", "nw": "↖", "n": "↑", "ne": "↗"}.get(
                        c.get("direction", "e"), "⟶")
                    pretty = c["to_zone"].replace("_", " ").title()
                    free_labels.append({
                        "text": f"{arrow} {pretty}",
                        "position": {"x": round(bp["x"] * 0.92, 1),
                                     "z": round(bp["z"] * 0.92, 1)},
                        "rotation_deg": 0,
                        "style": "directional",
                    })

        doc: dict = {
            "id": f"geography__{zone_id}",
            "zone": zone_id,
            "schema_version": "1.0",
            "biome_regions": biome_regions,
        }
        if existing_rivers:
            doc["rivers"] = existing_rivers
        doc["roads"] = roads_out
        doc["features"] = features
        if scatter:
            doc["scatter"] = scatter
        if free_labels:
            doc["free_labels"] = free_labels

        out_path.write_text(
            yaml.dump(doc, sort_keys=False, allow_unicode=True,
                      default_flow_style=False, width=100)
        )
        written += 1

    avg = sum(region_counts) / len(region_counts) if region_counts else 0.0
    print(f"\ngeography (re)generated: {written} files")
    print(f"  no-cell skipped: {skipped_no_cell}")
    print(f"  avg sub-Voronoi regions per zone: {avg:.1f} (range {min(region_counts) if region_counts else 0}–{max(region_counts) if region_counts else 0})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
