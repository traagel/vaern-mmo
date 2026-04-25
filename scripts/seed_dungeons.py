#!/usr/bin/env python3
"""Seed src/generated/world/dungeons/ with 5/10/20-man instance scaffolding.

Targets ~30 five-mans (denser than WoW Classic) spread across mid-tier,
contested, and endgame zones, plus 4-5 ten-man raids and 2 twenty-mans at
level 60. No 40-man tier — per strict-coop design, the largest social unit
is the guild; 40-man is explicitly out of scope.

Writes one core.yaml + one bosses.yaml per instance. Idempotent.
"""
from __future__ import annotations

from pathlib import Path
import yaml

REPO = Path(__file__).resolve().parents[1]
OUT = REPO / "src" / "generated" / "world" / "dungeons"


def write(path: Path, data: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "w") as f:
        yaml.safe_dump(data, f, sort_keys=False, default_flow_style=False)


# ---------------------------------------------------------------------------
# Dungeon manifest
# ---------------------------------------------------------------------------
# Entry: (id, name, zone, entrance_hub, group_size, lo, hi, theme, bosses)
# bosses: list of (id, name, role_tag, mechanic)
#   role_tag: trash_lead | miniboss | lieutenant | boss | final
#   mechanic: short descriptor; drives future ability slot assignment

