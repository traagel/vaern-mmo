#!/usr/bin/env python3
"""Seed src/generated/world/zones/<zone>/mobs/ with per-zone rosters.

Each zone gets:
  - biome-templated beasts (~4-6)
  - biome/ambient creatures (~2-3)
  - zone-specific humanoid warband (~4-8, scaled to zone size)
  - biome-appropriate undead/aberration/elemental mix (~2-4)
  - 1-3 named/elite mobs tied to main-chain final boss

Writes <mob_id>.yaml per mob + _roster.yaml per zone. Idempotent.
"""
from __future__ import annotations

from pathlib import Path
import yaml

REPO = Path(__file__).resolve().parents[1]
WORLD = REPO / "src" / "generated" / "world"
BESTIARY = REPO / "src" / "generated" / "bestiary"


def write(path: Path, data: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "w") as f:
        yaml.safe_dump(data, f, sort_keys=False, default_flow_style=False)


def load(path: Path) -> dict:
    with open(path) as f:
        return yaml.safe_load(f)


# Load bestiary at module load — needed for validation + default armor_class resolution
def _load_bestiary() -> tuple[dict, dict]:
    types: dict = {}
    armors: dict = {}
    types_dir = BESTIARY / "creature_types"
    armors_dir = BESTIARY / "armor_classes"
    if not types_dir.exists() or not armors_dir.exists():
        raise SystemExit(
            "bestiary/ not found — run scripts/seed_bestiary.py before seed_mobs.py"
        )
    for p in types_dir.glob("*.yaml"):
        t = load(p)
        types[t["id"]] = t
    for p in armors_dir.glob("*.yaml"):
        a = load(p)
        armors[a["id"]] = a
    return types, armors


CREATURE_TYPES, ARMOR_CLASSES = _load_bestiary()


# ---------------------------------------------------------------------------
# Biome palettes: (slot_key, name, family, attack_range, dps_tier, flavor)
# dps_tier: light | standard | heavy
# ---------------------------------------------------------------------------

