#!/usr/bin/env python3
"""Compile all src/generated/ YAML into a single web/data.json for the browser UI.

Also records which ability IDs have a PNG in icons/ and which races/classes have
a portrait in characters/.

Run with no arguments:
    python scripts/build_web_data.py
"""
from __future__ import annotations

import json
import time
from pathlib import Path

import yaml

REPO = Path(__file__).resolve().parents[1]
GENERATED = REPO / "src" / "generated"
OUT = REPO / "web" / "data.json"
ICONS_DIR = REPO / "icons"
CHAR_DIR = REPO / "characters"
EMBLEMS_DIR = REPO / "emblems"


def load_yaml(p: Path):
    with open(p) as f:
        return yaml.safe_load(f)


def load_entity(entity_dir: Path) -> dict | None:
    """Merge all YAMLs in a directory. core.yaml fields go to top level; any
    other <name>.yaml becomes merged[<name>] = contents. specs.yaml is
    unwrapped: {specs: [...]} → merged['specs'] = [...]."""
    if not entity_dir.is_dir():
        return None
    core = entity_dir / "core.yaml"
    if not core.exists():
        return None
    merged: dict = load_yaml(core) or {}
    for f in sorted(entity_dir.glob("*.yaml")):
        if f.name == "core.yaml":
            continue
        content = load_yaml(f)
        if content is None:
            continue
        if f.stem == "specs" and isinstance(content, dict) and "specs" in content:
            merged["specs"] = content["specs"]
        else:
            merged[f.stem] = content
    return merged


def existing_stems(d: Path) -> set[str]:
    if not d.exists():
        return set()
    return {p.stem for p in d.glob("*.png")}


def build_spells(icons_set: set[str]) -> list[dict]:
    out: list[dict] = []
    for pillar_dir in sorted((GENERATED / "flavored").iterdir()):
        if not pillar_dir.is_dir():
            continue
        pillar = pillar_dir.name
        for yaml_path in sorted(pillar_dir.glob("*.yaml")):
            category = yaml_path.stem
            doc = load_yaml(yaml_path)
            for tier in sorted(doc["variants"]):
                for school in sorted(doc["variants"][tier]):
                    ability = doc["variants"][tier][school]
                    aid = f"{pillar}.{category}.{tier}.{school}.{ability['name']}"
                    out.append({
                        "id": aid,
                        "pillar": pillar,
                        "category": category,
                        "tier": int(tier),
                        "school": school,
                        "name": ability["name"],
                        "display_name": ability["name"].replace("_", " "),
                        "description": ability.get("description", ""),
                        "damage_type": ability.get("damage_type"),
                        "morality": ability.get("morality"),
                        "has_icon": aid in icons_set,
                    })
    return out


def load_all_in(dirpath: Path, key_field: str = "id") -> list[dict]:
    items: list[dict] = []
    if not dirpath.exists():
        return items
    for p in sorted(dirpath.rglob("*.yaml")):
        if p.stem.startswith("_"):
            continue
        d = load_yaml(p)
        if d is None:
            continue
        d["_file"] = str(p.relative_to(REPO))
        items.append(d)
    return items


def build_schools(emblems: set[str]) -> list[dict]:
    out: list[dict] = []
    for pillar_dir in sorted((GENERATED / "schools").iterdir()):
        if not pillar_dir.is_dir():
            continue
        for p in sorted(pillar_dir.glob("*.yaml")):
            d = load_yaml(p)
            d["_file"] = str(p.relative_to(REPO))
            d["has_emblem"] = f"school_{d['name']}" in emblems
            out.append(d)
    return out


def build_categories() -> list[dict]:
    out: list[dict] = []
    for pillar_dir in sorted((GENERATED / "abilities").iterdir()):
        if not pillar_dir.is_dir():
            continue
        pillar = pillar_dir.name
        for p in sorted(pillar_dir.glob("*.yaml")):
            d = load_yaml(p)
            d["_file"] = str(p.relative_to(REPO))
            d["key"] = f"{pillar}.{p.stem}"
            out.append(d)
    return out


CLASS_REFERENCES = {
    "faction_a": "mannin",
    "faction_b": "skarn",
}


def build_classes(portraits: set[str]) -> list[dict]:
    out: list[dict] = []
    adir = GENERATED / "archetypes"
    if not adir.exists():
        return out
    for entity in sorted(adir.iterdir()):
        if not entity.is_dir() or entity.name.startswith("_"):
            continue
        d = load_entity(entity)
        if not d:
            continue
        d["_file"] = str((entity / "core.yaml").relative_to(REPO))
        cid = d["class_id"]
        refs = {}
        for faction, race in CLASS_REFERENCES.items():
            stem = f"{race}.class_{cid:02d}.{faction}"
            refs[faction] = {
                "stem": stem,
                "race": race,
                "male": f"{stem}.male" in portraits,
                "female": f"{stem}.female" in portraits,
            }
        d["references"] = refs
        out.append(d)
    return out


