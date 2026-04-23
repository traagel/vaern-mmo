#!/usr/bin/env python3
"""Seed src/generated/world/ with a WoW-Classic-style zone/progression scaffold.

Tier structure (28 zones, level 1-60, ~200h /played to cap):
  - 10 race starter zones (1-5)
  - 10 mid-tier faction zones (5-30)
  -  4 contested zones (30-45) along the Ruin Line
  -  4 endgame zones (45-60)

Writes: world.yaml, progression/*, biomes/*, continents/*, zones/<id>/core.yaml
and zones/<id>/hubs/<hub>.yaml. Idempotent — safe to re-run.
"""
from __future__ import annotations

from pathlib import Path
import yaml

REPO = Path(__file__).resolve().parents[1]
OUT = REPO / "src" / "generated" / "world"


def write(path: Path, data: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "w") as f:
        yaml.safe_dump(data, f, sort_keys=False, default_flow_style=False)


# ---------------------------------------------------------------------------
# Top-level world knobs
# ---------------------------------------------------------------------------

WORLD = {
    "id": "vaern",
    "setting_name": "Vaern",
    "max_level": 60,
    "faction_split": {
        "faction_a": "Concord (Veyr) — central & western two-thirds",
        "faction_b": "Rend (Hraun) — eastern third",
        "contested": "the Ruin Line — middle strip",
    },
    "design_reference": "WoW Classic zone-level structure — 3-8 levels per zone, shared mid-tier, contested endgame",
    "target_time_to_cap": {
        "played_hours_min": 150,
        "played_hours_max": 240,
        "days_active": "5-10 days of /played time",
        "basis": "Classic-style grind tuned slightly faster; strict-coop means duo/trio pace is the balance target, not solo",
    },
    "design_principles": [
        "every zone is coop-teachable — no solo-only content, even at level 1",
        "quest hubs cluster into 2-4 per zone; each hub offers enough xp to push ~1 level",
        "mob density is tuned so a duo can level on kills alone at ~60% of quest pace",
        "contested zones bleed PvP rules; endgame zones are the only true raid-scale content",
    ],
}

PROGRESSION_XP_CURVE = {
    "id": "xp_curve",
    "description": "XP required to advance level L -> L+1. Roughly quadratic; ~7M cumulative to 60.",
    "formula": "xp_to_next(L) = floor(400 * L + 120 * L^2)",
    "table": {
        L: int(400 * L + 120 * (L ** 2)) for L in range(1, 60)
    },
    "cumulative_to_60": sum(int(400 * L + 120 * (L ** 2)) for L in range(1, 60)),
    "notes": [
        "L1→L2 = 520; L30→L31 = 120,000; L59→L60 = 441,320",
        "cumulative 59→60 single-level represents ~6% of total grind — deliberate long tail",
    ],
}

PROGRESSION_RESTED = {
    "id": "rested_xp",
    "description": "WoW-style rested bonus accumulated when logged out at a hearth/hub.",
    "accrual": {
        "per_hour_logged_in_hub": 0.05,
        "unit": "fraction of one level's XP",
        "cap_multiplier": 1.5,
        "cap_basis": "multiples of one full level of rested pool",
    },
    "consumption": {
        "mob_kill_multiplier": 2.0,
        "quest_completion_multiplier": 1.0,
        "notes": "Rested doubles mob-kill XP only, not quest turn-ins — encourages hub-return rhythm",
    },
}

PROGRESSION_SESSION_TARGETS = {
    "id": "session_targets",
    "description": "Per-level-band targets. Drives zone design (mobs, quests, hub density).",
    "bands": [
        {
            "levels": "1-10",
            "hours_expected": 15,
            "quests_per_level": 10,
            "mob_kills_per_level": 80,
            "notes": "Starter zones. High-density quests, short travel, hub-to-hub breadcrumbs.",
        },
        {
            "levels": "11-30",
            "hours_expected": 60,
            "quests_per_level": 14,
            "mob_kills_per_level": 140,
            "notes": "Faction mid-tier. Players ride between 2-3 zones; elite mobs introduce group play.",
        },
        {
            "levels": "31-45",
            "hours_expected": 60,
            "quests_per_level": 16,
            "mob_kills_per_level": 200,
            "notes": "Contested zones + late faction zones. First dungeons at lvl 30.",
        },
        {
            "levels": "46-60",
            "hours_expected": 80,
            "quests_per_level": 12,
            "mob_kills_per_level": 300,
            "notes": "Endgame ramp. Quest count drops; mob density rises; dungeons carry the bulk.",
        },
    ],
    "totals": {
        "quests_approx": 10 * 10 + 14 * 20 + 16 * 15 + 12 * 15,
        "mob_kills_approx": 80 * 10 + 140 * 20 + 200 * 15 + 300 * 15,
    },
}


