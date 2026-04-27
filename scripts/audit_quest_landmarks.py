#!/usr/bin/env -S uv run --quiet --with pyyaml --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyyaml"]
# ///
"""Audit each zone's quests against its landmarks. Reports:

  - **Explicit refs**: quest objectives with `location: <id>` —
    flags any that don't resolve to a real landmark.
  - **Implicit refs**: quest objectives that have only a free-text
    `target_hint` (no `location`) — these often imply an unauthored
    place. Reported with the hint text and the quest path.
  - **Mob refs**: kill objectives with a specific `mob_id` are
    reported alongside the mob's `biome_context` (a string hint at
    where the mob is found).

Usage:

    ./scripts/audit_quest_landmarks.py                 # all zones
    ./scripts/audit_quest_landmarks.py dalewatch_marches  # one zone
"""

from __future__ import annotations

import pathlib
import re
import sys

import yaml

ROOT = pathlib.Path(__file__).resolve().parent.parent
ZONES_ROOT = ROOT / "src" / "generated" / "world" / "zones"


def load_yaml(p: pathlib.Path) -> dict:
    if not p.exists():
        return {}
    return yaml.safe_load(p.read_text()) or {}


def load_zone_landmarks(zone_dir: pathlib.Path) -> set[str]:
    lm = load_yaml(zone_dir / "landmarks.yaml")
    return {l["id"] for l in lm.get("landmarks", [])}


def load_zone_hubs(zone_dir: pathlib.Path) -> set[str]:
    out = set()
    hubs_dir = zone_dir / "hubs"
    if hubs_dir.is_dir():
        for hp in hubs_dir.glob("*.yaml"):
            data = load_yaml(hp)
            if "id" in data:
                out.add(data["id"])
    return out


def load_zone_mobs(zone_dir: pathlib.Path) -> dict[str, dict]:
    """Returns {mob_id: full_mob_data}. Skips files starting with `_`."""
    out: dict[str, dict] = {}
    mobs_dir = zone_dir / "mobs"
    if mobs_dir.is_dir():
        for mp in mobs_dir.glob("*.yaml"):
            if mp.name.startswith("_"):
                continue
            data = load_yaml(mp)
            if "id" in data:
                out[data["id"]] = data
    return out


def iter_quest_files(zone_dir: pathlib.Path):
    """Yields (rel_path, quest_doc) for every quest file under quests/."""
    qdir = zone_dir / "quests"
    if not qdir.is_dir():
        return
    for sub in ("chains", "side"):
        d = qdir / sub
        if d.is_dir():
            for p in sorted(d.glob("*.yaml")):
                yield (p.relative_to(ROOT), load_yaml(p))
    # filler.yaml is a single file at the quests root
    fp = qdir / "filler.yaml"
    if fp.exists():
        yield (fp.relative_to(ROOT), load_yaml(fp))
    # _summary.yaml at quests root — informational, also include
    sp = qdir / "_summary.yaml"
    if sp.exists():
        yield (sp.relative_to(ROOT), load_yaml(sp))


def iter_step_objectives(doc: dict):
    """Yields (where, objective_dict) for every objective in the doc.
    `where` is a short human-readable path like 'step 3' or 'side[0]'."""
    # Chains: top-level "steps"
    if "steps" in doc:
        for step in doc.get("steps", []) or []:
            obj = step.get("objective")
            if obj:
                yield (f"step {step.get('step', '?')} ({step.get('name', '')})", obj)
    # Side quest bundles: "quests" with each having "objective"
    if "quests" in doc:
        for q in doc.get("quests", []) or []:
            obj = q.get("objective")
            if obj:
                yield (f"side:{q.get('id', '?')}", obj)
    # Filler: "buckets" with "dominant_objective_kind" — informational
    if "buckets" in doc:
        for b in doc.get("buckets", []) or []:
            yield (
                f"filler:{b.get('id', '?')}",
                {
                    "kind": b.get("dominant_objective_kind", "?"),
                    "target_hint": b.get("description", ""),
                    "_filler": True,
                    "pool_size": b.get("pool_size", 0),
                },
            )


