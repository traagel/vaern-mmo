#!/usr/bin/env python3
"""
One-shot seed for the dalewatch_marches starter-zone redesign.

Emits the full YAML tree described in `dalewatch_redesign.md`:
  - 4 hubs (2 existing are regenerated, 2 are new)
  - 9 new mob entries + refreshed _roster.yaml
  - 1 main chain (10 steps, extended)
  - 2 side chains (4 steps each)
  - 20 individual side quests across 4 hubs
  - Trimmed filler pool (12)
  - Refreshed _summary.yaml, core.yaml, landmarks.yaml

Safe to rerun: always writes the same output for the same inputs. Uses
`--dry-run` to preview. Uses `--force` to overwrite existing files.
"""

from __future__ import annotations

import argparse
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import yaml


ROOT = Path(__file__).resolve().parents[1]
ZONE_DIR = ROOT / "src" / "generated" / "world" / "zones" / "dalewatch_marches"
ZONE_ID = "dalewatch_marches"


# ────────────────────────── data: hubs ──────────────────────────

# Each hub: id, name, role, amenities, quest_givers, offset (x, z).
# Offsets from the design doc §4.
HUBS: list[dict[str, Any]] = [
    {
        "id": "dalewatch_keep",
        "name": "Dalewatch Keep",
        "role": "capital",
        "amenities": [
            "innkeeper",
            "class_trainers",
            "bank",
            "auction",
            "flight_master",
            "profession_trainers",
        ],
        "quest_givers": 6,
        "offset": (0.0, 0.0),
        "description": (
            "Stone curtain wall, packed muster yard, the smell of horse and "
            "forge. Banners of the Warden Corps hang over the gate. The county "
            "seat — trainers, taxmen, and the chapel where you take the Oath. "
            "Nothing inside the walls is hostile yet, though half the experienced "
            "riders have already been pulled east to the Ford."
        ),
        "prompt": (
            "establishing shot, late-medieval Burgundian curtain wall and "
            "gatehouse, heraldic banners over the gate, packed muster yard, "
            "stone forge smoke, dawn warm gold light, painterly atmospheric, "
            "no figures, no characters"
        ),
    },
    {
        "id": "harriers_rest",
        "name": "Harrier's Rest",
        "role": "outpost",
        "amenities": ["innkeeper"],
        "quest_givers": 2,
        "offset": (60.0, -100.0),
        "description": (
            "A wooden chapel to a minor road-saint, with a one-pot hostel tucked "
            "behind the bell. Pilgrims and travelling smiths stop here before "
            "the bridge. Brother Fennick keeps the place — translator and "
            "letter-writer when he isn't tending the lamp. Sheltered from the "
            "fens by the upland scrub."
        ),
        "prompt": (
            "establishing shot, small wooden roadside chapel with bell tower, "
            "modest hostel beside it, upland scrub and dark pines, overcast "
            "afternoon, lantern by the door, painterly atmospheric, "
            "late-medieval Burgundian, no figures"
        ),
    },
    {
        "id": "kingsroad_waypost",
        "name": "Kingsroad Waypost",
        "role": "outpost",
        "amenities": ["repair"],
        "quest_givers": 2,
        "offset": (-110.0, -30.0),
        "description": (
            "A prefab patrol cabin on the kingsroad, halfway between the Keep "
            "and Miller's Crossing. Two bored deputies, a stove, a boot rack, "
            "and the courier relay for the eastern run. Roadside bandit work "
            "stages from here, and most of the Marches' couriers pass under "
            "its lantern at some hour of the night."
        ),
        "prompt": (
            "establishing shot, small timber patrol cabin beside a paved "
            "kingsroad at dusk, single hanging lantern, courier post sign, "
            "autumn rain in the distance, painterly atmospheric, "
            "late-medieval Burgundian, no figures"
        ),
    },
    {
        "id": "miller_crossing",
        "name": "Miller's Crossing",
        "role": "outpost",
        "amenities": ["innkeeper", "flight_master", "repair"],
        "quest_givers": 4,
        "offset": (-220.0, 20.0),
        "description": (
            "A stone bridge across the Ash, a working grain mill, and ten or so "
            "steadings packed close to the water. Bread and grain, suspicion of "
            "strangers, short coin. Old Brenn's flock grazes a half-day upriver, "
            "and the miller's daughter has been losing sacks to someone the "
            "deputies can't catch."
        ),
        "prompt": (
            "establishing shot, low stone arch bridge over a slow river, "
            "working timber grain mill with waterwheel, packed thatched "
            "steadings, late-afternoon golden light, painterly atmospheric, "
            "late-medieval Burgundian, no figures"
        ),
    },
    {
        "id": "ford_of_ashmere",
        "name": "Ford of Ashmere",
        "role": "outpost",
        "amenities": ["repair", "flight_master"],
        "quest_givers": 3,
        "offset": (270.0, 40.0),
        "description": (
            "A forward Warden camp at the strategic river crossing, east of "
            "everything safe. Earthworks, pickets, and skirmish scouts trading "
            "rumours over cold tea. Pact pressure is no longer hypothetical "
            "here — patrol parties have started crossing the shallows in numbers "
            "nobody at the Keep can explain. Veterans arrive from Dalewatch "
            "faster than recruits can replace them."
        ),
        "prompt": (
            "establishing shot, forward earthwork camp at a wide shallow river "
            "ford, picket fences and timber palisade, distant mist on the far "
            "bank, dusk overcast cool light, watchfires, painterly atmospheric, "
            "late-medieval Burgundian, no figures"
        ),
    },
]


