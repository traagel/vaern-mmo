"""Consumable bases — potions, food, elixirs, scrolls.

No material axis. Quality could apply (masterful healing potion with
2x heal) but is ignored at v1 — all consumables roll at regular only.

Edit the POTION / FOOD / ELIXIR / SCROLL tables to add content.

Each consumable carries a `kind.effect` that the runtime `ConsumeEffect`
enum round-trips through. Variants: `none` / `heal_hp` / `heal_mana` /
`heal_stamina` / `buff`. See `crates/vaern-items/src/lib.rs::ConsumeEffect`.
"""
from __future__ import annotations

from .common import DAMAGE_TYPES, OUT, resist_adds, write, zeros12


# (kind_id, kind_display)
POTION_KINDS = [
    ("healing", "Healing Potion"),
    ("mana",    "Mana Potion"),
    ("stamina", "Stamina Draught"),
]

# (strength_id, strength_display, heal_amount)
# Scales roughly with pillar-derived HP max (derive_primaries uses 80
# base + 8 per Might). Minor = ~40% of a 100-HP starter, major = ~1200
# at end-game-ish pool sizes.
POTION_STRENGTHS = [
    ("minor",   "Minor",   40),
    ("lesser",  "Lesser",  80),
    ("",        "",        160),
    ("greater", "Greater", 280),
    ("major",   "Major",   450),
]

# Damage-type resist potions: 12 channels × 2 strengths.
RESIST_STRENGTHS = [("lesser", "Lesser"), ("greater", "Greater")]

FOODS = [
    # (id, display_name, charges)
    ("hardtack",         "Hardtack",          1),
    ("traveler_rations", "Traveler's Rations", 1),
    ("bread_loaf",       "Bread Loaf",         1),
    ("cheese_wheel",     "Cheese Wheel",       1),
    ("salted_jerky",     "Salted Jerky",       1),
    ("hearty_stew",      "Hearty Stew",        1),
    ("honey_pie",         "Honey Pie",         1),
    ("festival_feast",   "Festival Feast",     2),
]

ELIXIRS = [
    ("might",        "Elixir of Might"),
    ("finesse",      "Elixir of Finesse"),
    ("arcana",       "Elixir of Arcana"),
    ("warding",      "Warding Elixir"),
    ("swiftness",    "Swiftness Elixir"),
    ("fortune",      "Lucky Elixir"),
    ("giants",       "Giant's Elixir"),
    ("invisibility", "Elixir of Fading"),
]

SCROLLS = [
    ("recall",         "Scroll of Recall"),
    ("identification", "Scroll of Identification"),
    ("repair",         "Scroll of Repair"),
    ("blessing",       "Scroll of Blessing"),
]


def _potion_effect(kind_id: str, amount: int) -> dict:
    """Map a potion kind + strength to its ConsumeEffect block."""
    effect_kind = {
        "healing": "heal_hp",
        "mana":    "heal_mana",
        "stamina": "heal_stamina",
    }[kind_id]
    return {"kind": effect_kind, "amount": amount}


def _potion_base(sid: str, sname: str, amount: int, kind_id: str, kind_name: str) -> dict:
    prefix = f"{sid}_" if sid else ""
    disp_prefix = f"{sname} " if sname else ""
    return {
        "id": f"{prefix}{kind_id}_potion",
        "piece_name": f"{disp_prefix}{kind_name}".strip(),
        "size": "tiny",
        "base_weight_kg": 0.3,
        "stackable": True,
        "stack_max": 20,
        "kind": {
            "type": "consumable",
            "charges": 1,
            "effect": _potion_effect(kind_id, amount),
        },
    }


# Per-strength resist-potion tuning: (resist added on the matching
# channel, duration in seconds). At RESIST_PER_POINT = 0.005, +30
# is 15% mitigation, +60 is 30% — stacks with gear up to the 80% cap.
RESIST_POTION_TUNING = {
    "lesser":  (30.0, 180.0),
    "greater": (60.0, 180.0),
}