INSTANCES = [
    # =========================================================================
    # PRE-ALPHA L10 CAPSTONE (Slice 6 — pseudo-dungeon, open-world)
    # =========================================================================
    # Open-world inside the Dalewatch zone room (no instance lockout); the
    # mob anchor is hard-coded at (470, 80) zone-local in
    # vaern-server/src/npc/spawn.rs::mob_anchor_for_level. Hub-anchored
    # dressing props at Ford of Ashmere give the entrance silhouette.
    ("drifters_lair", "The Drifter's Lair", "dalewatch_marches", "ford_of_ashmere",
     5, 9, 10,
     "The Wake's anchor in the Blackwash Fens — an Academian's expelled Wake-rite, now a 2-4-player capstone",
     [("master_drifter_halen", "Master Drifter Halen", "miniboss", "fire ranged caster + cult adds; flanks via ritual smoke"),
      ("grand_drifter_valenn", "Grand Drifter Valenn", "final", "arcane caster + Brine-Shade Primarch summon at 50%; phase shift on death of Primarch")]),

    # =========================================================================
    # LOW-TIER 5-MANS (levels 10-22)
    # =========================================================================
    ("burrows_of_bracken", "The Burrows of Bracken", "wyrling_downs", "brackenhollow",
     5, 10, 15,
     "Burrowing beast-clan of giant digger-rats and their half-feral handlers",
     [("bracken_matron", "The Bracken Matron", "miniboss", "pack-summon burrow-rat swarm"),
      ("mad_molecatcher", "The Mad Molecatcher", "miniboss", "trap-layer, stealth-ambush"),
      ("king_gnarlroot", "King Gnarlroot", "final", "charge-and-bite, burrows mid-fight to reset position")]),

    ("first_barrow", "The First Barrow", "barrow_coast", "silent_harbor",
     5, 12, 18,
     "Earliest Gravewrought burial-shell, now haunted by ancestor-shells that never woke",
     [("hollow_keeper", "The Hollow Keeper", "miniboss", "aoe funeral-chant silence"),
      ("twin_shells", "The Twin Shells", "miniboss", "paired bosses, heal each other if both alive"),
      ("first_mother", "The First Mother", "final", "summons grave-iron adds, phase-shift at 30%")]),

    ("old_anvil_halls", "Old Anvil Halls", "stoneguard_deep", "anvilgate",
     5, 12, 18,
     "Collapsed lower forges of the Hearthkin, now home to rockworm and deepfolk",
     [("anvil_warden", "The Anvil Warden", "miniboss", "stone-armor self-buff, knockback slam"),
      ("deepworm_brood", "Deepworm Brood", "miniboss", "trash-spawner, ground-rupture aoe"),
      ("old_smith", "The Old Smith", "final", "forge-heat aura, summons cinder-elementals")]),

    ("junk_warren_pit", "The Junk-Warren Pit", "scrap_marsh", "junk_warren",
     5, 14, 20,
     "Skrel scrap-lord's inner warren, all rusted traps and stolen alchemy",
     [("trap_tinker", "The Trap-Tinker", "miniboss", "room-wide trap-field, disarm check"),
      ("scrap_champion", "The Scrap Champion", "miniboss", "cobbled-armor absorb shield"),
      ("boss_skorr", "Boss Skorr the Many-Eyed", "final", "stolen-alchemy potion phases: fire, poison, stealth")]),

    ("sunken_grove_crypt", "The Sunken Grove Crypt", "ashen_holt", "blackleaf_bower",
     5, 14, 20,
     "Darkling Elen ancestral crypt, now seeping with the shadow-rot that made them dark",
     [("rot_cultist", "The Rot Cultist", "miniboss", "shadow DoT on tank, dispellable"),
      ("grove_revenant", "The Grove Revenant", "miniboss", "life-drain beam, tether-break mechanic"),
      ("exile_matriarch", "The Exile Matriarch", "final", "shadow-step assassin, mirror-images at 50%")]),

    ("watchtower_keep", "Bandit-Watch Tower Keep", "heartland_ride", "bandit_watchtower",
     5, 12, 20,
     "Dalewatch-era highway keep fallen to a bandit-captain's war-band",
     [("lieutenant_marl", "Lieutenant Marl", "miniboss", "crossbow volley, line-aoe"),
      ("cellar_torturer", "The Cellar Torturer", "miniboss", "fear-aura, captive-hostage mechanic"),
      ("captain_byrn", "Captain Byrn", "final", "dual-wield enrage, calls reinforcements at 40%")]),

    # =========================================================================
    # MID-TIER 5-MANS (levels 18-35)
    # =========================================================================
    ("collapsed_forge", "The Collapsed Forge", "irongate_pass", "collapsed_forge",
     5, 18, 25,
     "Hearthkin forge collapsed in a mining accident — still burning after centuries",
     [("shift_foreman", "The Shift Foreman", "miniboss", "whip-cleave, rally-yell buffing adds"),
      ("molten_worker", "The Molten Worker", "miniboss", "fire-self-immolate aoe, kite mechanic"),
      ("forge_lord", "The Forge-Lord Unstilled", "final", "three-phase: anvil, forge, quench — distinct mechanics each")]),

    ("lichweep_mausoleum", "Lich-Weep Mausoleum", "gravewatch_fields", "lich_weep",
     5, 18, 25,
     "Gravewrought necropolis where the ancestor-rite went wrong at scale",
     [("bone_catechist", "The Bone Catechist", "miniboss", "add-spawn waves from coffins"),
      ("weeping_shade", "The Weeping Shade", "miniboss", "tear-pools aoe, stationary"),
      ("mausoleum_keeper", "The Mausoleum Keeper", "final", "body-swap with adds — players must track which vessel")]),

    ("elder_grove_ruin", "Elder Grove Ruin", "silverleaf_wood", "elder_grove",
     5, 22, 28,
     "Pre-Veyr elvish ruin, taken over by feral nature-pact creatures",
     [("thornheart_druid", "Thornheart Druid", "miniboss", "root-and-dot, summon treant adds"),
      ("silver_stag", "The Silver Stag", "miniboss", "charge mechanic, hit-and-run"),
      ("grove_elder", "The Grove-Elder Awake", "final", "seasons rotation: spring/summer/autumn/winter mechanics")]),

    ("dark_glade_sanctum", "Dark Glade Sanctum", "shadegrove", "dark_glade",
     5, 22, 28,
     "Hraun shadow-cult inner sanctum, deep in the ashen forest",
     [("shadow_initiate", "Shadow Initiate", "miniboss", "teaches the others — kill last or they heal"),
      ("pact_speaker", "The Pact-Speaker", "miniboss", "life-tap summon, ramping damage"),
      ("sanctum_witch", "The Sanctum Witch", "final", "curse-stacking tank mechanic, rotating elemental orbs")]),

    ("oath_pillar_vault", "The Oath Pillar Vault", "pact_causeway", "oath_pillar",
     5, 24, 30,
     "Buried Kharun reliquary beneath the great oath-pillar of the pact",
     [("oath_keeper", "The Oath Keeper", "miniboss", "vow-mechanic: players must honor a targeted promise or take damage"),
      ("bound_thing", "The Bound Thing", "miniboss", "chain-tether breaks, boss moves"),
      ("pact_scribe", "The Pact-Scribe Indelible", "final", "writes a curse on each player; cleansed by specific adds")]),

    ("wyrd_tarn_depths", "The Wyrd Tarn Depths", "greenwood_deep", "wyrd_tarn",
     5, 26, 32,
     "Submerged Firland fey-lake, where the bottom is older than the forest above",
     [("drowned_huntress", "The Drowned Huntress", "miniboss", "bow mechanic, arrow-line avoidance"),
      ("tarn_serpent", "The Tarn-Serpent", "miniboss", "water-vortex mechanic, collision"),
      ("wyrd_warden", "The Wyrd Warden", "final", "alternates between nature and frost phases; wrong resist = death")]),

    ("old_river_watch", "Old River Watch", "market_crossing", "old_river_watch",
     5, 25, 32,
     "Pre-Concord river fortress, overrun by a Rend smuggling ring and its patron",
     [("smuggler_boss", "The Smuggler-Boss", "miniboss", "stealth-open, uses terrain"),
      ("caravan_sorcerer", "The Caravan Sorcerer", "miniboss", "crowd-control mage; interrupts matter"),
      ("patron_merchant", "The Patron", "final", "money-phase (drops gold that enrages adds), negotiation check")]),

    ("salt_watch_prison", "The Salt-Watch Prison", "skarncamp_wastes", "salt_watch",
     5, 28, 34,
     "Skarn war-camp prison — freed inmates and their warden-executioners",
     [("executioner", "The Executioner", "miniboss", "execute-below-threshold on tank"),
      ("freed_champion", "The Freed Champion", "miniboss", "fight as NPC ally, turns at 30%"),
      ("warden_skorrn", "Warden Skorrn", "final", "chain-whip pull, prisoner-wave phases")]),

    ("pit_tinker_works", "The Pit-Tinker Works", "scrap_flats", "pit_tinker",
     5, 30, 36,
     "Full industrial Skrel alchemy-works; a death-trap of acid, steam, and shrapnel",
     [("assembly_overseer", "The Assembly Overseer", "miniboss", "conveyor-belt add-spawn"),
      ("acid_brewer", "The Acid-Brewer", "miniboss", "environmental acid pools, stand-out mechanic"),
      ("master_tinker_glub", "Master Tinker Glub", "final", "three mechanical servitors, must be killed in specific order")]),

    # =========================================================================
    # HIGH-TIER 5-MANS (levels 32-50)
    # =========================================================================
    ("burned_abbey", "The Burned Abbey", "ruin_line_north", "burned_abbey",
     5, 32, 40,
     "Pre-war Concord abbey torched during the Coming; its priesthood never finished the rite",
     [("abbess_ember", "Abbess Ember", "miniboss", "holy/shadow alternating phase"),
      ("cloister_champion", "The Cloister Champion", "miniboss", "shield-block mechanic, break with interrupts"),
      ("scriptorium_wraith", "The Scriptorium Wraith", "miniboss", "cursed-page mechanic, debuff spread"),
      ("last_celebrant", "The Last Celebrant", "final", "attempts to finish the ritual; fail = wipe")]),

    ("sunken_tower", "The Sunken Tower", "ruin_line_south", "sunken_tower",
     5, 32, 40,
     "Sunward Elen arcane spire half-submerged in a cursed marsh after the Coming",
     [("tower_apprentice", "The Tower Apprentice", "miniboss", "mirror-image spawn"),
      ("broken_golem", "The Broken Golem", "miniboss", "reconstructs mid-fight"),
      ("tower_librarian", "The Tower Librarian", "miniboss", "summons spellbooks — interrupt the big one"),
      ("sundered_archmage", "The Sundered Archmage", "final", "polymorph phases, silence pulses")]),

    ("old_lighthouse", "The Old Lighthouse", "iron_strand", "old_lighthouse",
     5, 38, 45,
     "Cliffside beacon-tower overrun by a Rend raider crew and their bound storm-thing",
     [("raider_shipmaster", "The Raider Shipmaster", "miniboss", "cleave + raid-call add-wave"),
      ("lantern_keeper", "The Lantern Keeper", "miniboss", "light/dark phase — positioning matters"),
      ("storm_thing", "The Storm-Thing Bound", "final", "lightning-arc chain, wind knockback")]),

    ("charred_shrine", "The Charred Shrine", "ashweald", "charred_shrine",
     5, 38, 45,
     "A burned pre-war temple where both factions lost a prayer",
     [("ember_priest", "The Ember-Priest", "miniboss", "fire-dot stack, dispel rotation"),
      ("charcoal_champion", "The Charcoal Champion", "miniboss", "armor stacks as he burns down"),
      ("twin_saints", "The Twin Saints Weeping", "final", "one Concord, one Rend; they resurrect each other unless killed within 5s")]),

    ("lightless_gallery", "The Lightless Gallery", "sundering_mines", "lightless_gallery",
     5, 42, 50,
     "The deepest shaft of the Sundering Mines — no light reaches the bottom, and something digs back",
     [("pit_stalker", "The Pit-Stalker", "miniboss", "stealth-ambush, requires light sources"),
      ("deep_carver", "The Deep-Carver", "miniboss", "tunnel-and-emerge position mechanic"),
      ("dark_forgemaster", "The Dark Forgemaster", "miniboss", "crafts adds mid-fight"),
      ("the_carver_mother", "The Carver-Mother", "final", "blind-fight phase — all vision obscured, sound cues only")]),

    # =========================================================================
    # ENDGAME 5-MANS (levels 48-60)
    # =========================================================================
    ("frost_gate_summit", "The Frost Gate Summit", "frost_spine", "frost_gate",
     5, 50, 56,
     "A pre-Coming watchgate atop the Spine, now a storm-ice lair",
     [("ice_captain", "The Ice-Captain", "miniboss", "frost-nova stun, positional kite"),
      ("wyrm_broodling", "Wyrm-Broodling", "miniboss", "ice-breath cone"),
      ("gatekeeper_uthar", "Gatekeeper Uthar", "final", "summons old-kin bosses as shades of themselves")]),

    ("moth_hollow_deeps", "Moth-Hollow Deeps", "silverleaf_wood", "moth_hollow",
     5, 48, 54,
     "Moth-hollow's lower chambers — arcane wildlife, displaced by the Sundering",
     [("matriarch_moth", "The Matriarch Moth", "miniboss", "wing-dust blind mechanic"),
      ("silk_sorcerer", "The Silk Sorcerer", "miniboss", "web-wall environmental"),
      ("nightwing_alpha", "The Nightwing Alpha", "final", "fly-phase dodge mechanic")]),

    ("black_eel_warrens", "Black-Eel Warrens", "blackwater_deep", "black_eel_camp",
     5, 48, 55,
     "Submerged warrens of the Black-Eel clan, Rend infiltrators of the Deep",
     [("eel_harrier", "The Eel-Harrier", "miniboss", "hit-and-run, positional"),
      ("waterless_priest", "The Waterless Priest", "miniboss", "drain-aura, heal-reverse"),
      ("clanhead_sleth", "Clanhead Sleth", "final", "dual-weapon eel-binding, tether mechanic")]),

    ("yeti_warren", "The Yeti-Warren", "frost_spine", "yeti_warren",
     5, 48, 54,
     "Ice-mammal lair beneath the Spine — the yetis are only half the threat",
     [("alpha_yeti", "The Alpha Yeti", "miniboss", "grip-throw across the room"),
      ("mammoth_brute", "The Mammoth-Brute", "miniboss", "charge-collision on pillars"),
      ("frozen_shaman", "The Frozen Shaman", "final", "ancestor-spirit summons, cold-immune phase")]),

    ("deepwater_keep", "Deepwater Keep", "blackwater_deep", "deepwater_redoubt",
     5, 50, 56,
     "Ruined Concord river-keep, seat of a warlord-in-exile",
     [("exile_marshal", "The Exile-Marshal", "miniboss", "banner-buff, destroy the banner mechanic"),
      ("drowned_honor_guard", "Drowned Honor Guard", "miniboss", "trio of elites, rotating aggro"),
      ("warlord_kessen", "Warlord Kessen", "final", "betrayed-lord mechanic: adds were once your allies")]),

    ("old_tunnel_depths", "Old-Tunnel Depths", "sundering_mines", "old_tunnel_camp",
     5, 52, 58,
     "Pre-Hearthkin tunnels predating even the Iron Mountains themselves",
     [("tunneler_primarch", "Tunneler Primarch", "miniboss", "earthquake interrupt"),
      ("hollow_drake", "The Hollow Drake", "miniboss", "breath + tail-swipe"),
      ("old_thing", "The Old-Thing Beneath", "final", "tentacle-phase, positional environmental")]),

    # =========================================================================
    # 10-MAN RAIDS (levels 45-60)
    # =========================================================================
    ("drowned_abbey", "The Drowned Abbey", "blackwater_deep", "drowned_abbey",
     10, 50, 55,
     "The abbey didn't sink alone — it brought a saint with it",
     [("sunken_champion", "The Sunken Champion", "lieutenant", "aoe tank-swap, water rising phases"),
      ("choir_of_the_drowned", "Choir of the Drowned", "lieutenant", "6 adds, interrupt-chain"),
      ("abbot_silken", "Abbot Silken", "lieutenant", "holy-twisted, dispel-priority"),
      ("weeping_saint", "The Weeping Saint", "final", "cross-phase holy/shadow, raid-wide position dance")]),

    ("sundered_peak_raid", "The Sundered Peak", "frost_spine", "sundered_peak",
     10, 55, 60,
     "Pre-Coming sealed observatory — broken open during the Sundering, storm-thing still resident",
     [("sentinel_pair", "The Sentinel Pair", "lieutenant", "paired, share damage"),
      ("brood_mother_ice", "Ice Brood-Mother", "lieutenant", "continuous add-spawn, aoe"),
      ("storm_herald", "The Storm Herald", "lieutenant", "lightning-arc raid mechanic"),
      ("peak_thing", "The Peak-Thing", "final", "three phases: sky, wind, void; raid-level mechanics")]),

    ("lightless_depths_raid", "The Lightless Depths", "sundering_mines", "lightless_gallery",
     10, 55, 60,
     "Deeper than the 5-man gallery — where the Carver-Mother's kin still breed",
     [("pit_lords_three", "The Pit-Lords Three", "lieutenant", "three bosses; must die within 10s of each other"),
      ("deep_forge_primus", "The Deep-Forge Primus", "lieutenant", "forge-add-summon, aoe"),
      ("carver_prime", "The Carver Prime", "lieutenant", "bigger than the 5-man mother"),
      ("the_deeper_thing", "The Deeper Thing", "final", "four-phase, raid-wide mechanic rotation")]),

    ("charred_shrine_raid", "The Charred Shrine — Deep Vault", "ashweald", "charred_shrine",
     10, 50, 55,
     "Beneath the 5-man shrine — where the prayer that failed still echoes",
     [("ember_high_priest", "Ember High Priest", "lieutenant", "fire-dot raid aoe"),
      ("pact_saint", "The Pact-Saint", "lieutenant", "oath-mechanic on 3 raid members"),
      ("twin_saints_raid", "The Twin Saints — Ascended", "lieutenant", "raid-scale twin mechanic"),
      ("first_flame", "The First Flame", "final", "raid-wide fire-dance, positional")]),

    # =========================================================================
    # 20-MAN RAIDS (level 60)
    # =========================================================================
    ("crown_bastion", "The Crown Bastion", "crown_of_ruin", "crown_bastion",
     20, 60, 60,
     "The siege of the Crown — Concord and Rend both besieging the same ruin, from opposite sides",
     [("outer_gate_captain", "Outer-Gate Captain", "lieutenant", "aoe trash-clear mechanic"),
      ("siege_engineer_pair", "Siege-Engineer Pair", "lieutenant", "dual-boss, destroy siege engines"),
      ("inner_warden", "The Inner Warden", "lieutenant", "teleport mechanic, positional"),
      ("captain_of_the_crown", "Captain of the Crown", "lieutenant", "charge-phase, tank-rotate"),
      ("the_throne_claimant", "The Throne-Claimant", "final", "five-phase final encounter; a contested title, not a boss")]),

    ("ancient_wyrm_lair", "The Ancient Wyrm Lair", "frost_spine", "frost_gate",
     20, 60, 60,
     "Open-world-to-instance transition — the oldest thing on the Spine, older than both factions",
     [("wyrm_broodlings_pack", "Wyrm Broodlings — Pack", "lieutenant", "pack of 12 adds, cleave-focused"),
      ("ice_wyrm_scout", "Ice-Wyrm Scout", "lieutenant", "fly-phase, ranged-positional"),
      ("the_elder_wyrm", "The Elder Wyrm", "final", "six phases: breath, claw, fly, landing, tail, final-frenzy — iconic long fight")]),
]