BIOME_BEASTS = {
    "river_valley": [
        ("wolf",        "Grey Wolf",          "beast", "melee",       "standard", "pack-predator, common along the riverbank"),
        ("boar",        "River Boar",         "beast", "melee",       "standard", "charging-tusker, reed-brake native"),
        ("stag",        "Tall Stag",          "beast", "melee",       "light",    "antler-charge, flees at low HP"),
        ("heron",       "Reed Heron",         "beast", "ranged_short", "light",   "spear-beak critter, skittish"),
        ("otter",       "River-Otter Rake",   "beast", "melee",       "light",    "ambient, opportunistic nibble"),
        ("bear",        "Brown Bear",         "beast", "melee",       "heavy",    "territorial apex, slow but lethal"),
    ],
    "highland": [
        ("hillcat",     "Hill Cat",           "beast", "melee",       "standard", "ambush-stalker in fog"),
        ("stonehawk",   "Stone-Hawk",         "beast", "ranged_short", "light",   "stoop-attacker from above"),
        ("goat",        "Mountain Goat",      "beast", "melee",       "light",    "headbutt, high-ground advantage"),
        ("fogelk",      "Fog-Elk",            "beast", "melee",       "standard", "antler-pack, fog-cover"),
        ("cliffcrag",   "Cragside Badger",    "beast", "melee",       "light",    "ambient, burrow-shelter"),
        ("hillbear",    "Highland Bear",      "beast", "melee",       "heavy",    "apex-predator of the heights"),
    ],
    "temperate_forest": [
        ("wolf",        "Forest Wolf",        "beast", "melee",       "standard", "dense-woods pack-predator"),
        ("boar",        "Forest Boar",        "beast", "melee",       "standard", "root-grubber, charges under cover"),
        ("stag",        "Antler-Stag",        "beast", "melee",       "light",    "antler-charge"),
        ("raven",       "Carrion-Raven",      "beast", "ranged_short", "light",   "ambient, swoop-attack"),
        ("fox",         "Copper Fox",         "beast", "melee",       "light",    "ambient, skittish"),
        ("bear",        "Mossback Bear",      "beast", "melee",       "heavy",    "apex-predator"),
    ],
    "mountain": [
        ("rockdrake",   "Rock Drake",         "dragonkin", "melee",   "standard", "territorial, tail-swipe"),
        ("cavespider",  "Cave Spider",        "beast", "melee",       "light",    "venom-bite, ambush from ceiling"),
        ("mountainbear","Mountain Bear",      "beast", "melee",       "heavy",    "dens in deep caves"),
        ("yeti",        "Cave Yeti",          "aberration", "melee",  "heavy",    "grip-throw, frozen flesh"),
        ("rockworm",    "Deepworm Creep",     "aberration", "melee",  "standard", "emerges from burrowed tunnels"),
        ("batswarm",    "Cavern Bat-Swarm",   "beast", "melee",       "light",    "swarm damage, scatter-on-hit"),
    ],
    "marshland": [
        ("croc",        "Bog Crocodilian",    "beast", "melee",       "heavy",    "ambush from water, death-roll"),
        ("gnatswarm",   "Blood-Gnat Swarm",   "beast", "melee",       "light",    "swarm-drain, low HP each"),
        ("marshdrake",  "Marsh-Drake",        "dragonkin", "melee",   "standard", "poison-breath cone"),
        ("rottoad",     "Rot-Toad",           "beast", "ranged_short", "light",   "tongue-snap, disease-dot"),
        ("reedserpent", "Reed-Serpent",       "beast", "melee",       "standard", "stealth-stalker in water"),
        ("bogweeper",   "Bog-Weeper",         "aberration", "melee",  "standard", "half-drowned thing, life-drain"),
    ],
    "coastal_cliff": [
        ("seabird",     "Cliff Seabird",      "beast", "ranged_short", "light",   "dive-attacker from nesting cliffs"),
        ("seal",        "Crag-Seal",          "beast", "melee",       "standard", "aggressive when approached on land"),
        ("wyrm",        "Cliff Wyrm",         "dragonkin", "melee",   "heavy",    "rare, tail-sweep aoe"),
        ("crab",        "Giant Crab",         "beast", "melee",       "standard", "claw-crush, slow"),
        ("seaspider",   "Tidepool Spider",    "beast", "melee",       "light",    "ambient, quick"),
        ("stormray",    "Storm-Ray",          "beast", "ranged_short", "standard", "lightning-jolt on contact"),
    ],
    "fjord": [
        ("seal",        "Fjord-Seal",         "beast", "melee",       "standard", "barking sentinel"),
        ("orca",        "Ice-Orca",           "beast", "melee",       "heavy",    "rare beach-ambush"),
        ("serpent",     "Fjord-Serpent",      "beast", "melee",       "standard", "coiling, crush-grab"),
        ("seahawk",     "Fjord-Hawk",         "beast", "ranged_short", "light",   "dive-attack"),
        ("icebear",     "Ice-Bear",           "beast", "melee",       "heavy",    "apex, cold-immune"),
        ("drake",       "Ice-Drake",          "dragonkin", "melee",   "standard", "frost-breath, rare"),
    ],
    "ruin": [
        ("houndrev",    "Carrion-Hound",      "undead", "melee",      "standard", "rot-bite, pack-hunter"),
        ("warghost",    "War-Ghost",          "undead", "melee",      "light",    "confused soldier-shade"),
        ("revenant",    "Ruin-Revenant",      "undead", "melee",      "standard", "incorporeal, drain-touch"),
        ("spectralhawk","Spectral Hawk",      "undead", "ranged_short", "light",  "ghost-falcon, still hunting"),
        ("wraith",      "Battlefield Wraith", "undead", "ranged_short", "standard", "ranged chill-ray"),
        ("burrowwight", "Burrow-Wight",       "undead", "melee",      "heavy",    "grave-arms, slow but hard-hitting"),
    ],
    "ashland": [
        ("ashwolf",     "Ash-Wolf",           "beast", "melee",       "standard", "cinder-coated pack-hunter"),
        ("cinderdrake", "Cinder-Drake",       "dragonkin", "melee",   "heavy",    "fire-breath cone"),
        ("soottick",    "Soot-Tick",          "beast", "melee",       "light",    "swarm, latches and drains"),
        ("ashrevenant", "Ash-Revenant",       "undead", "melee",      "standard", "burned shell walking"),
        ("embersnake",  "Ember-Snake",        "beast", "melee",       "light",    "fire-bite, heat-aura"),
        ("pitvent",     "Pit-Vent Elemental", "elemental", "ranged_short", "standard", "erupts from vent holes"),
    ],
}

# Ambient (small, low-level critters — often not aggressive)
BIOME_AMBIENT = {
    "river_valley":    [("duck", "River-Duck", "beast", "light")],
    "highland":        [("hare", "Hill-Hare", "beast", "light")],
    "temperate_forest":[("squirrel", "Red-Squirrel", "beast", "light")],
    "mountain":        [("pika", "Mountain-Pika", "beast", "light")],
    "marshland":       [("frog", "Marsh-Frog", "beast", "light")],
    "coastal_cliff":   [("crab_small", "Tide-Crab", "beast", "light")],
    "fjord":           [("guillemot", "Fjord-Guillemot", "beast", "light")],
    "ruin":            [("ratrev", "Ruin-Rat", "beast", "light")],
    "ashland":         [("scarab", "Ash-Scarab", "beast", "light")],
}

