"""Armor base shapes — one entry per (armor_type, slot, piece) combo.

Material-agnostic: `base_armor_class` and `base_weight_kg` are the
pre-material values, multiplied at resolve time by Material.ac_mult /
weight_mult. The resolver also scales resists off ArmorType.
base_resist_profile() × Material.ac_mult × Quality.stat_mult, then
adds Material.resist_adds flat.

Layer assignment follows the KCD-phased model:
  cloth    → under (shirts) OR padding (robes) OR over (cloaks)
  gambeson → padding (arming garment)
  leather  → padding (jerkin) OR over (cloaks)
  mail     → chain
  plate    → plate

Edit the ARMOR_PIECES table to add/remove piece shapes. Scale tables
(SLOT_AC_SHARE, FAMILY_BASE_AC, etc.) control the baseline magnitudes.
"""
from __future__ import annotations

from .common import OUT, write


# Per-slot armor-class share relative to chest = 1.0. Scaling helmets,
# boots, etc. down from the chest baseline.
SLOT_AC_SHARE = {
    "head":      0.40,
    "neck":      0.10,
    "shoulders": 0.55,
    "chest":     1.00,
    "back":      0.30,
    "shirt":     0.15,   # Under-layer linen — near-token AC
    "wrists":    0.35,
    "hands":     0.45,
    "waist":     0.30,
    "legs":      0.80,
    "feet":      0.45,
}

# ArmorType-family AC on chest slot. Materials further multiply this.
FAMILY_BASE_AC = {
    "cloth":    2.0,
    "gambeson": 5.0,
    "leather":  7.0,
    "mail":     10.0,
    "plate":    14.0,
}

# ArmorType-family weight on chest (kg); slots scale down from here.
FAMILY_BASE_WEIGHT = {
    "cloth":    2.0,
    "gambeson": 4.0,
    "leather":  5.0,
    "mail":     10.0,
    "plate":    14.0,
}
SLOT_WEIGHT_SHARE = {
    "head":      0.18,
    "neck":      0.03,
    "shoulders": 0.28,
    "chest":     1.00,
    "back":      0.20,
    "shirt":     0.10,
    "wrists":    0.08,
    "hands":     0.12,
    "waist":     0.08,
    "legs":      0.40,
    "feet":      0.18,
}