# ---------------------------------------------------------------------------
# Writers
# ---------------------------------------------------------------------------

LEVEL_BAND = lambda lo: "1-10" if lo <= 10 else "11-30" if lo <= 30 else "31-45" if lo <= 45 else "46-60"


def instance_core(entry) -> dict:
    iid, name, zone, hub, size, lo, hi, theme, bosses = entry
    kind = "dungeon" if size == 5 else "raid"
    tier = "leveling" if hi < 45 else "endgame"
    return {
        "id": iid,
        "name": name,
        "kind": kind,
        "group_size": size,
        "zone": zone,
        "entrance_hub": hub,
        "level_range": {"min": lo, "max": hi},
        "level_band": LEVEL_BAND(lo),
        "tier": tier,
        "boss_count": len(bosses),
        "estimated_clear_minutes": {
            5: 45 if hi < 45 else 75,
            10: 120,
            20: 180,
        }[size],
        "loot_tier": (
            "low" if hi < 25
            else "mid" if hi < 40
            else "high" if hi < 50
            else "endgame"
        ),
        "lockout": {
            "dungeon": "none (rerunnable)",
            "raid": "weekly" if size >= 10 else "none",
        }[kind],
        "theme": theme,
        "coop_notes": (
            "tight-coop-friendly; duo/trio can attempt under-level with care"
            if size == 5 else
            "guild-scale content; organized group required"
        ),
    }


