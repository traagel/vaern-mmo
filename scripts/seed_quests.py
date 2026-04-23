#!/usr/bin/env python3
"""Seed src/generated/world/zones/<zone>/quests/ with main chain + side + filler.

Layout per zone:
  quests/
    _summary.yaml                 counts, chain refs, budget check
    chains/<chain_id>.yaml        main storyline, steps inline
    side/<hub_id>.yaml            side quests per hub, templated by biome
    filler.yaml                   filler pool descriptor (kill/collect/courier/bounty)

Idempotent — re-run after manifest edits.
"""
from __future__ import annotations

from pathlib import Path
import yaml

REPO = Path(__file__).resolve().parents[1]
WORLD = REPO / "src" / "generated" / "world"


def write(path: Path, data: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "w") as f:
        yaml.safe_dump(data, f, sort_keys=False, default_flow_style=False)


# ---------------------------------------------------------------------------
# Main chain storylines per zone — hand-crafted beats, procedurally expanded
# ---------------------------------------------------------------------------
# Format per zone: (chain_id, title, premise, [beats])
# Beat: (step_name, objective_kind, target_hint)
#   objective_kind: talk | investigate | kill | collect | deliver | escort | ritual | defeat

ZONE_CHAINS = {
    # ------- STARTER ZONES (1-5) — short 4-5 step chains ---------------------
    "dalewatch_marches": ("chain_dalewatch_first_ride",
        "The First Ride",
        "A new Dalewatch rider takes their first patrol; a shepherd's missing child is not missing for simple reasons",
        [("meet_the_warden", "talk", "the Dalewatch Warden"),
         ("find_the_shepherd", "talk", "Old Brenn the shepherd"),
         ("track_the_trail", "investigate", "bloodied path into the reed-brake"),
         ("confront_the_drifter", "kill", "the drifter-mage hiding by the river"),
         ("return_the_child", "deliver", "the rescued child to Dalewatch Keep")]),
    "stoneguard_deep": ("chain_stoneguard_hearth_oath",
        "The Hearth-Oath",
        "A newly-forged Hearthkin takes the hearth-oath; the forge-fire refuses to light for the first time in a generation",
        [("present_to_the_thane", "talk", "Thane Borin"),
         ("gather_oath_iron", "collect", "oath-iron from the collapsed upper mine"),
         ("wake_the_old_smith", "talk", "the old smith in his retirement cell"),
         ("relight_the_hearth", "ritual", "the cold-hearth ceremony"),
         ("defend_the_forge", "defeat", "deepworm that burst through during the rite")]),
    "sunward_reach": ("chain_sunward_first_rite",
        "The First Rite of Sun",
        "A Sunward initiate petitions the Spire; the ritual is interrupted by something that should not be able to reach a sun-bound tower",
        [("petition_the_spire", "talk", "Archon Tennadiel"),
         ("gather_sun_glass", "collect", "sun-glass shards from the ritual orchard"),
         ("stand_the_vigil", "ritual", "the dawn-vigil on the spire's west face"),
         ("repel_the_shadow", "kill", "shadow-touched intruder in the archive"),
         ("report_the_breach", "deliver", "the archon's sealed letter")]),
    "wyrling_downs": ("chain_wyrling_guild_dues",
        "The Guild Dues",
        "A Wyrling apprentice finds their first guild dues are a cover for something older than the guild itself",
        [("join_the_guild", "talk", "Warden-of-Dues Pip"),
         ("collect_first_dues", "collect", "unpaid dues from three reluctant shops"),
         ("follow_the_old_ledger", "investigate", "the back-alley cellar the ledger points to"),
         ("confront_the_shadow_broker", "kill", "the hidden broker who has been skimming for decades")]),
    "firland_greenwood": ("chain_firland_greenmoot",
        "The Greenmoot Call",
        "A new Firland wanderer is called to the Greenmoot; the wood is speaking to one elder tree, and not in a good voice",
        [("hear_the_call", "talk", "Elder-Mother Holle"),
         ("walk_the_old_path", "investigate", "the mile-tree circle"),
         ("listen_to_the_sick_tree", "ritual", "the sick oak at the path's end"),
         ("burn_the_rot", "kill", "rot-beast nested at the tree's root"),
         ("sing_the_moot_closed", "ritual", "the formal moot-closing song")]),

    "ashen_holt": ("chain_ashen_exile_oath",
        "The Exile-Oath",
        "A Darkling initiate learns why their people were exiled, one layer at a time — and the Sunward version is only half true",
        [("answer_the_summons", "talk", "Mistress Vael"),
         ("read_the_forbidden_tome", "investigate", "the sealed library chamber"),
         ("walk_the_exile_road", "investigate", "the old exile-road waystones"),
         ("meet_the_first_exile", "talk", "the first exile's bound shade"),
         ("swear_the_oath", "ritual", "the exile-oath at the blackleaf bower")]),
    "barrow_coast": ("chain_barrow_hollow_vow",
        "The Hollow Vow",
        "A newly-woken Gravewrought shell has a vow it does not remember; it must find the last witness who still remembers who they were",
        [("wake_to_duty", "talk", "Rite-Binder Corda"),
         ("walk_your_own_grave", "investigate", "the shell-wearer's own barrow"),
         ("find_the_last_witness", "talk", "a dying Hraun elder in the silent harbor"),
         ("defend_the_witness", "defeat", "Concord raiders sent to silence the elder"),
         ("honor_the_vow", "ritual", "the vow-honoring at sea")]),
    "pactmarch": ("chain_pactmarch_first_contract",
        "The First Contract",
        "A Kharun scholar signs their first personal pact with an Old Power; the terms are not what their elders told them",
        [("present_to_the_pactmaster", "talk", "Pactmaster Thell"),
         ("read_the_standard_contract", "investigate", "the Scholar's Camp archives"),
         ("interrogate_a_bound_spirit", "talk", "a lesser-bound thing in a warded circle"),
         ("renegotiate_the_terms", "ritual", "the private contract-rite"),
         ("bear_the_mark", "ritual", "the mark-receiving at Pactstone Hold")]),
    "skarnreach": ("chain_skarnreach_blood_price",
        "The Blood-Price",
        "A young Skarn warrior is assigned a blood-debt against a rival clan; the rival clan is not actually the debtor",
        [("claim_your_axe", "collect", "an unclaimed war-axe from the longhall"),
         ("ride_to_bloodwater", "talk", "Bloodwater Landing's chief"),
         ("find_the_rival_clanhead", "kill", "the rival clanhead on the fjord shore"),
         ("return_and_confront_the_liar", "kill", "the Skarn elder who sent you under false colors")]),
    "scrap_marsh": ("chain_scrap_warren_heist",
        "The Warren Heist",
        "A young Skrel is offered a shot at a heist; the heist target is their own warren's secret stash — setup by a rival crew",
        [("take_the_job", "talk", "Glib the fence"),
         ("case_the_mark", "investigate", "the target warren's back tunnels"),
         ("pull_the_heist", "collect", "the marked strongbox contents"),
         ("walk_into_the_trap", "defeat", "the rival crew waiting to rob you"),
         ("flip_the_trap", "kill", "Glib himself at the fence's den")]),

    # ------- MID-TIER FACTION ZONES (5-30) — longer 6-8 step chains ----------
    "heartland_ride": ("chain_heartland_highway_war",
        "The Highway War",
        "The Dalewatch highway is falling to a bandit captain whose real patron is a corrupt baron — a slow-burn political chain",
        [("report_the_raids", "talk", "Ride Warden Telyn"),
         ("clear_the_first_camp", "kill", "bandit captain at the crossroads"),
         ("interrogate_the_prisoner", "talk", "bandit lieutenant held at the Ride"),
         ("follow_the_caravan_ledger", "investigate", "the cooked caravan manifests"),
         ("confront_the_baron", "talk", "Baron Vessen at his manor"),
         ("assault_the_manor", "defeat", "the baron's house-guard"),
         ("break_the_captain", "kill", "Captain Byrn at the Watchtower Keep"),
         ("report_the_victory", "deliver", "Warden Telyn's dispatch to Dalewatch")]),
    "irongate_pass": ("chain_irongate_deep_silence",
        "The Deep Silence",
        "The Hearthkin deep-roads have gone quiet; the collapse was no accident, and what's digging up is not from this side",
        [("speak_with_the_gate_warden", "talk", "Gate-Warden Drum"),
         ("scout_the_silent_mine", "investigate", "the upper collapsed shafts"),
         ("recover_the_lost_survey", "collect", "the survey-map from a dead scout"),
         ("read_the_deep_song", "ritual", "the stone-reading at the old hearth"),
         ("seal_the_upper_breach", "ritual", "the upper-breach seal-song"),
         ("hunt_the_stone_eater", "kill", "the thing that broke the seal"),
         ("report_to_the_old_smith", "deliver", "the recovered fragments")]),
    "silverleaf_wood": ("chain_silverleaf_moth_court",
        "The Moth Court",
        "The pre-Veyr ruins in Silverleaf were not abandoned — the elvish wild-court is waking, and they do not recognize the new treaty",
        [("meet_the_lodge_keeper", "talk", "Lodge-Keeper Arvel"),
         ("track_the_moth_signs", "investigate", "moth-dust trails in the canopy"),
         ("stand_the_court_trial", "ritual", "the wild-court's welcome-challenge"),
         ("witness_the_old_verdict", "talk", "the moth-king's shade"),
         ("recover_the_broken_treaty", "collect", "the original elvish-Veyr treaty"),
         ("amend_the_treaty", "ritual", "the amendment-binding"),
         ("repel_the_refusers", "kill", "the faction of wild-court refusers")]),
    "market_crossing": ("chain_market_crossing_freeport_coup",
        "The Freeport Coup",
        "Market Crossing's merchant council is under a slow coup by a Rend-backed trade-guild; a visiting Concord inspector is the target",
        [("present_credentials", "talk", "Freeport Magistrate Hulle"),
         ("investigate_the_docks", "investigate", "the suspicious caravanserai warehouse"),
         ("plant_a_double_agent", "talk", "a sympathetic smuggler"),
         ("protect_the_inspector", "escort", "the Concord inspector through market row"),
         ("raid_the_warehouse", "defeat", "the trade-guild's private thugs"),
         ("confront_the_coup_leader", "kill", "the trade-guild's hidden patron"),
         ("restore_the_council", "deliver", "the inspector's sealed report")]),
    "greenwood_deep": ("chain_greenwood_wyrd_tarn",
        "The Wyrd Tarn",
        "The deep lake is speaking in old Firland tongue no one has used in a century; it remembers something the Firland elders deliberately forgot",
        [("petition_the_deep_moot", "talk", "Elder-Mother Holle at the Deep Moot"),
         ("walk_the_moss_cairn", "investigate", "the moss-cairn waystone"),
         ("gather_the_hunter_testimony", "talk", "the hunter at the lean-to"),
         ("dive_the_tarn", "ritual", "the tarn-depth diving-rite"),
         ("meet_the_drowned_warden", "talk", "the drowned-warden at the depths"),
         ("recover_the_forbidden_name", "collect", "the name-stone from the tarn's bed"),
         ("close_the_old_wound", "ritual", "the closing-rite at Wyrd Tarn"),
         ("return_to_the_moot", "deliver", "the resolution to the moot")]),
    "gravewatch_fields": ("chain_gravewatch_wrong_rite",
        "The Wrong Rite",
        "The Gravewatch Fields' ancestor-rite has slipped; the dead are waking half-made, neither shell nor person, and the rite-binders cannot stop it alone",
        [("present_to_the_rite_binder", "talk", "Rite-Binder Corda"),
         ("walk_the_failed_field", "investigate", "the failed burial rows"),
         ("collect_the_rite_fragments", "collect", "broken rite-stones from the field"),
         ("interrogate_a_half_made", "talk", "a half-made shell at the watch-crypt"),
         ("find_the_source_of_the_break", "investigate", "the lich-weep mausoleum's outer chamber"),
         ("perform_the_counter_rite", "ritual", "the counter-binding"),
         ("destroy_the_broken_vessels", "kill", "waves of half-made shells"),
         ("close_the_wound", "ritual", "the field-closing")]),
    "shadegrove": ("chain_shadegrove_coven_schism",
        "The Coven Schism",
        "The Shadegrove coven has split between the old pact-keepers and a new faction that wants to renegotiate with a different Old Power",
        [("present_to_the_reach_matron", "talk", "Matron Shade at the reach"),
         ("attend_the_council_meeting", "investigate", "the coven's secret council"),
         ("gather_intelligence_on_the_schismatics", "collect", "schismatic letters from the Cinder Post"),
         ("infiltrate_the_dark_glade", "investigate", "the schismatic inner sanctum"),
         ("duel_the_schismatic_champion", "kill", "the sanctum champion"),
         ("bear_witness_at_the_trial", "ritual", "the coven's formal trial"),
         ("execute_the_ringleader", "kill", "the schismatic leader"),
         ("report_to_the_matron", "deliver", "the trial's verdict")]),
    "pact_causeway": ("chain_pact_indelible_curse",
        "The Indelible Curse",
        "The Oath Pillar is writing a curse onto any Kharun who passes it; something new is binding itself into the old contract",
        [("present_to_causeway_hold", "talk", "Causeway Chief Nell"),
         ("document_the_curse_pattern", "investigate", "the pillar's new inscriptions"),
         ("consult_the_stone_reader", "talk", "the stone-reader in his camp"),
         ("enter_the_vault", "investigate", "the oath-pillar vault"),
         ("confront_the_pact_scribe", "kill", "the pact-scribe rewriting the contract"),
         ("untangle_the_new_terms", "ritual", "the untangling-rite"),
         ("reseal_the_pillar", "ritual", "the pillar-resealing")]),
    "skarncamp_wastes": ("chain_skarncamp_war_that_does_not_end",
        "The War That Does Not End",
        "The Skarncamp Wastes' war-stake has never been pulled down; it is the physical anchor of an old grudge the Skarn elders refuse to release",
        [("report_to_ravenmeet", "talk", "Chief Skorrn at Ravenmeet"),
         ("hear_the_war_tally", "investigate", "the tally-bones at the war-stake"),
         ("ride_the_old_battle_line", "investigate", "the old battle-line markers"),
         ("find_the_first_casualty", "collect", "the first casualty's relic"),
         ("speak_to_the_last_widow", "talk", "the last surviving widow of the first war"),
         ("negotiate_the_release", "ritual", "the release-negotiation"),
         ("pull_down_the_stake", "ritual", "the stake-pulling ceremony"),
         ("defeat_the_refusers", "kill", "war-hawks who refuse the release")]),
    "scrap_flats": ("chain_scrap_flats_tinker_cartel",
        "The Tinker Cartel",
        "The Scrap Flats tinker guilds are being rolled up by a single crime-cartel run out of the Pit-Tinker Works; the Great Warren is the next target",
        [("present_to_the_warren_council", "talk", "the Great Warren council"),
         ("case_the_rust_creek_shop", "investigate", "the rust creek tinker shop"),
         ("extract_a_witness", "escort", "a terrified tinker to the warren"),
         ("raid_the_scavenger_mound", "defeat", "the scavenger-mound enforcers"),
         ("steal_the_cartel_ledger", "collect", "the cartel's master ledger"),
         ("infiltrate_the_pit_tinker_works", "investigate", "the cartel's inner compound"),
         ("confront_master_tinker_glub", "kill", "Master Tinker Glub"),
         ("break_the_cartel", "ritual", "the warren's public ledger-burning")]),

    # ------- CONTESTED ZONES (30-45) — zone-spanning, PvP-aware --------------
    "ruin_line_north": ("chain_ruinline_north_the_rite_unfinished",
        "The Rite Unfinished",
        "The Burned Abbey's rite was never completed; if a Concord champion and a Rend champion can be brought to finish it together, the Ruin Line's scar stops bleeding",
        [("meet_the_scarhold_commander", "talk", "Commander Verrn at Scarhold"),
         ("recover_the_abbey_plan", "collect", "the original rite-plan from the No-Man's Ridge"),
         ("recruit_the_concord_champion", "talk", "a Concord paladin at Scarhold"),
         ("recruit_the_rend_champion", "talk", "a Rend pact-blade at the Scarhold neutral camp"),
         ("escort_both_to_the_abbey", "escort", "both champions through the Burned Abbey"),
         ("stand_the_joint_rite", "ritual", "the joint rite at the abbey altar"),
         ("defend_against_the_refusers", "defeat", "both factions' hawks who oppose the peace"),
         ("witness_the_closing", "ritual", "the rite's closing after centuries"),
         ("carry_the_news", "deliver", "the news to both capitals")]),
    "ruin_line_south": ("chain_ruinline_south_sunken_archive",
        "The Sunken Archive",
        "The Sunken Tower's archive contains the original Sunward-Veyr mutual-defense compact; both factions' leaders want it, for opposite reasons",
        [("receive_the_archive_brief", "talk", "Concord scout-captain at Southscar Camp"),
         ("cross_the_two_kings_field", "investigate", "the old battlefield's unmarked graves"),
         ("recover_the_archive_key", "collect", "the first-tower librarian's key"),
         ("breach_the_sunken_tower", "defeat", "the sunken tower's outer defense"),
         ("decipher_the_compact", "investigate", "the compact's wards"),
         ("choose_a_side", "ritual", "the choice-ritual — Concord or Rend receipt"),
         ("defend_the_choice", "defeat", "the opposing faction's strike-team"),
         ("deliver_the_compact", "deliver", "the compact to your chosen side")]),
    "iron_strand": ("chain_iron_strand_storm_call",
        "The Storm-Call",
        "A Rend raider-lord has bound a storm-thing to the Old Lighthouse; its light calls storms that will sink both factions' fleets without discrimination",
        [("receive_the_strand_brief", "talk", "Strand Redoubt commander"),
         ("scout_the_wreckshore", "investigate", "the wreckshore for raider sign"),
         ("recover_the_storm_binding_rite", "collect", "the storm-binding rite-book from a wrecked ship"),
         ("assault_the_lighthouse", "defeat", "the lighthouse's outer raiders"),
         ("break_the_storm_binding", "ritual", "the storm-releasing"),
         ("kill_the_raider_lord", "kill", "the raider-lord at the lighthouse summit"),
         ("witness_the_storm_dispersal", "investigate", "the storm-thing's departure"),
         ("report_to_both_sides", "deliver", "a joint report to Concord and Rend wardens")]),
    "ashweald": ("chain_ashweald_twin_saints",
        "The Twin Saints",
        "The Charred Shrine holds the Twin Saints — one Concord, one Rend — who died praying together during the Coming; someone is trying to resurrect them, and they do not want to come back",
        [("receive_the_shrine_brief", "talk", "the joint-commander at Ember Crossing"),
         ("gather_the_saints_accounts", "collect", "survivor testimonies about the Twin Saints' last prayer"),
         ("walk_the_pyre_camp", "investigate", "the pyre-camp's old funeral records"),
         ("confront_the_resurrectionist", "talk", "the cultist attempting the resurrection"),
         ("breach_the_charred_shrine", "defeat", "the shrine's cultist outer guard"),
         ("honor_the_saints_will", "ritual", "the saints' own requested rite"),
         ("destroy_the_resurrection_materials", "kill", "the remaining cultists at the altar"),
         ("witness_the_saints_peace", "ritual", "the saints' final resting-rite")]),

    # ------- ENDGAME ZONES (45-60) — guild-scale, multi-phase ---------------
    "blackwater_deep": ("chain_blackwater_exile_marshal",
        "The Exile-Marshal",
        "A Concord marshal exiled during the wars has built a kingdom in the Blackwater Deep; he is not wrong, exactly, but he is dangerous",
        [("receive_the_deepwater_brief", "talk", "Deepwater Redoubt commander"),
         ("gather_the_exile_history", "collect", "the exile-marshal's court-martial records"),
         ("infiltrate_the_black_eel_camp", "investigate", "the Black-Eel Camp for the marshal's agents"),
         ("meet_a_marshal_loyalist", "talk", "a guardsman still loyal to the exile"),
         ("breach_deepwater_keep", "defeat", "the keep's outer defenses"),
         ("duel_the_marshal", "kill", "Warlord Kessen in his hall"),
         ("decide_his_legacy", "ritual", "the posthumous judgment of his kingdom"),
         ("disperse_the_loyalists", "defeat", "the marshal's last warband"),
         ("report_the_closure", "deliver", "the final report to both Concord command and the Redoubt")]),
    "frost_spine": ("chain_frost_spine_elder_wyrm",
        "The Elder Wyrm",
        "The thing sleeping at the top of the Frost Spine is older than both factions; the old Veyr knew how to sing it back to sleep, but the song is lost",
        [("receive_the_spine_brief", "talk", "the Frost Gate commander"),
         ("recover_the_old_song_fragments", "collect", "song-stones from the Yeti-Warren and frozen watchtowers"),
         ("consult_the_shaman_of_the_frozen", "talk", "the Frozen Shaman (post-dungeon)"),
         ("decipher_the_full_song", "ritual", "the song-restoration"),
         ("climb_the_spine", "escort", "a joint-faction song-party up the spine"),
         ("sing_the_sleeping_song", "ritual", "the full song-rite at the summit"),
         ("defeat_the_refusers", "defeat", "factions who want the wyrm awake and weaponized"),
         ("complete_the_sleeping", "ritual", "the wyrm's return to sleep"),
         ("seal_the_summit", "ritual", "the summit-sealing rite")]),
    "sundering_mines": ("chain_sundering_mines_carver_mother",
        "The Carver-Mother",
        "The Carver-Mother is only the middle child of her brood; her own mother still carves, deeper than the mines go, and she is rising",
        [("receive_the_mines_brief", "talk", "Mineshaft Alpha warden"),
         ("gather_the_carver_research", "collect", "carver-kin research from the Old-Tunnel Camp"),
         ("interrogate_a_captured_carver", "talk", "a bound carver-kin"),
         ("recover_the_deep_map", "collect", "the deep-map from the Lightless Gallery approach"),
         ("lead_the_joint_expedition", "escort", "a joint-faction deep-expedition"),
         ("confront_the_carver_prime", "kill", "the Carver Prime at the galleries' bottom"),
         ("seal_the_deeper_shaft", "ritual", "the deep-sealing"),
         ("defend_the_seal", "defeat", "kin trying to break the seal from below"),
         ("close_the_mines", "ritual", "the mines' formal closing")]),
    "crown_of_ruin": ("chain_crown_the_throne_claimant",
        "The Throne-Claimant",
        "The Crown of Ruin's throne is not empty; something has sat in it since the Coming, and both factions now have a champion ready to claim it — neither of them should win",
        [("receive_the_crown_brief", "talk", "Crown Bastion high-commander"),
         ("gather_the_throne_histories", "collect", "throne-claim historical accounts from both factions"),
         ("meet_the_last_candle_keeper", "talk", "the hermit at the Last Candle"),
         ("walk_the_throne_approach", "investigate", "the throne-approach ruins"),
         ("witness_the_first_claimant", "defeat", "the Concord champion's claim-attempt"),
         ("witness_the_second_claimant", "defeat", "the Rend champion's claim-attempt"),
         ("confront_the_sitting_thing", "ritual", "the throne-thing's own choosing-rite"),
         ("refuse_the_throne", "ritual", "the refusal-rite — neither champion, nor the thing"),
         ("seal_the_throne_of_ash", "ritual", "the throne's permanent sealing"),
         ("carry_the_news_home", "deliver", "the news to both capitals — and the refusal-rite story")]),
}


