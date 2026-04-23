#!/usr/bin/env python3
"""Seed 46 alternative Orders for archetypes 03..14 (plus finish 03).

Creates core.yaml + aesthetic.yaml + lore.yaml + specs.yaml per order under
the right archetype. After running, re-run assign_order_institutions.py and
refactor_institutions.create_chapter_stubs() to populate institution links.

Idempotent: overwrites existing files.
"""
from __future__ import annotations

from pathlib import Path

import yaml

REPO = Path(__file__).resolve().parents[1]
GENERATED = REPO / "src" / "generated"
ARCH_DIR = GENERATED / "archetypes"


def dump(p: Path, obj):
    p.parent.mkdir(parents=True, exist_ok=True)
    with open(p, "w") as f:
        yaml.safe_dump(obj, f, sort_keys=False, default_flow_style=False, width=120)


POSITIONS = {
    0: (100, 0, 0), 1: (75, 25, 0), 2: (50, 50, 0), 3: (25, 75, 0),
    4: (0, 100, 0), 5: (0, 75, 25), 6: (0, 50, 50), 7: (0, 25, 75),
    8: (0, 0, 100), 9: (25, 0, 75), 10: (50, 0, 50), 11: (75, 0, 25),
    12: (50, 25, 25), 13: (25, 50, 25), 14: (25, 25, 50),
}

ARCH_DIRS = {
    0: "00_warrior", 1: "01_oathguard", 2: "02_warcaster", 3: "03_grovecaster",
    4: "04_archmage", 5: "05_spellblooded", 6: "06_pactblade", 7: "07_loresinger",
    8: "08_shadower", 9: "09_marcher", 10: "10_duellist", 11: "11_raider",
    12: "12_runemartial", 13: "13_spirit_adept", 14: "14_pathwarden",
}