# ────────────────────────── data: landmarks ──────────────────────────
# Non-hub sub-zones. Listed in landmarks.yaml for lore + future
# location-resolving logic. Not consumed by the server today.
LANDMARKS: list[dict[str, Any]] = [
    {
        "id": "old_brenns_croft",
        "name": "Old Brenn's Croft",
        "offset": (-240.0, 120.0),
        "description": (
            "A single farmhouse and three weather-worn barns set against the "
            "western ridge. Brenn keeps sheep and the occasional cow; the dogs "
            "are the best part of the local economy. Wolf packs come down from "
            "the ridge at night, and the croft has lost lambs four times this "
            "season."
        ),
        "prompt": (
            "establishing shot, single weathered timber farmhouse with three "
            "small barns, sheep grazing, low ridge in the background, late "
            "afternoon overcast light, painterly atmospheric, late-medieval, "
            "no figures"
        ),
    },
    {
        "id": "dalewatch_reed_brake",
        "name": "The Reed-Brake",
        "offset": (-150.0, 200.0),
        "description": (
            "A shallow marsh on a slow river bend, threaded with reed-cutter "
            "paths and the occasional half-rotten coracle. Mist comes off the "
            "water at first light. Corlen's old drifter camp was hidden in an "
            "abandoned fisherman's hut here — the burnt timbers and the symbol "
            "scratched into the door are still readable in good weather."
        ),
        "prompt": (
            "establishing shot, shallow reed marsh on a slow river bend, mist "
            "rising from black water, abandoned thatch fisher hut on stilts, "
            "dawn cool light, painterly atmospheric, no figures"
        ),
    },
    {
        "id": "thornroot_grove",
        "name": "Thornroot Grove",
        "offset": (100.0, 140.0),
        "description": (
            "A ring of old-growth oak around a black standing stone and a "
            "cold spring. The keeper — half druid, half forester, wholly "
            "suspicious of outsiders — tends a small fire under the largest "
            "tree. The beasts here get territorial when something's wrong "
            "with the grove, and lately something is."
        ),
        "prompt": (
            "establishing shot, ancient oak grove around a black standing "
            "stone and a cold spring, small fire under the central tree, "
            "dappled overcast light, painterly atmospheric, no figures"
        ),
    },
    {
        "id": "sidlow_cairn",
        "name": "Sidlow Cairn",
        "offset": (180.0, -180.0),
        "description": (
            "Low turf-covered barrow-mounds on the ridge south-east of the "
            "Keep — old Mannin grave-sites no one cuts hay near. Local lore "
            "says the cairns \"don't stay shut\", and at least three of the "
            "slabs have been moved recently in ways no badger could account "
            "for."
        ),
        "prompt": (
            "establishing shot, low turf-covered barrow mounds on a "
            "windswept ridge, weathered standing slabs, scattered crows, "
            "twilight cold light, oppressive mood, painterly atmospheric, "
            "no figures"
        ),
    },
    {
        "id": "copperstep_mine",
        "name": "Copperstep Mine",
        "offset": (150.0, -260.0),
        "description": (
            "A copper vein that played out a decade ago, and that the company "
            "sealed when too many men stopped coming back. Drifter cultists "
            "have pried open the upper shafts and dug deeper — not for ore "
            "but for something at the bottom of the longest drift. The lift "
            "cage is gone; you go down on rope."
        ),
        "prompt": (
            "establishing shot, abandoned copper mine entrance cut into a "
            "hillside, broken winch and rotted timber, scattered ore "
            "tailings, overcast cold light, painterly atmospheric, "
            "late-medieval, no figures"
        ),
    },
    {
        "id": "blackwash_fens",
        "name": "The Blackwash Fens",
        "offset": (-50.0, 280.0),
        "description": (
            "The deep marsh beyond the Reed-Brake. Black water, no roads, "
            "the wrong smells. Brine-shade aberrations rise from the channels "
            "at dusk and don't go quietly. This is where the main chain ends, "
            "and where the sealed confessions of three Drifters were found "
            "drifting in a nailed-shut barrel."
        ),
        "prompt": (
            "establishing shot, deep mist-bound black-water fen, broken sedge "
            "islands, twisted dead trees, sickly green twilight, oppressive "
            "atmosphere, painterly, no figures, no characters"
        ),
    },
    {
        "id": "drifters_lair",
        "name": "The Drifter's Lair",
        "offset": (20.0, 320.0),
        "description": (
            "A cave entrance in the fen, edged with hand-cut stone and the "
            "remains of a Drifter ward-circle. Inside: a tight encounter "
            "chamber where Grand Drifter Valenn finished what the Censure "
            "couldn't bury. The Wake's anchor sits at the back, still humming."
        ),
        "prompt": (
            "establishing shot, cave entrance in a marsh edged with carved "
            "stone and a broken ward-circle, lantern at the threshold, low "
            "fog over black water, painterly atmospheric, oppressive mood, "
            "no figures"
        ),
    },
]


# ────────────────────────── data: mobs ──────────────────────────
# Existing mobs (kept, NOT regenerated — the seed only TOUCHES the
# new entries and the _roster summary). The existing IDs are listed
# here because _roster.yaml needs the full mob list to stay accurate.
EXISTING_MOB_IDS: list[str] = [
    "mob_dalewatch_marches_beast_wolf",
    "mob_dalewatch_marches_beast_boar",
    "mob_dalewatch_marches_beast_stag",
    "mob_dalewatch_marches_beast_heron",
    "mob_dalewatch_marches_beast_otter",
    "mob_dalewatch_marches_beast_bear",
    "mob_dalewatch_marches_ambient_duck",
    "mob_dalewatch_marches_exotic_drifter_shade",
    "mob_dalewatch_marches_warband_drifter_mage",
    "mob_dalewatch_marches_warband_raid_stray",
    "mob_dalewatch_marches_named_drifter_mage_named",
    "mob_dalewatch_marches_juvenile_wolf",
    "mob_dalewatch_marches_juvenile_boar",
    "mob_dalewatch_marches_juvenile_stag",
    "mob_dalewatch_marches_juvenile_heron",
]

