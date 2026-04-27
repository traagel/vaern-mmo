#!/usr/bin/env -S uv run --quiet --with pyyaml --with scipy --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyyaml", "scipy"]
# ///
"""Seed `src/generated/world/zones/<zone>/connections.yaml` for every
zone that doesn't already have one.

Edges come from §4.5 of the zone-map brief, filtered twice:
  1. Both endpoints must currently exist (28-zone roster).
  2. The two zones must be spatially adjacent — their Voronoi cells
     in `world.yaml` must share a border. Otherwise the rendered
     connection line cuts across other zones, which looks broken.

Spatial adjacency is computed by re-running scipy.spatial.Voronoi on
the anchor points in world.yaml and reading `ridge_points` (pairs of
input indices that share a Voronoi edge).

Direction (n/e/s/w/...) is computed from each zone's world_origin —
no need to author cardinals by hand.

Run from repo root:

    ./scripts/seed_connections.py
"""

from __future__ import annotations

import math
import pathlib
import sys

import numpy as np
import yaml
from scipy.spatial import Voronoi

ROOT = pathlib.Path(__file__).resolve().parent.parent
ZONES_ROOT = ROOT / "src" / "generated" / "world" / "zones"
WORLD_YAML = ROOT / "src" / "generated" / "world" / "world.yaml"

# §4.5 edges where BOTH endpoints currently exist in the 28-zone roster.
# Format: (from_zone, to_zone, type, level_continuous, pvp_safe).
EDGES: list[tuple[str, str, str, bool, bool]] = [
    # Starter → Mid
    ("stoneguard_deep",   "irongate_pass",     "ridge_path",    True,  True),
    ("stoneguard_deep",   "heartland_ride",    "kingsroad",     True,  True),
    ("dalewatch_marches", "heartland_ride",    "kingsroad",     True,  True),
    ("dalewatch_marches", "irongate_pass",     "kingsroad",     True,  True),
    ("ashen_holt",        "shadegrove",        "ash_path",      True,  True),
    ("ashen_holt",        "gravewatch_fields", "coastal_path",  True,  True),

    # Concord mid mesh
    ("heartland_ride",    "irongate_pass",     "kingsroad",     True,  True),
    ("heartland_ride",    "silverleaf_wood",   "kingsroad",     True,  True),
    ("irongate_pass",     "silverleaf_wood",   "mountain_pass", True,  True),
    ("silverleaf_wood",   "market_crossing",   "kingsroad",     True,  True),
    ("silverleaf_wood",   "greenwood_deep",    "forest_path",   True,  True),
    ("market_crossing",   "greenwood_deep",    "forest_path",   True,  True),

    # Concord mid → Contested
    ("greenwood_deep",    "ruin_line_south",   "ruin_path",     False, False),

    # Pact mid mesh
    ("gravewatch_fields", "shadegrove",        "ash_path",      True,  True),
    ("shadegrove",        "pact_causeway",     "causeway",      True,  True),
    ("shadegrove",        "skarncamp_wastes",  "fjord_path",    True,  True),
    ("pact_causeway",     "skarncamp_wastes",  "causeway",      True,  True),
    ("pact_causeway",     "scrap_flats",       "marsh_path",    True,  True),
    ("skarncamp_wastes",  "scrap_flats",       "fjord_path",    True,  True),

    # Pact mid → Contested
    ("scrap_flats",       "ruin_line_south",   "ruin_path",     False, False),

    # Contested 30-45 mesh
    ("ruin_line_north",   "ashweald",          "ash_path",      True,  False),
    ("ruin_line_south",   "iron_strand",       "coastal_path",  True,  False),

    # Endgame
    ("blackwater_deep",   "sundering_mines",   "tunnel",        True,  False),
    ("blackwater_deep",   "frost_spine",       "mountain_pass", True,  False),
    ("frost_spine",       "sundering_mines",   "tunnel",        True,  False),
    ("frost_spine",       "crown_of_ruin",     "mountain_pass", True,  False),
    ("sundering_mines",   "crown_of_ruin",     "tunnel",        True,  False),

    # Stitch endgame back to mid-tier contested so the graph isn't disconnected.
    ("blackwater_deep",   "iron_strand",       "coastal_path",  False, False),
    ("ashweald",          "frost_spine",       "mountain_pass", False, False),

    # Bridge the 7 starter zones not covered by §4.5 (they were
    # routed via new consolidated starters in the brief). Each gets
    # a single connection to a same-region mid so the graph is
    # traversable today.
    ("barrow_coast",      "gravewatch_fields", "coastal_path",  True,  True),
    ("firland_greenwood", "silverleaf_wood",   "forest_path",   True,  True),
    ("pactmarch",         "pact_causeway",     "causeway",      True,  True),
    ("scrap_marsh",       "scrap_flats",       "marsh_path",    True,  True),
    ("skarnreach",        "skarncamp_wastes",  "fjord_path",    True,  True),
    ("sunward_reach",     "silverleaf_wood",   "kingsroad",     True,  True),
    ("wyrling_downs",     "heartland_ride",    "kingsroad",     True,  True),
]

