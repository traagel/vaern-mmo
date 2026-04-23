#!/usr/bin/env python3
"""Generate a portrait for each Order in src/generated/orders/.

Prompt composition: race.pitch (canonical race per faction, bumped if the
default can't reach the Order's archetype) + order.aesthetic.pitch + gender.

Output: characters/<order_id>.<gender>.png

Usage:
    python scripts/generate_order_portraits.py                # all 30 orders, both genders
    python scripts/generate_order_portraits.py --only order_dawn
    python scripts/generate_order_portraits.py --gender male
"""
from __future__ import annotations

import argparse
import base64
import sys
import time
from pathlib import Path

import requests
import yaml

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO / "scripts"))

from generate_character import API, CHAR_DIR, STYLE, NEGATIVE_BASE, load_entity  # noqa: E402

RACES_DIR = REPO / "src" / "generated" / "races"
ARCHETYPES_DIR = REPO / "src" / "generated" / "archetypes"

# Preferred race per faction; falls back in order if primary can't reach the
# archetype. Mannin reaches everything; for Rend we need multiple options
# because Skarn can't reach the caster/stealth corners.
RACE_PREFERENCE = {
    "faction_a": ["mannin"],
    "faction_b": ["skarn", "kharun", "darkling_elen", "skrel"],
}


def load(p: Path):
    with open(p) as f:
        return yaml.safe_load(f)


def reachable(affinity: dict, position: dict) -> bool:
    return (affinity["might"] >= position["might"]
            and affinity["arcana"] >= position["arcana"]
            and affinity["finesse"] >= position["finesse"])


def load_races() -> dict:
    races = {}
    for entity in sorted(RACES_DIR.iterdir()):
        if not entity.is_dir() or entity.name.startswith("_"):
            continue
        d = load_entity(entity)
        if d:
            races[d["id"]] = d
    return races


def load_orders() -> list:
    orders = []
    for arch in sorted(ARCHETYPES_DIR.iterdir()):
        if not arch.is_dir() or arch.name.startswith("_"):
            continue
        odir = arch / "orders"
        if not odir.exists():
            continue
        for entity in sorted(odir.iterdir()):
            if not entity.is_dir():
                continue
            d = load_entity(entity)
            if d:
                orders.append(d)
    return orders


def pick_race(order: dict, races: dict) -> dict:
    faction = order["faction"]
    position = order["archetype_position"]
    for rid in RACE_PREFERENCE.get(faction, []):
        r = races.get(rid)
        if r and reachable(r["affinity"], position):
            return r
    # fallback: any same-faction race that reaches it
    for r in races.values():
        if r["faction"] == faction and reachable(r["affinity"], position):
            return r
    raise SystemExit(f"no reachable race for order {order['id']}")


def build_prompt(race: dict, order: dict, gender: str) -> tuple[str, str]:
    race_pitch = race["visual"]["pitch"]
    order_pitch = order["aesthetic"]["pitch"]
    subject = f"{gender} {race_pitch}, wearing {order_pitch}"
    prompt = f"{STYLE}, {subject}"
    return prompt, NEGATIVE_BASE


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--only", help="order id prefix filter")
    p.add_argument("--gender", choices=["male", "female", "both"], default="both")
    p.add_argument("--overwrite", action="store_true")
    p.add_argument("--steps", type=int, default=30)
    p.add_argument("--cfg", type=float, default=7.5)
    p.add_argument("--sampler", default="dpmpp_2m")
    p.add_argument("--seed", type=int, default=-1)
    p.add_argument("--width", type=int, default=768)
    p.add_argument("--height", type=int, default=1024)
    p.add_argument("--api", default=API)
    args = p.parse_args()

    races = load_races()
    orders = load_orders()
    if args.only:
        orders = [o for o in orders if o["id"].startswith(args.only)]

    genders = ["male", "female"] if args.gender == "both" else [args.gender]
    CHAR_DIR.mkdir(exist_ok=True)

    jobs = [(o, g) for o in orders for g in genders]
    todo: list[tuple[dict, str]] = []
    skipped = 0
    for o, g in jobs:
        out = CHAR_DIR / f"{o['id']}.{g}.png"
        if out.exists() and not args.overwrite:
            skipped += 1
            continue
        todo.append((o, g))

    print(f"plan: generate={len(todo)} skip={skipped} (of {len(jobs)} total)")
    if not todo:
        return

    t0 = time.time()
    for i, (o, g) in enumerate(todo, 1):
        race = pick_race(o, races)
        prompt, negative = build_prompt(race, o, g)
        out = CHAR_DIR / f"{o['id']}.{g}.png"
        body = {
            "prompt": prompt,
            "negative_prompt": negative,
            "steps": args.steps,
            "width": args.width,
            "height": args.height,
            "cfg_scale": args.cfg,
            "sampler_name": args.sampler,
            "seed": args.seed,
        }
        t_start = time.time()
        try:
            r = requests.post(args.api, json=body, timeout=600)
            r.raise_for_status()
            b64 = r.json()["images"][0]
            if "," in b64:
                b64 = b64.split(",", 1)[1]
            out.write_bytes(base64.b64decode(b64))
            dt = time.time() - t_start
            size = out.stat().st_size
            elapsed = time.time() - t0
            eta = (elapsed / i) * (len(todo) - i)
            print(f"[{i}/{len(todo)}] {o['id']} ({g}, {race['id']}) {dt:.1f}s {size // 1024}KB ETA {eta/60:.1f}m")
        except Exception as e:
            print(f"[{i}/{len(todo)}] {o['id']} FAILED: {e}")


if __name__ == "__main__":
    main()
