"""Shared paths + helpers for every seeder module.

Every per-concern module imports `OUT` (destination root) and `write`
(atomic yaml dump) from here. Damage-type helpers live here too since
both materials (resist_adds) and runes (channel ids) use them.
"""
from __future__ import annotations

from pathlib import Path
import yaml

REPO = Path(__file__).resolve().parents[2]
OUT = REPO / "src" / "generated" / "items"


def write(path: Path, data: dict) -> None:
    """Write a dict to `path` as YAML (flow-style off for readability)."""
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "w") as f:
        yaml.safe_dump(data, f, sort_keys=False, default_flow_style=False)


# 12-channel damage-type index — matches vaern-core::DamageType exactly.
# Changing this order is a coordinated breaking change; see
# crates/vaern-core/src/damage_type.rs.
DAMAGE_TYPES = [
    "slashing", "piercing", "bludgeoning",
    "fire", "cold", "lightning", "force",
    "radiant", "necrotic", "blood", "poison", "acid",
]


def dt_index(name: str) -> int:
    return DAMAGE_TYPES.index(name)


def zeros12() -> list:
    """Fresh [0.0; 12] — every call returns a new list so callers can mutate."""
    return [0.0] * 12


def resist_adds(**pairs) -> list:
    """Build a 12-element resist_adds vector from channel keyword bumps.

    Example: `resist_adds(fire=10.0, radiant=-3.0)` → positions 3 and 7
    set, rest zero. Used heavily in materials.py for material-specific
    mechanical effects (silver vs necrotic, dragonscale vs fire, etc).
    """
    arr = zeros12()
    for channel, amount in pairs.items():
        arr[dt_index(channel)] = float(amount)
    return arr