# ---------------------------------------------------------------------------
# Quest builders
# ---------------------------------------------------------------------------

OBJECTIVE_KIND_TO_SCHEMA = {
    "talk":        lambda target: {"kind": "talk",        "target_hint": target},
    "investigate": lambda target: {"kind": "investigate", "target_hint": target},
    "kill":        lambda target: {"kind": "kill",        "target_hint": target, "count": 1},
    "collect":     lambda target: {"kind": "collect",     "target_hint": target, "count": 4},
    "deliver":     lambda target: {"kind": "deliver",     "target_hint": target},
    "escort":      lambda target: {"kind": "escort",      "target_hint": target},
    "ritual":      lambda target: {"kind": "ritual",      "target_hint": target},
    "defeat":      lambda target: {"kind": "defeat",      "target_hint": target, "group_suggested": True},
}


def build_chain(zone: str, zone_meta: dict, chain_def) -> dict:
    cid, title, premise, beats = chain_def
    lo = zone_meta["level_range"]["min"]
    hi = zone_meta["level_range"]["max"]
    # Spread step levels across the zone's level range
    n = len(beats)
    steps = []
    for i, (step_name, kind, target) in enumerate(beats):
        frac = i / max(1, n - 1)
        step_level = round(lo + (hi - lo) * frac)
        step_xp = 400 * step_level + 120 * step_level * step_level
        quest_xp = round(step_xp * 0.12)  # ~12% of one level per main-chain step
        steps.append({
            "step": i + 1,
            "id": f"{cid}__{i + 1:02d}_{step_name}",
            "name": step_name.replace("_", " ").title(),
            "level": step_level,
            "objective": OBJECTIVE_KIND_TO_SCHEMA[kind](target),
            "xp_reward": quest_xp,
            "gold_reward_copper": step_level * 15,
            "prerequisite": f"{cid}__{i:02d}_{beats[i-1][0]}" if i > 0 else None,
        })
    final_boss_hint = None
    for b in reversed(beats):
        if b[1] in ("kill", "defeat"):
            final_boss_hint = b[2]
            break
    return {
        "id": cid,
        "zone": zone,
        "title": title,
        "premise": premise,
        "total_steps": n,
        "final_reward": {
            "xp_bonus": steps[-1]["xp_reward"] * 2,
            "gold_bonus_copper": hi * 100,
            "item_hint": f"zone-appropriate {zone_meta.get('tier','?')}-tier reward",
            "title_hint": f"'{title}' completion title",
        },
        "breadcrumb_from": "capital hub of the zone's faction-home",
        "final_boss_hint": final_boss_hint,
        "steps": steps,
    }