# ---------------------------------------------------------------------------
# Biomes
# ---------------------------------------------------------------------------

BIOMES = [
    {
        "id": "temperate_forest",
        "name": "Temperate Forest",
        "climate": "mild, seasonal, deep leaf-canopy",
        "hazards": ["ambush-from-cover", "winter snow-travel penalty"],
        "typical_flora": ["oak", "hazel", "fern", "moss"],
        "typical_fauna": ["wolf", "boar", "stag", "corvid", "bear"],
        "faction_affinity": "faction_a",
    },
    {
        "id": "highland",
        "name": "Forested Highlands",
        "climate": "cool, windswept, fog-prone",
        "hazards": ["cliff-fall", "sudden fog ambush"],
        "typical_flora": ["pine", "heather", "gorse"],
        "typical_fauna": ["hillcat", "hawk", "mountain-goat"],
        "faction_affinity": "faction_a",
    },
    {
        "id": "river_valley",
        "name": "River Valley & Farmland",
        "climate": "warm, fertile, long summer",
        "hazards": ["flood-season", "bandit roads"],
        "typical_flora": ["wheat", "willow", "reed"],
        "typical_fauna": ["boar", "river-otter", "heron"],
        "faction_affinity": "faction_a",
    },
    {
        "id": "marshland",
        "name": "Marshland",
        "climate": "humid, fog-bound, disease-prone",
        "hazards": ["sinking-mire", "miasma", "rot-fever"],
        "typical_flora": ["sedge", "bog-myrtle", "lichen"],
        "typical_fauna": ["crocodilian", "blood-gnat", "marsh-drake"],
        "faction_affinity": "faction_b",
    },
    {
        "id": "coastal_cliff",
        "name": "Rocky Cliff Coast",
        "climate": "cold, salt-wind, storm-battered",
        "hazards": ["cliff-fall", "storm-wreck beaches", "sea-raid"],
        "typical_flora": ["sea-thrift", "kelp", "samphire"],
        "typical_fauna": ["seabird", "seal", "cliff-wyrm"],
        "faction_affinity": "contested",
    },
    {
        "id": "fjord",
        "name": "Fjord Coast",
        "climate": "cold, deep-water harbors, island-strewn",
        "hazards": ["ice-floe", "raider-longship landings"],
        "typical_flora": ["pine", "moss", "kelp"],
        "typical_fauna": ["seal", "orca", "fjord-serpent"],
        "faction_affinity": "faction_b",
    },
    {
        "id": "mountain",
        "name": "Mountain Interior",
        "climate": "alpine, thin air, blizzard-season",
        "hazards": ["rockfall", "exposure", "ancient collapsed ruin"],
        "typical_flora": ["lichen", "stonewort", "pine"],
        "typical_fauna": ["yeti-analogue", "rock-drake", "mountain-bear"],
        "faction_affinity": "contested",
    },
    {
        "id": "ruin",
        "name": "The Ruin Line",
        "climate": "scar-land — ash, broken earth, charnel pits",
        "hazards": ["unstable-ground", "ambient curse", "leftover-thing"],
        "typical_flora": ["char-thistle", "ghostgrass"],
        "typical_fauna": ["revenant", "carrion-hound", "war-ghost"],
        "faction_affinity": "contested",
    },
    {
        "id": "ashland",
        "name": "Ashen Wasteland",
        "climate": "dry, volcanic-ash soil, low visibility",
        "hazards": ["ash-storm", "pit-vents", "soot-lung"],
        "typical_flora": ["ashroot", "scorched-pine"],
        "typical_fauna": ["ashwolf", "cinder-drake"],
        "faction_affinity": "faction_b",
    },
]


# ---------------------------------------------------------------------------
# Continent
# ---------------------------------------------------------------------------