NEW_MOBS: list[dict[str, Any]] = [
    {
        "id": "mob_dalewatch_marches_named_wolf_alpha",
        "name": "Ashmane Alpha",
        "level": 5,
        "creature_type": "beast",
        "armor_class": "hide",
        "rarity": "named",
        "role": "captain",
        "faction_alignment": "neutral_aggressive",
        "hp_tier": "heroic",
        "damage": {
            "primary_school": "blade",
            "attack_range": "melee",
            "dps_tier": "heavy",
        },
        "behavior": {
            "aggro_range": "long",
            "social_radius": 15,
            "flee_threshold": 0.0,
            "calls_allies": True,
            "patrol": "short",
        },
        "loot_tier": "quest_marked",
        "drops": {
            "gold_copper_avg": 60,
            "item_hint": "alpha-pelt trinket; grove-keeper side chain finisher",
        },
        "biome_context": "pack leader of the ridge-wolves; Thornroot Grove side chain boss",
        "chain_target": True,
    },
    {
        "id": "mob_dalewatch_marches_warband_pact_scout",
        "name": "Pact Scout",
        "level": 6,
        "creature_type": "humanoid",
        "armor_class": "leather",
        "rarity": "common",
        "role": "skirmisher",
        "faction_alignment": "hostile_to_faction_a",
        "hp_tier": "standard",
        "damage": {
            "primary_school": "blade",
            "attack_range": "ranged",
            "dps_tier": "standard",
        },
        "behavior": {
            "aggro_range": "long",
            "social_radius": 10,
            "flee_threshold": 0.25,
            "calls_allies": True,
            "patrol": "medium",
        },
        "loot_tier": "standard",
        "drops": {
            "gold_copper_avg": 12,
            "item_hint": "pact-worked charm; cloth strips",
        },
        "biome_context": "probing the Ford of Ashmere from the eastern bank",
    },
    {
        "id": "mob_dalewatch_marches_warband_pact_skirmisher",
        "name": "Pact Skirmisher",
        "level": 7,
        "creature_type": "humanoid",
        "armor_class": "leather",
        "rarity": "common",
        "role": "brawler",
        "faction_alignment": "hostile_to_faction_a",
        "hp_tier": "standard",
        "damage": {
            "primary_school": "blade",
            "attack_range": "melee",
            "dps_tier": "standard",
        },
        "behavior": {
            "aggro_range": "medium",
            "social_radius": 10,
            "flee_threshold": 0.1,
            "calls_allies": True,
            "patrol": "medium",
        },
        "loot_tier": "standard",
        "drops": {
            "gold_copper_avg": 15,
            "item_hint": "short axe; leather straps",
        },
        "biome_context": "forward strikers screening scout parties at the Ford",
    },
    {
        "id": "mob_dalewatch_marches_named_pact_captain",
        "name": "Pact Captain Vask",
        "level": 7,
        "creature_type": "humanoid",
        "armor_class": "plate",
        "rarity": "named",
        "role": "captain",
        "faction_alignment": "hostile_to_faction_a",
        "hp_tier": "heroic",
        "damage": {
            "primary_school": "blade",
            "attack_range": "melee",
            "dps_tier": "heavy",
        },
        "behavior": {
            "aggro_range": "long",
            "social_radius": 20,
            "flee_threshold": 0.0,
            "calls_allies": True,
            "patrol": "stationary",
        },
        "loot_tier": "quest_marked",
        "drops": {
            "gold_copper_avg": 200,
            "item_hint": "pact captain's sigil + blade; Ashmere side-quest finisher",
        },
        "biome_context": "commands the eastern-bank pickets; named rare spawn at the Ford",
        "chain_target": True,
    },
    {
        "id": "mob_dalewatch_marches_warband_drifter_cultist",
        "name": "Drifter Cultist",
        "level": 5,
        "creature_type": "humanoid",
        "armor_class": "cloth",
        "rarity": "common",
        "role": "caster",
        "faction_alignment": "hostile_to_faction_a",
        "hp_tier": "standard",
        "damage": {
            "primary_school": "fire",
            "attack_range": "ranged",
            "dps_tier": "standard",
        },
        "behavior": {
            "aggro_range": "medium",
            "social_radius": 8,
            "flee_threshold": 0.0,
            "calls_allies": True,
            "patrol": "stationary",
        },
        "loot_tier": "standard",
        "drops": {
            "gold_copper_avg": 18,
            "item_hint": "cult-rag robe; inked paper leaf",
        },
        "biome_context": "ritualists staging inside the abandoned Copperstep Mine",
    },
    {
        "id": "mob_dalewatch_marches_named_drifter_master",
        "name": "Master Drifter Halen",
        "level": 9,
        "creature_type": "humanoid",
        "armor_class": "cloth",
        "rarity": "named",
        "role": "captain",
        "faction_alignment": "hostile_to_faction_a",
        "hp_tier": "heroic",
        "damage": {
            "primary_school": "fire",
            "attack_range": "ranged",
            "dps_tier": "heavy",
        },
        "behavior": {
            "aggro_range": "long",
            "social_radius": 15,
            "flee_threshold": 0.0,
            "calls_allies": True,
            "patrol": "stationary",
        },
        "loot_tier": "quest_marked",
        "drops": {
            "gold_copper_avg": 180,
            "item_hint": "pact-worked focus (main-chain turn-in); scorched tome",
        },
        "biome_context": "mine cultist leader; mini-boss in the Drifter's Lair (main-chain step 7 target)",
        "chain_target": True,
    },
    {
        "id": "mob_dalewatch_marches_exotic_brine_shade",
        "name": "Brine-Shade",
        "level": 6,
        "creature_type": "aberration",
        "armor_class": "ethereal",
        "rarity": "common",
        "role": "skirmisher",
        "faction_alignment": "hostile_to_all",
        "hp_tier": "standard",
        "damage": {
            "primary_school": "frost",
            "attack_range": "melee",
            "dps_tier": "standard",
        },
        "behavior": {
            "aggro_range": "long",
            "social_radius": 12,
            "flee_threshold": 0.0,
            "calls_allies": True,
            "patrol": "stationary",
        },
        "loot_tier": "standard",
        "drops": {
            "gold_copper_avg": 8,
            "item_hint": "shade-brine vial; rare ectoplasm",
        },
        "biome_context": "born of the Wake; haunts the Blackwash Fens' black-water channels",
    },
    {
        "id": "mob_dalewatch_marches_named_brine_primarch",
        "name": "The Brine-Shade Primarch",
        "level": 8,
        "creature_type": "aberration",
        "armor_class": "ethereal",
        "rarity": "named",
        "role": "captain",
        "faction_alignment": "hostile_to_all",
        "hp_tier": "heroic",
        "damage": {
            "primary_school": "frost",
            "attack_range": "melee",
            "dps_tier": "heavy",
        },
        "behavior": {
            "aggro_range": "long",
            "social_radius": 20,
            "flee_threshold": 0.0,
            "calls_allies": True,
            "patrol": "stationary",
        },
        "loot_tier": "quest_marked",
        "drops": {
            "gold_copper_avg": 400,
            "item_hint": "primarch's brine-core (main-chain finisher item)",
        },
        "biome_context": "the Wake's anchor in the Blackwash; paired with Grand Drifter Valenn",
        "chain_target": True,
    },
    {
        "id": "mob_dalewatch_marches_named_drifter_valenn",
        "name": "Grand Drifter Valenn",
        "level": 10,
        "creature_type": "humanoid",
        "armor_class": "cloth",
        "rarity": "named",
        "role": "captain",
        "faction_alignment": "hostile_to_faction_a",
        "hp_tier": "heroic",
        "damage": {
            "primary_school": "arcane",
            "attack_range": "ranged",
            "dps_tier": "heavy",
        },
        "behavior": {
            "aggro_range": "long",
            "social_radius": 20,
            "flee_threshold": 0.0,
            "calls_allies": True,
            "patrol": "stationary",
        },
        "loot_tier": "quest_marked",
        "drops": {
            "gold_copper_avg": 500,
            "item_hint": "valenn's signet + sealed confession (chain finisher)",
        },
        "biome_context": "the expelled Academian behind the Wake; L10 capstone boss of the Drifter's Lair",
        "chain_target": True,
    },
    # Slice 6 — Drifter's Lair trash adds (L8-L10).
    {
        "id": "mob_dalewatch_marches_warband_drifter_brute",
        "name": "Drifter Brute",
        "level": 8,
        "creature_type": "humanoid",
        "armor_class": "leather",
        "rarity": "common",
        "role": "brawler",
        "faction_alignment": "hostile_to_faction_a",
        "hp_tier": "standard",
        "damage": {
            "primary_school": "blade",
            "attack_range": "melee",
            "dps_tier": "standard",
        },
        "behavior": {
            "aggro_range": "medium",
            "social_radius": 10,
            "flee_threshold": 0.0,
            "calls_allies": True,
            "patrol": "stationary",
        },
        "loot_tier": "standard",
        "drops": {
            "gold_copper_avg": 32,
            "item_hint": "cult-rag wraps; chipped iron cleaver",
        },
        "biome_context": "enforcer of the cult; pulls form around him in the Drifter's Lair approach",
    },
    {
        "id": "mob_dalewatch_marches_warband_drifter_acolyte",
        "name": "Whispering Acolyte",
        "level": 9,
        "creature_type": "humanoid",
        "armor_class": "cloth",
        "rarity": "common",
        "role": "caster",
        "faction_alignment": "hostile_to_faction_a",
        "hp_tier": "standard",
        "damage": {
            "primary_school": "shadow",
            "attack_range": "ranged",
            "dps_tier": "standard",
        },
        "behavior": {
            "aggro_range": "long",
            "social_radius": 12,
            "flee_threshold": 0.2,
            "calls_allies": True,
            "patrol": "stationary",
        },
        "loot_tier": "standard",
        "drops": {
            "gold_copper_avg": 40,
            "item_hint": "ritual-inked vellum; bone censer",
        },
        "biome_context": "chants the Wake's litany; backline support in the Drifter's Lair",
    },
    {
        "id": "mob_dalewatch_marches_warband_drifter_fanatic",
        "name": "Lair Fanatic",
        "level": 10,
        "creature_type": "humanoid",
        "armor_class": "leather",
        "rarity": "elite",
        "role": "brawler",
        "faction_alignment": "hostile_to_faction_a",
        "hp_tier": "elite",
        "damage": {
            "primary_school": "blade",
            "attack_range": "melee",
            "dps_tier": "heavy",
        },
        "behavior": {
            "aggro_range": "long",
            "social_radius": 10,
            "flee_threshold": 0.0,
            "calls_allies": True,
            "patrol": "stationary",
        },
        "loot_tier": "standard",
        "drops": {
            "gold_copper_avg": 90,
            "item_hint": "cult-marked steel longsword; lair-passage token",
        },
        "biome_context": "front-line elite at the Drifter's Lair boss-room threshold; one per major pull",
    },
]

