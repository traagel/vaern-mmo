#!/usr/bin/env python3
"""Seed src/generated/races/<race>/core.yaml with bestiary-linked race data.

Each playable race now references a creature_type from the bestiary (humanoid
for nine of the ten races; living_construct for gravewrought) and inherits
hp_scaling, resistance baseline, and affinities from that type. The race adds:

  - size_class: small | medium | large (affects hitbox / animation scale only)
  - hp_modifier: racial multiplier on top of creature_type hp curve
  - racial_resistances: race-specific school-damage modifiers (ADDED to creature_type)
  - racial_school_bonuses: mild damage/effect bonuses to specific schools (flavor+)

Existing fields preserved: archetype, faction, favored_class, cultural_traits,
affinity (pillar caps — distinct from bestiary school-affinities).

Idempotent. Re-running overwrites core.yaml; visual.yaml is untouched.
"""
from __future__ import annotations

from pathlib import Path
import yaml

REPO = Path(__file__).resolve().parents[1]
RACES = REPO / "src" / "generated" / "races"
BESTIARY = REPO / "src" / "generated" / "bestiary"


def write(path: Path, data: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "w") as f:
        yaml.safe_dump(data, f, sort_keys=False, default_flow_style=False)


# ---------------------------------------------------------------------------
# Race definitions — canonical data for all 10 playable races
# ---------------------------------------------------------------------------

