#!/usr/bin/env python3
"""One-shot refactor:
  1. Add crossbow school (new finesse YAML).
  2. Rename forge_chapel → knight_order; add alchemists_compact; rewrite all
     15 institution cores with {tradition, major/secondary curriculum}.
  3. Create chapter stubs at institutions/<primary>/chapters/<chapter_id>/.

Idempotent-ish: safe to re-run (overwrites institution cores, re-creates
chapter stubs). Forge-chapel rename only runs if forge_chapel/ still exists.

Run from repo root:
    python scripts/refactor_institutions.py
"""
from __future__ import annotations

import shutil
from pathlib import Path

import yaml

REPO = Path(__file__).resolve().parents[1]
GENERATED = REPO / "src" / "generated"
INST_DIR = GENERATED / "institutions"
SCHOOLS_DIR = GENERATED / "schools"
ARCH_DIR = GENERATED / "archetypes"


def dump(p: Path, obj):
    p.parent.mkdir(parents=True, exist_ok=True)
    with open(p, "w") as f:
        yaml.safe_dump(obj, f, sort_keys=False, default_flow_style=False, width=120)


def load(p: Path):
    with open(p) as f:
        return yaml.safe_load(f)


# ============================================================================
# Phase 1: crossbow school
# ============================================================================

CROSSBOW = {
    "name": "crossbow",
    "pillar": "finesse",
    "morality": "neutral",
    "family": "ranged",
    "tag": "mechanical / concealable",
    "damage_type": "piercing",
    "applies_to_categories": ["precision", "trickery", "utility"],
    "icon_style": {
        "palette": "oiled darkwood stock, blackened iron mechanism, steel bolt-head, sinew-cord",
        "motif": "tensioned horizontal arms, trigger lever, loaded bolt, small stock",
        "silhouette": "compact horizontal crossbow held level, taut arms, short bolt loaded",
        "material": "seasoned wood, tempered steel, waxed cord, leather grip-wrap",
    },
}


# ============================================================================
# Phase 2: 15 institutions (rewrite cores)
# ============================================================================