# (family, layer, slot, piece_id, piece_name, coverage, size)
ARMOR_PIECES = [
    # --- Under-layer cloth: generic shirt (material = linen / silk / etc.
    # decides the fabric; "Shirt" stays the piece noun so composition
    # produces "Linen Shirt", "Silk Shirt" rather than "Linen Linen Shirt").
    ("cloth", "under", "shirt", "shirt", "Shirt", ["chest", "arms"], "small"),

    # --- Cloth padding: mage's main garment ---
    ("cloth", "padding", "head",      "cowl",     "Cowl",     ["head"],                      "small"),
    ("cloth", "padding", "shoulders", "mantle",   "Mantle",   ["shoulders"],                 "small"),
    ("cloth", "padding", "chest",     "robe",     "Robe",     ["chest", "arms", "legs"],     "medium"),
    ("cloth", "padding", "wrists",    "cuffs",    "Cuffs",    ["arms"],                      "small"),
    ("cloth", "padding", "hands",     "gloves",   "Gloves",   ["hands"],                     "small"),
    ("cloth", "padding", "waist",     "sash",     "Sash",     ["waist"],                     "small"),
    ("cloth", "padding", "legs",      "trousers", "Trousers", ["legs"],                      "medium"),
    ("cloth", "padding", "feet",      "slippers", "Slippers", ["feet"],                      "small"),

    # --- Cloth over-layer cloak ---
    ("cloth", "over", "back", "cloak", "Cloak", ["shoulders", "chest"], "medium"),

    # --- Gambeson padding: fighter's arming garment ---
    ("gambeson", "padding", "head",      "arming_cap",       "Arming Cap",       ["head"],            "small"),
    ("gambeson", "padding", "shoulders", "arming_pauldrons", "Arming Pauldrons", ["shoulders"],       "medium"),
    ("gambeson", "padding", "chest",     "gambeson",         "Gambeson",         ["chest", "arms"],   "medium"),
    ("gambeson", "padding", "wrists",    "bracers",          "Arming Bracers",   ["arms"],            "small"),
    ("gambeson", "padding", "hands",     "mitts",            "Mitts",            ["hands"],           "small"),
    ("gambeson", "padding", "waist",     "belt",             "Arming Belt",      ["waist"],           "small"),
    ("gambeson", "padding", "legs",      "breeches",         "Breeches",         ["legs"],            "medium"),
    ("gambeson", "padding", "feet",      "shoes",            "Padded Shoes",     ["feet"],            "small"),

    # --- Leather padding: light-fighter jerkin family ---
    ("leather", "padding", "head",      "hood",           "Hood",           ["head"],      "small"),
    ("leather", "padding", "shoulders", "shoulder_guard", "Shoulder Guard", ["shoulders"], "medium"),
    ("leather", "padding", "chest",     "jerkin",         "Jerkin",         ["chest"],     "medium"),
    ("leather", "padding", "wrists",    "bracers",        "Bracers",        ["arms"],      "small"),
    ("leather", "padding", "hands",     "gloves",         "Gloves",         ["hands"],     "small"),
    ("leather", "padding", "waist",     "belt",           "Belt",           ["waist"],     "small"),
    ("leather", "padding", "legs",      "leggings",       "Leggings",       ["legs"],      "medium"),
    ("leather", "padding", "feet",      "boots",          "Boots",          ["feet"],      "small"),

    # --- Leather over-layer cloak ---
    ("leather", "over", "back", "cloak", "Cloak", ["shoulders", "chest"], "medium"),

    # --- Mail chain-layer hauberk family ---
    ("mail", "chain", "head",      "coif",      "Coif",      ["head"],            "small"),
    ("mail", "chain", "shoulders", "spaulders", "Spaulders", ["shoulders"],       "medium"),
    ("mail", "chain", "chest",     "hauberk",   "Hauberk",   ["chest", "arms"],   "large"),
    ("mail", "chain", "wrists",    "vambraces", "Vambraces", ["arms"],            "small"),
    ("mail", "chain", "hands",     "gauntlets", "Gauntlets", ["hands"],           "small"),
    ("mail", "chain", "waist",     "belt",      "Mail Belt", ["waist"],           "small"),
    ("mail", "chain", "legs",      "chausses",  "Chausses",  ["legs"],            "medium"),
    ("mail", "chain", "feet",      "sabatons",  "Sabatons",  ["feet"],            "medium"),

    # --- Plate plate-layer harness family ---
    ("plate", "plate", "head",      "helm",        "Helm",        ["head"],      "medium"),
    ("plate", "plate", "shoulders", "pauldrons",   "Pauldrons",   ["shoulders"], "medium"),
    ("plate", "plate", "chest",     "breastplate", "Breastplate", ["chest"],     "large"),
    ("plate", "plate", "wrists",    "vambraces",   "Vambraces",   ["arms"],      "small"),
    ("plate", "plate", "hands",     "gauntlets",   "Gauntlets",   ["hands"],     "small"),
    ("plate", "plate", "waist",     "girdle",      "Girdle",      ["waist"],     "small"),
    ("plate", "plate", "legs",      "greaves",     "Greaves",     ["legs"],      "medium"),
    ("plate", "plate", "feet",      "sabatons",    "Sabatons",    ["feet"],      "medium"),
]


def _armor_base(family, layer, slot, piece_id, piece_name, coverage, size) -> dict:
    base_ac = FAMILY_BASE_AC[family] * SLOT_AC_SHARE[slot]
    base_weight = FAMILY_BASE_WEIGHT[family] * SLOT_WEIGHT_SHARE[slot]
    return {
        "id": f"{family}_{piece_id}",
        "piece_name": piece_name,
        "size": size,
        "base_weight_kg": round(base_weight, 2),
        "kind": {
            "type": "armor",
            "slot": slot,
            "armor_type": family,
            "layer": layer,
            "coverage": coverage,
            "base_armor_class": round(base_ac, 2),
        },
    }


def seed() -> dict[str, int]:
    """Write armor bases grouped by family. Returns counts per family."""
    by_family: dict[str, list] = {}
    for (family, layer, slot, piece_id, piece_name, coverage, size) in ARMOR_PIECES:
        by_family.setdefault(family, []).append(
            _armor_base(family, layer, slot, piece_id, piece_name, coverage, size)
        )
    counts = {}
    for family, bases in by_family.items():
        write(OUT / "bases" / "armor" / f"{family}.yaml", {"bases": bases})
        counts[family] = len(bases)
    return counts
