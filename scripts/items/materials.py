"""Material DEFINITIONS — the substance table (how metals/leathers/
cloths/gambesons modulate weapon + armor stats at resolve time).

Distinct from crafting.py (which seeds *item* bases like iron_ingot).
A Material here is a stat-modifier with `valid_for` gates. The
`resist_adds` field carries material-specific mechanical effects
without needing any new combat code — silver adds necrotic+radiant
resist, dragonscale adds massive fire resist, shadowsilk trades
radiant for necrotic.

Columns (consistent across all four tables):
    (id, display, tier, weight_mult, ac_mult, dmg_mult,
     resist_adds, valid_for, weapon_eligible, shield_eligible,
     base_rarity)

`tier` is used by loot tables to gate drops by encounter level.
"""
from __future__ import annotations

from .common import OUT, resist_adds, write, zeros12


# ---- Metals + alloys (plate, mail, weapons, shields) ----

METALS_AND_ALLOYS = [
    ("copper", "Copper", 1, 1.1,  0.7,  0.7,
        zeros12(),
        ["plate", "mail"], True, True, "common"),
    ("bronze", "Bronze", 2, 0.95, 0.85, 0.85,
        zeros12(),
        ["plate", "mail"], True, True, "common"),
    ("iron",   "Iron",   3, 1.0,  1.0,  1.0,
        zeros12(),
        ["plate", "mail"], True, True, "common"),
    ("steel",  "Steel",  4, 1.0,  1.25, 1.15,
        zeros12(),
        ["plate", "mail"], True, True, "uncommon"),
    # Silver is anti-undead / anti-dark. Soft metal → weapon dmg penalty.
    ("silver", "Silver", 4, 1.1,  0.9,  0.8,
        resist_adds(radiant=3.0, necrotic=5.0),
        ["plate", "mail"], True, True, "uncommon"),
    # Mithril — light + magically resonant; broad minor magic resist.
    ("mithril", "Mithril", 5, 0.6, 1.5, 1.3,
        resist_adds(fire=0.5, cold=0.5, lightning=0.5, force=0.5,
                    radiant=0.5, necrotic=0.5, blood=0.5, poison=0.5, acid=0.5),
        ["plate", "mail"], True, True, "rare"),
    # Adamantine — physical cap. Heavy but monstrous stats.
    ("adamantine", "Adamantine", 6, 1.2, 2.0, 1.5,
        resist_adds(slashing=5.0, piercing=5.0, bludgeoning=5.0),
        ["plate", "mail"], True, True, "epic"),
]


# ---- Leathers ----

LEATHERS = [
    ("boarhide", "Boarhide", 1, 1.0, 0.7, 0.7,
        zeros12(),
        ["leather"], True, False, "common"),
    ("deerhide", "Deerhide", 2, 0.9, 0.85, 0.85,
        zeros12(),
        ["leather"], False, False, "common"),
    ("bearhide", "Bearhide", 3, 1.1, 1.0, 1.0,
        resist_adds(cold=2.0),
        ["leather"], False, False, "common"),
    ("ironhide", "Ironhide", 4, 1.0, 1.3, 1.1,
        resist_adds(bludgeoning=2.0),
        ["leather"], False, False, "uncommon"),
    ("wyvern", "Wyvern Leather", 5, 0.85, 1.5, 1.2,
        resist_adds(poison=4.0, acid=2.0),
        ["leather"], False, False, "rare"),
    # Dragonscale — fantasy pinnacle. Massive fire resist, solid physical.
    ("dragonscale", "Dragonscale", 6, 0.9, 1.8, 1.3,
        resist_adds(fire=10.0, slashing=2.0, piercing=2.0),
        ["leather"], False, False, "epic"),
]


# ---- Gambeson paddings (arming garment variants) ----