def _resist_potion_base(sid: str, sname: str, dt: str) -> dict:
    amount, duration = RESIST_POTION_TUNING[sid]
    return {
        "id": f"{sid}_{dt}_resist_potion",
        "piece_name": f"{sname} {dt.capitalize()} Resistance Potion",
        "size": "tiny",
        "base_weight_kg": 0.3,
        "stackable": True,
        "stack_max": 10,
        "kind": {
            "type": "consumable",
            "charges": 1,
            "effect": {
                "kind": "buff",
                "id": f"{sid}_{dt}_resist",
                "duration_secs": duration,
                "damage_mult_add": 0.0,
                "resist_adds": resist_adds(**{dt: amount}),
            },
        },
    }


# Elixir buffs. Each entry carries damage_mult_add + optional resist_adds.
# Omitted elixirs fall through to `kind: none` (no-op on consume).
ELIXIR_BUFFS = {
    # Raw power boost — 5 min, +15% damage. Core "prep before a raid"
    # hardcore-loop consumable.
    "might":   {"damage_mult_add": 0.15, "duration_secs": 300.0, "resist_adds": zeros12()},
    "arcana":  {"damage_mult_add": 0.15, "duration_secs": 300.0, "resist_adds": zeros12()},
    "finesse": {"damage_mult_add": 0.15, "duration_secs": 300.0, "resist_adds": zeros12()},
    # Giant's Elixir — bigger buff, shorter duration.
    "giants":  {"damage_mult_add": 0.25, "duration_secs": 120.0, "resist_adds": zeros12()},
    # Warding Elixir — broad resist buff, no damage component. +15 on
    # every channel for 5 min (at RESIST_PER_POINT = 0.005 = 7.5%
    # mitigation across all 12 channels). Stacks with per-channel
    # resist potions for the "prep against specific boss" axis.
    "warding": {
        "damage_mult_add": 0.0,
        "duration_secs": 300.0,
        "resist_adds": [15.0] * 12,
    },
}


def _elixir_effect(eid: str) -> dict:
    buff = ELIXIR_BUFFS.get(eid)
    if buff is None:
        return {"kind": "none"}
    return {
        "kind": "buff",
        "id": f"elixir_of_{eid}",
        "duration_secs": buff["duration_secs"],
        "damage_mult_add": buff["damage_mult_add"],
        "resist_adds": buff["resist_adds"],
    }


def seed() -> int:
    bases = []

    # Healing / mana / stamina × 5 strengths
    for kind_id, kind_name in POTION_KINDS:
        for sid, sname, amount in POTION_STRENGTHS:
            bases.append(_potion_base(sid, sname, amount, kind_id, kind_name))

    # Resist potions: 12 damage types × 2 strengths
    for dt in DAMAGE_TYPES:
        for sid, sname in RESIST_STRENGTHS:
            bases.append(_resist_potion_base(sid, sname, dt))

    # Food — small HP heal, scales with charges (feasts pack 2× 60hp
    # chews).
    for (fid, fname, charges) in FOODS:
        bases.append({
            "id": fid,
            "piece_name": fname,
            "size": "small",
            "base_weight_kg": 0.5,
            "stackable": True,
            "stack_max": 20,
            "kind": {
                "type": "consumable",
                "charges": charges,
                "effect": {"kind": "heal_hp", "amount": 30},
            },
        })

    # Elixirs — buff or inert per ELIXIR_BUFFS table.
    for (eid, ename) in ELIXIRS:
        bases.append({
            "id": f"elixir_of_{eid}",
            "piece_name": ename,
            "size": "tiny",
            "base_weight_kg": 0.4,
            "stackable": True,
            "stack_max": 10,
            "kind": {
                "type": "consumable",
                "charges": 1,
                "effect": _elixir_effect(eid),
            },
        })

    # Scrolls — special effects (recall, repair, identification, blessing)
    # don't have ConsumeEffect variants yet. Mark `none` for now.
    for (sid, sname) in SCROLLS:
        bases.append({
            "id": f"scroll_of_{sid}",
            "piece_name": sname,
            "size": "small",
            "base_weight_kg": 0.1,
            "stackable": True,
            "stack_max": 10,
            "kind": {
                "type": "consumable",
                "charges": 1,
                "effect": {"kind": "none"},
            },
        })

    write(OUT / "bases" / "consumables.yaml", {"bases": bases})
    return len(bases)