# Each order: (archetype_id, faction, id_stem, class_name, schools_taught,
#              pitch, palette_shift, motif, oath, doctrine, [2 specs]).
# specs = [(id, name, emphasis, description, [schools_focus])]
ORDERS = [
    # ========== Archetype 3 — Grovecaster ==========
    (3, "faction_a", "storm_druid", "Storm-Druid",
     {"arcana": ["nature", "lightning", "arcane"], "might": ["spear"]},
     "weather-bound druid in storm-grey leather and mossy cloak, lightning-veined staff, wind-tangled hair, arc-light crackling on spear-tip",
     "Concord palette shifted to storm-grey, indigo thunderhead, arc-white highlights",
     "forked lightning over antler-crown",
     "the wind knows the oath and remembers it",
     "what the grove calls the storm answers",
     [("storm_druid_thunder", "Thunder-Druid", "magic_dps", "lightning-focused storm-caller", ["lightning", "nature"]),
      ("storm_druid_windward", "Windward-Druid", "control", "wind and entangle crowd-control", ["nature", "arcane"])]),

    (3, "faction_b", "rot_druid", "Rot-Druid",
     {"arcana": ["blood", "nature", "shadow"], "might": ["spear"]},
     "dark-nature druid in leaf-mold hide and grave-bloom cloak, thorn-staff dripping ichor, rot-sigil on brow",
     "Rend black-and-red stained with decay-green and bone-pale mushroom accents",
     "rotting fern fronds coiled around a skull",
     "the grove forgives; the rot remembers",
     "what lives ends; what ends feeds",
     [("rot_druid_blight", "Blight-Druid", "magic_dps", "decay-damage offense caster", ["blood", "nature"]),
      ("rot_druid_bone", "Bone-Druid", "support", "ancestor-grove hex and curse-binding", ["shadow", "nature"])]),

    (3, "faction_b", "black_grove", "Black-Grove Druid",
     {"arcana": ["shadow", "earth", "nature"], "might": ["spear"]},
     "shadow-bound forest druid in charcoal bark-leather, obsidian staff, bone-trophy circlet, black ivy coiling the haft",
     "Rend black with deep-shadow violet and obsidian-stone accents",
     "black ivy wrapped around a standing stone",
     "the grove that does not bloom still stands",
     "shadow is the grove's older face",
     [("black_grove_stone", "Stone-Shadow", "control", "earth-and-shadow terrain control", ["earth", "shadow"]),
      ("black_grove_thorn", "Thorn-Shade", "magic_dps", "shadow-entangle and piercing nature offense", ["nature", "shadow"])]),

    # ========== Archetype 4 — Archmage ==========
    (4, "faction_a", "frost_archmage", "Frost Archmage",
     {"arcana": ["frost", "arcane", "light"]},
     "ice-mage in pale silver robes with frost-rime trim, crystalline staff breathing cold mist, frozen-rune circlet",
     "Concord silver deepened with ice-blue and bone-white frost-crystal accents",
     "six-point snowflake over an open book",
     "the silence of winter is a kind of truth",
     "cold preserves; fire forgets",
     [("frost_mage_glacier", "Glacier-Mage", "magic_dps", "frozen-damage heavy caster", ["frost", "arcane"]),
      ("frost_mage_warder", "Ice-Warder", "control", "freeze and slow crowd control", ["frost", "light"])]),

    (4, "faction_a", "lightning_archmage", "Lightning Archmage",
     {"arcana": ["lightning", "arcane", "light"]},
     "storm-mage in silver-and-blue robes crackling with static, orb-staff emitting lightning arcs, wind-wild hair",
     "Concord silver charged with arc-white and bright azure",
     "forked bolt piercing an open scroll",
     "the bolt answers the called word",
     "speed is scholarship's reward",
     [("storm_mage_bolt", "Bolt-Mage", "magic_dps", "single-target lightning burst", ["lightning", "arcane"]),
      ("storm_mage_chain", "Chain-Mage", "magic_dps", "chain-lightning AoE", ["lightning", "light"])]),

    (4, "faction_b", "necromancer", "Necromancer",
     {"arcana": ["blood", "shadow", "arcane"]},
     "bone-robed mage with grave-silk sashes, black staff tipped with a skull, ritual-scar hands, pale bone-cameo at throat",
     "Rend black-and-red deepened with bone-white and grave-silk cream",
     "open book over a upturned skull",
     "every name was a breath once",
     "the dead answer the trained tongue",
     [("necro_reaper", "Reaper", "magic_dps", "necrotic damage specialist", ["blood", "shadow"]),
      ("necro_binder", "Bone-Binder", "summoner", "bound-dead summoner", ["shadow", "arcane"])]),

    (4, "faction_b", "void_mage", "Void-Mage",
     {"arcana": ["shadow", "arcane", "frost"]},
     "void-scholar in night-black robes with silver star-constellation trim, obsidian orb staff, hollow-eyed focus",
     "Rend black with deep-violet nebula swirls and cold bone-white star-points",
     "seven-point star with a hollow center",
     "the gap between stars is also a star",
     "what is not is the frame of what is",
     [("void_mage_hollow", "Hollow-Mage", "magic_dps", "shadow-annihilation offense", ["shadow", "arcane"]),
      ("void_mage_ice", "Void-Ice Mage", "control", "frost-void terrain control", ["frost", "shadow"])]),

    # ========== Archetype 5 — Spellblooded ==========
    (5, "faction_a", "firestep_sorcerer", "Firestep Sorcerer",
     {"arcana": ["fire", "arcane"], "finesse": ["acrobat", "dagger"]},
     "dancing fire-sorcerer in fitted silk with ember-tattooed arms, daggers that leave burning afterimages, barefoot poised stance",
     "Concord palette ignited with ember-orange and flash-white",
     "dagger trailed by a firestep arc",
     "the flame moves because I move",
     "I was born already burning",
     [("firestep_ember", "Ember-Dancer", "magic_dps", "fire-weave offense with dagger combo", ["fire", "dagger"]),
      ("firestep_flash", "Flash-Step", "ranged_dps", "ember-teleport and burst fire", ["fire", "acrobat"])]),

    (5, "faction_a", "arcane_duellist", "Arcane Duellist",
     {"arcana": ["arcane"], "finesse": ["dagger", "silent", "acrobat"]},
     "silvery-robed rune-duellist with silent-stitched leathers, twin rune-etched daggers, cantrip-glow around the fingertips",
     "Concord silver highlighted with arcane-violet rune-glow",
     "twin daggers crossed within a rune-ring",
     "the rune is the first strike",
     "duel-magic is math answered in motion",
     [("arcane_duel_runeblade", "Rune-Duellist", "melee_dps", "rune-augmented dagger strikes", ["arcane", "dagger"]),
      ("arcane_duel_silent", "Silent-Scholar", "stealth", "cantrip-cover infiltration", ["arcane", "silent"])]),

    (5, "faction_b", "bloodborn", "Bloodborn",
     {"arcana": ["blood", "fire"], "finesse": ["dagger", "acrobat"]},
     "scar-armed sorcerer in oxblood leather, twin curved daggers trailing blood-wisps, ember-tattooed throat",
     "Rend red deepened with oxblood and ember-orange tattoo glow",
     "dagger dripping a single fire-spark",
     "the blood I bleed is the spell I cast",
     "born of blood, fueled by blood",
     [("bloodborn_lash", "Blood-Lash", "melee_dps", "self-wounding striker with fire bursts", ["blood", "dagger"]),
      ("bloodborn_kindle", "Kindle", "magic_dps", "ember-and-blood ranged caster", ["fire", "blood"])]),

    (5, "faction_b", "shadow_spark", "Shadow-Spark",
     {"arcana": ["shadow", "fire"], "finesse": ["dagger", "poison"]},
     "dusk-cloaked sorcerer in fitted shadow-silk, ember-and-shadow flickering at the fingertips, poisoned fang-blades",
     "Rend violet-and-black with shadow-ember orange accents",
     "ember held by a shadow-hand",
     "the dark is where the spark begins",
     "shadow cups the flame; the flame answers",
     [("shadow_spark_flare", "Shadow-Flare", "magic_dps", "burst shadow-fire damage", ["shadow", "fire"]),
      ("shadow_spark_hex", "Hex-Spark", "control", "toxin-shadow battlefield hex", ["poison", "shadow"])]),

    # ========== Archetype 6 — Pactblade ==========
    (6, "faction_a", "seal_blade", "Seal-Blade",
     {"arcana": ["light", "devotion"], "finesse": ["dagger", "tonics"]},
     "celestial-bound duellist in white-and-silver leather, sealed-pact medallion at chest, twin light-tipped daggers, lanterned-face mask",
     "Concord silver-and-gold softened with radiant-pale and moonlight accents",
     "open-scroll flanked by twin daggers",
     "my witness is named; my blade is witnessed",
     "the pact is public or it is false",
     [("seal_blade_witness", "Witness-Blade", "melee_dps", "light-augmented dagger strikes with celestial pact", ["light", "dagger"]),
      ("seal_blade_warden", "Seal-Warden", "support", "ward-ally protection via tonic-and-light", ["devotion", "tonics"])]),

    (6, "faction_a", "oath_thief", "Oath-Thief",
     {"arcana": ["arcane"], "finesse": ["silent", "trickster", "dagger"]},
     "cloaked rune-thief in silent-stitched dark leathers, cantrip-shimmer at fingertips, pilfered rune-blade, sleight-of-hand poise",
     "Concord palette darkened with dusk-indigo and silent-charcoal",
     "dagger drawing a shadow-rune",
     "my promise is always to someone else",
     "a stolen oath is an oath kept",
     [("oath_thief_shadow", "Shadow-Thief", "stealth", "rune-silent infiltration", ["arcane", "silent"]),
      ("oath_thief_feint", "Feint-Caster", "control", "cantrip-trickster disruption", ["trickster", "arcane"])]),

    (6, "faction_b", "pact_reaper", "Pact-Reaper",
     {"arcana": ["shadow"], "finesse": ["dagger", "poison", "silent"]},
     "scythe-blade pact-bound reaper in layered grave-silk, poison-dagger pair, soot-stained face, pact-sigil glowing at chest",
     "Rend black deepened with bruise-purple shadow-glow and venom-green accents",
     "scythe-blade paired with a dripping fang",
     "the reaping is the bargain",
     "every life reaped is a debt paid",
     [("pact_reaper_sickle", "Sickle-Reaper", "melee_dps", "shadow-blade melee striker", ["shadow", "dagger"]),
      ("pact_reaper_fang", "Fang-Reaper", "control", "poison-and-silent battlefield attrition", ["poison", "silent"])]),

    (6, "faction_b", "blood_duelist", "Blood-Duelist",
     {"arcana": ["blood"], "finesse": ["dagger", "silent"]},
     "self-wounding pact-duelist in red-sashed dark leathers, curved ritual dagger dripping, bloodrune-tattooed arms, stoic face",
     "Rend oxblood with ember-red sigil-glow and bone-pale scar-marks",
     "cross-scar over a bleeding hand",
     "my blade is paid forward",
     "the pact is the scar",
     [("blood_duel_ripple", "Ripple-Blood", "melee_dps", "drain-attack offense", ["blood", "dagger"]),
      ("blood_duel_silent", "Silent-Blood", "stealth", "quiet-blood infiltration-killer", ["blood", "silent"])]),

    # ========== Archetype 7 — Loresinger ==========
    (7, "faction_a", "bladesinger", "Bladesinger",
     {"arcana": ["arcane"], "finesse": ["dagger", "acrobat", "bow"]},
     "elven-style fencer-bard in flowing silk-and-leather, rune-etched slender rapier, spell-glow at fingertips mid-verse",
     "Concord silver-and-blue with arcane-violet sword-glow",
     "slender rapier wreathed in notes",
     "the song is the dodge",
     "poetry is the shortest parry",
     [("bladesinger_verse", "Verse-Blade", "melee_dps", "cantrip-augmented rapier dance", ["arcane", "dagger"]),
      ("bladesinger_grace", "Grace-Dancer", "bruiser", "acrobat-rapier-arcane balanced combatant", ["acrobat", "arcane"])]),

    (7, "faction_a", "arrow_bard", "Arrow-Bard",
     {"arcana": ["arcane", "nature"], "finesse": ["bow", "acrobat"]},
     "forest-traveler bard in earth-tone leather and cloak, rune-etched longbow, lute across the back, wildflower in the hat-band",
     "Concord palette softened to forest-green and bow-wood brown",
     "arrow wrapped in leaves",
     "the arrow carries the verse",
     "every shot tells its own tale",
     [("arrow_bard_chord", "Chord-Archer", "ranged_dps", "rune-augmented longbow damage", ["arcane", "bow"]),
      ("arrow_bard_wood", "Wood-Herald", "support", "nature-and-arcane ally buff", ["nature", "arcane"])]),

    (7, "faction_b", "bone_piper", "Bone-Piper",
     {"arcana": ["shadow"], "finesse": ["bow", "trickster", "dagger"]},
     "funeral-bard in grave-silk and bone-beads, skeletal-bone flute at the belt, dark-etched longbow, soot face-paint",
     "Rend black with grave-silk cream and bone-white trophy accents",
     "bone flute crossed with a notched bow",
     "the song the dead ask for",
     "mourning is a contract I sign nightly",
     [("bone_piper_dirge", "Dirge-Piper", "support", "funeral-bard ally curse-lifter", ["shadow", "trickster"]),
      ("bone_piper_hunt", "Hunt-Piper", "ranged_dps", "bone-bow shadow archer", ["shadow", "bow"])]),

    (7, "faction_b", "rot_bard", "Rot-Bard",
     {"arcana": ["blood"], "finesse": ["dagger", "poison", "silent"]},
     "decadent court-fool in blood-red velvet with rot-stitched trim, poison-tipped dagger, ivory mask, grave-scented perfume",
     "Rend oxblood with rot-green and decay-yellow trim",
     "ivory mask smeared with blood",
     "the punchline is fatal",
     "what entertains can still kill",
     [("rot_bard_jest", "Jester-of-Rot", "control", "toxin-and-trickery debilitator", ["poison", "blood"]),
      ("rot_bard_silk", "Silk-Dagger", "melee_dps", "silent-dagger court-killer", ["dagger", "silent"])]),

    # ========== Archetype 8 — Shadower ==========
    (8, "faction_a", "acrobat_thief", "Acrobat-Thief",
     {"finesse": ["acrobat", "thrown", "trickster"]},
     "circus-trained thief in tight silks, tumbling pose mid-flip, bandolier of thrown daggers, mischievous grin",
     "Concord palette brightened with harlequin red-and-cream diamond accents",
     "crossed thrown-daggers over a tumbling figure",
     "I was never there; look twice",
     "balance is theft's longest game",
     [("acrobat_thief_flip", "Flip-Thief", "utility", "acrobatic infiltration-burglar", ["acrobat", "trickster"]),
      ("acrobat_thief_throw", "Throw-Artist", "ranged_dps", "thrown-dagger circus killer", ["thrown", "acrobat"])]),

    (8, "faction_a", "silent_stalker", "Silent-Stalker",
     {"finesse": ["silent", "bow", "tonics"]},
     "forest-silhouetted stalker in dark-green oiled leather, blackened longbow, herbal satchel, hooded eyes",
     "Concord palette darkened with forest-shade and moss-cloak accents",
     "bow drawn in silhouette against the moon",
     "the quiet has already marked you",
     "the arrow and the breath are one",
     [("silent_stalker_archer", "Shade-Archer", "ranged_dps", "silent-bow single-target", ["silent", "bow"]),
      ("silent_stalker_herbalist", "Forest-Herbalist", "utility", "scout-and-herb blend", ["tonics", "silent"])]),

    (8, "faction_b", "poisoner", "Poisoner",
     {"finesse": ["poison", "thrown", "silent"]},
     "hooded poisoner with venom-pocket belt, coated thrown-blades, chemical-stained fingertips, flat dead expression",
     "Rend black with venom-green and sickly-yellow accents",
     "dripping vial over crossed thrown-blades",
     "the dose knows better than the blade",
     "waiting is half the craft",
     [("poisoner_vial", "Vial-Poisoner", "control", "toxin-AoE battlefield debilitator", ["poison", "thrown"]),
      ("poisoner_kiss", "Silent-Kiss", "melee_dps", "coated-blade single-target killer", ["poison", "silent"])]),

    (8, "faction_b", "shadowstep", "Shadowstep",
     {"finesse": ["silent", "dagger", "trickster"]},
     "dark-leather shadow-walker with twin curved daggers, shadow-step afterimages, ash-grey face-wrap, ember-eye glint",
     "Rend void-black with ash-grey silhouette and faint ember-red eye accent",
     "two crossed daggers silhouetted by a half-moon",
     "between my steps is where I live",
     "the door is wherever I decide",
     [("shadowstep_twin", "Twin-Blade", "melee_dps", "dagger-duel shadow-step striker", ["dagger", "silent"]),
      ("shadowstep_ghost", "Ghost-Walker", "stealth", "pure infiltrator with misdirection", ["silent", "trickster"])]),

    # ========== Archetype 9 — Marcher ==========
    (9, "faction_a", "beastmaster", "Beastmaster Ranger",
     {"might": ["spear"], "finesse": ["bow", "dagger", "tonics"]},
     "forest-ranger with hound at heel, longbow across back, spear in hand, fur-trimmed traveling cloak, weather-worn face",
     "Concord palette warmed with earth-brown and forest-green, pelt-tan trim",
     "bow crossed with a spear over a paw-print",
     "the hunt teaches the hunter",
     "the pack keeps the border",
     [("beastmaster_pack", "Pack-Ranger", "ranged_dps", "companion-assisted hunter", ["bow", "spear"]),
      ("beastmaster_tracker", "Tracker", "utility", "survival-and-herb frontier scout", ["tonics", "dagger"])]),

    (9, "faction_a", "spear_scout", "Spear-Scout",
     {"might": ["spear"], "finesse": ["thrown", "silent", "bow"]},
     "light-armored spear-scout in forester leather, bandolier of javelins, short spear in hand, weather-cloak",
     "Concord palette darkened with scout-umber and silent-black accents",
     "spear crossed with a javelin over a watcher-eye",
     "the point goes first; I follow",
     "reconnaissance is its own battle",
     [("spear_scout_thrust", "Thrust-Scout", "melee_dps", "close-range spear-fighter", ["spear", "silent"]),
      ("spear_scout_hurl", "Hurl-Scout", "ranged_dps", "javelin-and-bow mobile harasser", ["thrown", "bow"])]),

    (9, "faction_b", "blood_raider", "Blood-Raider",
     {"might": ["spear", "fury"], "finesse": ["bow", "thrown"]},
     "raider-tracker in rust-iron leather, blood-sigil branded on cheek, javelins strapped to back, dark-etched longbow",
     "Rend iron-black with blood-red sigil-brand accents",
     "spear crossed with a javelin over a dripping brand",
     "the raid is the prayer",
     "what I track, I take",
     [("blood_raid_hunter", "Blood-Hunter", "ranged_dps", "longbow blood-hunter", ["bow", "spear"]),
      ("blood_raid_javelin", "Javelin-Raider", "bruiser", "thrown-javelin frontline skirmisher", ["thrown", "fury"])]),

    (9, "faction_b", "pact_tracker", "Pact-Tracker",
     {"might": ["spear"], "finesse": ["bow", "poison", "silent"]},
     "pact-bound hunter in dusky leather, crow-feather cloak, venom-tipped arrows, soot face-paint, grim silence",
     "Rend palette with crow-black, venom-green, and dusk-indigo accents",
     "arrow dipped in venom with a crow-feather fletching",
     "the mark was already mine",
     "the target does not know yet; that is the point",
     [("pact_tracker_arrow", "Venom-Arrow", "ranged_dps", "poison-bow mark-killer", ["poison", "bow"]),
      ("pact_tracker_ghost", "Pact-Ghost", "stealth", "silent-tracker infiltrator", ["silent", "spear"])]),

    # ========== Archetype 10 — Duellist ==========
    (10, "faction_a", "blade_duellist", "Blade-Duellist",
     {"might": ["blade"], "finesse": ["dagger", "acrobat", "tonics"]},
     "rapier-and-main-gauche duelist in courtly black-and-silver, mid-lunge stance, feathered hat cocked, dueling ribbon at wrist",
     "Concord palette intensified with court-black and silver-lace trim",
     "rapier crossed with a main-gauche under a ribbon-knot",
     "the touch; the point; the bow",
     "dueling is discipline given manners",
     [("blade_duel_rapier", "Rapier-Duellist", "melee_dps", "precise-thrust single-target", ["blade", "dagger"]),
      ("blade_duel_flourish", "Flourish-Duellist", "bruiser", "feint-and-acrobat harasser", ["acrobat", "blade"])]),

    (10, "faction_a", "fist_discipline", "Fist-Discipline Monk",
     {"might": ["unarmed", "blunt", "honor"], "finesse": ["tonics"]},
     "bare-knuckle monk in plain temple robes with linen hand-wraps, quarterstaff across back, prayer-beads at waist",
     "Concord palette muted to dawn-cream and monastery-wheat",
     "open-fist over prayer-beads",
     "the hand is the hammer; the breath is the forge",
     "discipline shaped becomes body",
     [("fist_disc_iron", "Iron-Fist", "melee_dps", "rapid-combo unarmed striker", ["unarmed", "blunt"]),
      ("fist_disc_breath", "Breath-Keeper", "sustain", "meditative self-heal with honor-stance", ["honor", "tonics"])]),

    (10, "faction_b", "dark_duellist", "Dark-Duellist",
     {"might": ["blade"], "finesse": ["dagger", "poison", "silent"]},
     "pact-dark duelist in matte-black leathers, coated twin daggers, shadow-etched rapier, silent predator stance",
     "Rend void-black with venom-green and crimson duel-trim",
     "twin curved daggers behind a rapier over a single drop of venom",
     "the touch is already lethal",
     "the duel ends at the first breath lost",
     [("dark_duel_venom", "Venom-Duelist", "melee_dps", "coated-blade assassin", ["poison", "blade"]),
      ("dark_duel_silent", "Silent-Duelist", "stealth", "ambush-duel hybrid", ["silent", "dagger"])]),

    (10, "faction_b", "fury_fist", "Fury-Fist",
     {"might": ["unarmed", "fury"], "finesse": ["poison", "dagger"]},
     "raging pact-fighter with tattoo-scarred bare chest, knuckle-bound fists, twin venom-daggers on hip, smoldering-eyed glare",
     "Rend red-and-black with ember-orange tattoo-glow",
     "clenched scarred fist inside a circle of venom",
     "my wound is my weapon",
     "every strike is a breath kept",
     [("fury_fist_rage", "Rage-Fist", "melee_dps", "rage-forward unarmed striker", ["unarmed", "fury"]),
      ("fury_fist_blade", "Fang-Fist", "bruiser", "knuckle-and-venom combat", ["poison", "dagger"])]),

    # ========== Archetype 11 — Raider ==========
    (11, "faction_a", "axe_champion", "Axe-Champion",
     {"might": ["blade", "blunt", "spear"]},
     "heavy-axe champion in layered mail and beard-braids with iron beads, massive two-handed greataxe, fur-trimmed mantle",
     "Concord palette deepened with iron-black and mantle-fur tan",
     "crossed greataxe and spear over a fur-edged shield",
     "the champion answers the challenge",
     "the axe ends the argument",
     [("axe_champ_great", "Greataxe-Champion", "melee_dps", "two-handed axe striker", ["blade", "blunt"]),
      ("axe_champ_spear", "Champion's Spear", "bruiser", "axe-and-spear mounted fighter", ["spear", "blade"])]),

    (11, "faction_a", "warhammer_guard", "Warhammer-Guard",
     {"might": ["blunt", "shield", "honor"]},
     "heavy-mailed sentinel with massive warhammer and heraldic tower shield, full-plate legs, crowned-helm with feather-plume",
     "Concord silver-and-blue intensified with plume-crimson and polished-steel",
     "warhammer crossed with a tower shield under a crown",
     "the hammer strikes last; the shield holds first",
     "the line that does not break is built by discipline",
     [("warhammer_guard_hold", "Shield-Hold", "tank", "shield-wall defender with hammer counter-strikes", ["shield", "blunt"]),
      ("warhammer_guard_crush", "Crusher", "bruiser", "two-handed hammer bruiser", ["blunt", "honor"])]),

    (11, "faction_b", "blood_ravager", "Blood-Ravager",
     {"might": ["fury", "blade", "spear"]},
     "blood-soaked raider with tattoo-scarred torso, twin hand-axes and a short-spear, trophy-bone necklace, ember-eye glare",
     "Rend oxblood with iron-black and trophy-bone white",
     "twin axes crossed over a broken spear",
     "my reach is always first",
     "what fears me feeds me",
     [("blood_ravager_axe", "Axe-Ravager", "melee_dps", "two-weapon rage-fighter", ["blade", "fury"]),
      ("blood_ravager_thrust", "Thrust-Ravager", "bruiser", "spear-forward momentum raider", ["spear", "fury"])]),

    (11, "faction_b", "skull_breaker", "Skull-Breaker",
     {"might": ["blunt", "fury", "shield"]},
     "heavy raider with flanged mace-maul and round shield, trophy-skulls hanging from the belt, iron-grey braids, stained teeth",
     "Rend iron-black with bone-white trophy-accents and oxblood face-paint",
     "flanged maul crossed with a trophy-skull",
     "the skull cracks before the argument",
     "trophies are receipts",
     [("skull_break_crush", "Crush-Breaker", "melee_dps", "single-target maul-striker", ["blunt", "fury"]),
      ("skull_break_hold", "Wall-Breaker", "bruiser", "shield-and-mace frontline", ["shield", "blunt"])]),

    # ========== Archetype 12 — Runemartial ==========
    (12, "faction_a", "lightning_blade", "Lightning-Blade",
     {"might": ["blade"], "arcana": ["lightning", "arcane"], "finesse": ["acrobat"]},
     "spellblade in rune-stitched coat and brigandine, longsword crackling with blue lightning along the fuller, arc-light on the gauntlets",
     "Concord silver-and-blue charged with arc-white and electric-violet rune-glow",
     "longsword forked by a lightning bolt",
     "the rune strikes before the blade",
     "speed is the scholar's blade",
     [("lightning_blade_strike", "Bolt-Blade", "melee_dps", "offensive lightning-augmented strikes", ["blade", "lightning"]),
      ("lightning_blade_dance", "Arc-Dancer", "bruiser", "acrobat-and-arcane duelist", ["acrobat", "arcane"])]),

    (12, "faction_a", "frost_knight", "Frost-Knight",
     {"might": ["blade", "shield"], "arcana": ["frost"]},
     "rime-coated spellblade in silver-and-white plate with ice-sheen, longsword crystallized at the edge, frost-ward shield",
     "Concord silver deepened with ice-blue, frost-white highlights on blade-edge",
     "longsword crossed with a shield rimed in frost",
     "the cold keeps the oath",
     "silence is winter's argument",
     [("frost_knight_ward", "Rime-Ward", "tank", "shield-ward with frost-counter", ["shield", "frost"]),
      ("frost_knight_edge", "Frost-Edge", "melee_dps", "cold-sword offensive spellblade", ["blade", "frost"])]),

    (12, "faction_b", "shadow_duskblade", "Shadow-Duskblade",
     {"might": ["blade", "fury"], "arcana": ["shadow", "arcane"]},
     "rune-etched spellblade in blackened leather-and-brigandine, shadow-pooled longsword dripping violet glow, hood thrown back",
     "Rend void-black with shadow-violet blade-glow and oxblood-rune accents",
     "blade dissolving at the tip into shadow",
     "the dark holds the edge",
     "my blade's shadow is a second blade",
     [("shadow_dusk_edge", "Shadow-Edge", "melee_dps", "shadow-augmented fury strikes", ["blade", "shadow"]),
      ("shadow_dusk_rune", "Rune-Dusk", "control", "arcane-shadow glyph disruptor", ["arcane", "shadow"])]),

    (12, "faction_b", "blood_runed", "Blood-Runed",
     {"might": ["blade", "shield"], "arcana": ["blood", "arcane"]},
     "blood-runed spellblade in oxblood-enamel plate, runes bleeding across the breastplate, sword-hilt dripping faint red-mist",
     "Rend oxblood-red with rune-bleed crimson and dark-iron plate",
     "shield emblazoned with a bleeding rune",
     "every rune I carry cost me",
     "scholarship is paid in pints",
     [("blood_runed_edge", "Bleed-Edge", "melee_dps", "self-wound rune-augmented sword", ["blade", "blood"]),
      ("blood_runed_ward", "Bleed-Ward", "tank", "shield-tank with blood-ritual reactive", ["shield", "arcane"])]),

    # ========== Archetype 13 — Spirit-adept ==========
    (13, "faction_a", "star_mystic", "Star-Mystic",
     {"might": ["spear"], "arcana": ["lightning", "arcane"], "finesse": ["dagger"]},
     "star-charting mystic in silver-embroidered midnight-blue robes, constellation-staff, small silver-etched dagger at belt",
     "Concord silver-and-blue deepened with midnight-violet and star-silver glint",
     "seven-point star framed by a spear",
     "the stars' ledger records what the realm forgets",
     "the stars speak slowly; listen long",
     [("star_mystic_oracle", "Star-Oracle", "support", "forecast-buff diviner", ["arcane", "lightning"]),
      ("star_mystic_blade", "Star-Blade", "hybrid", "spear-and-arcane mystic-combatant", ["spear", "lightning"])]),

    (13, "faction_a", "sun_seer", "Sun-Seer",
     {"might": ["unarmed"], "arcana": ["light", "devotion"], "finesse": ["tonics"]},
     "sun-mystic in cream-and-gold robes with solar-ray embroidery, sunburst-topped staff, herbal satchel at hip, calm expression",
     "Concord gold-and-white warmed with dawn-peach and healing-green",
     "rayed sun over open hands",
     "dawn speaks; I translate",
     "sight is the oldest service",
     [("sun_seer_dawn", "Dawn-Seer", "healer", "radiant-tonic healer", ["light", "tonics"]),
      ("sun_seer_witness", "Witness-Oracle", "support", "devotion-divination support", ["devotion", "light"])]),

    (13, "faction_b", "grave_seer", "Grave-Seer",
     {"might": ["spear"], "arcana": ["blood", "shadow"], "finesse": ["silent"]},
     "grave-whispering mystic in grave-silk and binding-wrap, bone-topped staff, ember-eye mask-accent, silent stance",
     "Rend black and grave-silk cream with ember-red mask accents",
     "skull-topped staff crossed by a spear",
     "the grave confides; I listen long",
     "the dead speak plainest",
     [("grave_seer_binder", "Grave-Binder", "control", "ancestor-hex crowd-controller", ["blood", "shadow"]),
      ("grave_seer_whisper", "Grave-Whisperer", "support", "silent-divination ally-buffer", ["silent", "shadow"])]),

    (13, "faction_b", "wraith_mystic", "Wraith-Mystic",
     {"might": ["spear"], "arcana": ["shadow", "arcane"], "finesse": ["dagger", "silent"]},
     "wraith-haunted mystic in dissolving shadow-robe, dusk-rune staff, twin shadow-daggers, ember-eye glow beneath hood",
     "Rend void-black with shadow-violet dissolution and ember-pupil glow",
     "dissolving silhouette with a staff at center",
     "my shadow is my second witness",
     "what is partly absent strikes fully",
     [("wraith_mystic_fade", "Wraith-Adept", "magic_dps", "shadow-rune offensive caster", ["shadow", "arcane"]),
      ("wraith_mystic_blade", "Wraith-Blade", "stealth", "silent-dagger spectral striker", ["dagger", "silent"])]),

    # ========== Archetype 14 — Pathwarden ==========
    (14, "faction_a", "thornblade", "Thornblade",
     {"might": ["blade"], "arcana": ["nature"], "finesse": ["dagger", "silent"]},
     "nature-bound blade-warden in dappled-green leather with vine-stitched trim, thorn-etched shortsword, bare-rooted dagger",
     "Concord palette greened with thorn-sap and forest-shadow",
     "twin daggers wrapped in thorned vine",
     "the grove's edge is my edge",
     "the thorn is the blade's teacher",
     [("thornblade_strike", "Vine-Strike", "melee_dps", "nature-augmented dual-blade striker", ["blade", "nature"]),
      ("thornblade_silent", "Silent-Thorn", "stealth", "forest-infiltrator with entangle", ["silent", "nature"])]),

    (14, "faction_a", "windwarden", "Windwarden",
     {"might": ["spear"], "arcana": ["lightning"], "finesse": ["bow", "silent"]},
     "wind-bound warden in storm-grey cloak, arc-charged spear, longbow across back, wind-tousled stance",
     "Concord palette charged with storm-grey and arc-white highlights",
     "forked lightning crossing a drawn bow",
     "the wind brings the word",
     "watch the sky; act with it",
     [("windwarden_bolt", "Bolt-Warden", "ranged_dps", "lightning-augmented bow-sniper", ["lightning", "bow"]),
      ("windwarden_silent", "Wind-Silent", "utility", "frontier scout with spear", ["silent", "spear"])]),

    (14, "faction_b", "thorn_warden", "Thorn-Warden",
     {"might": ["blade"], "arcana": ["nature", "shadow"], "finesse": ["dagger", "poison"]},
     "dark-wood warden in thorn-stitched black leather, poison-tipped thorn-blades, crow-feather mantle, shadow-haunted gaze",
     "Rend black-and-red cooled with thorn-sap green and venom-drip accents",
     "crossed thorn-daggers inside a crow-silhouette",
     "the forest answered me back",
     "the grove keeps its own law",
     [("thorn_warden_venom", "Venom-Thorn", "melee_dps", "poison-thorn dagger striker", ["poison", "nature"]),
      ("thorn_warden_shade", "Shade-Thorn", "control", "shadow-and-nature hex-binder", ["shadow", "nature"])]),

    (14, "faction_b", "shade_scout", "Shade-Scout",
     {"might": ["spear"], "arcana": ["shadow"], "finesse": ["silent", "bow", "trickster"]},
     "dusk-cloaked border-scout in ash-grey leather with clan-banner tatters, dark-etched longbow, short spear, shadow-wrapped hood",
     "Rend ash-grey with shadow-violet accents and clan-red banner-tatter flashes",
     "drawn bow with a shadow-arrow in flight",
     "the border answers me before it asks",
     "the watcher becomes the ghost",
     [("shade_scout_arrow", "Shadow-Arrow", "ranged_dps", "silent-bow shadow-shooter", ["bow", "shadow"]),
      ("shade_scout_ghost", "Shadow-Trick", "stealth", "misdirection-forward infiltrator", ["silent", "trickster"])]),
]


