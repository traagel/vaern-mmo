"""One-shot rescale: set zone `bounds` per tier so each zone has
a consistent in-meter footprint. Overwrites any existing bounds.

Tier targets (axis ranges centered on origin):

    tier      | half-extent (m)  | total km   | rough run time @ 6 m/s
    ----------+------------------+------------+------------------------
    starter   | ±1000  × ±1000   | 2.0 × 2.0  | ~8 min diagonal
    mid       | ±1300  × ±1200   | 2.6 × 2.4  | ~10 min diagonal
    contested | ±1700  × ±1500   | 3.4 × 3.0  | ~13 min diagonal
    endgame   | ±1500  × ±1300   | 3.0 × 2.6  | ~11 min diagonal

Run from repo root:

    python3 scripts/resize_zones_by_tier.py
"""

from __future__ import annotations

import pathlib

import yaml

ROOT = pathlib.Path(__file__).resolve().parent.parent
ZONES_ROOT = ROOT / "src" / "generated" / "world" / "zones"

TIER_BOUNDS = {
    "starter":   (1000.0, 1000.0),
    "mid":       (1300.0, 1200.0),
    "contested": (1700.0, 1500.0),
    "endgame":   (1500.0, 1300.0),
}
DEFAULT = (1200.0, 1200.0)


def load(p: pathlib.Path):
    return yaml.safe_load(p.read_text())


def dump(p: pathlib.Path, data) -> None:
    p.write_text(
        yaml.dump(
            data,
            sort_keys=False,
            allow_unicode=True,
            default_flow_style=False,
            width=100,
        )
    )


def main() -> int:
    by_tier = {t: 0 for t in TIER_BOUNDS}
    by_tier["other"] = 0
    for zone_dir in sorted(p for p in ZONES_ROOT.iterdir() if p.is_dir()):
        core_path = zone_dir / "core.yaml"
        if not core_path.exists():
            continue
        core = load(core_path)
        tier = core.get("tier", "")
        hx, hz = TIER_BOUNDS.get(tier, DEFAULT)
        by_tier[tier if tier in TIER_BOUNDS else "other"] += 1

        core["bounds"] = {
            "min": {"x": -hx, "z": -hz},
            "max": {"x":  hx, "z":  hz},
        }
        dump(core_path, core)
        print(f"  {zone_dir.name:24s} tier={tier:9s} → {2*hx/1000:.2f} × {2*hz/1000:.2f} km")

    print()
    for t, n in by_tier.items():
        print(f"  {t}: {n} zones")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