ALL_MOB_IDS = EXISTING_MOB_IDS + [m["id"] for m in NEW_MOBS]


# ────────────────────────── data: main chain ──────────────────────────
# Full 10-step chain replacing the existing 5-step file.
MAIN_CHAIN_NPCS: list[dict[str, Any]] = [
    {
        "id": "warden_telyn",
        "display_name": "Warden Telyn",
        "title": "Captain of the Dalewatch",
        "hub_id": "dalewatch_keep",
        "dialogue": (
            "\"Another rider for the march? Good — the border's been strange "
            "this week. A shepherd down by Miller's Crossing lost his grandson "
            "in the reed-brake. Ride out and find Old Brenn; bring the boy "
            "home. I don't like what the tracks out there are saying.\""
        ),
    },
    {
        "id": "old_brenn",
        "display_name": "Old Brenn the Shepherd",
        "title": "Village Elder of Miller's Crossing",
        "hub_id": "miller_crossing",
        "dialogue": (
            "\"Thank the old kings — the Warden sent you. My grandson Tam "
            "took the flock out to the reed-brake at dawn and hasn't come "
            "back. There's tracks in the mud. Not wolf. Not man. Please — "
            "he's all I've got.\""
        ),
    },
    {
        "id": "brother_fennick",
        "display_name": "Brother Fennick",
        "title": "Chapel-Keeper of Harrier's Rest",
        "hub_id": "harriers_rest",
        "dialogue": (
            "\"Academy writing, this is. Old Academy. Someone who studied "
            "under the Censured before they went east. Give me a day with "
            "the folio — I can crack the cipher, but you won't like what "
            "it says. There's a place named. Copperstep.\""
        ),
    },
    {
        "id": "scout_iwen",
        "display_name": "Scout Iwen",
        "title": "Ranger of Ashmere Ford",
        "hub_id": "ford_of_ashmere",
        "dialogue": (
            "\"Pact-worked. I'd know that groove-pattern in the dark. That "
            "focus was cut east of the border — but it's in a Concord "
            "drifter's hand. Whatever they're building in the fens, someone "
            "across the river's been helping.\""
        ),
    },
    {
        "id": "sergeant_rook",
        "display_name": "Sergeant Rook",
        "title": "Camp Commander at Ashmere Ford",
        "hub_id": "ford_of_ashmere",
        "dialogue": (
            "\"The Warden's orders came this morning. You're riding the "
            "strike on the Blackwash — I can't spare riders from the Ford, "
            "but I've marked the cut-trail on your map. End this Wake "
            "business before it crosses the Fen's edge.\""
        ),
    },
]

MAIN_CHAIN_STEPS: list[dict[str, Any]] = [
    {
        "step": 1,
        "name": "Report to Warden Telyn",
        "level": 1,
        "objective": {
            "kind": "talk",
            "npc": "warden_telyn",
            "target_hint": "Warden Telyn at Dalewatch Keep",
        },
        "xp_reward": 520,
        "gold_reward_copper": 15,
    },
    {
        "step": 2,
        "name": "Ride to Old Brenn at Miller's Crossing",
        "level": 1,
        "objective": {
            "kind": "talk",
            "npc": "old_brenn",
            "target_hint": "Old Brenn the Shepherd",
        },
        "xp_reward": 620,
        "gold_reward_copper": 20,
    },
    {
        "step": 3,
        "name": "Follow the Bloodied Trail",
        "level": 2,
        "objective": {
            "kind": "investigate",
            "target_hint": "the bloodied trail into the reed-brake",
            "location": "dalewatch_reed_brake",
        },
        "xp_reward": 720,
        "gold_reward_copper": 30,
    },
    {
        "step": 4,
        "name": "Confront Corlen the Drifter",
        "level": 3,
        "objective": {
            "kind": "kill",
            "target_hint": "Corlen the Drifter-Mage",
            "mob_id": "mob_dalewatch_marches_named_drifter_mage_named",
            "count": 1,
        },
        "xp_reward": 900,
        "gold_reward_copper": 45,
    },
    {
        "step": 5,
        "name": "Return Tam and the Sealed Folio",
        "level": 3,
        "objective": {
            "kind": "deliver",
            "npc": "warden_telyn",
            "target_hint": "return the rescued child and Corlen's folio to Warden Telyn",
        },
        "xp_reward": 1000,
        "gold_reward_copper": 60,
    },
    {
        "step": 6,
        "name": "The Sealed Folio",
        "level": 4,
        "objective": {
            "kind": "talk",
            "npc": "brother_fennick",
            "target_hint": "bring the folio to Brother Fennick at Harrier's Rest",
        },
        "xp_reward": 1100,
        "gold_reward_copper": 80,
    },
    {
        "step": 7,
        "name": "Clear Copperstep",
        "level": 8,
        "objective": {
            "kind": "kill",
            "target_hint": "Master Drifter Halen at Copperstep Mine",
            "mob_id": "mob_dalewatch_marches_named_drifter_master",
            "count": 1,
        },
        "xp_reward": 1600,
        "gold_reward_copper": 120,
    },
    {
        "step": 8,
        "name": "The Pact-Worked Focus",
        "level": 7,
        "objective": {
            "kind": "talk",
            "npc": "scout_iwen",
            "target_hint": "show the recovered focus to Scout Iwen at Ashmere Ford",
        },
        "xp_reward": 1800,
        "gold_reward_copper": 160,
    },
    {
        "step": 9,
        "name": "Strike the Blackwash",
        "level": 8,
        "objective": {
            "kind": "investigate",
            "target_hint": "the cut-trail into the Blackwash Fens",
            "location": "blackwash_fens",
        },
        "xp_reward": 2000,
        "gold_reward_copper": 200,
    },
    {
        "step": 10,
        "name": "The Wake's Bones",
        "level": 10,
        "objective": {
            "kind": "kill",
            "target_hint": "Grand Drifter Valenn and the Brine-Shade Primarch in the Drifter's Lair",
            "mob_id": "mob_dalewatch_marches_named_drifter_valenn",
            "count": 1,
        },
        "xp_reward": 3000,
        "gold_reward_copper": 280,
    },
]