# Skip zones that already have hand-authored connections.
SKIP_ZONES: set[str] = set()  # all zones now seed-driven (cardinals + edge midpoints)


def compute_voronoi_adjacency(
    origins: dict[str, tuple[float, float]],
) -> tuple[set[tuple[str, str]], dict[tuple[str, str], tuple[float, float]]]:
    """Returns:
      - adj: unordered pairs (a, b) where a < b, sharing a cell edge.
      - midpoints: {(a, b) → (x, z) world coords of the shared-edge
        midpoint}, when the ridge is finite. Pairs whose ridge extends
        to infinity (touching the sentinel boundary) are omitted from
        midpoints, in which case the caller falls back to the
        midway-between-anchors point.
    """
    ids = sorted(origins.keys())
    pts = np.array([origins[k] for k in ids])
    xmin = pts[:, 0].min() - 50_000
    xmax = pts[:, 0].max() + 50_000
    zmin = pts[:, 1].min() - 50_000
    zmax = pts[:, 1].max() + 50_000
    sentinels = np.array([
        (xmin, zmin), (xmax, zmin), (xmin, zmax), (xmax, zmax),
    ])
    pts_padded = np.vstack([pts, sentinels])
    vor = Voronoi(pts_padded)
    n_real = len(ids)
    adj: set[tuple[str, str]] = set()
    midpoints: dict[tuple[str, str], tuple[float, float]] = {}
    for ridge_idx, (i, j) in enumerate(vor.ridge_points):
        if i >= n_real or j >= n_real:
            continue
        a, b = ids[i], ids[j]
        if a > b:
            a, b = b, a
        adj.add((a, b))
        ridge_v = vor.ridge_vertices[ridge_idx]
        if -1 in ridge_v or len(ridge_v) < 2:
            continue
        v0 = vor.vertices[ridge_v[0]]
        v1 = vor.vertices[ridge_v[1]]
        midpoints[(a, b)] = (
            float((v0[0] + v1[0]) * 0.5),
            float((v0[1] + v1[1]) * 0.5),
        )
    return adj, midpoints


def cardinal_8(dx: float, dz: float) -> str:
    """Convert vector to one of n/ne/e/se/s/sw/w/nw. +z = south."""
    angle = math.degrees(math.atan2(dz, dx))  # angle from +x (east), CCW… but +z=south so atan2(z, x) with z south is CW from east
    # Normalize to [0, 360)
    angle = (angle + 360) % 360
    # 8 sectors of 45°, centered: e=0, se=45, s=90, sw=135, w=180, nw=225, n=270, ne=315
    if   angle < 22.5  or angle >= 337.5: return "e"
    elif angle < 67.5:                    return "se"
    elif angle < 112.5:                   return "s"
    elif angle < 157.5:                   return "sw"
    elif angle < 202.5:                   return "w"
    elif angle < 247.5:                   return "nw"
    elif angle < 292.5:                   return "n"
    else:                                 return "ne"