RACES_DEF = [
    # ---------- Concord ----------
    {
        "id": "mannin",
        "archetype": "humans",
        "faction": "faction_a",
        "favored_class": "any",
        "cultural_traits": "adaptable, institutional, dominant, pragmatic civic builders",
        "creature_type": "humanoid",
        "size_class": "medium",
        "hp_modifier": 1.00,
        "affinity": {
            "might": 100, "arcana": 100, "finesse": 100,
            "notes": "universal baseline — versatility IS the racial feature; can reach any corner",
        },
        "racial_resistances": {},
        "racial_school_bonuses": [],
        "lore_hook": "the Veyr's human majority — the political center of Concord institutions",
    },
    {
        "id": "hearthkin",
        "archetype": "dwarves",
        "faction": "faction_a",
        "favored_class": "fighter",
        "cultural_traits": "mountain-forged, lawful, oath-clan smiths and hearth-wardens",
        "creature_type": "humanoid",
        "size_class": "medium",
        "hp_modifier": 1.10,   # stocky
        "affinity": {
            "might": 100, "arcana": 75, "finesse": 50,
            "notes": "Fighter peak; hearth-rite arcana access; no Rogue corner",
        },
        "racial_resistances": {
            "poison": -0.20,   # dwarven constitution
            "fire": -0.10,     # forge-kin
            "blunt": -0.05,    # dense bone
        },
        "racial_school_bonuses": ["blade", "blunt", "shield", "honor"],
        "lore_hook": "mountain-clans of the Iron Mountains; oath-bound smiths",
    },
    {
        "id": "sunward_elen",
        "archetype": "high elves",
        "faction": "faction_a",
        "favored_class": "wizard",
        "cultural_traits": "ancient, arcane-literate, rigid, scholarly-hierarchical, sun-bound",
        "creature_type": "humanoid",
        "size_class": "medium",
        "hp_modifier": 0.95,   # slender
        "affinity": {
            "might": 50, "arcana": 100, "finesse": 75,
            "notes": "arcane peak; archery-stealth strong; medium-armor swordplay but no Fighter corner",
        },
        "racial_resistances": {
            "shadow": 0.10,    # light-aligned, shadow hurts more
            "arcane": -0.10,   # long attunement
            "frost": -0.05,
        },
        "racial_school_bonuses": ["arcane", "light", "frost", "bow"],
        "lore_hook": "pre-Veyr arcane scholars of the highland spires; sun-bound kin of the Darkling Elen",
    },
    {
        "id": "wyrling",
        "archetype": "halflings",
        "faction": "faction_a",
        "favored_class": "rogue or bard",
        "cultural_traits": "small, clever, urban-mercantile, quick-tongued, guild-inclined",
        "creature_type": "humanoid",
        "size_class": "small",
        "hp_modifier": 0.90,
        "affinity": {
            "might": 50, "arcana": 50, "finesse": 100,
            "notes": "finesse peak (Rogue/Bard); minor spellcraft; small-but-capable martial",
        },
        "racial_resistances": {
            "fire": -0.10,     # hearth-folk
            "poison": -0.05,
        },
        "racial_school_bonuses": ["dagger", "silent", "trickster", "acrobat"],
        "lore_hook": "guild-organized townsfolk of the Wyrling Downs — small but networked",
    },
    {
        "id": "firland",
        "archetype": "firbolg-analogue — large gentle forest-folk",
        "faction": "faction_a",
        "favored_class": "druid or warden",
        "cultural_traits": "deep-forest hermit-clans, oath-to-the-wild, seasonal-migratory, mediators between the woodlands and the Veyr kingdoms",
        "creature_type": "humanoid",
        "size_class": "large",
        "hp_modifier": 1.15,
        "affinity": {
            "might": 75, "arcana": 100, "finesse": 25,
            "notes": "Druid/nature-Wizard peak; heavy frame limits stealth; no Fighter corner",
        },
        "racial_resistances": {
            "nature": -0.20,   # deep-forest hermits
            "earth": -0.10,
            "fire": 0.10,      # forest kin, fire fears them less
        },
        "racial_school_bonuses": ["nature", "unarmed", "spear", "earth"],
        "lore_hook": "mediator-clans between the wild and the Veyr; too large for fine work",
    },

    # ---------- Rend ----------
    {
        "id": "darkling_elen",
        "archetype": "fallen elves / dark elves",
        "faction": "faction_b",
        "favored_class": "warlock",
        "cultural_traits": "shadow-touched, exiled kin of Sunward, pact-bound, vengeful scholars",
        "creature_type": "humanoid",
        "size_class": "medium",
        "hp_modifier": 0.95,
        "affinity": {
            "might": 50, "arcana": 100, "finesse": 100,
            "notes": "Wizard AND Rogue corners reachable — assassin-sorcerer identity distinguishes them from Sunward kin; no heavy martial",
        },
        "racial_resistances": {
            "shadow": -0.20,
            "light": 0.20,     # exile-kin, light hurts
            "poison": -0.10,
            "arcane": -0.10,
        },
        "racial_school_bonuses": ["shadow", "silent", "dagger", "arcane"],
        "lore_hook": "exiled kin of the Sunward; the original shadow-pact signers",
    },
    {
        "id": "gravewrought",
        "archetype": "ancestor-bound construct — iron-and-bone shells housing ritually-bound souls",
        "faction": "faction_b",
        "favored_class": "fighter or paladin-mirror (duskblade)",
        "cultural_traits": "walking funeral-armor, ritually-bound ancestor-soul in each shell, vow-holders and oath-keepers of fallen clans, formal and deliberate",
        "creature_type": "living_construct",
        "size_class": "large",
        "hp_modifier": 1.15,
        "affinity": {
            "might": 100, "arcana": 75, "finesse": 25,
            "notes": "heavy martial peak + ritual-tier arcana (binding-rite magic); slow construct, minimal finesse",
        },
        "racial_resistances": {
            # additive on top of living_construct's base (which already has
            # blood -0.80, poison -0.90, shadow -0.20, blunt +0.15)
            "cold": -0.05,     # metal frame, but wards against freezing crack
        },
        "racial_school_bonuses": ["blade", "shield", "honor", "spear"],
        "lore_hook": "when a Hraun warrior of standing falls, the ancestor-rite can bind their soul into a funeral-shell — gravewrought are that rite's deliberate product",
    },
    {
        "id": "kharun",
        "archetype": "pact-marked humans — ancestral shadow-bargain survivors",
        "faction": "faction_b",
        "favored_class": "warlock or sorcerer",
        "cultural_traits": "descendants of Hraun who struck the pact that let their line escape the consuming thing, pact-scarred scholars and pact-bound duellists",
        "creature_type": "humanoid",
        "size_class": "medium",
        "hp_modifier": 1.00,
        "affinity": {
            "might": 75, "arcana": 100, "finesse": 75,
            "notes": "pact-caster peak; versatile martial + stealth; no Fighter or Rogue corner (jack-of-all-casters)",
        },
        "racial_resistances": {
            "shadow": -0.15,
            "blood": -0.10,
            "light": 0.10,     # pact-marked, light is uncomfortable
        },
        "racial_school_bonuses": ["blood", "shadow", "arcane"],
        "lore_hook": "the generation-family that signed the pact to survive the crossing; every kharun is born already in debt",
    },
    {
        "id": "skarn",
        "archetype": "half-orc-lineage raiders",
        "faction": "faction_b",
        "favored_class": "barbarian or fighter",
        "cultural_traits": "Hraun border-clan with wild-blood ancestry, raider-warriors, oath-scarred, blood-remembrance",
        "creature_type": "humanoid",
        "size_class": "medium",   # larger than mannin but not large-size
        "hp_modifier": 1.10,
        "affinity": {
            "might": 100, "arcana": 50, "finesse": 50,
            "notes": "Fighter/Barbarian peak; shamanic tier arcana; mobile but not stealth master",
        },
        "racial_resistances": {
            "blunt": -0.10,    # thick-boned
            "poison": -0.05,
            "fire": 0.05,      # wild-blood reads fire as kin
        },
        "racial_school_bonuses": ["fury", "blade", "blunt", "spear"],
        "lore_hook": "the Hraun border-clans with wild-blood crossbreeding; the Rend's front-line warrior caste",
    },
    {
        "id": "skrel",
        "archetype": "goblin-analogue — small scavenger scrap-folk",
        "faction": "faction_b",
        "favored_class": "rogue or ranger",
        "cultural_traits": "opportunist scavengers of the Hraun warbands, trap-makers and scrap-smiths, snarling loyalty to their clan, fast-breeding and quarrelsome",
        "creature_type": "humanoid",
        "size_class": "small",
        "hp_modifier": 0.85,
        "affinity": {
            "might": 50, "arcana": 50, "finesse": 100,
            "notes": "Rogue peak; goblin-shaman arcana access (Mystic/Warlock); small-but-capable martial",
        },
        "racial_resistances": {
            "poison": -0.15,   # scavenger constitution
            "disease": -0.10,  # fast-breed tolerance
        },
        "racial_school_bonuses": ["dagger", "trickster", "poison", "thrown", "alchemy"],
        "lore_hook": "scavenger-folk of the Scrap-Marsh; Rend's trap-layers and back-line support",
    },
]


