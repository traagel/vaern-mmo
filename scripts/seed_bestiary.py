#!/usr/bin/env python3
"""Seed src/generated/bestiary/ with canonical creature-type + armor-class data.

Every mob inherits from a creature_type (beast, humanoid, undead, demon, ...)
and an armor_class (hide, plate, cloth, ethereal, ...). The type defines
HP scaling, default resistances, school affinities, and behavior defaults;
the armor_class defines physical/magic damage reduction and material
strengths/weaknesses.

Mobs reference these by id. Overrides go in the mob's own `override:` block.
"""
from __future__ import annotations

from pathlib import Path
import yaml

REPO = Path(__file__).resolve().parents[1]
OUT = REPO / "src" / "generated" / "bestiary"


def write(path: Path, data: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "w") as f:
        yaml.safe_dump(data, f, sort_keys=False, default_flow_style=False)


# ---------------------------------------------------------------------------
# Creature types — broad biological / magical categories
# ---------------------------------------------------------------------------
# hp_scaling: base_at_1 and per_level_mul define geometric growth.
#   hp(L) = round(base_at_1 * per_level_mul^(L-1))
#   At L1 a beast starts at 30; L60 standard-tier beast ≈ 30 * 1.15^59 ≈ 131,000
#   Per-rarity multipliers (standard/elite/heroic) apply on top of type base.
#
# resistances / weaknesses: school_id -> damage_modifier (negative = resist)
#   -0.25 means takes 25% LESS damage; +0.25 means takes 25% MORE.
#
# affinities.preferred: schools this type typically uses for attacks.
# affinities.forbidden: schools it cannot use at all (cosmetic constraint for
#   picking a mob's primary_school — prevents a bandit humanoid from being a
#   "light-devotion skirmisher", for example).

