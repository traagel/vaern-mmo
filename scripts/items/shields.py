"""Shield bases. Small table; each shield has base AC + base block params
that Material.ac_mult × Quality.stat_mult scale at resolve time.
"""
from __future__ import annotations

from .common import OUT, write


# (id, piece_name, base_ac, base_block_chance_pct, base_block_value, weight_kg, size)
SHIELD_PIECES = [
    ("buckler",      "Buckler",       3.5, 10.0, 15.0, 2.0, "small"),
    ("round_shield", "Round Shield",  5.5, 15.0, 20.0, 3.5, "medium"),
    ("kite_shield",  "Kite Shield",   7.5, 12.0, 30.0, 5.0, "medium"),
    ("tower_shield", "Tower Shield", 11.0, 10.0, 45.0, 8.0, "large"),
]


def seed() -> int:
    bases = [
        {
            "id": pid,
            "piece_name": pname,
            "size": sz,
            "base_weight_kg": w,
            "kind": {
                "type": "shield",
                "base_armor_class": ac,
                "base_block_chance_pct": bchance,
                "base_block_value": bvalue,
            },
        }
        for (pid, pname, ac, bchance, bvalue, w, sz) in SHIELD_PIECES
    ]
    write(OUT / "bases" / "shields.yaml", {"bases": bases})
    return len(bases)