# ---------------------------------------------------------------------------
# Side quests — templated per hub
# ---------------------------------------------------------------------------

SIDE_TEMPLATES_BY_BIOME = {
    "river_valley": [
        ("the_miller_s_problem",  "kill",    "wolves harrying the miller's livestock"),
        ("the_lost_ledger",       "collect", "flood-soaked ledger pages"),
        ("the_upstream_camp",     "investigate", "an upstream camp that hasn't reported in"),
        ("the_courier_run",       "deliver", "a message to the next river settlement"),
    ],
    "highland": [
        ("the_fog_walker",        "kill",    "fog-born predator stalking the high paths"),
        ("the_lost_herd",         "investigate", "goats gone missing near the cliff-line"),
        ("the_shrine_offering",   "deliver", "an offering to a remote hill-shrine"),
        ("the_cairn_count",       "collect", "cairn-stones displaced by last storm"),
    ],
    "temperate_forest": [
        ("the_timber_scout",      "investigate", "a timber-camp that went silent"),
        ("the_beast_cull",        "kill",    "overpopulated forest predators"),
        ("the_ranger_arms",       "collect", "recovered ranger-arms from an ambush site"),
        ("the_oath_sapling",      "deliver", "a ritual sapling to a distant grove"),
    ],
    "mountain": [
        ("the_rockfall_watch",    "investigate", "a recent rockfall for survivors"),
        ("the_ore_sample",        "collect", "ore samples from the upper shafts"),
        ("the_stoneworm_cull",    "kill",    "rockworms threatening a settlement"),
        ("the_old_forge_runner",  "deliver", "a forge-commission to the upper settlements"),
    ],
    "marshland": [
        ("the_rot_fever_patient", "deliver", "rot-fever tonic to an isolated hut"),
        ("the_drake_hunt",        "kill",    "marsh-drake preying on travelers"),
        ("the_sinking_camp",      "investigate", "a camp reportedly sinking into the mire"),
        ("the_mire_relics",       "collect", "mire-preserved relics"),
    ],
    "coastal_cliff": [
        ("the_wrecksalvage",      "collect", "salvage from a recent wreck"),
        ("the_seabird_plague",    "kill",    "aggressive seabirds nesting near the fishery"),
        ("the_cliff_watch",       "investigate", "a cliff-watcher who stopped reporting"),
        ("the_lighthouse_supplies", "deliver", "supplies to a remote lighthouse"),
    ],
    "fjord": [
        ("the_landing_sign",      "investigate", "a raider-landing sign"),
        ("the_seal_cull",         "kill",    "rogue orca harassing fjord fishers"),
        ("the_longship_repair",   "collect", "longship-repair timber"),
        ("the_ice_floe_search",   "investigate", "a missing boat on the ice-floe"),
    ],
    "ruin": [
        ("the_relic_hunter",      "collect", "pre-war relics from ruin-edges"),
        ("the_wight_cull",        "kill",    "war-wights that don't know the war ended"),
        ("the_broken_shrine",     "investigate", "a shrine reportedly glowing at night"),
        ("the_survivor_rescue",   "escort",  "a recent ruin-scavenger survivor home"),
    ],
    "ashland": [
        ("the_ashwolf_pack",      "kill",    "an ashwolf pack stalking caravans"),
        ("the_cinder_vein",       "collect", "cinder-vein samples"),
        ("the_storm_camp_check",  "investigate", "a camp caught in the last ash-storm"),
        ("the_soot_lung_remedy",  "deliver", "soot-lung remedy to afflicted workers"),
    ],
}

