"""Quality DEFINITIONS — the craft-roll table.

Quality is the orthogonal axis to Material: a crude mithril dagger
(jackpot material, bad craftsman) and a masterful iron dagger (peak
smith, basic metal) are both meaningful outcomes. Loot tables roll
quality separately from material tier.

`stat_mult` multiplies every rolled stat (armor, weapon damage,
resists, block chance/value — everything in SecondaryStats via
fold_base in the resolver).

`rarity_offset` shifts the material's base_rarity up/down. `crude` on
a common-tier material drops to junk; `masterful` on epic-tier stacks
up to legendary (clamped).

Empty `display` = "regular" quality, omitted from the resolved name
("Iron Longsword" rather than "Regular Iron Longsword").
"""
from __future__ import annotations

from .common import OUT, write


# (id, display, stat_mult, rarity_offset)
QUALITIES = [
    ("crude",        "Crude",         0.70, -1),
    ("regular",      "",              1.00,  0),
    ("well_crafted", "Well-Crafted",  1.15,  0),
    ("fine",         "Fine",          1.30,  1),
    ("superior",     "Superior",      1.50,  1),
    ("exceptional",  "Exceptional",   1.75,  2),
    ("masterful",    "Masterful",     2.00,  2),
]


def seed() -> int:
    entries = [
        {
            "id": qid,
            "display": disp,
            "stat_mult": mult,
            "rarity_offset": offset,
        }
        for (qid, disp, mult, offset) in QUALITIES
    ]
    write(OUT / "qualities.yaml", {"qualities": entries})
    return len(entries)
