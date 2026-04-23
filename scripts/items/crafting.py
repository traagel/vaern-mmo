"""Crafting material + reagent item bases (iron_ingot, silk_bolt, ...).

These are the ITEMS of type `material` / `reagent` — distinct from the
Material DEFINITIONS in materials.py that modulate weapon/armor stats.
Unfortunate naming collision: here "material" = stackable crafting
input (physical ingot in your inventory); over there = "steel as a
substance reshapes stat rolls".

Both coexist; loot tables reference both ends (drop an iron_ingot AND
drop an item composed of Material("iron")).
"""
from __future__ import annotations

from .common import OUT, write


METAL_STOCK = [
    ("copper_ingot",     "Copper Ingot"),
    ("bronze_ingot",     "Bronze Ingot"),
    ("iron_ingot",       "Iron Ingot"),
    ("steel_ingot",      "Steel Ingot"),
    ("silver_ingot",     "Silver Ingot"),
    ("mithril_ingot",    "Mithril Ingot"),
    ("adamantine_ingot", "Adamantine Ingot"),
]
LEATHER_STOCK = [
    ("rawhide",          "Rawhide"),
    ("light_leather",    "Light Leather"),
    ("tough_leather",    "Tough Leather"),
    ("hardened_leather", "Hardened Leather"),
    ("wyvern_leather",   "Wyvern Leather"),
    ("dragonscale",      "Dragonscale"),
]
CLOTH_STOCK = [
    ("linen_bolt",      "Linen Bolt"),
    ("wool_bolt",       "Wool Bolt"),
    ("silk_bolt",       "Silk Bolt"),
    ("mageweave_bolt",  "Mageweave Bolt"),
    ("shadowsilk_bolt", "Shadowsilk Bolt"),
]
HERBS = [
    ("stanchweed",  "Stanchweed"),
    ("sunleaf",     "Sunleaf"),
    ("blightroot",  "Blightroot"),
    ("silverfrond", "Silverfrond"),
    ("emberbloom",  "Emberbloom"),
    ("ghostcap",    "Ghostcap"),
]
WOODS = [
    ("pine_plank",     "Pine Plank"),
    ("oak_plank",      "Oak Plank"),
    ("yew_plank",      "Yew Plank"),
    ("ironwood_plank", "Ironwood Plank"),
]
GEMS = [
    ("rough_quartz", "Rough Quartz"),
    ("cut_quartz",   "Cut Quartz"),
    ("amber",        "Amber"),
    ("sapphire",     "Sapphire"),
    ("ruby",         "Ruby"),
    ("diamond",      "Diamond"),
    ("soul_gem",     "Soul Gem"),
]
REAGENTS = [
    ("mana_dust",         "Mana Dust"),
    ("spirit_shard",      "Spirit Shard"),
    ("essence_of_fire",   "Essence of Fire"),
    ("essence_of_cold",   "Essence of Cold"),
    ("essence_of_storm",  "Essence of Storm"),
    ("essence_of_shadow", "Essence of Shadow"),
    ("essence_of_light",  "Essence of Light"),
]


def _material_item(mid: str, name: str, weight: float, size: str) -> dict:
    return {
        "id": mid,
        "piece_name": name,
        "size": size,
        "base_weight_kg": weight,
        "stackable": True,
        "stack_max": 100,
        "kind": {"type": "material"},
    }


def _reagent_item(rid: str, name: str) -> dict:
    return {
        "id": rid,
        "piece_name": name,
        "size": "tiny",
        "base_weight_kg": 0.05,
        "stackable": True,
        "stack_max": 50,
        "kind": {"type": "reagent"},
    }


def seed() -> int:
    bases = []
    for mid, name in METAL_STOCK:
        bases.append(_material_item(mid, name, 1.0, "small"))
    for mid, name in LEATHER_STOCK:
        bases.append(_material_item(mid, name, 0.5, "small"))
    for mid, name in CLOTH_STOCK:
        bases.append(_material_item(mid, name, 0.4, "small"))
    for mid, name in HERBS:
        bases.append(_material_item(mid, name, 0.05, "tiny"))
    for mid, name in WOODS:
        bases.append(_material_item(mid, name, 2.0, "medium"))
    for mid, name in GEMS:
        bases.append(_material_item(mid, name, 0.05, "tiny"))
    for rid, name in REAGENTS:
        bases.append(_reagent_item(rid, name))
    write(OUT / "bases" / "materials.yaml", {"bases": bases})
    return len(bases)