CONTINENT = {
    "id": "vaern_mainland",
    "name": "Vaern",
    "type": "island-continent",
    "scale": "approximately Britain + Ireland combined",
    "climate": "temperate, strongly seasonal, harsh winters",
    "regions": [
        {"id": "western_dales", "faction": "faction_a", "biomes": ["river_valley", "temperate_forest"]},
        {"id": "central_highlands", "faction": "faction_a", "biomes": ["highland", "temperate_forest"]},
        {"id": "iron_mountains", "faction": "faction_a", "biomes": ["mountain"]},
        {"id": "southern_shore", "faction": "faction_a", "biomes": ["river_valley", "coastal_cliff"]},
        {"id": "northern_cliffs", "faction": "faction_a", "biomes": ["coastal_cliff", "highland"]},
        {"id": "ruin_line", "faction": "contested", "biomes": ["ruin", "mountain"]},
        {"id": "eastern_fjords", "faction": "faction_b", "biomes": ["fjord", "coastal_cliff"]},
        {"id": "ashen_march", "faction": "faction_b", "biomes": ["ashland", "marshland"]},
        {"id": "pact_steppes", "faction": "faction_b", "biomes": ["marshland", "highland"]},
        {"id": "barrow_shore", "faction": "faction_b", "biomes": ["coastal_cliff", "ruin"]},
    ],
}


# ---------------------------------------------------------------------------
# Zones
# ---------------------------------------------------------------------------

# tier: starter | mid | contested | endgame
# Each zone: (id, name, faction, biome, region, level_min, level_max, starter_race, hubs)
# hubs are (hub_id, hub_name, role). role: capital | outpost | waypoint | ruin