ROLE_QUEST_COUNT = {
    "capital": 5,
    "outpost": 3,
    "waypoint": 2,
    "ruin": 1,
}


def build_side_quests(zone: str, zone_meta: dict, hub_id: str, hub_role: str, biome: str) -> dict:
    lo = zone_meta["level_range"]["min"]
    hi = zone_meta["level_range"]["max"]
    templates = SIDE_TEMPLATES_BY_BIOME.get(biome, SIDE_TEMPLATES_BY_BIOME["temperate_forest"])
    n = min(ROLE_QUEST_COUNT[hub_role], len(templates))
    quests = []
    for i in range(n):
        tname, kind, target = templates[i]
        level = round(lo + (hi - lo) * (i / max(1, n - 1))) if n > 1 else lo + (hi - lo) // 2
        quest_xp = round((400 * level + 120 * level * level) * 0.04)
        quests.append({
            "id": f"side__{hub_id}__{tname}",
            "name": tname.replace("_", " ").title(),
            "hub": hub_id,
            "type": "side",
            "level": level,
            "objective": OBJECTIVE_KIND_TO_SCHEMA[kind](target),
            "xp_reward": quest_xp,
            "gold_reward_copper": level * 5,
            "repeatable": False,
        })
    return {
        "id": f"side_quests__{hub_id}",
        "hub": hub_id,
        "hub_role": hub_role,
        "zone": zone,
        "biome": biome,
        "quest_count": len(quests),
        "quests": quests,
    }


