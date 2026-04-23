#!/usr/bin/env python3
"""Trim every Order's schools_taught to at most MAX_SCHOOLS.

Priority (kept first):
  1. Schools in the Order's primary institution's curriculum
  2. Schools in the secondary institution's curriculum
  3. Schools in the tertiary institution's curriculum
  4. Remaining schools (dropped first if over limit)

Preserves pillar structure: the kept set is re-grouped by pillar so that
schools_taught stays a { pillar: [...] } dict.

Run after assign_order_institutions.py.
"""
from __future__ import annotations

from pathlib import Path

import yaml

REPO = Path(__file__).resolve().parents[1]
GENERATED = REPO / "src" / "generated"
INST_DIR = GENERATED / "institutions"
ARCH_DIR = GENERATED / "archetypes"

MAX_SCHOOLS = 6


def load(p: Path):
    with open(p) as f:
        return yaml.safe_load(f)


def dump(p: Path, obj):
    with open(p, "w") as f:
        yaml.safe_dump(obj, f, sort_keys=False, default_flow_style=False, width=120)


def load_institutions() -> dict:
    out: dict[str, dict] = {}
    for d in sorted(INST_DIR.iterdir()):
        if not d.is_dir() or d.name.startswith("_"):
            continue
        core = load(d / "core.yaml")
        if core:
            out[core["id"]] = core
    return out


def trim_one(core_path: Path, insts: dict) -> tuple[int, int, list[str]]:
    core = load(core_path)
    st = core.get("schools_taught", {}) or {}

    # Flatten with pillar tags so we can regroup
    flat: list[tuple[str, str]] = []
    for pillar, schools in st.items():
        for s in (schools or []):
            flat.append((pillar, s))

    before = len(flat)
    if before <= MAX_SCHOOLS:
        return before, before, []

    # priority: 0 = primary, 1 = secondary, 2 = tertiary, 3 = not in any
    inst_ids = core.get("institutions", {}) or {}
    prim = inst_ids.get("primary")
    sec = inst_ids.get("secondary")
    tert = inst_ids.get("tertiary")

    def priority(school: str) -> int:
        for rank, iid in enumerate([prim, sec, tert]):
            if iid and school in _curriculum_flat(insts.get(iid, {})):
                return rank
        return 3

    # Stable sort by priority (lowest kept); ties preserve insertion order
    ranked = sorted(
        enumerate(flat), key=lambda ix: (priority(ix[1][1]), ix[0])
    )
    kept = sorted(ranked[:MAX_SCHOOLS], key=lambda ix: ix[0])
    dropped = [s for _, (_, s) in ranked[MAX_SCHOOLS:]]

    # Regroup kept by pillar, preserving within-pillar order
    new_st: dict[str, list[str]] = {}
    for _, (pillar, s) in kept:
        new_st.setdefault(pillar, []).append(s)

    core["schools_taught"] = new_st
    dump(core_path, core)
    return before, len(kept), sorted(dropped)


def _curriculum_flat(inst_core: dict) -> set[str]:
    out: set[str] = set()
    for _p, schools in (inst_core.get("curriculum") or {}).items():
        out.update(schools or [])
    return out


def main():
    insts = load_institutions()
    print(f"loaded {len(insts)} institutions; trimming to <= {MAX_SCHOOLS} schools/order")
    trimmed_report: list[tuple[str, int, int, list[str]]] = []
    for arch in sorted(ARCH_DIR.iterdir()):
        if not arch.is_dir() or arch.name.startswith("_"):
            continue
        odir = arch / "orders"
        if not odir.exists():
            continue
        for entity in sorted(odir.iterdir()):
            if not entity.is_dir():
                continue
            core_path = entity / "core.yaml"
            if not core_path.exists():
                continue
            before, after, dropped = trim_one(core_path, insts)
            if dropped:
                trimmed_report.append((entity.name, before, after, dropped))
    print(f"trimmed {len(trimmed_report)} orders")
    for oid, b, a, d in trimmed_report:
        print(f"  {oid}: {b}→{a}, dropped: {d}")


if __name__ == "__main__":
    main()