CREATURE_TYPES = [
    {
        "id": "beast",
        "name": "Beast",
        "category": "natural",
        "description": "Natural fauna — bears, wolves, boars, deer, raptors, seals, swarms. Organic, fleshy, mortal.",
        "hp_scaling": {
            "base_at_level_1": 30,
            "per_level_multiplier": 1.15,
            "formula": "round(base_at_1 * multiplier^(level-1))",
        },
        "default_armor_class": "hide",
        "resistances": {
            # beasts are just meat — minor natural-world toughness, weak to flame
            "nature": -0.15,
            "fire": 0.15,
        },
        "affinities": {
            "preferred": ["blade", "unarmed"],  # claws and fangs count as these
            "allowed": ["blunt"],
            "forbidden": [
                "arcane", "fire", "frost", "lightning", "earth", "light",
                "devotion", "shadow", "blood",
                "honor", "fury",
                "poison", "tonics", "alchemy", "trickster",
            ],
        },
        "behavior_defaults": {
            "intelligence": "low",
            "social": "pack",
            "flee_threshold": 0.25,
            "aggro_range": "medium",
        },
        "tags": ["flesh", "organic", "wild"],
    },
    {
        "id": "dragonkin",
        "name": "Dragonkin",
        "category": "natural",
        "description": "Drakes, wyrms, cinder-drakes, serpent-kin. Scaled, breath-weapon lineage, apex predators.",
        "hp_scaling": {
            "base_at_level_1": 55,
            "per_level_multiplier": 1.17,
            "formula": "round(base_at_1 * multiplier^(level-1))",
        },
        "default_armor_class": "scales",
        "resistances": {
            "fire": -0.50,     # fire-line dragonkin take half fire damage
            "frost": -0.25,    # cold-adapted lineages
            "nature": -0.10,
            "blade": -0.15,    # scales turn blades
            "light": 0.10,
        },
        "affinities": {
            "preferred": ["fire", "frost", "blade"],
            "allowed": ["lightning", "blunt", "spear"],
            "forbidden": [
                "arcane", "devotion", "light",
                "honor", "fury",
                "bow", "dagger", "thrown", "acrobat",
                "silent", "trickster", "alchemy", "tonics", "poison",
                "shadow", "blood", "earth",
            ],
        },
        "behavior_defaults": {
            "intelligence": "standard",
            "social": "solitary",
            "flee_threshold": 0.05,
            "aggro_range": "long",
        },
        "tags": ["scaled", "breath-weapon", "apex"],
    },
    {
        "id": "humanoid",
        "name": "Humanoid",
        "category": "natural",
        "description": "Sentient bipeds — bandits, cultists, raiders, guards, merchants-gone-wrong. Use full school access per faction morality.",
        "hp_scaling": {
            "base_at_level_1": 35,
            "per_level_multiplier": 1.14,
            "formula": "round(base_at_1 * multiplier^(level-1))",
        },
        "default_armor_class": "leather",
        "resistances": {},   # humanoids have no racial resistance baseline
        "affinities": {
            "preferred": [],  # all schools allowed; per-mob override drives school
            "allowed": [
                "arcane", "fire", "frost", "lightning", "earth", "nature",
                "light", "devotion", "shadow", "blood",
                "blade", "blunt", "spear", "shield", "unarmed", "honor", "fury",
                "bow", "dagger", "thrown", "acrobat", "silent", "trickster",
                "alchemy", "tonics", "poison",
            ],
            "forbidden": [],
        },
        "behavior_defaults": {
            "intelligence": "high",
            "social": "warband",
            "flee_threshold": 0.20,
            "aggro_range": "medium",
        },
        "tags": ["sapient", "social", "tool-user"],
    },
    {
        "id": "undead",
        "name": "Undead",
        "category": "unnatural",
        "description": "Revenants, wraiths, wights, shades, bone-bound shells. Animated corpses, ghost-matter, or both.",
        "hp_scaling": {
            "base_at_level_1": 40,
            "per_level_multiplier": 1.14,
            "formula": "round(base_at_1 * multiplier^(level-1))",
        },
        "default_armor_class": "bone",
        "resistances": {
            "shadow": -0.50,
            "blood": -0.30,
            "frost": -0.25,    # dead don't feel cold
            "poison": -0.80,   # dead can't be poisoned meaningfully
            "nature": -0.15,
            "light": 0.40,     # HOLY hits undead hard — classic anti-undead damage
            "devotion": 0.40,
            "fire": 0.20,      # fire burns the bindings
        },
        "affinities": {
            "preferred": ["shadow", "blood", "frost"],
            "allowed": ["arcane", "blade", "blunt", "dagger"],
            "forbidden": [
                "fire", "nature", "earth", "lightning",
                "light", "devotion",
                "honor",
                "tonics", "alchemy",
                "poison",  # undead using poison is cosmetically wrong
            ],
        },
        "behavior_defaults": {
            "intelligence": "low",
            "social": "swarm",
            "flee_threshold": 0.0,   # undead do not flee
            "aggro_range": "long",
        },
        "tags": ["undead", "incorporeal-kin", "rite-bound"],
    },
    {
        "id": "demon",
        "name": "Demon",
        "category": "unnatural",
        "description": "Extraplanar bound-things, Old-Power petitioners, pact-called entities. Almost exclusively Rend-affiliated or hostile-to-all.",
        "hp_scaling": {
            "base_at_level_1": 50,
            "per_level_multiplier": 1.16,
            "formula": "round(base_at_1 * multiplier^(level-1))",
        },
        "default_armor_class": "scales",
        "resistances": {
            "fire": -0.40,
            "shadow": -0.50,
            "blood": -0.40,
            "poison": -0.50,
            "light": 0.50,     # light hurts demons more than almost anything
            "devotion": 0.50,
            "nature": 0.10,
        },
        "affinities": {
            "preferred": ["fire", "shadow", "blood"],
            "allowed": ["arcane", "lightning", "dagger", "unarmed"],
            "forbidden": [
                "nature", "earth", "frost",
                "light", "devotion",
                "honor",
                "tonics", "alchemy", "silent",
                "bow", "thrown",
                "spear", "shield", "blade", "blunt",
            ],
        },
        "behavior_defaults": {
            "intelligence": "high",
            "social": "solitary",
            "flee_threshold": 0.0,
            "aggro_range": "long",
        },
        "tags": ["extraplanar", "bound", "pact-kin"],
    },
    {
        "id": "aberration",
        "name": "Aberration",
        "category": "unnatural",
        "description": "Wrong-shaped things — fog-things, scar-things, carver-kin, drowned-warden shades. Often left over from the Coming.",
        "hp_scaling": {
            "base_at_level_1": 45,
            "per_level_multiplier": 1.15,
            "formula": "round(base_at_1 * multiplier^(level-1))",
        },
        "default_armor_class": "ethereal",
        "resistances": {
            "shadow": -0.30,
            "blade": -0.20,    # weapons pass strangely through them
            "blunt": -0.20,
            "arcane": -0.15,
            "light": 0.25,
            "devotion": 0.25,
            "fire": 0.05,
            "poison": -1.00,   # can't poison what isn't biological
        },
        "affinities": {
            "preferred": ["shadow", "arcane"],
            "allowed": ["frost", "lightning", "unarmed"],
            "forbidden": [
                "fire", "earth", "nature",
                "light", "devotion", "blood",
                "honor", "fury",
                "blade", "blunt", "spear", "shield",
                "bow", "dagger", "thrown", "acrobat", "silent", "trickster",
                "alchemy", "tonics", "poison",
            ],
        },
        "behavior_defaults": {
            "intelligence": "standard",
            "social": "solitary",
            "flee_threshold": 0.10,
            "aggro_range": "long",
        },
        "tags": ["wrong", "leftover", "coming-kin"],
    },
    {
        "id": "elemental",
        "name": "Elemental",
        "category": "unnatural",
        "description": "Embodied school — pit-vents, storm-bound spirits, rot-spirits, frost-bound wights. Pure-element in form.",
        "hp_scaling": {
            "base_at_level_1": 35,
            "per_level_multiplier": 1.15,
            "formula": "round(base_at_1 * multiplier^(level-1))",
        },
        "default_armor_class": "none",
        "resistances": {
            # Elementals take -75% from their own school, +50% from opposite.
            # Specific elemental subtypes tighten this further in mob overrides.
            "fire": -0.30,
            "frost": -0.30,
            "lightning": -0.30,
            "earth": -0.30,
            "nature": -0.15,
            "blade": 0.25,
            "dagger": 0.25,
            "bow": 0.10,
        },
        "affinities": {
            "preferred": ["fire", "frost", "lightning", "earth"],
            "allowed": ["arcane"],
            "forbidden": [
                "light", "devotion", "shadow", "blood",
                "nature",
                "honor", "fury",
                "blade", "blunt", "spear", "shield", "unarmed",
                "bow", "dagger", "thrown", "acrobat", "silent", "trickster",
                "alchemy", "tonics", "poison",
            ],
        },
        "behavior_defaults": {
            "intelligence": "low",
            "social": "solitary",
            "flee_threshold": 0.0,
            "aggro_range": "medium",
        },
        "tags": ["elemental", "embodied-school", "non-corporeal"],
    },
    {
        "id": "construct",
        "name": "Construct",
        "category": "unnatural",
        "description": "Built things — stone golems, forge-sentinels, animated armor, bound-shell Gravewrought kin.",
        "hp_scaling": {
            "base_at_level_1": 60,
            "per_level_multiplier": 1.16,
            "formula": "round(base_at_1 * multiplier^(level-1))",
        },
        "default_armor_class": "plate",
        "resistances": {
            "blade": -0.25,
            "blunt": 0.25,    # blunt weapons crack stone
            "shadow": -0.40,
            "blood": -1.00,   # no blood to drain
            "poison": -1.00,  # no flesh to poison
            "nature": -0.15,
            "fire": -0.20,
            "frost": 0.10,    # thermal shock cracks some constructs
            "lightning": 0.15,
        },
        "affinities": {
            "preferred": ["blunt", "blade", "spear"],
            "allowed": ["unarmed", "earth"],
            "forbidden": [
                "arcane", "fire", "frost", "lightning", "nature",
                "light", "devotion", "shadow", "blood",
                "honor", "fury",
                "bow", "dagger", "thrown", "acrobat", "silent", "trickster",
                "alchemy", "tonics", "poison",
            ],
        },
        "behavior_defaults": {
            "intelligence": "low",
            "social": "solitary",
            "flee_threshold": 0.0,
            "aggro_range": "short",
        },
        "tags": ["built", "non-biological", "fabricated"],
    },
    {
        "id": "fey",
        "name": "Fey",
        "category": "natural",
        "description": "Wild-court creatures — pre-Veyr elvish woodland kin, moth-court attendants, fey-stags. Older than the factions.",
        "hp_scaling": {
            "base_at_level_1": 40,
            "per_level_multiplier": 1.15,
            "formula": "round(base_at_1 * multiplier^(level-1))",
        },
        "default_armor_class": "leather",
        "resistances": {
            "nature": -0.50,
            "arcane": -0.25,
            "shadow": 0.25,
            "blood": 0.25,
            "fire": 0.30,     # fey fear flame
        },
        "affinities": {
            "preferred": ["nature", "arcane", "dagger", "bow"],
            "allowed": ["frost", "trickster", "silent", "acrobat"],
            "forbidden": [
                "fire",
                "shadow", "blood",
                "devotion",
                "honor", "fury",
                "blunt", "shield", "unarmed",
                "alchemy",  # fey do not practice human alchemy
                "poison",
                "tonics",
            ],
        },
        "behavior_defaults": {
            "intelligence": "high",
            "social": "herd",
            "flee_threshold": 0.30,
            "aggro_range": "long",
        },
        "tags": ["wild-court", "pre-Veyr", "sylvan"],
    },
    {
        "id": "living_construct",
        "name": "Living Construct",
        "category": "unnatural",
        "description": "Soul-bound construct — gravewrought shells, animated funeral-armor with a ritually-bound ancestor-spirit inside. Construct immunities without construct affinity-lockouts; the bound soul retains the schools its former life could use.",
        "hp_scaling": {
            "base_at_level_1": 55,
            "per_level_multiplier": 1.15,
            "formula": "round(base_at_1 * multiplier^(level-1))",
        },
        "default_armor_class": "plate",
        "resistances": {
            "blood": -0.80,    # no blood to drain — near-immunity
            "poison": -0.90,   # no biology to poison
            "shadow": -0.20,   # ancestor-bound, familiar with death
            "blunt": 0.15,     # joints still vulnerable to crushing
            "fire": -0.10,     # iron-and-bone resists fire
            "frost": 0.10,     # thermal shock cracks the shell
        },
        "affinities": {
            # soul-bound: the bound spirit's schools still work.
            # Wider affinity than pure construct — this is what makes gravewrought playable.
            "preferred": ["blade", "spear", "shield", "honor"],
            "allowed": [
                "arcane", "blood", "shadow", "earth",
                "blunt", "unarmed", "fury",
                "devotion",
            ],
            "forbidden": [
                "fire", "frost", "lightning", "nature",
                "light",
                "bow", "dagger", "thrown", "acrobat",
                "silent", "trickster", "alchemy", "tonics", "poison",
            ],
        },
        "behavior_defaults": {
            "intelligence": "high",
            "social": "warband",
            "flee_threshold": 0.0,   # bound-duty; do not flee
            "aggro_range": "medium",
        },
        "tags": ["soul-bound", "construct-kin", "playable"],
    },
    {
        "id": "giant",
        "name": "Giant",
        "category": "natural",
        "description": "Oversized bipedal kin — yeti, mountain-giants, ogre-analogues. Larger than humanoid, simpler in affinity.",
        "hp_scaling": {
            "base_at_level_1": 70,
            "per_level_multiplier": 1.15,
            "formula": "round(base_at_1 * multiplier^(level-1))",
        },
        "default_armor_class": "hide",
        "resistances": {
            "frost": -0.25,    # most giants tolerate cold
            "blunt": -0.10,
            "blade": 0.10,
        },
        "affinities": {
            "preferred": ["blunt", "unarmed", "spear"],
            "allowed": ["blade", "thrown", "shield"],
            "forbidden": [
                "arcane", "fire", "frost", "lightning", "nature", "earth",
                "light", "devotion", "shadow", "blood",
                "honor",
                "bow", "dagger", "acrobat", "silent", "trickster",
                "alchemy", "tonics", "poison",
            ],
        },
        "behavior_defaults": {
            "intelligence": "low",
            "social": "herd",
            "flee_threshold": 0.15,
            "aggro_range": "medium",
        },
        "tags": ["oversized", "bipedal", "simple"],
    },
]


