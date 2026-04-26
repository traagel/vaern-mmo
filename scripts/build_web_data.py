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
MESHY_DIR = REPO / "assets" / "meshy"


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


def overlay_prose_zone(d: dict, hubs: list[dict], landmarks: list[dict],
                       prose_path: Path) -> None:
    """If prose.yaml exists next to a zone's core.yaml, overlay its
    description / prompt / vibe onto the zone, hubs and landmarks. Existing
    fields on the entity (e.g. Dalewatch's in-core descriptions) win — the
    overlay only fills gaps."""
    if not prose_path.exists():
        return
    p = load_yaml(prose_path) or {}
    z_prose = p.get("zone") or {}
    for k in ("description", "prompt", "negative_prompt", "vibe"):
        if z_prose.get(k) and not d.get(k):
            d[k] = z_prose[k]
    hub_prose = p.get("hubs") or {}
    for hd in hubs:
        hp = hub_prose.get(hd.get("id"))
        if not hp:
            continue
        for k in ("description", "prompt", "negative_prompt"):
            if hp.get(k) and not hd.get(k):
                hd[k] = hp[k]
    lm_prose = p.get("landmarks") or {}
    for lm in landmarks:
        lp = lm_prose.get(lm.get("id"))
        if not lp:
            continue
        for k in ("description", "prompt", "negative_prompt"):
            if lp.get(k) and not lm.get(k):
                lm[k] = lp[k]


def overlay_prose_dungeon(d: dict, bosses: list[dict], prose_path: Path) -> None:
    if not prose_path.exists():
        return
    p = load_yaml(prose_path) or {}
    d_prose = p.get("dungeon") or {}
    for k in ("description", "prompt", "negative_prompt"):
        if d_prose.get(k) and not d.get(k):
            d[k] = d_prose[k]
    boss_prose = p.get("bosses") or {}
    for b in bosses:
        bp = boss_prose.get(b.get("id"))
        if not bp:
            continue
        for k in ("description", "prompt", "negative_prompt"):
            if bp.get(k) and not b.get(k):
                b[k] = bp[k]


