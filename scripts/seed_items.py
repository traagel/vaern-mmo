#!/usr/bin/env python3
"""Seed src/generated/items/ with the compositional item model (Model B).

Entry point only. The real work lives in the `items/` package:

    items/common.py       — paths, write(), damage-type helpers
    items/armor.py        — armor piece shapes (cloth/gambeson/leather/mail/plate)
    items/weapons.py      — weapon shapes by school × grip
    items/shields.py      — shield shapes
    items/runes.py        — rune bases, one per damage channel
    items/consumables.py  — potions, food, elixirs, scrolls
    items/crafting.py     — crafting material + reagent ITEMS (ingots, bolts, herbs)
    items/materials.py    — material DEFINITIONS (substance table w/ stat mods)
    items/qualities.py    — craft-roll quality table (crude → masterful)

Each submodule exposes a `seed() -> int | dict` that writes its slice
and returns a count (or counts-per-family for armor). This file just
wipes the output tree, calls every `seed()`, and prints a summary.

Add content by editing the matching submodule — never hand-author YAML.
"""
from __future__ import annotations

import shutil
import sys
from pathlib import Path

# Allow `from items import ...` when invoked as `python3 scripts/seed_items.py`.
sys.path.insert(0, str(Path(__file__).resolve().parent))

from items import (  # noqa: E402 — intentional, follows the sys.path insert above
    affixes,
    armor,
    consumables,
    crafting,
    materials,
    qualities,
    runes,
    shields,
    weapons,
)
from items.common import OUT, REPO  # noqa: E402


def main() -> None:
    if OUT.exists():
        shutil.rmtree(OUT)
    OUT.mkdir(parents=True)

    armor_counts = armor.seed()
    weapon_n = weapons.seed()
    shield_n = shields.seed()
    rune_n = runes.seed()
    consumable_n = consumables.seed()
    crafting_n = crafting.seed()
    material_n = materials.seed()
    quality_n = qualities.seed()
    affix_n = affixes.seed()

    total_bases = (
        sum(armor_counts.values())
        + weapon_n
        + shield_n
        + rune_n
        + consumable_n
        + crafting_n
    )

    # Rough (base × material × quality) combinatorial count — useful
    # sanity check on variety. Each armor family counts against its
    # valid cloth/leather/gambeson/mail/plate material pool.
    family_material_count = {"cloth": 7, "gambeson": 5, "leather": 6, "mail": 7, "plate": 7}
    combos = 0
    for family, n in armor_counts.items():
        combos += n * family_material_count.get(family, 0) * quality_n
    combos += weapon_n * materials.weapon_eligible_count() * quality_n
    combos += shield_n * materials.shield_eligible_count() * quality_n
    combos += rune_n * quality_n  # runes have no material axis

    print("Seeded composition tables:")
    print("  armor bases per family:")
    for family in sorted(armor_counts):
        print(f"    {family:>9}: {armor_counts[family]}")
    print(f"  weapon bases:        {weapon_n}")
    print(f"  shield bases:        {shield_n}")
    print(f"  rune bases:          {rune_n}")
    print(f"  consumable bases:    {consumable_n}")
    print(f"  material-item bases: {crafting_n}")
    print(f"  material defs:       {material_n}")
    print(f"  quality defs:        {quality_n}")
    print(f"  affix defs:          {affix_n}")
    print( "  ---")
    print(f"  total bases:         {total_bases}")
    print(f"  resolvable combos:   ≈{combos} (base × material × quality)")
    print(f"  output:              {OUT.relative_to(REPO)}/")


if __name__ == "__main__":
    main()
