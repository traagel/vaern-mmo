#!/usr/bin/env python3
"""One-shot migration: for every Order, assign primary / secondary / tertiary
institutions by computing school-overlap against each faction's institutions.

Writes an `institutions:` block into each Order's core.yaml. Idempotent —
re-running overwrites prior assignments.

Run from repo root:
    python scripts/assign_order_institutions.py
"""
from __future__ import annotations

import sys
from pathlib import Path

import yaml

REPO = Path(__file__).resolve().parents[1]
GENERATED = REPO / "src" / "generated"
INST_DIR = GENERATED / "institutions"
ARCH_DIR = GENERATED / "archetypes"


def load(p: Path):
    with open(p) as f:
        return yaml.safe_load(f)


def dump(p: Path, obj):
    with open(p, "w") as f:
        yaml.safe_dump(obj, f, sort_keys=False, default_flow_style=False, width=120)


def flatten_pillar_dict(d: dict) -> set[str]:
    """Flatten {pillar: [schools]} → set of schools."""
    out: set[str] = set()
    for _pillar, schools in (d or {}).items():
        out.update(schools or [])
    return out


def flatten_schools_taught(st: dict) -> set[str]:
    return flatten_pillar_dict(st)


def load_institutions() -> dict:
    """Return {faction: {id: {major: set, secondary: set, all: set, ...}}}."""
    by_faction: dict[str, dict] = {"faction_a": {}, "faction_b": {}}
    for d in sorted(INST_DIR.iterdir()):
        if not d.is_dir() or d.name.startswith("_"):
            continue
        core = load(d / "core.yaml")
        if not core:
            continue
        fid = core["faction"]
        iid = core["id"]
        curriculum = core.get("curriculum", {}) or {}
        major = flatten_pillar_dict(curriculum.get("major", {}))
        secondary = flatten_pillar_dict(curriculum.get("secondary", {}))
        by_faction[fid][iid] = {
            "major": major,
            "secondary": secondary,
            "all": major | secondary,
        }
    return by_faction


def iter_order_dirs():
    for arch in sorted(ARCH_DIR.iterdir()):
        if not arch.is_dir() or arch.name.startswith("_"):
            continue
        odir = arch / "orders"
        if not odir.exists():
            continue
        for entity in sorted(odir.iterdir()):
            if entity.is_dir():
                yield entity


def assign_for_order(order_core_path: Path, insts_by_faction: dict) -> tuple[list, list]:
    """Identity-first + coverage assignment.

    Primary: the institution whose MAJOR schools have the most overlap with
             the order's schools_taught (the order "belongs to" this institution
             as its home / identity).  Tiebreak on total overlap, then specialist
             (smaller curriculum).

    Secondary/tertiary: chosen to maximize remaining coverage. Preference goes
             to institutions adding the most new schools; tiebreak specialist."""
    from itertools import combinations

    core = load(order_core_path)
    faction = core.get("faction")
    schools = flatten_schools_taught(core.get("schools_taught", {}))
    insts = insts_by_faction.get(faction, {})

    if not schools or not insts:
        return [], sorted(schools)

    iids = list(insts.keys())

    # 1. Pick primary: best TOTAL overlap (identity = most of the order's
    #    schools come from this institution). Tiebreaks: more major overlap,
    #    then specialist (smallest curriculum).
    primary = max(
        iids,
        key=lambda iid: (
            len(schools & insts[iid]["all"]),
            len(schools & insts[iid]["major"]),
            -len(insts[iid]["all"]),
        ),
    )

    # 2. Pick best 2-combo for secondary+tertiary to maximize coverage
    #    AFTER primary is locked.
    primary_cov = schools & insts[primary]["all"]
    remaining_iids = [iid for iid in iids if iid != primary]
    if not remaining_iids:
        assignment = {"primary": primary}
        uncovered = sorted(schools - primary_cov)
        core["institutions"] = assignment
        dump(order_core_path, core)
        return [primary], uncovered

    best_pair: tuple[str, ...] | None = None
    best_cov = -1
    best_breadth = 10**9
    for k in (2, 1):
        for combo in combinations(remaining_iids, min(k, len(remaining_iids))):
            covered = set(primary_cov)
            breadth = 0
            for iid in combo:
                covered |= (schools & insts[iid]["all"])
                breadth += len(insts[iid]["all"])
            cov = len(covered)
            if cov > best_cov or (cov == best_cov and breadth < best_breadth):
                best_cov = cov
                best_breadth = breadth
                best_pair = combo
        if best_pair is not None and best_cov >= len(schools):
            break

    extras = sorted(
        best_pair or (),
        key=lambda iid: -len(schools & insts[iid]["all"]),
    )

    selected = [primary, *extras]
    slot_names = ["primary", "secondary", "tertiary"]
    assignment = {slot_names[i]: iid for i, iid in enumerate(selected)}

    covered = set(primary_cov)
    for iid in extras:
        covered |= (schools & insts[iid]["all"])
    uncovered = sorted(schools - covered)

    # Preserve chapter back-ref if present
    chapter = core.get("chapter")
    core["institutions"] = assignment
    if chapter:
        core["chapter"] = chapter
    dump(order_core_path, core)

    return selected, uncovered


def main():
    insts_by_faction = load_institutions()
    print(f"loaded institutions: a={len(insts_by_faction['faction_a'])} b={len(insts_by_faction['faction_b'])}")

    total = 0
    uncovered_report: list[tuple[str, list[str]]] = []
    for odir in iter_order_dirs():
        core_path = odir / "core.yaml"
        if not core_path.exists():
            continue
        iids, uncovered = assign_for_order(core_path, insts_by_faction)
        total += 1
        if uncovered:
            uncovered_report.append((odir.name, uncovered))

    print(f"assigned institutions to {total} orders")
    if uncovered_report:
        print(f"\norders with uncovered schools ({len(uncovered_report)}):")
        for oid, missing in uncovered_report:
            print(f"  {oid}: missing={missing}")
    else:
        print("all orders fully covered by their top 3 institutions")


if __name__ == "__main__":
    main()