INSTITUTIONS = {
    # ---- Concord ----
    "radiant_church": {
        "faction": "faction_a", "name": "The Radiant Church",
        "tradition": "Tradition of Dawn",
        "major": {"arcana": ["light", "devotion"]},
        "secondary": {"arcana": ["arcane"], "might": ["blunt", "honor"]},
    },
    "silver_quill": {
        "faction": "faction_a", "name": "The Guild of the Silver Quill",
        "tradition": "Silver Scribe Tradition",
        "major": {"arcana": ["arcane"]},
        "secondary": {"arcana": ["lightning", "frost", "fire", "light"]},
    },
    "circle_of_roots": {
        "faction": "faction_a", "name": "The Circle of Roots",
        "tradition": "Greenkeeper Tradition",
        "major": {"arcana": ["nature"]},
        "secondary": {"arcana": ["earth"], "might": ["spear"], "finesse": ["tonics", "bow"]},
    },
    "knight_order": {
        "faction": "faction_a", "name": "The Knight-Order of the Dawn",
        "tradition": "Order of the Dawn-Sword",
        "major": {"might": ["blade", "shield"]},
        "secondary": {"might": ["honor"]},
    },
    "phalanx_school": {
        "faction": "faction_a", "name": "The Phalanx School",
        "tradition": "March Discipline",
        "major": {"might": ["spear", "shield"]},
        "secondary": {"might": ["blunt", "unarmed", "honor"], "finesse": ["bow"]},
    },
    "wayhouse_accord": {
        "faction": "faction_a", "name": "The Wayhouse Accord",
        "tradition": "Wayhouse Songcraft",
        "major": {"finesse": ["trickster"]},
        "secondary": {"finesse": ["acrobat", "bow", "silent", "crossbow"], "arcana": ["arcane"]},
    },
    "night_quick_guild": {
        "faction": "faction_a", "name": "The Night-Quick Guild",
        "tradition": "Night-Quick Arts",
        "major": {"finesse": ["silent", "dagger"]},
        "secondary": {"finesse": ["thrown", "crossbow", "tonics", "acrobat"]},
    },
    "alchemists_compact": {
        "faction": "faction_a", "name": "The Alchemists' Compact",
        "tradition": "Elixir-Craft Tradition",
        "major": {"finesse": ["alchemy"]},
        "secondary": {"finesse": ["tonics", "thrown"]},
    },
    # ---- Rend ----
    "ancestor_binder_houses": {
        "faction": "faction_b", "name": "The Ancestor-Binder Houses",
        "tradition": "Ancestor-Rite",
        "major": {"arcana": ["blood"]},
        "secondary": {"arcana": ["nature", "shadow"], "might": ["spear"], "finesse": ["silent"]},
    },
    "bleeding_vaults": {
        "faction": "faction_b", "name": "The Bleeding Vaults",
        "tradition": "Scarblood Scholarship",
        "major": {"arcana": ["shadow"]},
        "secondary": {"arcana": ["blood", "arcane", "fire", "frost"]},
    },
    "fury_houses": {
        "faction": "faction_b", "name": "The Fury-Houses",
        "tradition": "Fury-Rite",
        "major": {"might": ["fury"]},
        "secondary": {"might": ["blade", "blunt"], "arcana": ["blood"]},
    },
    "hearth_broken_clans": {
        "faction": "faction_b", "name": "The Hearth-Broken Clans",
        "tradition": "Raider's Way",
        "major": {"might": ["blade", "shield"]},
        "secondary": {"might": ["spear", "blunt", "fury", "unarmed"], "finesse": ["bow"]},
    },
    "shadow_conclave": {
        "faction": "faction_b", "name": "The Shadow-Conclave",
        "tradition": "Pact-Witness Tradition",
        "major": {"finesse": ["dagger", "silent"]},
        "secondary": {"arcana": ["shadow"], "finesse": ["poison", "trickster", "crossbow"]},
    },
    "storm_pact": {
        "faction": "faction_b", "name": "The Storm-Pact",
        "tradition": "Storm-Binding",
        "major": {"arcana": ["lightning"]},
        "secondary": {"arcana": ["nature", "earth", "frost"], "finesse": ["bow"]},
    },
    "alchemist_tinker_clans": {
        "faction": "faction_b", "name": "The Alchemist-Tinker Clans",
        "tradition": "Scrap-Craft",
        "major": {"finesse": ["alchemy"]},
        "secondary": {"finesse": ["poison", "thrown", "acrobat", "crossbow"]},
    },
}


# Extra lore/aesthetic for freshly-renamed or new institutions (existing ones
# keep their old lore/aesthetic.yaml untouched).
NEW_LORE = {
    "knight_order": {
        "description": "Concord's formal chivalric order — elite knights who swear oaths at the Radiant Church but train at the Knight-Order. Produces paladins (combined with the church) and pure knights/blademasters (alone).",
        "founded": "the Third Concord — when the Knight-Order was chartered as distinct from the militia",
        "home": "the Keep of the Dawn-Sword, Concord's capital",
        "doctrine": "the blade serves the oath; the oath serves the realm",
        "patron": "the Dawn-Sword — a blade said to have been the first Concord oath-weapon",
        "recruitment": "earn a squire's spurs in service to a sworn knight, then take the Dawn-Oath",
    },
    "knight_order_aesthetic": {
        "pitch": "training-ground courtyard, racks of consecrated longswords, heraldic shields lining the walls, knights in plate drilling under the dawn",
        "palette": "polished silver, royal blue, heraldic gold, clean leather, dawn-cream",
        "motif": "upraised longsword crossed with a kite shield under a rising-sun crown",
    },
    "alchemists_compact": {
        "description": "Concord's sanctioned guild of alchemists, herbalists, and tinctern-crafters. Small but highly respected; license required, rigorous apprenticeship.",
        "founded": "the Compact of Vials — the first Concord alchemy guild charter",
        "home": "the Guild House — a compound of workshops and reading rooms in the capital",
        "doctrine": "measure twice; mix once; label always",
        "patron": "the Three Measures — a mnemonic about ratios that stood in for a patron",
        "recruitment": "apprenticed young; a Compact member must produce an original tincture for certification",
    },
    "alchemists_compact_aesthetic": {
        "pitch": "alchemy workshop with labeled bottles, copper distillation apparatus, bound ledgers on a reading desk, apothecary-style shelves",
        "palette": "amber glass, copper, cream linen, muted green, ink-black",
        "motif": "corked flask crossed with a measuring rod",
    },
}