GAMBESONS = [
    ("linen_padding", "Linen-Padded", 1, 1.0, 0.8, 0.0,
        zeros12(),
        ["gambeson"], False, False, "common"),
    ("wool_padding", "Wool-Padded", 2, 1.1, 1.0, 0.0,
        resist_adds(cold=1.5),
        ["gambeson"], False, False, "common"),
    ("silk_padding", "Silk-Padded", 4, 0.9, 1.15, 0.0,
        resist_adds(fire=1.0, cold=1.0),
        ["gambeson"], False, False, "uncommon"),
    ("hardened_linen", "Hardened Linen", 3, 1.1, 1.2, 0.0,
        zeros12(),
        ["gambeson"], False, False, "common"),
    ("mageweave_padding", "Mageweave-Padded", 5, 0.85, 1.3, 0.0,
        resist_adds(fire=2.0, cold=2.0, lightning=2.0, force=2.0),
        ["gambeson"], False, False, "rare"),
]


# ---- Cloths (mage robes + undergarments + cloaks) ----

CLOTHS = [
    ("linen", "Linen", 1, 1.0, 0.7, 0.6,
        zeros12(),
        ["cloth"], False, False, "common"),
    ("wool", "Wool", 2, 1.1, 0.85, 0.7,
        resist_adds(cold=2.0),
        ["cloth"], False, False, "common"),
    ("cotton", "Cotton", 2, 1.0, 0.9, 0.7,
        zeros12(),
        ["cloth"], False, False, "common"),
    ("silk", "Silk", 4, 0.7, 1.1, 0.8,
        zeros12(),
        ["cloth"], False, False, "uncommon"),
    # Mageweave — general magical amplification.
    ("mageweave", "Mageweave", 5, 0.8, 1.3, 0.9,
        resist_adds(fire=2.0, cold=2.0, lightning=2.0, force=2.0,
                    radiant=2.0, necrotic=2.0, blood=2.0, poison=2.0, acid=2.0),
        ["cloth"], False, False, "rare"),
    # Shadowsilk — dark arts caster. Bonus necrotic/blood/poison, radiant PENALTY.
    ("shadowsilk", "Shadowsilk", 6, 0.7, 1.5, 0.9,
        resist_adds(necrotic=6.0, blood=6.0, poison=4.0, radiant=-3.0),
        ["cloth"], False, False, "epic"),
    # Voidcloth — endgame caster. Universal magical boost, physical weak.
    ("voidcloth", "Voidcloth", 7, 0.6, 1.6, 1.0,
        resist_adds(fire=3.0, cold=3.0, lightning=3.0, force=3.0,
                    radiant=3.0, necrotic=3.0, blood=3.0, poison=3.0, acid=3.0),
        ["cloth"], False, False, "legendary"),
]


ALL_TABLES = [METALS_AND_ALLOYS, LEATHERS, GAMBESONS, CLOTHS]


def seed() -> int:
    """Write every material def into `materials.yaml`."""
    entries = []
    for table in ALL_TABLES:
        for (mid, disp, tier, w, ac, dmg, adds, valid_for,
             weap, shield, rarity) in table:
            entries.append({
                "id": mid,
                "display": disp,
                "tier": tier,
                "weight_mult": w,
                "ac_mult": ac,
                "dmg_mult": dmg,
                "resist_adds": adds,
                "valid_for": valid_for,
                "weapon_eligible": weap,
                "shield_eligible": shield,
                "base_rarity": rarity,
            })
    write(OUT / "materials.yaml", {"materials": entries})
    return len(entries)


def weapon_eligible_count() -> int:
    """Used by the orchestrator's combo count — how many materials
    forge into weapons? Keep here so adding a new weapon-eligible
    material auto-updates the printout."""
    return sum(
        1
        for table in ALL_TABLES
        for row in table
        if row[8]  # weapon_eligible
    )


def shield_eligible_count() -> int:
    return sum(
        1
        for table in ALL_TABLES
        for row in table
        if row[9]  # shield_eligible
    )
