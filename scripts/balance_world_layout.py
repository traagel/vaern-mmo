#!/usr/bin/env -S uv run --quiet --with scipy --with shapely --with pyyaml --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["scipy", "shapely", "pyyaml"]
# ///
"""Generate `src/generated/world/world.yaml` with a region-clustered,
faction-banded world layout AND zone-level Voronoi cells clipped to an
authored island coastline.

Pipeline:
  1. Auto-place each zone within its region by (tier, id)-driven layout
     (REGION_CENTROID is the only hand-tuned table). Special case for
     ruin_line which has 6 zones in a custom chain layout.
  2. Per-zone overrides for lore-tuned positions
     (e.g. dalewatch_marches sits east of heartland in western_dales).
  3. Hand-author an irregular coastline polygon containing all anchors.
  4. Compute Voronoi cells (scipy.spatial.Voronoi) and clip each to the
     coastline (shapely intersection).
  5. Write coastline + per-zone zone_cell into world.yaml.

Run from repo root:

    python3 scripts/balance_world_layout.py
    # or directly: ./scripts/balance_world_layout.py
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
OUT_PATH = ROOT / "src" / "generated" / "world" / "world.yaml"

MIN_SPACING_M = 1700.0  # tight enough that cells roughly match zone bounds

# ─── Region centroids (single source of truth) ──────────────────────
REGION_CENTROID = {
    # Concord (faction_a) — western half
    "iron_mountains":    (-6500.0, -4250.0),
    "central_highlands": (-6000.0,  4250.0),
    "western_dales":     (-6000.0,     0.0),
    "southern_shore":    (-2250.0,  5750.0),
    # Contested
    "northern_cliffs":   (    0.0, -5250.0),
    "ruin_line":         (    0.0,     0.0),
    # Rend (faction_b) — eastern half
    "pact_steppes":      ( 6500.0, -4250.0),
    "ashen_march":       ( 6000.0,     0.0),
    "barrow_shore":      ( 6000.0,  4250.0),
    "eastern_fjords":    ( 2500.0,  5500.0),
}

# Tier ordering — outer (starter) is farthest from the world origin
# (where ruin_line sits); inner (endgame) is closest.
TIER_RANK = {"starter": 0, "mid": 1, "contested": 2, "endgame": 3}

# Per-zone lore overrides (offset relative to the zone's region centroid).
# Reserve for cases where in-zone content was authored assuming a specific
# spatial role within its region.
ZONE_OVERRIDE: dict[str, tuple[float, float]] = {
    # dalewatch sits central-east in western_dales (kingsroad runs from
    # heartland_ride west, through dale, east to ford_of_ashmere).
    "dalewatch_marches": (750.0, 0.0),
    # Heartland is one stop west of dale (its connection goes "w" from dale).
    "heartland_ride":    (-1000.0, 750.0),
    # Wyrling is the far-NW outer of western_dales.
    "wyrling_downs":     (-1250.0, -1000.0),
}

# ruin_line is a long-thin region; auto-placement would bunch it. Explicit chain.
RUIN_LINE_OFFSETS = {
    "ruin_line_north": (    0.0, -2750.0),  # L30-38
    "ashweald":        (-1250.0, -1500.0),  # L38-45
    "blackwater_deep": ( 1250.0,  -750.0),  # L45-52
    "sundering_mines": (-1250.0,   750.0),  # L50-58
    "crown_of_ruin":   ( 1500.0,  1750.0),  # L55-60
    "ruin_line_south": ( -250.0,  2750.0),  # L30-38
}

# ─── Coastline (irregular continent silhouette) ─────────────────────
# CCW ordering. Contains every anchor with multi-km buffer.
COASTLINE: list[tuple[float, float]] = [
    (-8750.0, -6250.0),  # NW corner
    (-4000.0, -6750.0),  # N (slight bay)
    ( 2000.0, -6500.0),  # N
    ( 7250.0, -5500.0),  # NE corner
    ( 8750.0, -2750.0),  # E (Rend coast)
    ( 8500.0,  1250.0),  # E
    ( 8750.0,  4000.0),  # E
    ( 5750.0,  6750.0),  # SE corner
    ( 1500.0,  7000.0),  # S
    (-1500.0,  6750.0),  # S (Greenwood inlet)
    (-4000.0,  7000.0),  # S
    (-6750.0,  6250.0),  # SW corner
    (-8500.0,  3250.0),  # W (Concord coast)
    (-8750.0,    0.0),   # W
    (-8500.0, -3250.0),  # W
]


# ─── Auto-placement algorithm ───────────────────────────────────────

def tier_axis(centroid: tuple[float, float]) -> tuple[float, float]:
    """Unit vector pointing OUTWARD (away from world origin)."""
    cx, cz = centroid
    mag = math.hypot(cx, cz)
    if mag < 1e-6:
        return (1.0, 0.0)
    return (cx / mag, cz / mag)


def perp(v: tuple[float, float]) -> tuple[float, float]:
    return (-v[1], v[0])


def auto_place(
    centroid: tuple[float, float],
    zones: list[dict],
) -> dict[str, tuple[float, float]]:
    cx, cz = centroid
    sorted_zones = sorted(zones, key=lambda z: (TIER_RANK.get(z["tier"], 99), z["id"]))
    n = len(sorted_zones)
    axis = tier_axis(centroid)
    p = perp(axis)

    if n == 0:
        return {}
    if n == 1:
        return {sorted_zones[0]["id"]: (cx, cz)}
    if n == 2:
        r = 950.0  # slightly above MIN_SPACING/2 to clear the assertion
        return {
            sorted_zones[0]["id"]: (cx + axis[0] * r, cz + axis[1] * r),
            sorted_zones[1]["id"]: (cx - axis[0] * r, cz - axis[1] * r),
        }
    if n == 3:
        r = 1100.0
        out: dict[str, tuple[float, float]] = {}
        out[sorted_zones[0]["id"]] = (cx + axis[0] * r, cz + axis[1] * r)
        ix = cx - axis[0] * (r * 0.5)
        iz = cz - axis[1] * (r * 0.5)
        s = r * (math.sqrt(3) * 0.5)
        out[sorted_zones[1]["id"]] = (ix + p[0] * s, iz + p[1] * s)
        out[sorted_zones[2]["id"]] = (ix - p[0] * s, iz - p[1] * s)
        return out
    if n == 4:
        r = 1250.0
        outers = sorted_zones[:2]
        inners = sorted_zones[2:]
        out = {}
        out[outers[0]["id"]] = (cx + axis[0] * r + p[0] * r, cz + axis[1] * r + p[1] * r)
        out[outers[1]["id"]] = (cx + axis[0] * r - p[0] * r, cz + axis[1] * r - p[1] * r)
        out[inners[0]["id"]] = (cx - axis[0] * r + p[0] * r, cz - axis[1] * r + p[1] * r)
        out[inners[1]["id"]] = (cx - axis[0] * r - p[0] * r, cz - axis[1] * r - p[1] * r)
        return out
    raise SystemExit(
        f"region centroid {centroid} has {n} zones; auto-placement only "
        "supports N=1..4. Add a layout pattern."
    )


# ─── Voronoi via scipy + shapely clip ───────────────────────────────

def voronoi_cells(
    anchors_ordered: list[tuple[str, float, float]],
    coastline: list[tuple[float, float]],
) -> dict[str, list[tuple[float, float]]]:
    """Returns {zone_id: cell_polygon} with each cell clipped to coastline.

    For Voronoi to be bounded, we add 4 sentinel "far" points well outside
    the coastline so every real anchor's cell is finite. We then take only
    the regions belonging to the real anchors and intersect with coastline.
    """
    pts = np.array([(x, z) for _, x, z in anchors_ordered])
    # Sentinel points far outside the coastline bounding box.
    xmin = min(p[0] for p in coastline) - 50_000
    xmax = max(p[0] for p in coastline) + 50_000
    zmin = min(p[1] for p in coastline) - 50_000
    zmax = max(p[1] for p in coastline) + 50_000
    sentinels = np.array([
        (xmin, zmin),
        (xmax, zmin),
        (xmin, zmax),
        (xmax, zmax),
    ])
    pts_padded = np.vstack([pts, sentinels])
    vor = Voronoi(pts_padded)
    coast_poly = sg.Polygon(coastline)

    out: dict[str, list[tuple[float, float]]] = {}
    for i, (zid, _, _) in enumerate(anchors_ordered):
        region_idx = vor.point_region[i]
        region = vor.regions[region_idx]
        if -1 in region or not region:
            print(f"WARNING: unbounded Voronoi region for {zid}", file=sys.stderr)
            out[zid] = []
            continue
        cell_pts = [tuple(vor.vertices[v]) for v in region]
        cell = sg.Polygon(cell_pts)
        if not cell.is_valid:
            cell = cell.buffer(0)
        clipped = cell.intersection(coast_poly)
        if clipped.is_empty:
            out[zid] = []
            continue
        # Take exterior ring of the (possibly multi) polygon
        if clipped.geom_type == "Polygon":
            ring = list(clipped.exterior.coords)
        elif clipped.geom_type == "MultiPolygon":
            biggest = max(clipped.geoms, key=lambda g: g.area)
            ring = list(biggest.exterior.coords)
        else:
            print(f"WARNING: unexpected clip geometry {clipped.geom_type} for {zid}", file=sys.stderr)
            out[zid] = []
            continue
        # Drop the closing duplicate vertex (shapely repeats first point)
        if ring and ring[0] == ring[-1]:
            ring = ring[:-1]
        out[zid] = [(x, z) for x, z in ring]
    return out


def round_poly(poly: list[tuple[float, float]], digits: int = 1) -> list[tuple[float, float]]:
    return [(round(x, digits), round(z, digits)) for x, z in poly]


# ─── Load zones ──────────────────────────────────────────────────────

def load_zones() -> list[dict]:
    out = []
    for d in sorted(p for p in ZONES_ROOT.iterdir() if p.is_dir()):
        core = d / "core.yaml"
        if not core.exists():
            continue
        c = yaml.safe_load(core.read_text())
        out.append({
            "id": c["id"],
            "region": c.get("region", ""),
            "tier": c.get("tier", ""),
            "faction": c.get("faction_control", ""),
        })
    return out


def compute_anchors(zones: list[dict]) -> dict[str, tuple[float, float]]:
    by_region: dict[str, list[dict]] = {}
    for z in zones:
        if z["region"] not in REGION_CENTROID:
            raise SystemExit(
                f"zone {z['id']!r} has region {z['region']!r} not in REGION_CENTROID"
            )
        by_region.setdefault(z["region"], []).append(z)

    anchors: dict[str, tuple[float, float]] = {}

    if "ruin_line" in by_region:
        cx, cz = REGION_CENTROID["ruin_line"]
        for z in by_region["ruin_line"]:
            if z["id"] not in RUIN_LINE_OFFSETS:
                raise SystemExit(f"ruin_line zone {z['id']!r} has no RUIN_LINE_OFFSETS entry")
            ox, oz = RUIN_LINE_OFFSETS[z["id"]]
            anchors[z["id"]] = (cx + ox, cz + oz)

    for region, region_zones in by_region.items():
        if region == "ruin_line":
            continue
        # Auto-place zones EXCEPT those carrying an override.
        autoplace_zones = [z for z in region_zones if z["id"] not in ZONE_OVERRIDE]
        placed = auto_place(REGION_CENTROID[region], autoplace_zones)
        anchors.update(placed)

    # Per-zone overrides last.
    for zid, (ox, oz) in ZONE_OVERRIDE.items():
        z = next((z for z in zones if z["id"] == zid), None)
        if not z:
            continue
        cx, cz = REGION_CENTROID[z["region"]]
        anchors[zid] = (cx + ox, cz + oz)

    return anchors


# ─── Main ───────────────────────────────────────────────────────────

def main() -> int:
    zones = load_zones()
    anchors = compute_anchors(zones)

    keys = sorted(anchors.keys())
    violations = []
    for i in range(len(keys)):
        for j in range(i + 1, len(keys)):
            ax, az = anchors[keys[i]]
            bx, bz = anchors[keys[j]]
            d = math.hypot(ax - bx, az - bz)
            if d < MIN_SPACING_M:
                violations.append((keys[i], keys[j], d))
    if violations:
        print(
            f"ERROR: {len(violations)} zone-pair distances below {MIN_SPACING_M:.0f} m:",
            file=sys.stderr,
        )
        for a, b, d in sorted(violations, key=lambda v: v[2])[:20]:
            print(f"  {a} ↔ {b}: {d:.0f} m", file=sys.stderr)
        return 1

    # Compute Voronoi cells.
    anchors_ordered = [(z["id"], *anchors[z["id"]]) for z in zones]
    cells = voronoi_cells(anchors_ordered, COASTLINE)

    placements = []
    for z in zones:
        zid = z["id"]
        x, dz = anchors[zid]
        cell = round_poly(cells.get(zid, []))
        placement = {
            "zone": zid,
            "world_origin": {"x": round(x, 1), "z": round(dz, 1)},
            "rotation_deg": 0,
            "z_index": 1,
        }
        if cell:
            placement["zone_cell"] = {
                "points": [{"x": p[0], "z": p[1]} for p in cell],
            }
        placements.append(placement)

    layout = {
        "id": "world__vaern",
        "schema_version": "1.0",
        "zone_placements": placements,
        "coastline": {
            "points": [{"x": round(x, 1), "z": round(z, 1)} for x, z in COASTLINE],
        },
        "world_features": [],
    }
    OUT_PATH.write_text(
        yaml.dump(
            layout, sort_keys=False, allow_unicode=True,
            default_flow_style=False, width=100,
        )
    )

    xs = [a[0] for a in anchors.values()]
    zs = [a[1] for a in anchors.values()]
    min_pair = min(
        math.hypot(anchors[keys[i]][0] - anchors[keys[j]][0],
                   anchors[keys[i]][1] - anchors[keys[j]][1])
        for i in range(len(keys)) for j in range(i + 1, len(keys))
    )
    cells_count = sum(1 for p in placements if "zone_cell" in p)
    print(f"wrote {OUT_PATH}")
    print(f"  {len(placements)} placements ({cells_count} with cells)")
    print(
        f"  world extent: x ∈ [{min(xs):.0f}, {max(xs):.0f}] "
        f"({(max(xs) - min(xs)) / 1000:.1f} km), "
        f"z ∈ [{min(zs):.0f}, {max(zs):.0f}] "
        f"({(max(zs) - min(zs)) / 1000:.1f} km)"
    )
    print(f"  closest anchor pair: {min_pair:.0f} m  (limit: {MIN_SPACING_M:.0f} m)")
    print(f"  coastline: {len(COASTLINE)} vertices")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