ZONES = [
    # --- STARTER (10) — one per race, levels 1-5 -------------------------------
    ("dalewatch_marches", "The Dalewatch Marches", "faction_a", "river_valley",
     "western_dales", 1, 5, "mannin",
     [("dalewatch_keep", "Dalewatch Keep", "capital"),
      ("miller_crossing", "Miller's Crossing", "outpost")]),
    ("stoneguard_deep", "Stoneguard Deep", "faction_a", "mountain",
     "iron_mountains", 1, 5, "hearthkin",
     [("stoneguard_halls", "Stoneguard Halls", "capital"),
      ("anvilgate", "Anvilgate", "outpost")]),
    ("sunward_reach", "Sunward Reach", "faction_a", "highland",
     "central_highlands", 1, 5, "sunward_elen",
     [("sunward_spire", "Sunward Spire", "capital"),
      ("dawnleaf_glade", "Dawnleaf Glade", "outpost")]),
    ("wyrling_downs", "The Wyrling Downs", "faction_a", "temperate_forest",
     "western_dales", 1, 5, "wyrling",
     [("market_town", "Wyrling Market-Town", "capital"),
      ("brackenhollow", "Brackenhollow", "outpost")]),
    ("firland_greenwood", "Firland Greenwood", "faction_a", "temperate_forest",
     "central_highlands", 1, 5, "firland",
     [("greenmoot", "The Greenmoot", "capital"),
      ("oldfir_camp", "Old-Fir Camp", "outpost")]),

    ("ashen_holt", "Ashen Holt", "faction_b", "ashland",
     "ashen_march", 1, 5, "darkling_elen",
     [("shadegrove_spire", "Shadegrove Spire", "capital"),
      ("blackleaf_bower", "Blackleaf Bower", "outpost")]),
    ("barrow_coast", "The Barrow Coast", "faction_b", "coastal_cliff",
     "barrow_shore", 1, 5, "gravewrought",
     [("iron_barrow", "Iron-Barrow", "capital"),
      ("silent_harbor", "Silent Harbor", "outpost")]),
    ("pactmarch", "The Pactmarch", "faction_b", "highland",
     "pact_steppes", 1, 5, "kharun",
     [("pactstone_hold", "Pactstone Hold", "capital"),
      ("scholar_camp", "Scholar's Camp", "outpost")]),
    ("skarnreach", "Skarnreach", "faction_b", "fjord",
     "eastern_fjords", 1, 5, "skarn",
     [("skarn_longhall", "Skarn Longhall", "capital"),
      ("bloodwater_landing", "Bloodwater Landing", "outpost")]),
    ("scrap_marsh", "The Scrap-Marsh", "faction_b", "marshland",
     "ashen_march", 1, 5, "skrel",
     [("junk_warren", "The Junk-Warren", "capital"),
      ("trap_post", "Trap-Post", "outpost")]),

    # --- MID TIER (10) — 5 per faction, 5-30 ------------------------------------
    ("heartland_ride", "The Heartland Ride", "faction_a", "river_valley",
     "western_dales", 5, 12, None,
     [("ride_crossroads", "Ride Crossroads", "capital"),
      ("flood_mill", "Flood-Mill", "outpost"),
      ("bandit_watchtower", "Bandit-Watch Tower", "outpost")]),
    ("irongate_pass", "Irongate Pass", "faction_a", "mountain",
     "iron_mountains", 10, 18, None,
     [("irongate_keep", "Irongate Keep", "capital"),
      ("snowline_camp", "Snowline Camp", "outpost"),
      ("collapsed_forge", "Collapsed Forge", "ruin")]),
    ("silverleaf_wood", "Silverleaf Wood", "faction_a", "temperate_forest",
     "central_highlands", 15, 22, None,
     [("silverleaf_lodge", "Silverleaf Lodge", "capital"),
      ("moth_hollow", "Moth-Hollow", "outpost"),
      ("elder_grove", "Elder Grove", "waypoint")]),
    ("market_crossing", "Market Crossing", "faction_a", "river_valley",
     "southern_shore", 18, 26, None,
     [("crossing_freeport", "Crossing Freeport", "capital"),
      ("silver_caravanserai", "Silver Caravanserai", "outpost"),
      ("old_river_watch", "Old River Watch", "waypoint")]),
    ("greenwood_deep", "The Greenwood Deep", "faction_a", "temperate_forest",
     "central_highlands", 22, 30, None,
     [("deep_moot", "The Deep Moot", "capital"),
      ("moss_cairn", "Moss Cairn", "outpost"),
      ("hunter_leanto", "Hunter's Lean-To", "waypoint"),
      ("wyrd_tarn", "Wyrd Tarn", "ruin")]),

    ("gravewatch_fields", "Gravewatch Fields", "faction_b", "coastal_cliff",
     "barrow_shore", 5, 12, None,
     [("watch_crypt", "Watch-Crypt", "capital"),
      ("mourner_camp", "Mourner's Camp", "outpost"),
      ("lich_weep", "Lich-Weep", "ruin")]),
    ("shadegrove", "Shadegrove", "faction_b", "ashland",
     "ashen_march", 10, 18, None,
     [("shadegrove_reach", "Shadegrove Reach", "capital"),
      ("cinder_post", "Cinder Post", "outpost"),
      ("dark_glade", "Dark Glade", "waypoint")]),
    ("pact_causeway", "The Pact Causeway", "faction_b", "highland",
     "pact_steppes", 15, 22, None,
     [("causeway_hold", "Causeway Hold", "capital"),
      ("stone_reader", "Stone-Reader Camp", "outpost"),
      ("oath_pillar", "The Oath Pillar", "ruin")]),
    ("skarncamp_wastes", "Skarncamp Wastes", "faction_b", "fjord",
     "eastern_fjords", 18, 26, None,
     [("ravenmeet", "Ravenmeet Longhall", "capital"),
      ("war_stake", "War-Stake Camp", "outpost"),
      ("salt_watch", "Salt Watch", "waypoint")]),
    ("scrap_flats", "The Scrap Flats", "faction_b", "marshland",
     "ashen_march", 22, 30, None,
     [("great_warren", "The Great Warren", "capital"),
      ("rust_creek", "Rust Creek", "outpost"),
      ("scavenger_mound", "Scavenger Mound", "outpost"),
      ("pit_tinker", "Pit-Tinker Works", "ruin")]),

    # --- CONTESTED (4) — 30-45, Ruin Line and flanks ---------------------------
    ("ruin_line_north", "The Ruin Line — Northern Scar", "contested", "ruin",
     "ruin_line", 30, 38, None,
     [("scarhold", "Scarhold", "waypoint"),
      ("burned_abbey", "The Burned Abbey", "ruin"),
      ("no_man_ridge", "No-Man's Ridge", "outpost")]),
    ("ruin_line_south", "The Ruin Line — Southern Scar", "contested", "ruin",
     "ruin_line", 30, 38, None,
     [("southscar_camp", "Southscar Camp", "waypoint"),
      ("sunken_tower", "The Sunken Tower", "ruin"),
      ("two_kings_field", "Field of Two Kings", "outpost")]),
    ("iron_strand", "The Iron Strand", "contested", "coastal_cliff",
     "northern_cliffs", 38, 45, None,
     [("strand_redoubt", "Strand Redoubt", "waypoint"),
      ("wreckshore", "Wreckshore", "outpost"),
      ("old_lighthouse", "The Old Lighthouse", "ruin")]),
    ("ashweald", "The Ashweald", "contested", "ashland",
     "ruin_line", 38, 45, None,
     [("ember_crossing", "Ember Crossing", "waypoint"),
      ("charred_shrine", "The Charred Shrine", "ruin"),
      ("pyre_camp", "Pyre Camp", "outpost")]),

    # --- ENDGAME (4) — 45-60 -----------------------------------------------------
    ("blackwater_deep", "The Blackwater Deep", "contested", "marshland",
     "ruin_line", 45, 52, None,
     [("deepwater_redoubt", "Deepwater Redoubt", "waypoint"),
      ("drowned_abbey", "The Drowned Abbey", "ruin"),
      ("black_eel_camp", "Black-Eel Camp", "outpost")]),
    ("frost_spine", "The Frost Spine", "contested", "mountain",
     "iron_mountains", 48, 55, None,
     [("frost_gate", "The Frost Gate", "waypoint"),
      ("yeti_warren", "Yeti-Warren", "outpost"),
      ("sundered_peak", "Sundered Peak", "ruin")]),
    ("sundering_mines", "The Sundering Mines", "contested", "mountain",
     "ruin_line", 50, 58, None,
     [("mineshaft_alpha", "Mineshaft Alpha", "waypoint"),
      ("old_tunnel_camp", "Old-Tunnel Camp", "outpost"),
      ("lightless_gallery", "The Lightless Gallery", "ruin")]),
    ("crown_of_ruin", "The Crown of Ruin", "contested", "ruin",
     "ruin_line", 55, 60, None,
     [("crown_bastion", "Crown Bastion", "waypoint"),
      ("throne_of_ash", "The Throne of Ash", "ruin"),
      ("last_candle", "The Last Candle", "outpost")]),
]