# Biome exotic/aberration (higher-level, flavor-loud)
BIOME_EXOTIC = {
    "river_valley":    [("drifter_shade", "Drifter-Mage Shade", "aberration", "ranged_short", "standard", "water-bound mage-remnant")],
    "highland":        [("fog_thing", "Fog-Thing", "aberration", "melee", "standard", "formless in the fog")],
    "temperate_forest":[("wild_thing", "Wild-Court Stag", "aberration", "melee", "heavy", "fey-touched elder beast")],
    "mountain":        [("stone_golem", "Rockborn Golem", "construct", "melee", "heavy", "ancient Hearthkin sentinel")],
    "marshland":       [("rot_spirit", "Rot-Spirit", "elemental", "ranged_short", "standard", "disease elemental")],
    "coastal_cliff":   [("stormbound", "Storm-Bound Spirit", "elemental", "ranged_short", "standard", "sky-tethered thing")],
    "fjord":           [("frostbound", "Frost-Bound Wight", "undead", "melee", "standard", "frozen raider-ghost")],
    "ruin":            [("scarthing", "Scar-Thing", "aberration", "melee", "heavy", "leftover of the Coming")],
    "ashland":         [("ashwraith", "Ash-Wraith", "undead", "ranged_short", "standard", "char-bound shade")],
}

# ---------------------------------------------------------------------------
# Faction/zone humanoid warbands — per zone
# Entries: list of (mob_key, display, role_hint)
# role_hint: skirmisher | caster | elite | captain | scout
# ---------------------------------------------------------------------------

