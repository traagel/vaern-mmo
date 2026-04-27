"""Generate `src/generated/world/world.yaml` — the world-level layout
file. Places every zone on a 2800 m ring around the world origin,
sorted by zone id. Starter zones (those with `starter_race` set) get
placed first to mirror the current hardcoded ring in vaern-client.

Run from repo root:

    python3 scripts/emit_world_layout.py
"""

from __future__ import annotations

import math
import pathlib

import yaml

ROOT = pathlib.Path(__file__).resolve().parent.parent
ZONES_ROOT = ROOT / "src" / "generated" / "world" / "zones"
OUT_PATH = ROOT / "src" / "generated" / "world" / "world.yaml"

ZONE_RING_RADIUS = 2800.0


def load_zone_ids() -> list[tuple[str, bool]]:
    """Return [(zone_id, is_starter)] sorted: starters first, then by id."""
    out: list[tuple[str, bool]] = []
    for zd in sorted(p for p in ZONES_ROOT.iterdir() if p.is_dir()):
        core = zd / "core.yaml"
        if not core.exists():
            continue
        data = yaml.safe_load(core.read_text())
        is_starter = bool(data.get("starter_race"))
        out.append((data["id"], is_starter))
    starters = sorted([z for z, s in out if s])
    others = sorted([z for z, s in out if not s])
    return [(z, True) for z in starters] + [(z, False) for z in others]


def main() -> int:
    zones = load_zone_ids()
    n = max(len(zones), 1)
    placements = []
    for i, (zid, _) in enumerate(zones):
        angle = (i / n) * math.tau
        placements.append({
            "zone": zid,
            "world_origin": {
                "x": round(ZONE_RING_RADIUS * math.cos(angle), 1),
                "z": round(ZONE_RING_RADIUS * math.sin(angle), 1),
            },
            "rotation_deg": 0,
            "z_index": 1,
        })

    layout = {
        "id": "world__vaern",
        "schema_version": "1.0",
        "zone_placements": placements,
        "world_features": [],
    }

    OUT_PATH.write_text(
        yaml.dump(
            layout,
            sort_keys=False,
            allow_unicode=True,
            default_flow_style=False,
            width=100,
        )
    )
    print(f"wrote {OUT_PATH} — {len(placements)} zone placements")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