def write_school_crossbow():
    path = SCHOOLS_DIR / "finesse" / "crossbow.yaml"
    dump(path, CROSSBOW)
    print(f"wrote {path.relative_to(REPO)}")


def rename_forge_to_knight():
    src = INST_DIR / "forge_chapel"
    dst = INST_DIR / "knight_order"
    if src.exists() and not dst.exists():
        src.rename(dst)
        print(f"renamed forge_chapel → knight_order")


def rewrite_institution_cores():
    INST_DIR.mkdir(parents=True, exist_ok=True)
    for iid, data in INSTITUTIONS.items():
        entity_dir = INST_DIR / iid
        entity_dir.mkdir(parents=True, exist_ok=True)
        core = {
            "id": iid,
            "faction": data["faction"],
            "name": data["name"],
            "tradition": data["tradition"],
            "curriculum": {
                "major": data["major"],
                "secondary": data["secondary"],
            },
            "pillars": sorted({p for group in ("major", "secondary") for p in data[group].keys()}),
        }
        dump(entity_dir / "core.yaml", core)

        # Write fresh lore/aesthetic only for renamed/new institutions.
        if iid in ("knight_order", "alchemists_compact"):
            if not (entity_dir / "lore.yaml").exists() or iid in ("knight_order", "alchemists_compact"):
                dump(entity_dir / "lore.yaml", NEW_LORE[iid])
            aesthetic_key = f"{iid}_aesthetic"
            if aesthetic_key in NEW_LORE:
                dump(entity_dir / "aesthetic.yaml", NEW_LORE[aesthetic_key])
    print(f"rewrote {len(INSTITUTIONS)} institution cores")


# ============================================================================
# Phase 3: Chapter stubs — one per Order, placed under its primary institution.
# Also writes `chapter: <chapter_id>` back-ref into each Order's core.yaml.
# ============================================================================


def iter_order_cores():
    for arch in sorted(ARCH_DIR.iterdir()):
        if not arch.is_dir() or arch.name.startswith("_"):
            continue
        odir = arch / "orders"
        if not odir.exists():
            continue
        for entity in sorted(odir.iterdir()):
            if not entity.is_dir():
                continue
            core = entity / "core.yaml"
            if core.exists():
                yield core


def create_chapter_stubs():
    created = 0
    for core_path in iter_order_cores():
        core = load(core_path)
        order_id = core["id"]
        insts = core.get("institutions", {}) or {}
        primary = insts.get("primary")
        if not primary:
            print(f"warning: {order_id} has no primary institution; skipping chapter stub")
            continue

        # Handle renamed forge_chapel → knight_order in existing order data.
        if primary == "forge_chapel":
            primary = "knight_order"
            insts["primary"] = "knight_order"
            core["institutions"] = insts

        chapter_id = f"chapter_{order_id.removeprefix('order_')}"
        chapter_dir = INST_DIR / primary / "chapters" / chapter_id
        chapter_dir.mkdir(parents=True, exist_ok=True)

        chapter_core = {
            "id": chapter_id,
            "institution": primary,
            "produces_order": order_id,
            "secondary_institution": insts.get("secondary"),
            "tertiary_institution": insts.get("tertiary"),
        }
        dump(chapter_dir / "core.yaml", chapter_core)

        # Back-ref on the order
        core["chapter"] = chapter_id
        dump(core_path, core)
        created += 1
    print(f"created {created} chapter stubs + back-refs")


# Also rewrite any institutions references within orders that used forge_chapel.
def patch_forge_refs_in_orders():
    patched = 0
    for core_path in iter_order_cores():
        core = load(core_path)
        insts = core.get("institutions") or {}
        changed = False
        for slot in ("primary", "secondary", "tertiary"):
            if insts.get(slot) == "forge_chapel":
                insts[slot] = "knight_order"
                changed = True
        if changed:
            core["institutions"] = insts
            dump(core_path, core)
            patched += 1
    print(f"patched forge_chapel→knight_order refs in {patched} orders")


def main():
    write_school_crossbow()
    rename_forge_to_knight()
    rewrite_institution_cores()
    patch_forge_refs_in_orders()
    create_chapter_stubs()


if __name__ == "__main__":
    main()
