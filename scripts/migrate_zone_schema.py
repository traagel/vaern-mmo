"""One-shot migration: patch every zone with `bounds` + `coordinate_system`
in core.yaml, and place every hub without `offset_from_zone_origin` on a
deterministic ring around (0, 0).

Run from repo root:

    python3 scripts/migrate_zone_schema.py

Dalewatch Marches is skipped for hub-offset placement because its 5 hubs
already carry real lore-consistent positions.

Bounds are derived from the union of landmark and hub offsets, padded by
PADDING meters and floored to a starter-zone-shaped box (~2.4 x 3.1 km).
"""

from __future__ import annotations

import hashlib
import math
import pathlib
import sys

import yaml

ROOT = pathlib.Path(__file__).resolve().parent.parent
ZONES_ROOT = ROOT / "src" / "generated" / "world" / "zones"

DEFAULT_HALF_X = 1200.0   # half-width (east-west)
DEFAULT_HALF_Z = 1500.0   # half-height (north-south)
PADDING = 200.0           # added around landmark/hub coord range

# Hubs in this zone already carry real offsets — leave them alone.
SKIP_HUB_PATCH = {"dalewatch_marches"}


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


def deterministic_ring_offset(hub_id: str, idx: int, total: int) -> dict:
    """Place hub on a 700m ring at a deterministic-but-spread angle.

    Uses the hub_id hash to break ties so a zone re-run produces the same
    placement, but distinct hubs in the same zone don't pile on top of
    each other.
    """
    salt = int(hashlib.md5(hub_id.encode()).hexdigest()[:8], 16)
    base_angle = (idx / max(total, 1)) * math.tau
    jitter = ((salt % 1000) / 1000.0 - 0.5) * (math.tau / max(total * 2, 1))
    angle = base_angle + jitter
    radius = 700.0
    return {
        "x": round(radius * math.cos(angle), 1),
        "z": round(radius * math.sin(angle), 1),
    }


def expand_range(rng: tuple[float, float, float, float], pt: dict) -> tuple:
    min_x, max_x, min_z, max_z = rng
    x = float(pt["x"])
    z = float(pt["z"])
    return (min(min_x, x), max(max_x, x), min(min_z, z), max(max_z, z))


def floor_to_default(min_x, max_x, min_z, max_z) -> tuple:
    """Ensure bounds are at least the starter-zone default size."""
    if max_x - min_x < 2 * DEFAULT_HALF_X:
        cx = (min_x + max_x) / 2
        min_x = cx - DEFAULT_HALF_X
        max_x = cx + DEFAULT_HALF_X
    if max_z - min_z < 2 * DEFAULT_HALF_Z:
        cz = (min_z + max_z) / 2
        min_z = cz - DEFAULT_HALF_Z
        max_z = cz + DEFAULT_HALF_Z
    return min_x, max_x, min_z, max_z


def migrate_zone(zone_dir: pathlib.Path) -> tuple[int, int]:
    """Returns (cores_patched, hubs_patched)."""
    zone_id = zone_dir.name
    core_path = zone_dir / "core.yaml"
    if not core_path.exists():
        return (0, 0)

    core = load(core_path)
    cores_patched = 0

    # Compute the coord-range from landmarks + existing hub offsets.
    rng = (-DEFAULT_HALF_X, DEFAULT_HALF_X, -DEFAULT_HALF_Z, DEFAULT_HALF_Z)
    landmarks_path = zone_dir / "landmarks.yaml"
    if landmarks_path.exists():
        lm_data = load(landmarks_path)
        for lm in lm_data.get("landmarks", []) or []:
            rng = expand_range(rng, lm["offset_from_zone_origin"])

    hubs_dir = zone_dir / "hubs"
    hub_paths: list[pathlib.Path] = []
    if hubs_dir.exists():
        hub_paths = sorted(hubs_dir.glob("*.yaml"))
        for hp in hub_paths:
            h = load(hp)
            off = h.get("offset_from_zone_origin")
            if off:
                rng = expand_range(rng, off)

    # Pad and floor to starter-zone default.
    min_x, max_x, min_z, max_z = rng
    min_x -= PADDING
    max_x += PADDING
    min_z -= PADDING
    max_z += PADDING
    min_x, max_x, min_z, max_z = floor_to_default(min_x, max_x, min_z, max_z)

    # Pick origin hub: hub at (0,0) if any, else first alphabetically.
    origin_hub = None
    for hp in hub_paths:
        h = load(hp)
        off = h.get("offset_from_zone_origin")
        if off and abs(float(off["x"])) < 1.0 and abs(float(off["z"])) < 1.0:
            origin_hub = h["id"]
            break
    if origin_hub is None and hub_paths:
        origin_hub = load(hub_paths[0])["id"]
    if origin_hub is None:
        origin_hub = f"{zone_id}_center"

    if "bounds" not in core:
        core["bounds"] = {
            "min": {"x": round(min_x, 1), "z": round(min_z, 1)},
            "max": {"x": round(max_x, 1), "z": round(max_z, 1)},
        }
        cores_patched = 1
    if "coordinate_system" not in core:
        core["coordinate_system"] = {
            "origin": origin_hub,
            "axes": {"x_positive": "east", "z_positive": "south"},
            "unit": "meter",
        }
        cores_patched = 1

    if cores_patched:
        dump(core_path, core)

    # Patch hub offsets where missing — but skip zones whose hubs already
    # carry authored positions.
    hubs_patched = 0
    if zone_id in SKIP_HUB_PATCH:
        return (cores_patched, 0)

    n = len(hub_paths)
    for idx, hp in enumerate(hub_paths):
        h = load(hp)
        if h.get("offset_from_zone_origin") is not None:
            continue
        h["offset_from_zone_origin"] = deterministic_ring_offset(
            h["id"], idx, n
        )
        dump(hp, h)
        hubs_patched += 1

    return (cores_patched, hubs_patched)


def main() -> int:
    if not ZONES_ROOT.exists():
        print(f"zones dir not found: {ZONES_ROOT}", file=sys.stderr)
        return 1
    total_cores = 0
    total_hubs = 0
    zones = sorted(p for p in ZONES_ROOT.iterdir() if p.is_dir())
    for z in zones:
        c, h = migrate_zone(z)
        if c or h:
            print(f"  {z.name}: core={c} hubs={h}")
        total_cores += c
        total_hubs += h
    print(f"\nmigrated {len(zones)} zones — cores patched: {total_cores}, "
          f"hubs patched: {total_hubs}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