# ---------------------------------------------------------------------------
# Armor classes — physical/magical reduction + material strengths/weaknesses
# ---------------------------------------------------------------------------
# physical_reduction: flat % off incoming physical damage (blade/blunt/spear/bow/dagger/unarmed/thrown)
# magic_reduction: flat % off incoming magical damage (all arcana schools)
# weak_against: schools that bypass some of this armor's physical reduction
# strong_against: schools this armor resists well in addition to base
# mobility_penalty: behavioral flag — heavier armor = reduced aggro_range for NPCs

ARMOR_CLASSES = [
    {
        "id": "none",
        "name": "No Armor",
        "tier": "none",
        "physical_reduction": 0.00,
        "magic_reduction": 0.00,
        "weak_against": [],
        "strong_against": [],
        "mobility_penalty": 0.00,
        "notes": "wisps, elementals, swarms — nothing to reduce incoming damage",
    },
    {
        "id": "hide",
        "name": "Hide",
        "tier": "light",
        "physical_reduction": 0.10,
        "magic_reduction": 0.05,
        "weak_against": ["blade"],          # sharp edges cut hide
        "strong_against": ["blunt"],        # padding absorbs crushing
        "mobility_penalty": 0.00,
        "notes": "natural hide or tanned-leather-equivalent — most beasts default here",
    },
    {
        "id": "leather",
        "name": "Leather",
        "tier": "light",
        "physical_reduction": 0.15,
        "magic_reduction": 0.08,
        "weak_against": ["spear", "thrown"],
        "strong_against": ["dagger"],       # layered leather deflects stabs
        "mobility_penalty": 0.00,
        "notes": "crafted leather — skirmishers, rogues, scouts",
    },
    {
        "id": "mail",
        "name": "Mail",
        "tier": "medium",
        "physical_reduction": 0.25,
        "magic_reduction": 0.10,
        "weak_against": ["spear", "bow"],  # piercing spreads chainmail
        "strong_against": ["blade"],
        "mobility_penalty": 0.10,
        "notes": "rings of forged link — standard humanoid warrior kit",
    },
    {
        "id": "plate",
        "name": "Plate",
        "tier": "heavy",
        "physical_reduction": 0.40,
        "magic_reduction": 0.12,
        "weak_against": ["blunt"],         # hammers crush plate joints
        "strong_against": ["blade", "dagger"],
        "mobility_penalty": 0.25,
        "notes": "forged solid plate — elite warriors, knight-guards",
    },
    {
        "id": "scales",
        "name": "Scales",
        "tier": "heavy",
        "physical_reduction": 0.35,
        "magic_reduction": 0.15,
        "weak_against": ["spear"],         # spears slip between scales
        "strong_against": ["blade", "bow"],
        "mobility_penalty": 0.05,
        "notes": "natural scales — dragonkin, scaled demons, serpent-kin",
    },
    {
        "id": "chitin",
        "name": "Chitin",
        "tier": "medium",
        "physical_reduction": 0.25,
        "magic_reduction": 0.05,
        "weak_against": ["blunt"],
        "strong_against": ["blade", "dagger", "bow"],
        "mobility_penalty": 0.05,
        "notes": "insect/crustacean shell — crabs, scarabs, some aberrations",
    },
    {
        "id": "bone",
        "name": "Bone",
        "tier": "medium",
        "physical_reduction": 0.20,
        "magic_reduction": 0.15,   # undead bones resist magic more than mail does
        "weak_against": ["blunt"],
        "strong_against": ["blade", "dagger"],
        "mobility_penalty": 0.05,
        "notes": "exposed bone plating — most undead, bound-shell constructs",
    },
    {
        "id": "cloth",
        "name": "Cloth",
        "tier": "light",
        "physical_reduction": 0.05,
        "magic_reduction": 0.20,   # casters' robes carry protective weaves
        "weak_against": ["blade", "dagger", "bow"],
        "strong_against": [],
        "mobility_penalty": 0.00,
        "notes": "robes, wraps — casters, scribes, priests",
    },
    {
        "id": "ethereal",
        "name": "Ethereal",
        "tier": "special",
        "physical_reduction": 0.50,   # weapons pass through
        "magic_reduction": 0.00,      # magic hits normally
        "weak_against": ["arcane", "shadow", "light", "devotion"],
        "strong_against": ["blade", "blunt", "spear", "dagger", "bow", "thrown", "unarmed"],
        "mobility_penalty": 0.00,
        "notes": "incorporeal — aberrations, some undead; magic damage is the only reliable option",
    },
]