def main() -> int:
    layout = yaml.safe_load(WORLD_YAML.read_text())
    origins: dict[str, tuple[float, float]] = {
        p["zone"]: (p["world_origin"]["x"], p["world_origin"]["z"])
        for p in layout["zone_placements"]
    }
    bounds_by_zone: dict[str, dict] = {}
    for zd in sorted(p for p in ZONES_ROOT.iterdir() if p.is_dir()):
        core_p = zd / "core.yaml"
        if not core_p.exists():
            continue
        c = yaml.safe_load(core_p.read_text())
        bounds_by_zone[c["id"]] = c.get("bounds")

    # Compute Voronoi adjacency: which zone-pairs share a cell border
    # in world space, plus the world-space midpoint of each shared edge.
    adjacency, ridge_midpoints = compute_voronoi_adjacency(origins)
    print(f"Voronoi adjacency: {len(adjacency)} zone-pairs share a cell border "
          f"({len(ridge_midpoints)} with finite ridges)")

    def is_adjacent(a: str, b: str) -> bool:
        key = (a, b) if a < b else (b, a)
        return key in adjacency

    def border_position_local(src: str, dst: str) -> dict:
        """Border position in src's zone-local coords. Prefers the
        shared-cell-edge midpoint; falls back to the anchor-pair
        midpoint when the ridge is infinite (zone on the coastline)."""
        key = (src, dst) if src < dst else (dst, src)
        sx, sz = origins[src]
        if key in ridge_midpoints:
            wx, wz = ridge_midpoints[key]
        else:
            dx, dz = origins[dst]
            wx, wz = (sx + dx) * 0.5, (sz + dz) * 0.5
        return {"x": round(wx - sx, 1), "z": round(wz - sz, 1)}

    # Group edges by source zone (one connections.yaml per zone, both
    # directions emitted as separate entries — i.e. emit both A→B and
    # B→A so each zone's file lists *its* outgoing edges).
    by_zone: dict[str, list[dict]] = {}
    dropped_non_adjacent: list[tuple[str, str]] = []
    for from_z, to_z, type_, level_cont, pvp_safe in EDGES:
        if from_z not in origins or to_z not in origins:
            print(f"  skip: {from_z} ↔ {to_z} (missing world placement)", file=sys.stderr)
            continue
        if not is_adjacent(from_z, to_z):
            dropped_non_adjacent.append((from_z, to_z))
            continue
        for src, dst in [(from_z, to_z), (to_z, from_z)]:
            sx, sz = origins[src]
            dx, dz = origins[dst]
            direction = cardinal_8(dx - sx, dz - sz)
            border_pos = border_position_local(src, dst)
            entry = {
                "to_zone": dst,
                "direction": direction,
                "type": type_,
                "border_position": border_pos,
                "border_label": f"to {dst.replace('_', ' ').title()}",
                "level_continuous": level_cont,
                "pvp_safe": pvp_safe,
            }
            by_zone.setdefault(src, []).append(entry)

    # Fill in adjacency-driven edges for zone-pairs that ARE adjacent
    # but didn't appear in §4.5 (no lore-defined road). These get a
    # generic `dirt_path` so the world graph is connected even where
    # the brief was silent. Same direction/border-position computation
    # as above.
    adjacent_with_edge: set[tuple[str, str]] = set()
    for from_z, to_z, _, _, _ in EDGES:
        key = (from_z, to_z) if from_z < to_z else (to_z, from_z)
        adjacent_with_edge.add(key)
    generic_added = 0
    for a, b in sorted(adjacency):
        if (a, b) in adjacent_with_edge:
            continue
        for src, dst in [(a, b), (b, a)]:
            sx, sz = origins[src]
            dx, dz = origins[dst]
            direction = cardinal_8(dx - sx, dz - sz)
            border_pos = border_position_local(src, dst)
            entry = {
                "to_zone": dst,
                "direction": direction,
                "type": "dirt_path",
                "border_position": border_pos,
                "border_label": f"to {dst.replace('_', ' ').title()}",
                "level_continuous": False,
                "pvp_safe": True,
            }
            by_zone.setdefault(src, []).append(entry)
            generic_added += 1

    written = 0
    skipped = 0
    for zone_id, edges in sorted(by_zone.items()):
        if zone_id in SKIP_ZONES:
            skipped += 1
            print(f"  skip: {zone_id} (hand-authored connections.yaml)")
            continue
        out_path = ZONES_ROOT / zone_id / "connections.yaml"
        if out_path.exists():
            skipped += 1
            print(f"  skip: {zone_id} (file exists)")
            continue
        # Dedup + sort edges by to_zone for stable diffs.
        seen = set()
        unique = []
        for e in edges:
            if e["to_zone"] in seen:
                continue
            seen.add(e["to_zone"])
            unique.append(e)
        unique.sort(key=lambda e: e["to_zone"])
        doc = {
            "id": f"connections__{zone_id}",
            "zone": zone_id,
            "connections": unique,
        }
        out_path.write_text(
            yaml.dump(doc, sort_keys=False, allow_unicode=True,
                      default_flow_style=False, width=100)
        )
        written += 1

    print(f"\nconnections seeded: {written} new files, {skipped} skipped")
    print(f"  §4.5 edges kept (adjacent):     {len(EDGES) - len(dropped_non_adjacent)} / {len(EDGES)}")
    print(f"  §4.5 edges dropped (not adjacent): {len(dropped_non_adjacent)}")
    if dropped_non_adjacent:
        for a, b in dropped_non_adjacent[:8]:
            print(f"    drop: {a} ↔ {b}")
        if len(dropped_non_adjacent) > 8:
            print(f"    ... and {len(dropped_non_adjacent) - 8} more")
    print(f"  generic adjacency edges added: {generic_added // 2} (×2 directions)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
