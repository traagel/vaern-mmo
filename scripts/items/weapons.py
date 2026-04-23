"""Weapon base shapes — one per (school, variant, grip) combination.

The `school` column must match a real leaf-name in
`src/generated/schools/{might,finesse,arcana}/<name>.yaml`. Pillar is
looked up from the school's `pillar:` field at load time, so schools
live under `might/` or `finesse/` or `arcana/` implicitly.

`base_min_dmg` / `base_max_dmg` are the pre-material weapon damage
range; resolver multiplies by Material.dmg_mult × Quality.stat_mult.

Add new weapon variants by appending rows to WEAPON_PIECES. Keep IDs
snake_case and unique; display names Title Case.
"""
from __future__ import annotations

from .common import OUT, write


# (id, piece_name, school, grip, base_min_dmg, base_max_dmg, weight_kg, size)
WEAPON_PIECES = [
    # --- might/blade — slashing martial (swords + axes + curved blades share
    # this school since all are primarily slashing; class abilities for "blade"
    # apply to the whole family). Rapier/short_sword are piercing IRL but
    # live here for proficiency — accepted fidelity loss until per-weapon
    # damage_type override lands.
    # Swords:
    ("sword",         "Sword",         "blade", "one_handed", 4.0,  8.0, 1.8, "medium"),
    ("short_sword",   "Short Sword",   "blade", "light",      3.0,  6.0, 1.5, "small"),
    ("rapier",        "Rapier",        "blade", "one_handed", 3.0,  7.0, 1.0, "medium"),
    ("scimitar",      "Scimitar",      "blade", "one_handed", 4.0,  7.0, 1.8, "medium"),
    ("falchion",      "Falchion",      "blade", "one_handed", 5.0,  9.0, 2.2, "medium"),
    ("bastard_sword", "Bastard Sword", "blade", "one_handed", 5.0, 10.0, 3.0, "medium"),
    ("longsword",     "Longsword",     "blade", "two_handed", 7.0, 12.0, 3.5, "large"),
    ("greatsword",    "Greatsword",    "blade", "two_handed", 9.0, 15.0, 4.5, "large"),
    # Short curved blades (crit-focused feel — 18-20 crit range in D&D 3.5):
    ("kukri",         "Kukri",         "blade", "light",      2.0,  5.0, 0.6, "small"),
    # Axes:
    ("handaxe",       "Handaxe",       "blade", "light",      3.0,  6.0, 1.5, "small"),
    ("battleaxe",     "Battleaxe",     "blade", "one_handed", 4.0,  8.0, 2.8, "medium"),
    ("waraxe",        "Waraxe",        "blade", "one_handed", 5.0,  9.0, 3.5, "medium"),
    ("greataxe",      "Greataxe",      "blade", "two_handed", 9.0, 16.0, 5.0, "large"),
    # Exotic 2h slashers:
    ("scythe",        "Scythe",        "blade", "two_handed", 5.0, 12.0, 4.5, "large"),

    # --- might/blunt — bludgeoning (maces, clubs, hammers, flails,
    # quarterstaff). Morningstar is piercing+bludgeoning hybrid in 3.5
    # but simplifies to bludgeoning here.
    ("club",          "Club",          "blunt", "light",       2.0,  5.0, 1.2, "small"),
    ("sap",           "Sap",           "blunt", "light",       2.0,  4.0, 0.5, "small"),
    ("light_hammer",  "Light Hammer",  "blunt", "light",       2.0,  5.0, 1.0, "small"),
    ("light_mace",    "Light Mace",    "blunt", "light",       3.0,  6.0, 1.8, "small"),
    ("mace",          "Mace",          "blunt", "one_handed",  5.0,  9.0, 2.5, "medium"),
    ("morningstar",   "Morningstar",   "blunt", "one_handed",  4.0,  8.0, 2.8, "medium"),
    ("flail",         "Flail",         "blunt", "one_handed",  4.0, 10.0, 2.8, "medium"),
    ("warhammer",     "Warhammer",     "blunt", "two_handed",  8.0, 14.0, 5.0, "large"),
    ("greatclub",     "Greatclub",     "blunt", "two_handed",  5.0, 11.0, 3.5, "large"),
    ("heavy_flail",   "Heavy Flail",   "blunt", "two_handed",  5.0, 12.0, 4.5, "large"),
    ("maul",          "Maul",          "blunt", "two_handed", 10.0, 16.0, 6.0, "large"),
    ("quarterstaff",  "Quarterstaff",  "blunt", "two_handed",  4.0,  8.0, 2.5, "large"),

    # --- might/spear — piercing polearms. Reach polearms (longspear,
    # glaive, guisarme, ranseur) belong here in 3.5; reach tactical
    # metadata comes later via a weapon flag.
    ("shortspear", "Shortspear", "spear", "one_handed", 4.0,  7.0, 2.0, "medium"),
    ("trident",    "Trident",    "spear", "one_handed", 4.0,  9.0, 2.5, "medium"),
    ("longspear",  "Longspear",  "spear", "two_handed", 5.0, 10.0, 3.0, "large"),
    ("pike",       "Pike",       "spear", "two_handed", 6.0, 11.0, 4.0, "large"),
    ("glaive",     "Glaive",     "spear", "two_handed", 6.0, 12.0, 4.5, "large"),
    ("guisarme",   "Guisarme",   "spear", "two_handed", 5.0, 11.0, 5.0, "large"),
    ("ranseur",    "Ranseur",    "spear", "two_handed", 5.0, 11.0, 5.0, "large"),
    ("halberd",    "Halberd",    "spear", "two_handed", 8.0, 13.0, 5.5, "large"),

    # --- finesse/dagger — its own school under the finesse pillar ---
    ("dagger", "Dagger", "dagger", "light", 2.0, 4.0, 0.6, "small"),

    # --- finesse/bow. Composite variants scale with strength in 3.5;
    # here they just roll higher damage ranges than their plain cousins.
    ("shortbow",           "Shortbow",           "bow", "two_handed", 4.0,  8.0, 1.5, "medium"),
    ("composite_shortbow", "Composite Shortbow", "bow", "two_handed", 5.0, 10.0, 1.5, "medium"),
    ("longbow",            "Longbow",            "bow", "two_handed", 6.0, 10.0, 2.0, "large"),
    ("composite_longbow",  "Composite Longbow",  "bow", "two_handed", 7.0, 12.0, 2.5, "large"),

    # --- finesse/crossbow. Hand crossbow is the rogue/assassin's concealed
    # tool (light exotic); repeating crossbow is the exotic fast-fire.
    ("hand_crossbow",      "Hand Crossbow",      "crossbow", "light",      2.0,  5.0, 1.0, "small"),
    ("light_crossbow",     "Light Crossbow",     "crossbow", "two_handed", 5.0, 10.0, 3.5, "medium"),
    ("heavy_crossbow",     "Heavy Crossbow",     "crossbow", "two_handed", 8.0, 14.0, 5.0, "large"),
    ("repeating_crossbow", "Repeating Crossbow", "crossbow", "two_handed", 4.0,  9.0, 5.5, "large"),

    # --- finesse/thrown — ranged throwable weapons.
    # School's damage_type is piercing, so darts/javelins/shuriken fit
    # cleanly. Throwing axe / throwing dagger live here for ability-
    # proficiency (thrown-school abilities work on them) even though
    # their "real" damage type would be slashing/piercing respectively.
    ("dart",             "Dart",             "thrown", "light",      2.0, 4.0, 0.2, "tiny"),
    ("throwing_dagger",  "Throwing Dagger",  "thrown", "light",      2.0, 5.0, 0.3, "small"),
    ("shuriken",         "Shuriken",         "thrown", "light",      1.0, 3.0, 0.05,"tiny"),
    ("javelin",          "Javelin",          "thrown", "one_handed", 3.0, 6.0, 1.0, "medium"),
    ("throwing_axe",     "Throwing Axe",     "thrown", "one_handed", 3.0, 7.0, 0.8, "small"),
    ("bolas",            "Bolas",            "thrown", "one_handed", 2.0, 4.0, 0.5, "small"),
    ("net",              "Net",              "thrown", "one_handed", 1.0, 3.0, 1.5, "medium"),

    # --- might/unarmed — brawler + monk weapons.
    # All monk weapons share unarmed school so the monk class's
    # ability kit applies uniformly. Accepted tradeoff: kama (slashing
    # IRL) and sai/siangham (piercing IRL) read as bludgeoning from
    # the school — fix later with a per-weapon damage_type override.
    ("gauntlet",         "Gauntlet",         "unarmed", "light", 1.0, 3.0, 0.8, "small"),
    ("spiked_gauntlet",  "Spiked Gauntlet",  "unarmed", "light", 2.0, 4.0, 1.0, "small"),
    ("cestus",           "Cestus",           "unarmed", "light", 2.0, 4.0, 0.6, "small"),
    ("knuckledusters",   "Knuckledusters",   "unarmed", "light", 2.0, 4.0, 0.4, "tiny"),
    ("kama",             "Kama",             "unarmed", "light", 3.0, 6.0, 1.0, "small"),
    ("nunchaku",         "Nunchaku",         "unarmed", "light", 3.0, 6.0, 0.8, "small"),
    ("sai",              "Sai",              "unarmed", "light", 2.0, 5.0, 0.6, "small"),
    ("siangham",         "Siangham",         "unarmed", "light", 3.0, 6.0, 0.5, "small"),

    # --- arcana/arcane — caster foci. Low melee dmg; future spell_power
    # stat will roll on these. Generic ("arcane") for now; elemental variants
    # (firestaff → arcana/fire, frostorb → arcana/frost) come later.
    ("wand",      "Wand",      "arcane", "light",      2.0, 5.0, 0.4, "small"),
    ("scepter",   "Scepter",   "arcane", "one_handed", 3.0, 7.0, 1.5, "medium"),
    ("orb",       "Orb",       "arcane", "light",      2.0, 6.0, 0.8, "small"),
    ("runestaff", "Runestaff", "arcane", "two_handed", 3.0, 7.0, 2.5, "large"),
]


def seed() -> int:
    bases = [
        {
            "id": pid,
            "piece_name": pname,
            "size": sz,
            "base_weight_kg": w,
            "kind": {
                "type": "weapon",
                "grip": grip,
                "school": school,
                "base_min_dmg": mn,
                "base_max_dmg": mx,
            },
        }
        for (pid, pname, school, grip, mn, mx, w, sz) in WEAPON_PIECES
    ]
    write(OUT / "bases" / "weapons.yaml", {"bases": bases})
    return len(bases)