ZONE_WARBAND = {
    "dalewatch_marches":  [("drifter_mage", "River Drifter", "caster"), ("raid_stray", "Raid Stray", "skirmisher")],
    "stoneguard_deep":    [("deepfolk_raider", "Deepfolk Raider", "skirmisher"), ("deepfolk_shaman", "Deepfolk Shaman", "caster")],
    "sunward_reach":      [("shadow_intruder", "Shadow-Touched Intruder", "skirmisher"), ("rogue_scholar", "Rogue Scholar", "caster")],
    "wyrling_downs":      [("bandit_recruit", "Bandit Recruit", "skirmisher"), ("guild_enforcer", "Guild Enforcer", "skirmisher"), ("shadow_broker_agent", "Shadow-Broker Agent", "elite")],
    "firland_greenwood":  [("rot_cultist", "Rot-Cultist", "skirmisher"), ("greenwood_poacher", "Greenwood Poacher", "scout")],

    "ashen_holt":         [("cult_initiate", "Cult Initiate", "skirmisher"), ("shadow_scribe", "Shadow-Scribe", "caster")],
    "barrow_coast":       [("half_made", "Half-Made Shell", "skirmisher"), ("concord_raider", "Concord Coastal Raider", "skirmisher")],
    "pactmarch":          [("bound_spirit", "Bound-Spirit Thing", "caster"), ("pactbreaker", "Pact-Breaker Rogue", "skirmisher")],
    "skarnreach":         [("rival_clan_raider", "Rival-Clan Raider", "skirmisher"), ("rival_clan_axehand", "Rival-Clan Axehand", "elite")],
    "scrap_marsh":        [("rival_skrel", "Rival Skrel Scavenger", "skirmisher"), ("rival_trap_maker", "Rival Trap-Maker", "caster")],

    "heartland_ride":     [("highway_bandit", "Highway Bandit", "skirmisher"), ("bandit_crossbow", "Bandit Crossbowman", "scout"), ("baron_houseguard", "Baron's House-Guard", "elite"), ("corrupt_merchant", "Corrupt Merchant", "skirmisher")],
    "irongate_pass":      [("deep_stoneeater", "Deep-Stone-Eater", "skirmisher"), ("deep_drumthing", "Deep-Drum-Thing", "caster"), ("gate_breaker", "Gate-Breaker Elite", "elite")],
    "silverleaf_wood":    [("wildcourt_refuser", "Wild-Court Refuser", "skirmisher"), ("moth_attendant", "Moth-Court Attendant", "caster"), ("court_champion", "Court Champion", "elite")],
    "market_crossing":    [("tradeguild_thug", "Trade-Guild Thug", "skirmisher"), ("tradeguild_sorcerer", "Trade-Guild Sorcerer", "caster"), ("double_agent", "Turned Double-Agent", "elite"), ("caravan_guard", "Corrupt Caravan Guard", "scout")],
    "greenwood_deep":     [("drowned_warden", "Drowned-Warden Shade", "elite"), ("fey_attendant", "Fey Attendant", "caster"), ("old_poacher", "Old Poacher", "scout")],

    "gravewatch_fields":  [("bad_rite_shell", "Bad-Rite Shell", "skirmisher"), ("rite_cultist", "Rite-Cultist", "caster"), ("mausoleum_guard", "Mausoleum Guard", "elite")],
    "shadegrove":         [("schismatic_initiate", "Schismatic Initiate", "skirmisher"), ("schismatic_speaker", "Schismatic Speaker", "caster"), ("sanctum_thug", "Sanctum Thug", "elite")],
    "pact_causeway":      [("causeway_bandit", "Causeway Bandit", "skirmisher"), ("rogue_scribe", "Rogue Pact-Scribe", "caster"), ("bound_enforcer", "Bound-Enforcer", "elite"), ("pillar_cultist", "Pillar-Cultist", "skirmisher")],
    "skarncamp_wastes":   [("war_hawk", "War-Hawk Skarn", "skirmisher"), ("grudge_elder", "Grudge-Elder Caster", "caster"), ("war_stake_keeper", "War-Stake Keeper", "elite"), ("refusing_kin", "Refusing Kin Warrior", "skirmisher")],
    "scrap_flats":        [("cartel_enforcer", "Cartel Enforcer", "skirmisher"), ("cartel_alchemist", "Cartel Alchemist", "caster"), ("pit_tinker_guard", "Pit-Tinker Guard", "elite"), ("junk_scavenger", "Junk-Scavenger Rival", "scout")],

    "ruin_line_north":    [("refuser_paladin", "Concord Refuser-Paladin", "elite"), ("refuser_pactblade", "Rend Refuser-Pactblade", "elite"), ("ruin_scavenger", "Ruin-Line Scavenger", "skirmisher"), ("abbey_cultist", "Abbey Cultist", "caster")],
    "ruin_line_south":    [("concord_strike_team", "Concord Strike-Team", "elite"), ("rend_strike_team", "Rend Strike-Team", "elite"), ("archive_sentinel", "Archive Sentinel", "elite"), ("ruin_deserter", "Ruin-Deserter", "skirmisher")],
    "iron_strand":        [("raider_longshipman", "Raider Longshipman", "skirmisher"), ("raider_storm_speaker", "Raider Storm-Speaker", "caster"), ("shipmaster_bodyguard", "Shipmaster Bodyguard", "elite"), ("wrecked_sailor", "Wrecked Sailor Shade", "skirmisher")],
    "ashweald":           [("resurrectionist_cultist", "Resurrectionist Cultist", "caster"), ("pyre_adept", "Pyre-Adept", "caster"), ("saint_vigil_guard", "Saint-Vigil Guard", "elite"), ("ash_pilgrim", "Ash-Pilgrim", "skirmisher")],

    "blackwater_deep":    [("exile_loyalist", "Exile-Loyalist Guard", "elite"), ("black_eel_raider", "Black-Eel Raider", "skirmisher"), ("drowned_rite_caster", "Drowned-Rite Caster", "caster"), ("marshal_herald", "Marshal Herald", "scout"), ("deep_warband", "Deep-Warband Warrior", "skirmisher")],
    "frost_spine":        [("spine_cultist", "Spine-Cultist", "caster"), ("wyrm_awakener", "Wyrm-Awakener", "caster"), ("summit_guard", "Summit Guard", "elite"), ("refuser_faction_agent", "Refuser Faction Agent", "skirmisher")],
    "sundering_mines":    [("deep_expedition_loyalist", "Deep-Expedition Loyalist", "elite"), ("carver_cultist", "Carver-Cultist", "caster"), ("mine_warden_corrupted", "Corrupted Mine-Warden", "elite"), ("scavenger_expedition", "Scavenger Expedition Rival", "scout")],
    "crown_of_ruin":      [("concord_claimant_guard", "Concord Claimant Guard", "elite"), ("rend_claimant_guard", "Rend Claimant Guard", "elite"), ("throne_acolyte", "Throne-Acolyte", "caster"), ("throne_refuser", "Throne-Refuser", "skirmisher"), ("last_candle_pilgrim", "Last-Candle Pilgrim", "skirmisher"), ("crown_watcher", "Crown-Watcher Elite", "elite")],
}

# ---------------------------------------------------------------------------
# Named/elite mobs tied to main-chain final bosses
# Entries keyed by zone_id, list of (mob_id, name, role_hint, notes)
# ---------------------------------------------------------------------------