# ────────────────────────── data: side chains ──────────────────────────
SIDE_CHAIN_GRAIN_THIEF = {
    "id": "chain_dalewatch_grain_thief",
    "title": "The Grain Thief",
    "premise": (
        "Miller Hadrin's granary is bleeding sacks. Tracks go upriver, into "
        "the tall-grass flats. Someone knows the mill's schedule."
    ),
    "total_steps": 4,
    "breadcrumb_from": "miller_crossing",
    "final_boss_hint": "bandit caches in the tall-grass flats",
    "final_reward": {
        "xp_bonus": 800,
        "gold_bonus_copper": 200,
        "item_hint": "Miller's chit (minor off-hand accessory)",
    },
    "npcs": [
        {
            "id": "miller_hadrin",
            "display_name": "Miller Hadrin",
            "title": "Millkeep of Miller's Crossing",
            "hub_id": "miller_crossing",
            "dialogue": (
                "\"Another three sacks. They take just enough I can't ignore it "
                "and not so much I can close the mill. That's not wolves. "
                "That's a man who knows my counting-stick.\""
            ),
        },
    ],
    "steps": [
        {
            "step": 1,
            "name": "Speak with Miller Hadrin",
            "level": 3,
            "objective": {
                "kind": "talk",
                "npc": "miller_hadrin",
                "target_hint": "Miller Hadrin at Miller's Crossing",
            },
            "xp_reward": 200,
            "gold_reward_copper": 15,
        },
        {
            "step": 2,
            "name": "Search the Granary",
            "level": 3,
            "objective": {
                "kind": "investigate",
                "target_hint": "the pilfered granary at Miller's Crossing",
                "location": "miller_crossing",
            },
            "xp_reward": 260,
            "gold_reward_copper": 20,
        },
        {
            "step": 3,
            "name": "Break the Bandit Cache",
            "level": 4,
            "objective": {
                "kind": "kill",
                "target_hint": "raiders watching the thief's cache",
                "mob_id": "mob_dalewatch_marches_warband_raid_stray",
                "count": 5,
            },
            "xp_reward": 340,
            "gold_reward_copper": 45,
        },
        {
            "step": 4,
            "name": "Return the Tally",
            "level": 4,
            "objective": {
                "kind": "deliver",
                "npc": "miller_hadrin",
                "target_hint": "bring Hadrin his recovered counting-stick",
            },
            "xp_reward": 400,
            "gold_reward_copper": 60,
        },
    ],
}

SIDE_CHAIN_GROVE_KEEPER = {
    "id": "chain_dalewatch_grove_keeper",
    "title": "The Grove Keeper",
    "premise": (
        "Keeper Anselm of Thornroot Grove is ill and the grove knows it. "
        "Ridge-wolves have moved inside the stones. Someone has to escort "
        "the herb up and the keeper home."
    ),
    "total_steps": 4,
    "breadcrumb_from": "harriers_rest",
    "final_boss_hint": "Ashmane Alpha inside the Thornroot stones",
    "final_reward": {
        "xp_bonus": 1000,
        "gold_bonus_copper": 250,
        "item_hint": "Grove-blessed token (minor healing trinket)",
    },
    "npcs": [
        {
            "id": "sister_avel",
            "display_name": "Sister Avel",
            "title": "Herbalist of Harrier's Rest",
            "hub_id": "harriers_rest",
            "dialogue": (
                "\"Anselm asked for sunweed. I haven't heard him ask for "
                "anything in ten years. The grove is unquiet — if you've got "
                "a strong back, you'd better go.\""
            ),
        },
        {
            "id": "keeper_anselm",
            "display_name": "Keeper Anselm",
            "title": "Warden of Thornroot Grove",
            "hub_id": "harriers_rest",
            "dialogue": (
                "\"The stones are lonely, and the wolves have heard it. I "
                "can walk, but not run. Clear the grove and I'll give you "
                "something the grove remembers.\""
            ),
        },
    ],
    "steps": [
        {
            "step": 1,
            "name": "Speak with Sister Avel",
            "level": 4,
            "objective": {
                "kind": "talk",
                "npc": "sister_avel",
                "target_hint": "Sister Avel at Harrier's Rest",
            },
            "xp_reward": 260,
            "gold_reward_copper": 18,
        },
        {
            "step": 2,
            "name": "Gather Pilgrim's Herb",
            "level": 4,
            "objective": {
                "kind": "collect",
                "target_hint": "six stems of roadside sunweed",
                "count": 6,
            },
            "xp_reward": 320,
            "gold_reward_copper": 25,
        },
        {
            "step": 3,
            "name": "Escort Keeper Anselm to the Grove",
            "level": 5,
            "objective": {
                "kind": "talk",
                "npc": "keeper_anselm",
                "target_hint": "walk Keeper Anselm to Thornroot Grove",
            },
            "xp_reward": 400,
            "gold_reward_copper": 45,
        },
        {
            "step": 4,
            "name": "The Alpha in the Stones",
            "level": 5,
            "objective": {
                "kind": "kill",
                "target_hint": "Ashmane Alpha, the ridge-wolf leader",
                "mob_id": "mob_dalewatch_marches_named_wolf_alpha",
                "count": 1,
            },
            "xp_reward": 520,
            "gold_reward_copper": 80,
        },
    ],
}

SIDE_CHAINS = [SIDE_CHAIN_GRAIN_THIEF, SIDE_CHAIN_GROVE_KEEPER]


