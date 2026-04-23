#!/usr/bin/env python3
"""Batch-generate spell icons for every flavored ability.

Walks src/generated/flavored/<pillar>/<category>.yaml, calls the local SDXL
server once per ability, writes each PNG to icons/<ability_id>.png, and appends
a CSV audit row to icons/generation_log.csv.

Examples:
    # preview
    python scripts/generate_icons_batch.py --dry-run
    # stage rollout: just arcana/damage, cap to 5 to sanity-check
    python scripts/generate_icons_batch.py --only arcana.damage --limit 5
    # full run
    python scripts/generate_icons_batch.py
    # retry a single failing id
    python scripts/generate_icons_batch.py --only arcana.summoning.100.shadow.shadow_dragon --overwrite
"""
from __future__ import annotations

import argparse
import base64
import csv
import json
import sys
import time
from pathlib import Path

import requests
import yaml

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO / "scripts"))

from generate_icon import API, GENERATED, ICONS_DIR, build_prompt  # noqa: E402

LOG_PATH = REPO / "icons" / "generation_log.csv"


def iter_ability_ids():
    for pillar_dir in sorted((GENERATED / "flavored").iterdir()):
        if not pillar_dir.is_dir():
            continue
        pillar = pillar_dir.name
        for yaml_path in sorted(pillar_dir.glob("*.yaml")):
            category = yaml_path.stem
            with open(yaml_path) as f:
                doc = yaml.safe_load(f)
            for tier in sorted(doc["variants"]):
                for school in sorted(doc["variants"][tier]):
                    ability = doc["variants"][tier][school]
                    yield f"{pillar}.{category}.{tier}.{school}.{ability['name']}"


def filter_ids(ids, only: str | None):
    if not only:
        yield from ids
        return
    for aid in ids:
        if aid == only or aid.startswith(only + "."):
            yield aid


def generate_one(ability_id: str, params: dict, api: str) -> tuple[Path, int]:
    prompt, negative, _ability, _defaults = build_prompt(ability_id)
    body = {"prompt": prompt, "negative_prompt": negative, **params}
    r = requests.post(api, json=body, timeout=600)
    r.raise_for_status()
    payload = r.json()
    png_b64 = payload["images"][0]
    if "," in png_b64:
        png_b64 = png_b64.split(",", 1)[1]
    out = ICONS_DIR / f"{ability_id}.png"
    out.write_bytes(base64.b64decode(png_b64))
    info = {}
    try:
        info = json.loads(payload.get("info", "{}"))
    except (json.JSONDecodeError, TypeError):
        pass
    real_seed = int(info.get("seed", -1)) if info else -1
    return out, real_seed


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--only", help="filter prefix, e.g. 'arcana' or 'arcana.damage' or a full id")
    p.add_argument("--limit", type=int, default=None, help="stop after N new icons (head of list)")
    p.add_argument("--sample", type=int, default=None, help="pick N ids evenly strided across the filtered list")
    p.add_argument("--dry-run", action="store_true", help="list ids without generating")
    p.add_argument("--overwrite", action="store_true", help="regenerate even if png exists")
    p.add_argument("--steps", type=int, default=30)
    p.add_argument("--cfg", type=float, default=8.5)
    p.add_argument("--sampler", default="dpmpp_2m")
    p.add_argument("--size", type=int, default=1024)
    p.add_argument("--seed", type=int, default=-1)
    p.add_argument("--api", default=API)
    args = p.parse_args()

    ICONS_DIR.mkdir(exist_ok=True)

    params = {
        "steps": args.steps,
        "width": args.size,
        "height": args.size,
        "cfg_scale": args.cfg,
        "sampler_name": args.sampler,
        "seed": args.seed,
    }

    all_ids = list(filter_ids(iter_ability_ids(), args.only))
    pending = [a for a in all_ids if args.overwrite or not (ICONS_DIR / f"{a}.png").exists()]
    skipped = len(all_ids) - len(pending)

    if args.sample:
        stride = max(1, len(pending) // args.sample)
        todo = pending[::stride][: args.sample]
    elif args.limit:
        todo = pending[: args.limit]
    else:
        todo = pending

    print(f"plan: generate={len(todo)} skip={skipped} matched={len(all_ids)}")
    if args.dry_run:
        for aid in todo:
            print(aid)
        return
    if not todo:
        print("nothing to do")
        return

    first_run = not LOG_PATH.exists()
    with open(LOG_PATH, "a", newline="") as lf:
        w = csv.writer(lf)
        if first_run:
            w.writerow(["timestamp", "ability_id", "seed", "elapsed_s", "size_bytes", "status"])

        batch_start = time.time()
        for i, aid in enumerate(todo, 1):
            t0 = time.time()
            ts = time.strftime("%Y-%m-%dT%H:%M:%S")
            try:
                out, real_seed = generate_one(aid, params, args.api)
                dt = time.time() - t0
                size = out.stat().st_size
                w.writerow([ts, aid, real_seed, f"{dt:.2f}", size, "ok"])
                lf.flush()
                avg = (time.time() - batch_start) / i
                eta_s = avg * (len(todo) - i)
                print(f"[{i}/{len(todo)}] {aid:<60} {dt:5.1f}s  {size // 1024:>5}KB  ETA {eta_s/60:.1f}m")
            except KeyboardInterrupt:
                w.writerow([ts, aid, -1, 0, 0, "interrupted"])
                lf.flush()
                print("interrupted")
                return
            except Exception as e:
                dt = time.time() - t0
                w.writerow([ts, aid, -1, f"{dt:.2f}", 0, f"error: {e}"])
                lf.flush()
                print(f"[{i}/{len(todo)}] {aid}  FAILED: {e}")

    print(f"done: {len(todo)} icons in {(time.time() - batch_start)/60:.1f}m")


if __name__ == "__main__":
    main()