def build_races(portraits: set[str]) -> list[dict]:
    out: list[dict] = []
    rdir = GENERATED / "races"
    if not rdir.exists():
        return out
    for entity in sorted(rdir.iterdir()):
        if not entity.is_dir() or entity.name.startswith("_"):
            continue
        d = load_entity(entity)
        if not d:
            continue
        d["_file"] = str((entity / "core.yaml").relative_to(REPO))
        rid = d.get("id", entity.name)
        d["portraits"] = {
            "male": f"{rid}.male" in portraits,
            "female": f"{rid}.female" in portraits,
            "neutral": rid in portraits,
        }
        d["has_portrait"] = any(d["portraits"].values())
        out.append(d)
    return out


def build_combos(portraits: set[str]) -> list[dict]:
    cpath = GENERATED / "character_combos.yaml"
    if not cpath.exists():
        return []
    doc = load_yaml(cpath)
    out: list[dict] = []
    for c in doc.get("flagship", []):
        stem = f"{c['race']}.class_{c['class_id']:02d}.{c['faction']}"
        c = dict(c)
        c["portraits"] = {
            "male": f"{stem}.male" in portraits,
            "female": f"{stem}.female" in portraits,
        }
        c["_stem"] = stem
        out.append(c)
    return out


def build_orders(portraits: set[str]) -> list[dict]:
    """Walk archetypes/*/orders/*/ — orders nest under their archetype."""
    out: list[dict] = []
    adir = GENERATED / "archetypes"
    if not adir.exists():
        return out
    for arch in sorted(adir.iterdir()):
        if not arch.is_dir() or arch.name.startswith("_"):
            continue
        orders_dir = arch / "orders"
        if not orders_dir.exists():
            continue
        for entity in sorted(orders_dir.iterdir()):
            if not entity.is_dir():
                continue
            d = load_entity(entity)
            if not d:
                continue
            d["_file"] = str((entity / "core.yaml").relative_to(REPO))
            oid = d["id"]
            d["portraits"] = {
                "male": f"{oid}.male" in portraits,
                "female": f"{oid}.female" in portraits,
            }
            d.setdefault("flagship", True)
            d.setdefault("specs", [])
            out.append(d)
    return out


def build_factions(emblems: set[str]) -> list[dict]:
    out: list[dict] = []
    fdir = GENERATED / "factions"
    for p in sorted(fdir.glob("*.yaml")):
        if p.stem.startswith("_"):
            continue
        d = load_yaml(p)
        d["_file"] = str(p.relative_to(REPO))
        d["has_emblem"] = d.get("id", p.stem) in emblems
        out.append(d)
    return out


def build_institutions(emblems: set[str]) -> list[dict]:
    out: list[dict] = []
    idir = GENERATED / "institutions"
    if not idir.exists():
        return out
    for entity in sorted(idir.iterdir()):
        if not entity.is_dir() or entity.name.startswith("_"):
            continue
        d = load_entity(entity)
        if not d:
            continue
        d["_file"] = str((entity / "core.yaml").relative_to(REPO))
        d["has_emblem"] = f"institution_{d['id']}" in emblems
        # collect chapter ids (lightweight stubs under chapters/)
        chapters_dir = entity / "chapters"
        chapter_ids: list[str] = []
        order_ids: list[str] = []
        if chapters_dir.exists():
            for chap in sorted(chapters_dir.iterdir()):
                if not chap.is_dir():
                    continue
                chapter_core = chap / "core.yaml"
                if chapter_core.exists():
                    cd = load_yaml(chapter_core)
                    chapter_ids.append(cd.get("id", chap.name))
                    if cd.get("produces_order"):
                        order_ids.append(cd["produces_order"])
        d["chapters"] = chapter_ids
        d["orders_produced"] = order_ids
        out.append(d)
    return out


def main() -> None:
    icons_set = existing_stems(ICONS_DIR)
    portrait_set = existing_stems(CHAR_DIR)
    emblem_set = existing_stems(EMBLEMS_DIR)
    spells = build_spells(icons_set)
    data = {
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%S"),
        "counts": {
            "spells_total": len(spells),
            "spells_with_icon": sum(1 for s in spells if s["has_icon"]),
            "portraits": len(portrait_set),
            "emblems": len(emblem_set),
        },
        "spells": spells,
        "schools": build_schools(emblem_set),
        "categories": build_categories(),
        "classes": build_classes(portrait_set),
        "races": build_races(portrait_set),
        "factions": build_factions(emblem_set),
        "combos": build_combos(portrait_set),
        "orders": build_orders(portrait_set),
        "institutions": build_institutions(emblem_set),
    }
    OUT.parent.mkdir(exist_ok=True)
    with open(OUT, "w") as f:
        json.dump(data, f, indent=2, default=str)
    print(f"wrote {OUT}  ({OUT.stat().st_size // 1024}KB)")
    for k, v in data["counts"].items():
        print(f"  {k}: {v}")
    print(f"  schools: {len(data['schools'])}")
    print(f"  classes: {len(data['classes'])}")
    print(f"  races: {len(data['races'])}")
    print(f"  factions: {len(data['factions'])}")
    print(f"  combos: {len(data['combos'])}")
    print(f"  orders: {len(data['orders'])}")
    print(f"  institutions: {len(data['institutions'])}")


if __name__ == "__main__":
    main()