def meshy_images_for(slug: str) -> list[str]:
    """Return repo-relative paths to image_*.png|jpg|jpeg|webp for a slug,
    sorted by numeric index. Empty if the dir doesn't exist."""
    d = MESHY_DIR / slug
    if not d.exists():
        return []
    candidates: list[Path] = []
    for ext in (".png", ".jpg", ".jpeg", ".webp"):
        candidates.extend(d.glob(f"image_*{ext}"))
    def idx(p: Path) -> int:
        try:
            return int(p.stem.split("_", 1)[1])
        except (IndexError, ValueError):
            return 9999
    candidates.sort(key=idx)
    return [str(p.relative_to(REPO)) for p in candidates]


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
    """Race portraits prefer Meshy (assets/meshy/race__<id>__<gender>/image_1.*)
    over the legacy SDXL portraits in characters/. The `portraits` map now
    holds repo-relative paths (or None) instead of booleans — app.js reads
    paths directly."""
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

        def pick(gender_key: str, legacy_stem: str) -> str | None:
            meshy = meshy_images_for(f"race__{rid}__{gender_key}")
            if meshy:
                return meshy[0]
            if legacy_stem in portraits:
                return f"characters/{legacy_stem}.png"
            return None

        d["portraits"] = {
            "male":    pick("male",    f"{rid}.male"),
            "female":  pick("female",  f"{rid}.female"),
            "neutral": pick("neutral", rid),
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


def build_world() -> dict | None:
    p = GENERATED / "world" / "world.yaml"
    if not p.exists():
        return None
    d = load_yaml(p) or {}
    d["_file"] = str(p.relative_to(REPO))
    return d


def build_continents() -> list[dict]:
    out: list[dict] = []
    cdir = GENERATED / "world" / "continents"
    if not cdir.exists():
        return out
    for p in sorted(cdir.glob("*.yaml")):
        if p.stem.startswith("_"):
            continue
        d = load_yaml(p)
        if d is None:
            continue
        d["_file"] = str(p.relative_to(REPO))
        out.append(d)
    return out


def build_biomes() -> list[dict]:
    out: list[dict] = []
    bdir = GENERATED / "world" / "biomes"
    if not bdir.exists():
        return out
    for p in sorted(bdir.glob("*.yaml")):
        if p.stem.startswith("_"):
            continue
        d = load_yaml(p)
        if d is None:
            continue
        d["_file"] = str(p.relative_to(REPO))
        d["images"] = meshy_images_for(f"biome__{d.get('id', p.stem)}")
        out.append(d)
    return out


def build_zones() -> list[dict]:
    """Each zone dir: core.yaml + hubs/*.yaml + landmarks.yaml. Returns
    flattened cards with hubs nested as a list."""
    out: list[dict] = []
    zdir = GENERATED / "world" / "zones"
    if not zdir.exists():
        return out
    for entity in sorted(zdir.iterdir()):
        if not entity.is_dir() or entity.name.startswith("_"):
            continue
        core = entity / "core.yaml"
        if not core.exists():
            continue
        d = load_yaml(core) or {}
        d["_file"] = str(core.relative_to(REPO))
        zone_id = d.get("id", entity.name)
        # zone-level images live under assets/meshy/<zone_id>__zone/
        d["images"] = meshy_images_for(f"{zone_id}__zone")
        # hubs
        hubs: list[dict] = []
        hubs_dir = entity / "hubs"
        if hubs_dir.exists():
            for hp in sorted(hubs_dir.glob("*.yaml")):
                hd = load_yaml(hp) or {}
                # strip the giant props list — keep counts only
                if "props" in hd:
                    hd["prop_count"] = len(hd["props"])
                    del hd["props"]
                hub_id = hd.get("id", hp.stem)
                hd["images"] = meshy_images_for(f"{zone_id}__{hub_id}")
                hubs.append(hd)
        d["hubs"] = hubs
        # landmarks
        landmarks_path = entity / "landmarks.yaml"
        if landmarks_path.exists():
            ld = load_yaml(landmarks_path) or {}
            lms = ld.get("landmarks", []) or []
            for lm in lms:
                lm_id = lm.get("id", "")
                lm["images"] = meshy_images_for(f"{zone_id}__{lm_id}")
            d["landmarks"] = lms
        else:
            d["landmarks"] = []
        # strip the verbose scatter rules — keep a category count for the card
        if "scatter" in d and isinstance(d["scatter"], list):
            d["scatter_categories"] = sorted({s.get("category") for s in d["scatter"] if s.get("category")})
            del d["scatter"]
        # overlay prose.yaml (description/prompt/vibe) — fills gaps without
        # clobbering anything in core.yaml
        overlay_prose_zone(d, hubs, d["landmarks"], entity / "prose.yaml")
        out.append(d)
    return out


def build_dungeons() -> list[dict]:
    out: list[dict] = []
    ddir = GENERATED / "world" / "dungeons"
    if not ddir.exists():
        return out
    for entity in sorted(ddir.iterdir()):
        if not entity.is_dir() or entity.name.startswith("_"):
            continue
        core = entity / "core.yaml"
        if not core.exists():
            continue
        d = load_yaml(core) or {}
        d["_file"] = str(core.relative_to(REPO))
        d["images"] = meshy_images_for(f"dungeon__{d.get('id', entity.name)}")
        bosses_path = entity / "bosses.yaml"
        if bosses_path.exists():
            bd = load_yaml(bosses_path) or {}
            bosses = bd.get("bosses", []) or []
            for b in bosses:
                b["images"] = meshy_images_for(f"boss__{b.get('id', '')}")
            d["bosses"] = bosses
        else:
            d["bosses"] = []
        loot_path = entity / "loot.yaml"
        if loot_path.exists():
            d["loot"] = load_yaml(loot_path) or {}
        # overlay prose.yaml (dungeon + boss description/prompt)
        overlay_prose_dungeon(d, d["bosses"], entity / "prose.yaml")
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
        "world": build_world(),
        "continents": build_continents(),
        "biomes": build_biomes(),
        "zones": build_zones(),
        "dungeons": build_dungeons(),
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
    print(f"  continents: {len(data['continents'])}")
    print(f"  biomes: {len(data['biomes'])}")
    print(f"  zones: {len(data['zones'])}")
    print(f"  dungeons: {len(data['dungeons'])}")


if __name__ == "__main__":
    main()