def zone_entry(z) -> dict:
    zid, name, faction, biome, region, lo, hi, race, hubs = z
    tier = (
        "starter" if hi <= 5
        else "mid" if hi <= 30
        else "contested" if hi <= 45
        else "endgame"
    )
    levels = hi - lo + 1
    band = (
        "1-10" if lo <= 10
        else "11-30" if lo <= 30
        else "31-45" if lo <= 45
        else "46-60"
    )
    band_targets = {
        "1-10": (10, 80),
        "11-30": (14, 140),
        "31-45": (16, 200),
        "46-60": (12, 300),
    }[band]
    quests = levels * band_targets[0]
    mob_kills = levels * band_targets[1]
    return {
        "id": zid,
        "name": name,
        "faction_control": faction,
        "biome": biome,
        "region": region,
        "tier": tier,
        "level_range": {"min": lo, "max": hi},
        "starter_race": race,
        "hub_count": len(hubs),
        "budget": {
            "quest_count_target": quests,
            "unique_mob_types": max(8, levels * 3),
            "mob_kills_to_complete": mob_kills,
            "estimated_hours_to_complete": {
                "solo": round(levels * {"1-10": 1.8, "11-30": 3.5, "31-45": 4.5, "46-60": 6.5}[band], 1),
                "duo": round(levels * {"1-10": 1.3, "11-30": 2.5, "31-45": 3.2, "46-60": 4.5}[band], 1),
            },
        },
        "notes": f"{tier.title()} zone in the {region.replace('_', ' ')}. Targets {band} band session math.",
    }


def hub_entry(zid: str, hub) -> dict:
    hub_id, hub_name, role = hub
    return {
        "id": hub_id,
        "zone": zid,
        "name": hub_name,
        "role": role,
        "amenities": {
            "capital": ["innkeeper", "class_trainers", "bank", "auction", "flight_master", "profession_trainers"],
            "outpost": ["innkeeper", "flight_master", "repair"],
            "waypoint": ["innkeeper", "flight_master"],
            "ruin": [],
        }[role],
        "quest_givers": {
            "capital": 6,
            "outpost": 3,
            "waypoint": 2,
            "ruin": 1,
        }[role],
    }


# ---------------------------------------------------------------------------
# Write
# ---------------------------------------------------------------------------

def main() -> None:
    write(OUT / "world.yaml", WORLD)
    write(OUT / "progression" / "xp_curve.yaml", PROGRESSION_XP_CURVE)
    write(OUT / "progression" / "rested_xp.yaml", PROGRESSION_RESTED)
    write(OUT / "progression" / "session_targets.yaml", PROGRESSION_SESSION_TARGETS)
    for b in BIOMES:
        write(OUT / "biomes" / f"{b['id']}.yaml", b)
    write(OUT / "continents" / f"{CONTINENT['id']}.yaml", CONTINENT)

    for z in ZONES:
        zid = z[0]
        hubs = z[-1]
        write(OUT / "zones" / zid / "core.yaml", zone_entry(z))
        for hub in hubs:
            write(OUT / "zones" / zid / "hubs" / f"{hub[0]}.yaml", hub_entry(zid, hub))

    n_zones = len(ZONES)
    n_hubs = sum(len(z[-1]) for z in ZONES)
    print(f"wrote world scaffold: {n_zones} zones, {n_hubs} hubs, {len(BIOMES)} biomes")


if __name__ == "__main__":
    main()