def write_order(arch_id: int, faction: str, id_stem: str, class_name: str,
                schools: dict, pitch: str, palette: str, motif: str,
                oath: str, doctrine: str, specs_list: list):
    arch_dir = ARCH_DIRS[arch_id]
    m, a, f = POSITIONS[arch_id]
    order_dir = ARCH_DIR / arch_dir / "orders" / f"order_{id_stem}"

    core = {
        "id": f"order_{id_stem}",
        "faction": faction,
        "archetype_id": arch_id,
        "archetype_position": {"might": m, "arcana": a, "finesse": f},
        "flagship": False,
        "player_facing": {
            "class_name": class_name,
            "title_singular": class_name,
            "title_plural": class_name + "s",
        },
        "schools_taught": schools,
    }
    dump(order_dir / "core.yaml", core)

    palette_fallback = "Concord" if faction == "faction_a" else "Rend"
    dump(order_dir / "aesthetic.yaml", {
        "pitch": pitch,
        "palette_shift": palette,
        "motif": motif,
    })
    dump(order_dir / "lore.yaml", {
        "founded": f"from the {palette_fallback}-side branch of this archetype's tradition",
        "home": "varies by chapter",
        "oath": oath,
        "doctrine": doctrine,
    })
    specs_yaml = [
        {"id": s[0], "name": s[1], "emphasis": s[2],
         "description": s[3], "schools_focus": s[4]}
        for s in specs_list
    ]
    dump(order_dir / "specs.yaml", {"specs": specs_yaml})


def main():
    for entry in ORDERS:
        write_order(*entry)
    print(f"wrote {len(ORDERS)} orders across archetypes 03..14")


if __name__ == "__main__":
    main()