# ---------------------------------------------------------------------------
# Schema reference file (documentation, not data)
# ---------------------------------------------------------------------------

SCHEMA = {
    "id": "_bestiary_schema",
    "description": "Schema for creature_types and armor_classes. Not loaded as data.",
    "inheritance_rule": (
        "Each mob references exactly one creature_type and one armor_class. "
        "The mob inherits hp_scaling, resistances, affinities, and behavior_defaults "
        "from its creature_type, and physical/magic reduction + material match-ups "
        "from its armor_class. Mobs MAY override specific fields via an 'override:' "
        "block in their own yaml; override is a shallow merge."
    ),
    "hp_resolution": (
        "hp = round(type.hp_scaling.base_at_level_1 * "
        "type.hp_scaling.per_level_multiplier^(mob.level - 1) * "
        "rarity_multiplier[mob.rarity])"
    ),
    "rarity_multipliers": {
        "common": 1.00,
        "elite": 2.75,
        "rare": 3.50,
        "named": 5.00,
    },
    "damage_resolution": (
        "incoming_damage = raw_damage * (1 + type.resistances.get(school, 0)) * "
        "(1 - armor.physical_reduction if school is physical else 1 - armor.magic_reduction) * "
        "(1 + armor_weakness_bonus if school in armor.weak_against else 1) * "
        "(1 - armor_strength_bonus if school in armor.strong_against else 1)"
    ),
    "affinity_validation": (
        "A mob's primary_school MUST be in its creature_type.affinities.preferred "
        "or .allowed. Schools in .forbidden cannot be assigned — this prevents "
        "LLM-generated incoherence like 'light-devotion ashwolf' or 'poison golem'."
    ),
    "field_order_convention": [
        "id",
        "name",
        "category",
        "description",
        "hp_scaling",
        "default_armor_class",
        "resistances",
        "affinities",
        "behavior_defaults",
        "tags",
    ],
}


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main() -> None:
    for t in CREATURE_TYPES:
        write(OUT / "creature_types" / f"{t['id']}.yaml", t)
    for a in ARMOR_CLASSES:
        write(OUT / "armor_classes" / f"{a['id']}.yaml", a)
    write(OUT / "_schema.yaml", SCHEMA)
    print(f"wrote {len(CREATURE_TYPES)} creature_types, {len(ARMOR_CLASSES)} armor_classes")


if __name__ == "__main__":
    main()