def instance_bosses(entry) -> dict:
    iid, _name, _zone, _hub, _size, lo, hi, _theme, bosses = entry
    # per-boss level scales across the range
    spread = (hi - lo) / max(1, len(bosses) - 1) if len(bosses) > 1 else 0
    out = []
    for i, (bid, bname, role, mechanic) in enumerate(bosses):
        boss_level = round(lo + spread * i) if spread else hi
        out.append({
            "id": bid,
            "name": bname,
            "role_tag": role,
            "level": boss_level,
            "mechanic": mechanic,
            "hp_tier": (
                "heroic" if role == "final"
                else "elite" if role in ("lieutenant", "miniboss")
                else "standard"
            ),
        })
    return {"id": f"{iid}_bosses", "instance": iid, "bosses": out}


# ---------------------------------------------------------------------------
# Summary index
# ---------------------------------------------------------------------------

def summary() -> dict:
    by_size = {5: 0, 10: 0, 20: 0}
    by_band = {"1-10": 0, "11-30": 0, "31-45": 0, "46-60": 0}
    total_bosses = 0
    for e in INSTANCES:
        by_size[e[4]] += 1
        by_band[LEVEL_BAND(e[5])] += 1
        total_bosses += len(e[-1])
    return {
        "id": "_dungeons_index",
        "description": "Aggregate stats of all instances. Regenerated by seed_dungeons.py.",
        "counts": {
            "total_instances": len(INSTANCES),
            "total_bosses": total_bosses,
            "by_group_size": by_size,
            "by_level_band": by_band,
        },
        "design_notes": [
            "Denser than WoW Classic's ~20 five-mans; Vaern aims for ~26 five-mans + 4 ten-mans + 2 twenty-mans",
            "No 40-man tier — the strict-coop household/guild social unit does not support it",
            "Every contested and endgame zone has at least one instance entrance",
            "Mid-tier 5-mans cluster at 18-35 to carry players through the second long xp stretch",
        ],
    }


def main() -> None:
    for e in INSTANCES:
        iid = e[0]
        write(OUT / iid / "core.yaml", instance_core(e))
        write(OUT / iid / "bosses.yaml", instance_bosses(e))
    write(OUT / "_index.yaml", summary())
    print(f"wrote {len(INSTANCES)} instances, {sum(len(e[-1]) for e in INSTANCES)} bosses")


if __name__ == "__main__":
    main()