# ────────────────────────── data: side quests (20) ──────────────────────────
# Each grouped by hub for the existing `side/<hub>.yaml` pattern.
SIDE_QUESTS_BY_HUB: dict[str, list[dict[str, Any]]] = {
    "dalewatch_keep": [
        {
            "slug": "reynes_armory",
            "name": "Reyne's Armory",
            "level": 2,
            "objective": {
                "kind": "kill",
                "target_hint": "grey wolves culled for the armory's hide stock",
                "count": 8,
            },
            "xp_reward": 140,
            "gold_reward_copper": 25,
        },
        {
            "slug": "quartermasters_cache",
            "name": "The Quartermaster's Cache",
            "level": 2,
            "objective": {
                "kind": "collect",
                "target_hint": "boar tusks for the quartermaster's stamp-stock",
                "count": 10,
            },
            "xp_reward": 140,
            "gold_reward_copper": 25,
        },
        {
            "slug": "stray_recruits",
            "name": "Stray Recruits",
            "level": 3,
            "objective": {
                "kind": "talk",
                "target_hint": "three recruits gone missing on the Kingsroad",
                "count": 3,
            },
            "xp_reward": 180,
            "gold_reward_copper": 35,
        },
        {
            "slug": "letter_to_ashmere",
            "name": "Letter to Ashmere",
            "level": 4,
            "objective": {
                "kind": "deliver",
                "target_hint": "Sergeant Rook at the Ford of Ashmere",
            },
            "xp_reward": 220,
            "gold_reward_copper": 50,
        },
        {
            "slug": "the_silent_bell",
            "name": "The Silent Bell",
            "level": 3,
            "objective": {
                "kind": "investigate",
                "target_hint": "why the chapel bell at Harrier's Rest stopped ringing",
                "location": "harriers_rest",
            },
            "xp_reward": 180,
            "gold_reward_copper": 30,
        },
    ],
    "harriers_rest": [
        {
            "slug": "tending_the_pilgrims",
            "name": "Tending the Pilgrims",
            "level": 2,
            "objective": {
                "kind": "collect",
                "target_hint": "roadside sunweed for the pilgrims' kettle",
                "count": 6,
            },
            "xp_reward": 140,
            "gold_reward_copper": 22,
        },
        {
            "slug": "road_beast_cull",
            "name": "Road-Beast Cull",
            "level": 3,
            "objective": {
                "kind": "kill",
                "target_hint": "wolves harrying the Kingsroad",
                "count": 10,
            },
            "xp_reward": 180,
            "gold_reward_copper": 35,
        },
        {
            "slug": "brothers_favor",
            "name": "Brother's Favor",
            "level": 3,
            "objective": {
                "kind": "deliver",
                "target_hint": "sanctified oil for the mill at Miller's Crossing",
            },
            "xp_reward": 180,
            "gold_reward_copper": 30,
        },
        {
            "slug": "corrupted_reeds",
            "name": "Corrupted Reeds",
            "level": 4,
            "objective": {
                "kind": "collect",
                "target_hint": "tainted reeds from the Reed-Brake",
                "count": 8,
            },
            "xp_reward": 220,
            "gold_reward_copper": 45,
        },
    ],
    "kingsroad_waypost": [
        {
            "slug": "lost_mail",
            "name": "Lost Mail",
            "level": 2,
            "objective": {
                "kind": "collect",
                "target_hint": "scattered courier letter-cases along the Kingsroad",
                "count": 5,
            },
            "xp_reward": 140,
            "gold_reward_copper": 22,
        },
        {
            "slug": "bandit_ambush",
            "name": "Bandit Ambush",
            "level": 3,
            "objective": {
                "kind": "kill",
                "target_hint": "raid strays ambushing courier riders",
                "count": 6,
            },
            "xp_reward": 180,
            "gold_reward_copper": 35,
        },
        {
            "slug": "ridgeline_patrol",
            "name": "Ridgeline Patrol",
            "level": 5,
            "objective": {
                "kind": "investigate",
                "target_hint": "the opened barrow-mounds at Sidlow Cairn",
                "location": "sidlow_cairn",
            },
            "xp_reward": 300,
            "gold_reward_copper": 60,
        },
    ],
    "miller_crossing": [
        {
            "slug": "wolf_cull",
            "name": "The Wolf Cull",
            "level": 3,
            "objective": {
                "kind": "kill",
                "target_hint": "river-bank wolves threatening the flock",
                "count": 12,
            },
            "xp_reward": 200,
            "gold_reward_copper": 40,
        },
        {
            "slug": "mill_stone_recovery",
            "name": "Mill Stone Recovery",
            "level": 4,
            "objective": {
                "kind": "kill",
                "target_hint": "drifter mages hiding stolen grinding stones",
                "count": 4,
            },
            "xp_reward": 240,
            "gold_reward_copper": 55,
        },
        {
            "slug": "lost_flock",
            "name": "Lost Flock",
            "level": 3,
            "objective": {
                "kind": "collect",
                "target_hint": "bell-tokens from the scattered sheep",
                "count": 5,
            },
            "xp_reward": 180,
            "gold_reward_copper": 35,
        },
        {
            "slug": "fishing_trouble",
            "name": "Fishing Trouble",
            "level": 2,
            "objective": {
                "kind": "kill",
                "target_hint": "river otters eating the day's catch",
                "count": 8,
            },
            "xp_reward": 140,
            "gold_reward_copper": 22,
        },
        {
            "slug": "deputys_errand",
            "name": "The Deputy's Errand",
            "level": 5,
            "objective": {
                "kind": "deliver",
                "target_hint": "Sergeant Rook at the Ford of Ashmere",
            },
            "xp_reward": 280,
            "gold_reward_copper": 60,
        },
    ],
    "ford_of_ashmere": [
        {
            "slug": "pact_patrol_kills",
            "name": "Thin the Pact Patrols",
            "level": 6,
            "objective": {
                "kind": "kill",
                "target_hint": "Pact scouts probing the eastern bank",
                "count": 10,
            },
            "xp_reward": 420,
            "gold_reward_copper": 90,
        },
        {
            "slug": "scouts_tokens",
            "name": "Scout Iwen's Tokens",
            "level": 7,
            "objective": {
                "kind": "collect",
                "target_hint": "pact-worked charms off fallen skirmishers",
                "count": 6,
            },
            "xp_reward": 520,
            "gold_reward_copper": 110,
        },
        {
            "slug": "engineers_survey",
            "name": "The Engineer's Survey",
            "level": 6,
            "objective": {
                "kind": "deliver",
                "target_hint": "the survey bag to Engineer Tuck near Copperstep Mine",
            },
            "xp_reward": 420,
            "gold_reward_copper": 90,
        },
    ],
}


# ────────────────────────── data: filler (trimmed) ──────────────────────────
FILLER_BUCKETS = [
    {
        "id": "filler__kill_grinder",
        "bucket": "kill_grinder",
        "dominant_objective_kind": "kill",
        "pool_size": 5,
        "description": "pool of 'kill N of local mob-type' quests scaling with zone mobs",
    },
    {
        "id": "filler__collect_grinder",
        "bucket": "collect_grinder",
        "dominant_objective_kind": "collect",
        "pool_size": 3,
        "description": "pool of 'gather N of local resource' quests tied to biome",
    },
    {
        "id": "filler__courier_relay",
        "bucket": "courier_relay",
        "dominant_objective_kind": "deliver",
        "pool_size": 2,
        "description": "pool of inter-hub courier runs within the zone",
    },
    {
        "id": "filler__bounty_board",
        "bucket": "bounty_board",
        "dominant_objective_kind": "kill",
        "pool_size": 2,
        "description": "rare-spawn bounties posted at capital hubs",
    },
]


# ────────────────────────── yaml helpers ──────────────────────────

class _Dumper(yaml.SafeDumper):
    """Preserve readable list indentation."""


def _represent_str(dumper: yaml.SafeDumper, value: str):
    # Use block scalar for anything with a newline; otherwise plain.
    if "\n" in value:
        return dumper.represent_scalar("tag:yaml.org,2002:str", value, style="|")
    return dumper.represent_scalar("tag:yaml.org,2002:str", value)


_Dumper.add_representer(str, _represent_str)


def dump_yaml(data: Any) -> str:
    return yaml.dump(
        data,
        Dumper=_Dumper,
        sort_keys=False,
        default_flow_style=False,
        allow_unicode=True,
        width=100,
    )


# ────────────────────────── builders ──────────────────────────

