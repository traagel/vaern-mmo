"""Rune bases — one per damage-type channel, no material axis.

Runes specialize in warding a single damage channel. Quality scales
both resist magnitude AND mp5 drain (higher-quality ward is stronger
but costs more upkeep — the magic-tank trade).
"""
from __future__ import annotations

from .common import OUT, write


# (damage_channel, flavor_name, flavor_text)
RUNE_SCHOOLS = [
    ("slashing",    "Wardbind",    "physical cuts"),
    ("piercing",    "Thornguard",  "piercing thrusts"),
    ("bludgeoning", "Stoneward",   "crushing blows"),
    ("fire",        "Flameguard",  "searing flame"),
    ("cold",        "Frostward",   "biting cold"),
    ("lightning",   "Stormward",   "arcing lightning"),
    ("force",       "Voidward",    "raw kinetic force"),
    ("radiant",     "Duskward",    "searing radiance"),
    ("necrotic",    "Lifeward",    "necrotic withering"),
    ("blood",       "Ichorward",   "blood magic drain"),
    ("poison",      "Toxinward",   "venom and blight"),
    ("acid",        "Aegisward",   "corrosive dissolution"),
]

# Pre-quality magnitudes — quality.stat_mult scales both at resolve time.
BASE_RESIST = 15.0       # per-channel absorb at regular quality
BASE_MP5_DRAIN = -1.5    # mana upkeep at regular quality (negative)


def seed() -> int:
    bases = [
        {
            "id": f"rune_of_{channel}",
            "piece_name": f"Rune of {flavor_name}",
            "size": "tiny",
            "base_weight_kg": 0.2,
            "kind": {
                "type": "rune",
                "school": channel,
                "base_resist": BASE_RESIST,
                "base_mp5_drain": BASE_MP5_DRAIN,
            },
        }
        for (channel, flavor_name, _) in RUNE_SCHOOLS
    ]
    write(OUT / "bases" / "runes.yaml", {"bases": bases})
    return len(bases)