def audit_zone(zone_dir: pathlib.Path) -> dict:
    """Returns a structured audit report for one zone."""
    zone_id = zone_dir.name
    landmarks = load_zone_landmarks(zone_dir)
    hubs = load_zone_hubs(zone_dir)
    mobs = load_zone_mobs(zone_dir)

    explicit_ok: list[tuple[str, str, str]] = []        # (path, where, location_id)
    explicit_missing: list[tuple[str, str, str]] = []   # (path, where, location_id)
    implicit_hints: list[tuple[str, str, str]] = []     # (path, where, target_hint)
    mob_kills: dict[str, list[tuple[str, str]]] = {}    # mob_id -> [(path, where), ...]
    filler_hints: list[tuple[str, str, str, int]] = []  # (path, kind, hint, pool_size)

    for rel_path, doc in iter_quest_files(zone_dir):
        for where, obj in iter_step_objectives(doc):
            kind = obj.get("kind", "?")
            location = obj.get("location")
            target_hint = obj.get("target_hint", "")
            mob_id = obj.get("mob_id")

            if obj.get("_filler"):
                filler_hints.append((str(rel_path), kind, target_hint, obj.get("pool_size", 0)))
                continue

            if location:
                if location in landmarks or location in hubs:
                    explicit_ok.append((str(rel_path), where, location))
                else:
                    explicit_missing.append((str(rel_path), where, location))

            if mob_id:
                mob_kills.setdefault(mob_id, []).append((str(rel_path), where))

            # Flag as implicit if there's a target_hint but no
            # location reference (and it's not a pure NPC talk step).
            if target_hint and not location and kind in ("kill", "collect", "investigate", "explore"):
                implicit_hints.append((str(rel_path), where, target_hint))

    return {
        "zone": zone_id,
        "landmarks": sorted(landmarks),
        "hubs": sorted(hubs),
        "explicit_ok": explicit_ok,
        "explicit_missing": explicit_missing,
        "implicit_hints": implicit_hints,
        "mob_kills": mob_kills,
        "mobs": mobs,
        "filler_hints": filler_hints,
    }


def print_report(rep: dict, *, verbose: bool = True) -> None:
    z = rep["zone"]
    print(f"\n=== {z} ===")
    print(f"  landmarks ({len(rep['landmarks'])}): {', '.join(rep['landmarks']) or '(none)'}")
    print(f"  hubs ({len(rep['hubs'])}): {', '.join(rep['hubs']) or '(none)'}")

    if rep["explicit_ok"]:
        print(f"\n  ✓ explicit refs covered ({len(rep['explicit_ok'])}):")
        for path, where, loc in rep["explicit_ok"]:
            if verbose:
                print(f"    {loc:30s} ← {path} {where}")

    if rep["explicit_missing"]:
        print(f"\n  ✗ explicit refs MISSING ({len(rep['explicit_missing'])}):")
        for path, where, loc in rep["explicit_missing"]:
            print(f"    {loc:30s} ← {path} {where}")

    if rep["implicit_hints"]:
        print(f"\n  ⚠ implicit hints (no location: …) ({len(rep['implicit_hints'])}):")
        for path, where, hint in rep["implicit_hints"]:
            print(f"    {where}")
            print(f"      hint: {hint!r}")
            print(f"      file: {path}")

    if rep["mob_kills"]:
        print(f"\n  mob_id kill targets ({len(rep['mob_kills'])}):")
        for mob_id, refs in sorted(rep["mob_kills"].items()):
            mob = rep["mobs"].get(mob_id, {})
            ctx = mob.get("biome_context", "")
            print(f"    {mob_id}")
            print(f"      ctx: {ctx!r}")
            if verbose:
                for path, where in refs:
                    print(f"      ← {path} {where}")

    if rep["filler_hints"]:
        print(f"\n  filler buckets ({len(rep['filler_hints'])}):")
        for path, kind, hint, pool in rep["filler_hints"]:
            print(f"    [{kind}] pool={pool} :: {hint}")


def main() -> int:
    args = sys.argv[1:]
    if args:
        zones = [ZONES_ROOT / a for a in args]
    else:
        zones = sorted(p for p in ZONES_ROOT.iterdir() if p.is_dir())

    grand_implicit = 0
    for zd in zones:
        if not zd.is_dir():
            print(f"skip: {zd.name} (not a directory)", file=sys.stderr)
            continue
        rep = audit_zone(zd)
        if rep["explicit_missing"] or rep["implicit_hints"] or rep["mob_kills"]:
            print_report(rep, verbose=(len(zones) <= 3))
            grand_implicit += len(rep["implicit_hints"])

    print(f"\n=== TOTAL ===  implicit hints across audited zones: {grand_implicit}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