def build_core() -> dict[str, Any]:
    return {
        "id": ZONE_ID,
        "name": "The Dalewatch Marches",
        "faction_control": "faction_a",
        "biome": "river_valley",
        "region": "western_dales",
        "tier": "starter",
        "level_range": {"min": 1, "max": 10},
        "starter_race": "mannin",
        "hub_count": len(HUBS),
        "vibe": (
            "Late-medieval river-valley frontier; civic, practical, not heroic."
        ),
        "description": (
            "The Concord's eastern frontier — a green river-valley of yeoman "
            "farms, kingsroads, and warden patrols, watched from Dalewatch "
            "Keep at the heart of the Marches. Three crises meet here. East "
            "at the Ford of Ashmere, Pact scouts are crossing in numbers "
            "nobody can explain. South in the Blackwash Fens, the drifters "
            "who fled the Censure three years ago have made common cause "
            "with something older than the Concord. And along the cairn-"
            "ridges and copper drifts under the keep, the country itself is "
            "starting to misbehave. The Warden Corps rides the line so the "
            "plough stays in the field — but the line is thinning, and you "
            "ride it the morning you take the oath."
        ),
        "prompt": (
            "wide establishing landscape, late-medieval Burgundian river "
            "valley, stone keep on a low ridge above tilled fields, kingsroad "
            "winding past mills, distant marsh on the eastern horizon, summer "
            "dusk warm gold-hour light, painterly atmospheric, no figures, "
            "no characters"
        ),
        "negative_prompt": (
            "modern, futuristic, sci-fi, neon, anime, cyberpunk, "
            "concept sheet, character portrait, figure, person, watermark, text"
        ),
        "budget": {
            "quest_count_target": 50,
            "unique_mob_types": len(ALL_MOB_IDS),
            "mob_kills_to_complete": 600,
            "estimated_hours_to_complete": {"solo": 14.0, "duo": 10.0},
        },
        "notes": (
            "Starter zone in the western dales, redesigned 2026-04 to "
            "starter-scale scope: 12 sub-zones, 4 hubs, 10-step main "
            "chain, two side chains, plus side quests and filler."
        ),
    }


def build_hub(hub: dict[str, Any]) -> dict[str, Any]:
    out: dict[str, Any] = {
        "id": hub["id"],
        "zone": ZONE_ID,
        "name": hub["name"],
        "role": hub["role"],
        "description": hub["description"],
        "prompt": hub["prompt"],
        "amenities": hub["amenities"],
        "quest_givers": hub["quest_givers"],
        "offset_from_zone_origin": {
            "x": hub["offset"][0],
            "z": hub["offset"][1],
        },
    }
    return out


def build_landmarks() -> dict[str, Any]:
    out_landmarks: list[dict[str, Any]] = []
    for lm in LANDMARKS:
        entry: dict[str, Any] = {
            "id": lm["id"],
            "name": lm["name"],
            "offset_from_zone_origin": {"x": lm["offset"][0], "z": lm["offset"][1]},
        }
        if lm.get("description"):
            entry["description"] = lm["description"]
        if lm.get("prompt"):
            entry["prompt"] = lm["prompt"]
        out_landmarks.append(entry)
    return {
        "id": f"landmarks__{ZONE_ID}",
        "zone": ZONE_ID,
        "landmarks": out_landmarks,
    }


def build_mob(mob: dict[str, Any]) -> dict[str, Any]:
    """Translate compact mob record → full YAML shape."""
    out: dict[str, Any] = {
        "id": mob["id"],
        "name": mob["name"],
        "zone": ZONE_ID,
        "level": mob["level"],
        "creature_type": mob["creature_type"],
        "armor_class": mob["armor_class"],
        "rarity": mob["rarity"],
        "role": mob["role"],
        "faction_alignment": mob["faction_alignment"],
        "hp_tier": mob["hp_tier"],
        "damage": mob["damage"],
        "behavior": mob["behavior"],
        "loot_tier": mob["loot_tier"],
        "drops": mob["drops"],
        "biome_context": mob["biome_context"],
    }
    if mob.get("chain_target"):
        out["chain_target"] = True
    return out


def build_roster() -> dict[str, Any]:
    all_mobs = EXISTING_MOB_IDS + [m["id"] for m in NEW_MOBS]

    # Count by creature_type / armor_class / rarity for the existing mobs.
    # We don't re-read their YAML here (the seed is regeneration-from-source);
    # so these rollups reflect the known existing schema + new mobs.
    # Approximate — server's data tests validate the final truth on load.
    type_counts = {"beast": 11, "aberration": 2, "humanoid": 10, "plant": 0}
    for m in NEW_MOBS:
        ct = m["creature_type"]
        # Recount properly — reset then tally from ALL data we have.
    type_counts = {"beast": 0, "aberration": 0, "humanoid": 0, "plant": 0}
    # Tally from existing IDs using their naming conventions.
    for mob_id in EXISTING_MOB_IDS:
        if "exotic_drifter_shade" in mob_id:
            type_counts["aberration"] += 1
        elif "ambient_duck" in mob_id:
            type_counts["beast"] += 1
        elif "warband_drifter_mage" in mob_id or "warband_raid_stray" in mob_id:
            type_counts["humanoid"] += 1
        elif "named_drifter_mage_named" in mob_id:
            type_counts["humanoid"] += 1
        else:
            type_counts["beast"] += 1
    for m in NEW_MOBS:
        type_counts[m["creature_type"]] += 1

    rarity_counts = {"common": 0, "rare": 0, "elite": 0, "named": 0}
    # Existing: 13 common, 1 elite, 1 named (from existing _roster.yaml).
    rarity_counts["common"] += 13
    rarity_counts["elite"] += 1
    rarity_counts["named"] += 1
    for m in NEW_MOBS:
        rarity_counts[m["rarity"]] = rarity_counts.get(m["rarity"], 0) + 1

    armor_counts: dict[str, int] = {}
    # Existing roster values carried forward.
    for k, v in {"hide": 11, "ethereal": 1, "cloth": 1, "leather": 1, "plate": 1}.items():
        armor_counts[k] = v
    for m in NEW_MOBS:
        armor_counts[m["armor_class"]] = armor_counts.get(m["armor_class"], 0) + 1

    return {
        "id": f"_roster__{ZONE_ID}",
        "zone": ZONE_ID,
        "mob_count": len(all_mobs),
        "mob_ids": all_mobs,
        "by_creature_type": type_counts,
        "by_armor_class": armor_counts,
        "by_rarity": rarity_counts,
        "level_min": 1,
        "level_max": 10,
        "named_or_chain_targets": [
            m["id"] for m in NEW_MOBS if m.get("chain_target") or m["rarity"] == "named"
        ] + ["mob_dalewatch_marches_named_drifter_mage_named"],
    }