# ---------------------------------------------------------------------------
# Validation
# ---------------------------------------------------------------------------

def validate(race: dict, creature_types: dict) -> list[str]:
    errors = []
    ctype = race["creature_type"]
    if ctype not in creature_types:
        errors.append(f"{race['id']}: unknown creature_type {ctype!r}")
        return errors
    forbidden = set(creature_types[ctype]["affinities"]["forbidden"])
    # Check racial_school_bonuses don't include forbidden schools
    for school in race.get("racial_school_bonuses", []):
        if school in forbidden:
            errors.append(
                f"{race['id']}: racial_school_bonus {school!r} is FORBIDDEN for "
                f"creature_type {ctype} (cannot grant bonus to a school the type cannot use)"
            )
    return errors


def load_creature_types() -> dict:
    types = {}
    for p in (BESTIARY / "creature_types").glob("*.yaml"):
        with open(p) as f:
            t = yaml.safe_load(f)
        types[t["id"]] = t
    return types


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main() -> None:
    creature_types = load_creature_types()
    if not creature_types:
        raise SystemExit("bestiary/ empty — run seed_bestiary.py first")

    errors: list[str] = []
    for race in RACES_DEF:
        errors.extend(validate(race, creature_types))
        if errors:
            continue
        write(RACES / race["id"] / "core.yaml", race)

    if errors:
        print(f"\nVALIDATION FAILURES ({len(errors)}):")
        for e in errors:
            print(f"  ! {e}")
        raise SystemExit(1)

    print(f"wrote {len(RACES_DEF)} race cores; all validate against bestiary creature_types")
    # Distribution sanity
    ct_count = {}
    for r in RACES_DEF:
        ct_count[r["creature_type"]] = ct_count.get(r["creature_type"], 0) + 1
    print(f"  creature_type distribution: {ct_count}")


if __name__ == "__main__":
    main()
