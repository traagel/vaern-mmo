#!/usr/bin/env python3
"""One-shot migration: split generated/ YAMLs into hierarchical per-entity
subdirectories, with orders nested under their archetype.

Source (flat):
  src/generated/classes/NN_label.yaml
  src/generated/races/<id>.yaml
  src/generated/orders/order_<id>.yaml
  src/generated/orders/_specs.yaml        (central specs)
  src/generated/orders/_schema.yaml       (design docs)

Target (nested):
  src/generated/_orders_schema.yaml                           # moved up
  src/generated/archetypes/
    _roster.yaml
    NN_label/
      core.yaml                    # position, labels, roles, capabilities
      visual.yaml                  # visual block
      orders/                      # orders specializing this archetype
        order_<id>/
          core.yaml                # id, faction, archetype, player_facing, schools, flagship
          aesthetic.yaml
          lore.yaml
          specs.yaml               # extracted from central _specs.yaml
  src/generated/races/
    <id>/
      core.yaml
      visual.yaml

Old flat files are DELETED after successful split.

Run once:
    python scripts/refactor_yaml_layout.py
"""
from __future__ import annotations

import shutil
import sys
from pathlib import Path

import yaml

REPO = Path(__file__).resolve().parents[1]
GENERATED = REPO / "src" / "generated"


def load(p: Path):
    with open(p) as f:
        return yaml.safe_load(f)


def dump(p: Path, obj):
    p.parent.mkdir(parents=True, exist_ok=True)
    with open(p, "w") as f:
        yaml.safe_dump(obj, f, sort_keys=False, default_flow_style=False, width=120)


def pick(d: dict, keys: list[str]) -> dict:
    return {k: d[k] for k in keys if k in d}


def migrate_archetypes() -> tuple[int, dict]:
    """Split each class_NN yaml into archetypes/NN_label/{core,visual}.yaml.
    Returns (count, map of class_id → entity_dir name) for order-nesting.
    """
    src = GENERATED / "classes"
    dst = GENERATED / "archetypes"
    dst.mkdir(parents=True, exist_ok=True)

    # sidecars
    roster_src = src / "_roster.yaml"
    if roster_src.exists():
        shutil.copy(roster_src, dst / "_roster.yaml")

    class_dir_by_id: dict = {}
    count = 0
    for p in sorted(src.glob("*.yaml")):
        if p.stem.startswith("_"):
            continue
        d = load(p)
        stem = p.stem  # e.g. "01_paladin"
        entity_dir = dst / stem
        core_keys = [
            "class_id", "internal_label", "abstract_name",
            "position", "pillar_classification", "dominant_pillar",
            "edge", "primary_roles", "note",
            "active_tiers", "capabilities", "faction_labels",
        ]
        core = pick(d, core_keys)
        visual = d.get("visual") or {}
        dump(entity_dir / "core.yaml", core)
        if visual:
            dump(entity_dir / "visual.yaml", visual)
        class_dir_by_id[d["class_id"]] = stem
        count += 1
    shutil.rmtree(src)
    return count, class_dir_by_id


def migrate_races() -> int:
    src = GENERATED / "races"
    if not src.exists():
        return 0
    count = 0
    for p in sorted(src.glob("*.yaml")):
        if p.stem.startswith("_"):
            continue
        d = load(p)
        stem = p.stem
        entity_dir = src / stem
        core_keys = [
            "id", "archetype", "faction", "favored_class",
            "cultural_traits", "affinity",
        ]
        core = pick(d, core_keys)
        visual = d.get("visual") or {}
        dump(entity_dir / "core.yaml", core)
        if visual:
            dump(entity_dir / "visual.yaml", visual)
        p.unlink()
        count += 1
    return count


def migrate_orders(class_dir_by_id: dict) -> int:
    """Distribute orders into archetypes/<archetype_dir>/orders/<order>/..."""
    src = GENERATED / "orders"
    if not src.exists():
        return 0

    # Load central specs matrix first
    specs_doc = {}
    specs_path = src / "_specs.yaml"
    if specs_path.exists():
        specs_doc = load(specs_path) or {}
    specs_by_order: dict = specs_doc.get("specs_by_order") or {}

    # Move schema doc to generated/ root
    schema_path = src / "_schema.yaml"
    if schema_path.exists():
        shutil.copy(schema_path, GENERATED / "_orders_schema.yaml")

    count = 0
    for p in sorted(src.glob("order_*.yaml")):
        d = load(p)
        order_id = d["id"]
        archetype_id = d["archetype_id"]
        arch_dir_name = class_dir_by_id.get(archetype_id)
        if not arch_dir_name:
            print(f"warning: order {order_id} archetype_id={archetype_id} has no archetype dir", file=sys.stderr)
            continue
        entity_dir = GENERATED / "archetypes" / arch_dir_name / "orders" / order_id

        core_keys = [
            "id", "faction", "archetype_id", "archetype_position",
            "player_facing", "schools_taught", "flagship",
        ]
        core = pick(d, core_keys)
        core.setdefault("flagship", True)
        dump(entity_dir / "core.yaml", core)

        aesthetic = d.get("aesthetic") or {}
        if aesthetic:
            dump(entity_dir / "aesthetic.yaml", aesthetic)
        lore = d.get("lore") or {}
        if lore:
            dump(entity_dir / "lore.yaml", lore)

        specs = specs_by_order.get(order_id) or []
        if specs:
            dump(entity_dir / "specs.yaml", {"specs": specs})

        p.unlink()
        count += 1

    # Clean up sidecar files and empty dir
    if specs_path.exists():
        specs_path.unlink()
    if schema_path.exists():
        schema_path.unlink()
    # Remove src dir if empty
    try:
        src.rmdir()
    except OSError:
        pass  # non-empty
    return count


def main():
    if (GENERATED / "archetypes").exists():
        print("error: src/generated/archetypes/ already exists — abort", file=sys.stderr)
        sys.exit(1)

    n_arch, class_dir_by_id = migrate_archetypes()
    n_race = migrate_races()
    n_ord = migrate_orders(class_dir_by_id)
    print(f"migrated: {n_arch} archetypes, {n_race} races, {n_ord} orders")


if __name__ == "__main__":
    main()