def _step_with_prereq(chain_id: str, steps: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """Wire up step ids + prerequisites linearly."""
    prev_id: str | None = None
    out: list[dict[str, Any]] = []
    for s in steps:
        step_num = s["step"]
        step_slug = s["name"].lower()
        for ch in [" ", "'", ",", "-", "."]:
            step_slug = step_slug.replace(ch, "_" if ch == " " or ch == "-" else "")
        step_slug = step_slug.replace("__", "_").strip("_")
        step_id = f"{chain_id}__{step_num:02d}_{step_slug}"
        step_out = dict(s)
        step_out["id"] = step_id
        step_out["prerequisite"] = prev_id
        out.append(step_out)
        prev_id = step_id
    return out


def build_main_chain() -> dict[str, Any]:
    chain_id = "chain_dalewatch_first_ride"
    return {
        "id": chain_id,
        "zone": ZONE_ID,
        "title": "The First Ride",
        "premise": (
            "A new Dalewatch rider takes the Warden's Oath — then rides "
            "into a missing-child case that widens into something the "
            "Academies buried three years ago. What starts as a patrol "
            "ends at the edge of the Blackwash Fens, where a Grand "
            "Drifter has cut a seal no one was meant to cut."
        ),
        "total_steps": len(MAIN_CHAIN_STEPS),
        "breadcrumb_from": "dalewatch_keep",
        "final_boss_hint": "Grand Drifter Valenn and the Brine-Shade Primarch in the Drifter's Lair",
        "final_reward": {
            "xp_bonus": 3500,
            "gold_bonus_copper": 1400,
            "item_hint": "Warden's full cloak + Ashmere patrol-sigil + rider's steel belt",
            "title_hint": "'Rider of the Marches'",
        },
        "npcs": MAIN_CHAIN_NPCS,
        "steps": _step_with_prereq(chain_id, [
            {k: v for k, v in s.items() if k != "id"} for s in MAIN_CHAIN_STEPS
        ]),
    }


def build_side_chain(chain: dict[str, Any]) -> dict[str, Any]:
    chain_id = chain["id"]
    return {
        "id": chain_id,
        "zone": ZONE_ID,
        "title": chain["title"],
        "premise": chain["premise"],
        "total_steps": chain["total_steps"],
        "breadcrumb_from": chain["breadcrumb_from"],
        "final_boss_hint": chain["final_boss_hint"],
        "final_reward": chain["final_reward"],
        "npcs": chain["npcs"],
        "steps": _step_with_prereq(chain_id, chain["steps"]),
    }


def build_side_hub(hub_id: str, quests: list[dict[str, Any]]) -> dict[str, Any]:
    hub = next(h for h in HUBS if h["id"] == hub_id)
    return {
        "id": f"side_quests__{hub_id}",
        "hub": hub_id,
        "hub_role": hub["role"],
        "zone": ZONE_ID,
        "biome": "river_valley",
        "quest_count": len(quests),
        "quests": [
            {
                "id": f"side__{hub_id}__{q['slug']}",
                "name": q["name"],
                "hub": hub_id,
                "type": "side",
                "level": q["level"],
                "objective": q["objective"],
                "xp_reward": q["xp_reward"],
                "gold_reward_copper": q["gold_reward_copper"],
                "repeatable": False,
            }
            for q in quests
        ],
    }


def build_filler() -> dict[str, Any]:
    total = sum(b["pool_size"] for b in FILLER_BUCKETS)
    return {
        "id": f"filler_pool__{ZONE_ID}",
        "zone": ZONE_ID,
        "tier": "starter",
        "total_pool_size": total,
        "buckets": [
            {
                "id": b["id"],
                "bucket": b["bucket"],
                "dominant_objective_kind": b["dominant_objective_kind"],
                "pool_size": b["pool_size"],
                "level_range": {"min": 1, "max": 8},
                "avg_xp_reward_per_quest": 180,
                "avg_gold_reward_copper": 30,
                "repeatable": b["bucket"] in ("kill_grinder", "collect_grinder", "courier_relay"),
                "description": b["description"],
            }
            for b in FILLER_BUCKETS
        ],
    }


def build_summary() -> dict[str, Any]:
    main_chain_steps = len(MAIN_CHAIN_STEPS)
    side_chain_steps = sum(len(c["steps"]) for c in SIDE_CHAINS)
    side_quest_count = sum(len(v) for v in SIDE_QUESTS_BY_HUB.values())
    filler_total = sum(b["pool_size"] for b in FILLER_BUCKETS)
    total = main_chain_steps + side_chain_steps + side_quest_count + filler_total
    return {
        "id": f"quest_summary__{ZONE_ID}",
        "zone": ZONE_ID,
        "level_range": {"min": 1, "max": 8},
        "tier": "starter",
        "budget_target": 50,
        "actual": {
            "main_chain_steps": main_chain_steps,
            "side_chain_steps": side_chain_steps,
            "side_quests": side_quest_count,
            "filler_pool": filler_total,
            "total": total,
        },
        "coverage_ratio": round(total / 50.0, 2),
        "main_chain": "chain_dalewatch_first_ride",
        "side_chains": [c["id"] for c in SIDE_CHAINS],
        "notes": (
            "Starter-scale redesign — 12 sub-zones across the "
            "county, 4 hubs, hand-written main + 2 side chains, 20 "
            "side quests, and a trimmed filler pool."
        ),
    }


# ────────────────────────── writer ──────────────────────────

@dataclass
class Write:
    path: Path
    content: str

    def bytes(self) -> int:
        return len(self.content.encode("utf-8"))


def plan_writes() -> list[Write]:
    writes: list[Write] = []

    # core.yaml (update)
    writes.append(Write(ZONE_DIR / "core.yaml", dump_yaml(build_core())))

    # landmarks.yaml (new)
    writes.append(Write(ZONE_DIR / "landmarks.yaml", dump_yaml(build_landmarks())))

    # hubs/
    for hub in HUBS:
        writes.append(Write(
            ZONE_DIR / "hubs" / f"{hub['id']}.yaml",
            dump_yaml(build_hub(hub)),
        ))

    # mobs/
    for mob in NEW_MOBS:
        writes.append(Write(
            ZONE_DIR / "mobs" / f"{mob['id']}.yaml",
            dump_yaml(build_mob(mob)),
        ))
    # mobs/_roster.yaml
    writes.append(Write(
        ZONE_DIR / "mobs" / "_roster.yaml",
        dump_yaml(build_roster()),
    ))

    # quests/chains/
    writes.append(Write(
        ZONE_DIR / "quests" / "chains" / "chain_dalewatch_first_ride.yaml",
        dump_yaml(build_main_chain()),
    ))
    for sc in SIDE_CHAINS:
        writes.append(Write(
            ZONE_DIR / "quests" / "chains" / f"{sc['id']}.yaml",
            dump_yaml(build_side_chain(sc)),
        ))

    # quests/side/<hub>.yaml
    for hub_id, quests in SIDE_QUESTS_BY_HUB.items():
        writes.append(Write(
            ZONE_DIR / "quests" / "side" / f"{hub_id}.yaml",
            dump_yaml(build_side_hub(hub_id, quests)),
        ))

    # quests/filler.yaml
    writes.append(Write(
        ZONE_DIR / "quests" / "filler.yaml",
        dump_yaml(build_filler()),
    ))

    # quests/_summary.yaml
    writes.append(Write(
        ZONE_DIR / "quests" / "_summary.yaml",
        dump_yaml(build_summary()),
    ))

    return writes


# ────────────────────────── main ──────────────────────────

def main() -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--dry-run", action="store_true", help="print targets, no writes")
    p.add_argument("--force", action="store_true", help="overwrite existing files")
    args = p.parse_args()

    writes = plan_writes()

    total_bytes = sum(w.bytes() for w in writes)
    print(f"planned {len(writes)} files, {total_bytes:,} bytes total")
    for w in writes:
        status = "EXISTS" if w.path.exists() else "NEW   "
        rel = w.path.relative_to(ROOT)
        print(f"  [{status}] {w.bytes():>6}B  {rel}")

    if args.dry_run:
        print("\ndry-run: nothing written")
        return 0

    # Check conflicts.
    conflicts = [w for w in writes if w.path.exists()]
    if conflicts and not args.force:
        print(f"\n{len(conflicts)} existing files would be overwritten; rerun with --force")
        return 1

    for w in writes:
        w.path.parent.mkdir(parents=True, exist_ok=True)
        w.path.write_text(w.content, encoding="utf-8")
    print(f"\nwrote {len(writes)} files")
    return 0


if __name__ == "__main__":
    sys.exit(main())