ZONE_NAMED = {
    "dalewatch_marches":  [("drifter_mage_named", "Corlen the Drifter", "named", "main-chain final target at the reed-brake")],
    "stoneguard_deep":    [("deepworm_brood_named", "The Deepworm That Broke Through", "elite", "main-chain rite-defense boss")],
    "sunward_reach":      [("shadow_intruder_named", "The Intruder Behind the Archive", "named", "main-chain shadow breach target")],
    "wyrling_downs":      [("shadow_broker_named", "The Shadow Broker", "named", "main-chain guild-dues final target")],
    "firland_greenwood":  [("rot_beast_named", "The Rot at the Root", "elite", "main-chain rot-beast at the sick-oak root")],

    "ashen_holt":         [("exile_shade_named", "The First Exile's Shade", "named", "main-chain oath-ritual witness")],
    "barrow_coast":       [("concord_raid_captain", "Concord Raid-Captain Ness", "named", "main-chain raider-silencer target")],
    "pactmarch":          [("bound_thing_named", "The Circle-Bound Thing", "elite", "main-chain interrogation subject")],
    "skarnreach":         [("rival_clanhead", "Rival Clanhead Draag", "named", "main-chain blood-price target"),
                           ("skarn_liar_elder", "Elder Hef the Liar", "named", "main-chain reveal-target")],
    "scrap_marsh":        [("fence_glib", "Glib the Fence", "named", "main-chain heist-flip final target")],

    "heartland_ride":     [("baron_vessen", "Baron Vessen", "named", "main-chain political finale"),
                           ("captain_byrn_field", "Captain Byrn (field)", "elite", "field-side rare spawn before keep")],
    "irongate_pass":      [("stone_eater_named", "The Stone-Eater Mother", "elite", "main-chain seal-breach hunter")],
    "silverleaf_wood":    [("mothking_shade", "The Moth-King's Shade", "named", "main-chain witness"),
                           ("refuser_champion", "The Refuser Champion", "elite", "main-chain combat finale")],
    "market_crossing":    [("trade_patron", "The Trade-Guild Patron", "named", "main-chain coup-leader")],
    "greenwood_deep":     [("drowned_warden_named", "The Drowned-Warden", "named", "main-chain forbidden-name keeper")],

    "gravewatch_fields":  [("rite_break_source", "The Rite-Break at the Mausoleum", "elite", "main-chain source-of-break")],
    "shadegrove":         [("schismatic_leader", "The Schismatic Leader", "named", "main-chain execution target")],
    "pact_causeway":      [("pact_scribe_named", "The Pact-Scribe Indelible (field)", "named", "main-chain final target (also dungeon boss)")],
    "skarncamp_wastes":   [("war_hawk_leader", "The War-Hawk Leader", "named", "main-chain refuser leader")],
    "scrap_flats":        [("cartel_lieutenant", "The Cartel Lieutenant", "named", "main-chain prelude to Glub")],

    "ruin_line_north":    [("abbey_rite_refuser", "The Abbey Rite-Refuser", "named", "main-chain rite-opposer")],
    "ruin_line_south":    [("archive_sentinel_named", "The Archive Sentinel", "named", "main-chain archive-guardian")],
    "iron_strand":        [("raider_lord", "The Raider-Lord of the Strand", "named", "main-chain lighthouse-summit finale")],
    "ashweald":           [("resurrectionist_leader", "The Resurrectionist-Leader", "named", "main-chain altar finale")],

    "blackwater_deep":    [("kessen_field", "Warlord Kessen (field-manifestation)", "named", "pre-dungeon field encounter")],
    "frost_spine":        [("wyrm_awakener_leader", "The Wyrm-Awakener Prime", "named", "main-chain refuser-leader")],
    "sundering_mines":    [("carver_kin_prime", "The Carver-Kin Prime Scout", "named", "main-chain deep-map keeper")],
    "crown_of_ruin":      [("concord_champion", "The Concord Throne-Claimant", "named", "main-chain Concord claim-attempt"),
                           ("rend_champion", "The Rend Throne-Claimant", "named", "main-chain Rend claim-attempt")],
}


# ---------------------------------------------------------------------------
# Mob builder
# ---------------------------------------------------------------------------

SCHOOL_BY_FAMILY = {
    "beast":      "blade",
    "humanoid":   "blade",
    "undead":     "shadow",
    "elemental":  "fire",
    "aberration": "shadow",
    "construct":  "blunt",
    "dragonkin":  "fire",
    "demon":      "fire",
    "fey":        "nature",
    "giant":      "blunt",
}

