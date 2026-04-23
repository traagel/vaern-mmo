"""Affix DEFINITIONS — the "of Warding" / "Enchanted" / "of the Flamecaller"
layer on top of (base, material, quality).

Sourced four ways, all feed the same `ItemInstance.affixes` Vec:
  - Random roll on world drops (weighted pool filtered by tier)
  - Crafter-applied via reagents (random within profession skill band)
  - Boss shards (deterministic — shard_X applies affix_X)
  - Rare gathered reagents (hybrid)

Shard-only affixes set `weight: 0` (never random-rolled) and
`soulbinds: true` (applying them converts the item to BoP).

Resolver weaves them into the display name:
  `{quality} {prefix} {material} {piece} {suffix}`

So `of Warding` suffix + `Enchanted` prefix on a masterful steel longsword →
  "Masterful Enchanted Steel Longsword of Warding"

Stat deltas fold into SecondaryStats after material + quality contributions.
"""
from __future__ import annotations

from .common import OUT, resist_adds, write, zeros12


def _stat(**kwargs) -> dict:
    """Build a SecondaryStats payload with defaults for every field.
    Explicit kwargs override — e.g. `_stat(armor=5, resists=resist_adds(fire=10))`.
    """
    base = {
        "armor": 0,
        "weapon_min_dmg": 0.0,
        "weapon_max_dmg": 0.0,
        "crit_rating_pct": 0.0,
        "haste_rating_pct": 0.0,
        "fortune_pct": 0.0,
        "mp5": 0.0,
        "block_chance_pct": 0.0,
        "block_value": 0,
        "resists": zeros12(),
    }
    base.update(kwargs)
    return base


# Every affix row mirrors `vaern_items::composition::Affix`.
#
# Columns:
#   id              snake_case id referenced by ItemInstance.affixes
#   display         how it shows in the composed name (empty = silent)
#   position        "prefix" | "suffix"
#   applies_to      [weapon, armor, shield, rune, ...]
#   min_tier        earliest tier where this can random-roll
#   max_tier        latest tier
#   stat_delta      SecondaryStats payload folded on resolve
#   weight          loot-roll weight (0 = never random, only deterministic)
#   soulbinds       item becomes BoP once this affix is applied