# ---------------------------------------------------------------------------
# Filler pool — procedural pool descriptor, not individual files
# ---------------------------------------------------------------------------

FILLER_BUCKETS = [
    ("kill_grinder",  "kill",    "pool of 6-10 'kill N of local mob-type' quests scaling with zone mobs"),
    ("collect_grinder", "collect", "pool of 6-10 'gather N of local resource' quests tied to biome"),
    ("courier_relay", "deliver", "pool of 4-6 inter-hub courier runs within the zone"),
    ("bounty_board",  "kill",    "pool of 4-6 rare-spawn bounties posted at capital hubs"),
    ("lore_collection", "investigate", "pool of 3-5 ambient-lore discovery quests (no combat)"),
]


def build_filler(zone: str, zone_meta: dict, chain_steps: int, side_count: int) -> dict:
    lo = zone_meta["level_range"]["min"]
    hi = zone_meta["level_range"]["max"]
    band = zone_meta.get("tier", "mid")
    # Target filler so (chain + side + filler) >= 0.95 * budget
    budget = zone_meta["budget"]["quest_count_target"]
    target_filler = max(8, int(budget * 0.95) - chain_steps - side_count)
    # Split across 5 buckets with weights (kill-heavy, collect-heavy, courier, bounty, lore)
    weights = {
        "kill_grinder": 0.35,
        "collect_grinder": 0.30,
        "courier_relay": 0.15,
        "bounty_board": 0.12,
        "lore_collection": 0.08,
    }
    buckets = []
    for name, kind, desc in FILLER_BUCKETS:
        size = max(2, round(target_filler * weights[name]))
        buckets.append({
            "id": f"filler__{name}",
            "bucket": name,
            "dominant_objective_kind": kind,
            "pool_size": size,
            "level_range": {"min": lo, "max": hi},
            "avg_xp_reward_per_quest": round((400 * ((lo + hi) // 2) + 120 * ((lo + hi) // 2) ** 2) * 0.03),
            "avg_gold_reward_copper": ((lo + hi) // 2) * 3,
            "repeatable": True if name in ("kill_grinder", "collect_grinder", "courier_relay") else False,
            "description": desc,
        })
    return {
        "id": f"filler_pool__{zone}",
        "zone": zone,
        "tier": band,
        "total_pool_size": sum(b["pool_size"] for b in buckets),
        "buckets": buckets,
        "notes": "Filler quests are templated pools. Individual quest records are generated at runtime or in a later seed pass.",
    }


# ---------------------------------------------------------------------------
# Per-zone summary
# ---------------------------------------------------------------------------

def zone_summary(zone: str, zone_meta: dict, chain_data: dict, side_sum: int, filler_data: dict) -> dict:
    lo = zone_meta["level_range"]["min"]
    hi = zone_meta["level_range"]["max"]
    band_target = zone_meta["budget"]["quest_count_target"]
    chain_steps = chain_data["total_steps"] if chain_data else 0
    total_quests = chain_steps + side_sum + filler_data["total_pool_size"]
    return {
        "id": f"quest_summary__{zone}",
        "zone": zone,
        "level_range": {"min": lo, "max": hi},
        "tier": zone_meta.get("tier"),
        "budget_target": band_target,
        "actual": {
            "main_chain_steps": chain_steps,
            "side_quests": side_sum,
            "filler_pool": filler_data["total_pool_size"],
            "total": total_quests,
        },
        "coverage_ratio": round(total_quests / band_target, 2) if band_target else None,
        "main_chain": chain_data["id"] if chain_data else None,
        "notes": "Main chain is hand-crafted story; side is templated by biome+hub-role; filler is a procedural pool.",
    }


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def load(path: Path) -> dict:
    with open(path) as f:
        return yaml.safe_load(f)


def main() -> None:
    zones_root = WORLD / "zones"
    total_files = 0
    for zone_dir in sorted(zones_root.iterdir()):
        if not zone_dir.is_dir():
            continue
        zone = zone_dir.name
        zone_meta = load(zone_dir / "core.yaml")
        biome = zone_meta["biome"]

        # --- main chain -----------------------------------------------------
        chain_data = None
        if zone in ZONE_CHAINS:
            chain_def = ZONE_CHAINS[zone]
            chain_data = build_chain(zone, zone_meta, chain_def)
            write(zone_dir / "quests" / "chains" / f"{chain_data['id']}.yaml", chain_data)
            total_files += 1

        # --- side quests per hub --------------------------------------------
        side_sum = 0
        hubs_dir = zone_dir / "hubs"
        for hub_file in sorted(hubs_dir.iterdir()):
            hub = load(hub_file)
            side_pack = build_side_quests(zone, zone_meta, hub["id"], hub["role"], biome)
            write(zone_dir / "quests" / "side" / f"{hub['id']}.yaml", side_pack)
            side_sum += side_pack["quest_count"]
            total_files += 1

        # --- filler pool -----------------------------------------------------
        chain_steps = chain_data["total_steps"] if chain_data else 0
        filler_data = build_filler(zone, zone_meta, chain_steps, side_sum)
        write(zone_dir / "quests" / "filler.yaml", filler_data)
        total_files += 1

        # --- summary ---------------------------------------------------------
        write(zone_dir / "quests" / "_summary.yaml",
              zone_summary(zone, zone_meta, chain_data, side_sum, filler_data))
        total_files += 1

    print(f"wrote {total_files} quest files across {len(list(zones_root.iterdir()))} zones")


if __name__ == "__main__":
    main()