# School override for casters (role_hint == "caster") per family
CASTER_SCHOOL_BY_FAMILY = {
    "humanoid":   "arcane",
    "undead":     "shadow",
    "elemental":  "fire",
    "aberration": "shadow",
    "dragonkin":  "fire",
    "demon":      "shadow",
    "fey":        "nature",
}

RARITY_HP_TIER = {
    "common": "standard",
    "elite":  "elite",
    "rare":   "elite",
    "named":  "heroic",
}

# Armor-class selection for humanoid warband, driven by role
HUMANOID_ARMOR_BY_ROLE = {
    "skirmisher": "leather",
    "scout":      "leather",
    "caster":     "cloth",
    "elite":      "plate",
    "captain":    "plate",
    "named":      "plate",
}


def _resolve_school(family: str, role_hint: str, requested: str | None = None) -> str:
    """Pick a primary_school that's legal under the creature_type's affinities."""
    type_def = CREATURE_TYPES[family]
    pref = type_def["affinities"]["preferred"]
    allowed = type_def["affinities"]["allowed"]
    forbidden = set(type_def["affinities"]["forbidden"])

    if requested and requested not in forbidden and (requested in pref or requested in allowed):
        return requested

    if role_hint == "caster":
        candidate = CASTER_SCHOOL_BY_FAMILY.get(family)
        if candidate and candidate not in forbidden and (candidate in pref or candidate in allowed):
            return candidate

    if pref:
        return pref[0]
    if allowed:
        return allowed[0]
    return "blade"  # fallback, should never hit for a valid type


def _resolve_armor(family: str, role_hint: str, rarity: str) -> str:
    """Armor_class selection. Humanoids scale by role; others use type default."""
    if family == "humanoid":
        base = HUMANOID_ARMOR_BY_ROLE.get(role_hint, "leather")
        # Named humanoids almost always in plate (boss-coded)
        if rarity == "named":
            return "plate"
        return base
    return CREATURE_TYPES[family]["default_armor_class"]


def mob_family_for_humanoid(zone_faction: str) -> str:
    return "humanoid"


def build_mob(mob_id: str, display: str, zone: str, zone_meta: dict,
              family: str, attack_range: str, dps_tier: str,
              rarity: str = "common", role_hint: str = "skirmisher",
              level_offset: int = 0, flavor: str = "",
              chain_target: bool = False) -> dict:
    lo = zone_meta["level_range"]["min"]
    hi = zone_meta["level_range"]["max"]
    # Level: spread across range; offset lets named mobs sit at the top
    if rarity == "named":
        level = hi
    elif rarity == "elite":
        level = min(hi, lo + int((hi - lo) * 0.75) + level_offset)
    else:
        # common mobs scale by role_hint: scouts/casters at top half, skirmishers at bottom half
        base = lo + (hi - lo) // 2
        level = max(lo, min(hi, base + level_offset))
    faction_alignment = {
        "faction_a":  "hostile_to_faction_a",
        "faction_b":  "hostile_to_faction_b",
        "contested":  "hostile_to_all",
    }.get(zone_meta["faction_control"], "hostile_to_all")
    # Wildlife is faction-blind — beasts, dragonkin, and most exotics default to neutral-aggressive.
    # Humanoids, undead, and faction-aligned warbands keep the zone's hostility coding.
    if family in ("beast", "dragonkin") or (family == "aberration" and rarity != "named"):
        faction_alignment = "neutral_aggressive"
    if role_hint == "caster":
        attack_range = "ranged_short"
    primary_school = _resolve_school(family, role_hint, requested=SCHOOL_BY_FAMILY.get(family))
    armor_class = _resolve_armor(family, role_hint, rarity)

    entry = {
        "id": mob_id,
        "name": display,
        "zone": zone,
        "level": level,
        "creature_type": family,         # inherits hp_scaling, resistances, affinities
        "armor_class": armor_class,       # inherits physical/magic reduction + matchups
        "rarity": rarity,
        "role": role_hint,
        "faction_alignment": faction_alignment,
        "hp_tier": RARITY_HP_TIER[rarity],
        "damage": {
            "primary_school": primary_school,
            "attack_range": attack_range,
            "dps_tier": dps_tier,
        },
        "behavior": {
            "aggro_range": {"common": "medium", "elite": "long", "rare": "long", "named": "long"}[rarity],
            "social_radius": {"skirmisher": 8, "scout": 4, "caster": 6, "elite": 10, "captain": 12, "named": 0}.get(role_hint, 6) if family == "humanoid" else (5 if dps_tier != "light" else 0),
            "flee_threshold": 0.15 if family == "beast" and dps_tier == "light" else 0.0,
            "calls_allies": family == "humanoid" and role_hint in ("skirmisher", "elite", "captain"),
            "patrol": "long" if role_hint == "scout" else ("short" if rarity == "common" else "stationary"),
        },
        "loot_tier": {"common": "standard", "elite": "rare", "rare": "rare", "named": "quest_marked"}[rarity],
        "drops": {
            "gold_copper_avg": level * {"light": 1, "standard": 2, "heavy": 3}[dps_tier]
                                       * {"common": 1, "elite": 3, "rare": 5, "named": 8}[rarity],
            "item_hint": {
                "common": "biome-flavored trash material",
                "elite":  "rare biome reagent; low chance of blue-tier item",
                "rare":   "guaranteed rare drop; bonus quest-item chance",
                "named":  "named-drop set (weapon/trinket); quest turn-in item",
            }[rarity],
        },
        "biome_context": flavor,
    }
    if chain_target:
        entry["chain_target"] = True
    return entry