AFFIXES = [
    # ── Universal random-rollable suffixes ─────────────────────────────
    dict(
        id="of_warding",
        display="of Warding",
        position="suffix",
        applies_to=["weapon", "armor", "shield"],
        min_tier=1, max_tier=10,
        stat_delta=_stat(
            resists=resist_adds(
                fire=1.0, cold=1.0, lightning=1.0, force=1.0,
                radiant=1.0, necrotic=1.0, blood=1.0, poison=1.0, acid=1.0,
            ),
        ),
        weight=40, soulbinds=False,
    ),
    dict(
        id="of_the_bear",
        display="of the Bear",
        position="suffix",
        applies_to=["armor", "shield"],
        min_tier=1, max_tier=10,
        stat_delta=_stat(armor=4),
        weight=35, soulbinds=False,
    ),
    dict(
        id="of_the_fox",
        display="of the Fox",
        position="suffix",
        applies_to=["armor"],
        min_tier=1, max_tier=10,
        stat_delta=_stat(crit_rating_pct=1.5),
        weight=30, soulbinds=False,
    ),
    dict(
        id="of_the_eagle",
        display="of the Eagle",
        position="suffix",
        applies_to=["weapon", "armor"],
        min_tier=2, max_tier=10,
        stat_delta=_stat(crit_rating_pct=2.5),
        weight=25, soulbinds=False,
    ),
    dict(
        id="of_the_owl",
        display="of the Owl",
        position="suffix",
        applies_to=["armor"],
        min_tier=2, max_tier=10,
        stat_delta=_stat(mp5=1.5),
        weight=25, soulbinds=False,
    ),
    dict(
        id="of_the_wolf",
        display="of the Wolf",
        position="suffix",
        applies_to=["weapon", "armor"],
        min_tier=2, max_tier=10,
        stat_delta=_stat(haste_rating_pct=1.0),
        weight=25, soulbinds=False,
    ),
    dict(
        id="of_vitality",
        display="of Vitality",
        position="suffix",
        applies_to=["armor", "shield"],
        min_tier=1, max_tier=10,
        stat_delta=_stat(armor=2),
        weight=35, soulbinds=False,
    ),
    dict(
        id="of_striking",
        display="of Striking",
        position="suffix",
        applies_to=["weapon"],
        min_tier=1, max_tier=10,
        stat_delta=_stat(weapon_min_dmg=1.0, weapon_max_dmg=2.0),
        weight=40, soulbinds=False,
    ),
    dict(
        id="of_swift_fangs",
        display="of Swift Fangs",
        position="suffix",
        applies_to=["weapon"],
        min_tier=2, max_tier=10,
        stat_delta=_stat(haste_rating_pct=2.0, crit_rating_pct=1.0),
        weight=15, soulbinds=False,
    ),
    dict(
        id="of_fortune",
        display="of Fortune",
        position="suffix",
        applies_to=["weapon", "armor", "shield"],
        min_tier=1, max_tier=10,
        stat_delta=_stat(fortune_pct=2.0),
        weight=20, soulbinds=False,
    ),
    dict(
        id="of_blood",
        display="of Blood",
        position="suffix",
        applies_to=["weapon"],
        min_tier=3, max_tier=10,
        stat_delta=_stat(weapon_min_dmg=0.5, weapon_max_dmg=1.5),
        weight=10, soulbinds=False,
    ),

    # ── Elemental banes (tier-gated, channel-specific) ─────────────────
    dict(
        id="of_flamebane",
        display="of Flamebane",
        position="suffix",
        applies_to=["armor", "shield"],
        min_tier=2, max_tier=10,
        stat_delta=_stat(resists=resist_adds(fire=8.0)),
        weight=15, soulbinds=False,
    ),
    dict(
        id="of_frostbane",
        display="of Frostbane",
        position="suffix",
        applies_to=["armor", "shield"],
        min_tier=2, max_tier=10,
        stat_delta=_stat(resists=resist_adds(cold=8.0)),
        weight=15, soulbinds=False,
    ),
    dict(
        id="of_stormbane",
        display="of Stormbane",
        position="suffix",
        applies_to=["armor", "shield"],
        min_tier=2, max_tier=10,
        stat_delta=_stat(resists=resist_adds(lightning=8.0)),
        weight=15, soulbinds=False,
    ),
    dict(
        id="of_shadowbane",
        display="of Shadowbane",
        position="suffix",
        applies_to=["armor", "shield"],
        min_tier=3, max_tier=10,
        stat_delta=_stat(resists=resist_adds(necrotic=8.0, blood=4.0)),
        weight=10, soulbinds=False,
    ),
    dict(
        id="of_lightbane",
        display="of Lightbane",
        position="suffix",
        applies_to=["armor", "shield"],
        min_tier=3, max_tier=10,
        stat_delta=_stat(resists=resist_adds(radiant=8.0)),
        weight=10, soulbinds=False,
    ),
    dict(
        id="of_venom_resist",
        display="of Venom-Resist",
        position="suffix",
        applies_to=["armor", "shield"],
        min_tier=2, max_tier=10,
        stat_delta=_stat(resists=resist_adds(poison=6.0, acid=4.0)),
        weight=15, soulbinds=False,
    ),

    # ── Prefixes (random-rollable) ─────────────────────────────────────
    dict(
        id="enchanted",
        display="Enchanted",
        position="prefix",
        applies_to=["weapon", "armor", "shield", "rune"],
        min_tier=2, max_tier=10,
        stat_delta=_stat(crit_rating_pct=1.0, haste_rating_pct=1.0),
        weight=20, soulbinds=False,
    ),
    dict(
        id="runecarved",
        display="Runecarved",
        position="prefix",
        applies_to=["weapon", "shield"],
        min_tier=3, max_tier=10,
        stat_delta=_stat(
            resists=resist_adds(
                fire=2.0, cold=2.0, lightning=2.0, force=2.0,
            ),
        ),
        weight=12, soulbinds=False,
    ),
    dict(
        id="blessed",
        display="Blessed",
        position="prefix",
        applies_to=["weapon", "armor", "shield"],
        min_tier=2, max_tier=10,
        stat_delta=_stat(resists=resist_adds(radiant=3.0, necrotic=3.0)),
        weight=15, soulbinds=False,
    ),
    dict(
        id="cursed",
        display="Cursed",
        position="prefix",
        applies_to=["weapon", "armor"],
        min_tier=3, max_tier=10,
        stat_delta=_stat(
            weapon_min_dmg=1.0, weapon_max_dmg=2.0,
            resists=resist_adds(necrotic=5.0, radiant=-3.0),
        ),
        weight=8, soulbinds=False,
    ),
    dict(
        id="sturdy",
        display="Sturdy",
        position="prefix",
        applies_to=["armor", "shield"],
        min_tier=1, max_tier=10,
        stat_delta=_stat(armor=3),
        weight=25, soulbinds=False,
    ),

    # ── Shard-only affixes (weight 0, soulbinds true) ──────────────────
    # These never roll on random drops — only applied when a player
    # consumes a matching boss shard via the crafter rite. Converts the
    # item to BoP on imprint. Design: one shard per major raid boss.
    dict(
        id="of_the_frostwarden",
        display="of the Frostwarden",
        position="suffix",
        applies_to=["weapon", "armor", "shield"],
        min_tier=4, max_tier=10,
        stat_delta=_stat(
            resists=resist_adds(cold=20.0, fire=-3.0),
            weapon_min_dmg=2.0, weapon_max_dmg=4.0,
        ),
        weight=0, soulbinds=True,
    ),
    dict(
        id="of_the_flamecaller",
        display="of the Flamecaller",
        position="suffix",
        applies_to=["weapon", "armor", "shield"],
        min_tier=4, max_tier=10,
        stat_delta=_stat(
            resists=resist_adds(fire=20.0, cold=-3.0),
            weapon_min_dmg=2.0, weapon_max_dmg=4.0,
        ),
        weight=0, soulbinds=True,
    ),
    dict(
        id="of_worldserpent",
        display="of the Worldserpent",
        position="suffix",
        applies_to=["weapon", "armor"],
        min_tier=5, max_tier=10,
        stat_delta=_stat(
            armor=10,
            resists=resist_adds(poison=15.0, acid=15.0),
            haste_rating_pct=2.0,
        ),
        weight=0, soulbinds=True,
    ),
    dict(
        id="of_the_first_ember",
        display="of the First Ember",
        position="suffix",
        applies_to=["weapon", "rune"],
        min_tier=5, max_tier=10,
        stat_delta=_stat(
            weapon_min_dmg=3.0, weapon_max_dmg=6.0,
            crit_rating_pct=3.0,
            resists=resist_adds(fire=10.0),
        ),
        weight=0, soulbinds=True,
    ),
    dict(
        id="of_dawnkeeper",
        display="of the Dawnkeeper",
        position="suffix",
        applies_to=["armor", "shield"],
        min_tier=5, max_tier=10,
        stat_delta=_stat(
            armor=15, block_value=10,
            resists=resist_adds(radiant=15.0, necrotic=-5.0),
        ),
        weight=0, soulbinds=True,
    ),
]


def seed() -> int:
    entries = []
    for a in AFFIXES:
        entries.append({
            "id": a["id"],
            "display": a["display"],
            "position": a["position"],
            "applies_to": a["applies_to"],
            "min_tier": a["min_tier"],
            "max_tier": a["max_tier"],
            "stat_delta": a["stat_delta"],
            "weight": a["weight"],
            "soulbinds": a["soulbinds"],
        })
    write(OUT / "affixes.yaml", {"affixes": entries})
    return len(entries)
