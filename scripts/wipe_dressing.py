#!/usr/bin/env python3
"""Wipe all spawned dressing across all zones.

Removes:
- `props:` arrays from every `world/zones/*/hubs/*.yaml`.
- `scatter:` rules from every `world/zones/*/core.yaml`.
- `world/voxel_edits.bin` (persisted voxel sculpts).

Other YAML fields (id, biome, npcs, dialogue, etc.) are preserved
verbatim. Run from any directory:

    python3 scripts/wipe_dressing.py
"""

from __future__ import annotations

import sys
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parent.parent
WORLD = ROOT / "src" / "generated" / "world"
ZONES = WORLD / "zones"


def strip_key(path: Path, key: str) -> bool:
    """Drop `key` from the YAML root mapping. Returns True if changed."""
    text = path.read_text()
    data = yaml.safe_load(text)
    if not isinstance(data, dict) or key not in data:
        return False
    del data[key]
    path.write_text(yaml.safe_dump(data, sort_keys=False, default_flow_style=False))
    return True


def main() -> int:
    if not ZONES.is_dir():
        print(f"error: zones dir not found at {ZONES}", file=sys.stderr)
        return 2

    hubs_changed = 0
    cores_changed = 0
    for zone_dir in sorted(ZONES.iterdir()):
        if not zone_dir.is_dir():
            continue
        hubs = zone_dir / "hubs"
        if hubs.is_dir():
            for hub_yaml in sorted(hubs.glob("*.yaml")):
                if strip_key(hub_yaml, "props"):
                    print(f"  stripped props from {hub_yaml.relative_to(ROOT)}")
                    hubs_changed += 1
        core = zone_dir / "core.yaml"
        if core.is_file() and strip_key(core, "scatter"):
            print(f"  stripped scatter from {core.relative_to(ROOT)}")
            cores_changed += 1

    voxel_edits = WORLD / "voxel_edits.bin"
    if voxel_edits.is_file():
        size = voxel_edits.stat().st_size
        voxel_edits.unlink()
        print(f"  deleted {voxel_edits.relative_to(ROOT)} ({size:,} bytes)")

    print(
        f"done: stripped props from {hubs_changed} hub yamls, "
        f"scatter from {cores_changed} zone yamls."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