def validate_mob(entry: dict) -> list[str]:
    """Return a list of validation errors. Empty list = valid."""
    errors = []
    ctype = entry.get("creature_type")
    if ctype not in CREATURE_TYPES:
        errors.append(f"{entry['id']}: unknown creature_type {ctype!r}")
        return errors
    armor = entry.get("armor_class")
    if armor not in ARMOR_CLASSES:
        errors.append(f"{entry['id']}: unknown armor_class {armor!r}")
    school = entry["damage"]["primary_school"]
    forbidden = set(CREATURE_TYPES[ctype]["affinities"]["forbidden"])
    allowed = set(CREATURE_TYPES[ctype]["affinities"]["preferred"]) | set(CREATURE_TYPES[ctype]["affinities"]["allowed"])
    if school in forbidden:
        errors.append(f"{entry['id']}: school {school!r} is FORBIDDEN for {ctype}")
    elif allowed and school not in allowed:
        errors.append(f"{entry['id']}: school {school!r} not in {ctype} affinities (allowed={sorted(allowed)})")
    return errors


# ---------------------------------------------------------------------------
# Per-zone roster assembly
# ---------------------------------------------------------------------------

def build_zone_mobs(zone: str, zone_meta: dict) -> list[dict]:
    biome = zone_meta["biome"]
    tier = zone_meta["tier"]
    target_count = zone_meta["budget"]["unique_mob_types"]

    entries: list[dict] = []

    # 1. Biome beasts — 4-6
    for i, (slot, name, family, arange, dps, flavor) in enumerate(BIOME_BEASTS.get(biome, [])):
        entries.append(build_mob(
            f"mob_{zone}_beast_{slot}", name, zone, zone_meta,
            family, arange, dps,
            rarity="common", role_hint="skirmisher",
            level_offset=-1 if dps == "light" else (2 if dps == "heavy" else 0),
            flavor=flavor,
        ))

    # 2. Ambient critters — 1 per biome
    for slot, name, family, dps in BIOME_AMBIENT.get(biome, []):
        entries.append(build_mob(
            f"mob_{zone}_ambient_{slot}", name, zone, zone_meta,
            family, "melee", dps,
            rarity="common", role_hint="scout",
            level_offset=-2,
            flavor="ambient critter — low threat",
        ))

    # 3. Biome exotic — 1 per biome, elite-adjacent
    for slot, name, family, arange, dps, flavor in BIOME_EXOTIC.get(biome, []):
        entries.append(build_mob(
            f"mob_{zone}_exotic_{slot}", name, zone, zone_meta,
            family, arange, dps,
            rarity="elite",
            role_hint="caster" if arange == "ranged_short" else "elite",
            flavor=flavor,
        ))

    # 4. Zone-specific humanoid warband
    for slot, name, role_hint in ZONE_WARBAND.get(zone, []):
        dps = {"elite": "heavy", "caster": "standard", "scout": "light"}.get(role_hint, "standard")
        rarity = "elite" if role_hint == "elite" else "common"
        arange = "ranged_short" if role_hint in ("caster", "scout") else "melee"
        entries.append(build_mob(
            f"mob_{zone}_warband_{slot}", name, zone, zone_meta,
            "humanoid", arange, dps,
            rarity=rarity, role_hint=role_hint,
            flavor=f"zone warband — {role_hint} role",
        ))

    # 5. Named/elite chain-boss mobs
    for slot, name, rarity_hint, notes in ZONE_NAMED.get(zone, []):
        rarity = rarity_hint if rarity_hint in ("named", "elite", "rare") else "named"
        entries.append(build_mob(
            f"mob_{zone}_named_{slot}", name, zone, zone_meta,
            "humanoid", "melee", "heavy",
            rarity=rarity, role_hint="captain",
            flavor=notes,
            chain_target=True,
        ))

    # 6. Pad to budget with real sub-variants (life-stage / sex / color-morph)
    # Each variant is mechanically distinct: different level offset, role, or dps tier.
    variant_prefixes = [
        ("juvenile",  "Juvenile",   -2, "scout",      "light",    "young, lower HP but faster"),
        ("alpha",     "Alpha",      +2, "elite",      "heavy",    "pack-leader, higher HP and damage"),
        ("matriarch", "Matriarch",  +3, "captain",    "heavy",    "rare-spawn leader; calls adds"),
        ("lean",      "Lean",       -1, "skirmisher", "standard", "underfed, roams further from group"),
        ("scarred",   "Scarred",    +1, "skirmisher", "standard", "older survivor, wary"),
        ("greater",   "Greater",    +2, "elite",      "heavy",    "oversized, stronger drops"),
        ("blighted",  "Blighted",   +0, "caster",     "standard", "diseased — spreads dot"),
        ("shadowed",  "Shadow-",    +1, "skirmisher", "standard", "touched by nearby shadow-school; shadow-damage"),
    ]
    pal = BIOME_BEASTS.get(biome, [])
    idx = 0
    while len(entries) < target_count and idx < len(pal) * len(variant_prefixes):
        base_slot, base_name, fam, arange, dps_base, flavor = pal[idx % len(pal)]
        vprefix = variant_prefixes[idx // len(pal) % len(variant_prefixes)]
        vkey, vname, voff, vrole, vdps, vflavor = vprefix
        mob_id = f"mob_{zone}_{vkey}_{base_slot}"
        # skip if already produced
        if any(e["id"] == mob_id for e in entries):
            idx += 1
            continue
        rarity = "elite" if vrole in ("elite", "captain") else "common"
        entries.append(build_mob(
            mob_id, f"{vname} {base_name}", zone, zone_meta,
            fam, arange, vdps,
            rarity=rarity, role_hint=vrole,
            level_offset=voff,
            flavor=f"{vflavor}; base form: {base_name}",
        ))
        idx += 1

    # Trim if we overshot (keep budget honest)
    if len(entries) > target_count + 2:
        # Keep all named + elite; drop excess commons
        keep = [e for e in entries if e["rarity"] in ("named", "elite", "rare")]
        commons = [e for e in entries if e["rarity"] == "common"]
        keep.extend(commons[:max(0, target_count - len(keep))])
        entries = keep

    return entries


def build_roster(zone: str, entries: list[dict]) -> dict:
    by_type: dict = {}
    by_armor: dict = {}
    by_rarity: dict = {}
    for e in entries:
        by_type[e["creature_type"]] = by_type.get(e["creature_type"], 0) + 1
        by_armor[e["armor_class"]] = by_armor.get(e["armor_class"], 0) + 1
        by_rarity[e["rarity"]] = by_rarity.get(e["rarity"], 0) + 1
    return {
        "id": f"_roster__{zone}",
        "zone": zone,
        "mob_count": len(entries),
        "mob_ids": [e["id"] for e in entries],
        "by_creature_type": by_type,
        "by_armor_class": by_armor,
        "by_rarity": by_rarity,
        "level_min": min(e["level"] for e in entries),
        "level_max": max(e["level"] for e in entries),
        "named_or_chain_targets": [e["id"] for e in entries if e["rarity"] == "named" or e.get("chain_target")],
    }


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main() -> None:
    zones_root = WORLD / "zones"
    total_mobs = 0
    all_errors: list[str] = []
    for zone_dir in sorted(zones_root.iterdir()):
        if not zone_dir.is_dir():
            continue
        zone = zone_dir.name
        zone_meta = load(zone_dir / "core.yaml")
        entries = build_zone_mobs(zone, zone_meta)
        for entry in entries:
            errs = validate_mob(entry)
            all_errors.extend(errs)
            write(zone_dir / "mobs" / f"{entry['id']}.yaml", entry)
        write(zone_dir / "mobs" / "_roster.yaml", build_roster(zone, entries))
        total_mobs += len(entries)
    print(f"wrote {total_mobs} mobs across {len(list(zones_root.iterdir()))} zones")
    if all_errors:
        print(f"\nVALIDATION FAILURES ({len(all_errors)}):")
        for err in all_errors[:30]:
            print(f"  ! {err}")
        if len(all_errors) > 30:
            print(f"  ... and {len(all_errors) - 30} more")
        raise SystemExit(1)
    print("all mobs validate against bestiary (creature_type + armor_class + school affinity)")


if __name__ == "__main__":
    main()
