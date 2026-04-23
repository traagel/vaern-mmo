#!/usr/bin/env python3
"""Generate flagship character portraits from character_combos.yaml.

Reads src/generated/character_combos.yaml and generates one portrait per
(combo × gender). Uses generate_character.build_prompt + the same SDXL endpoint.

Usage:
    python scripts/generate_flagship.py                 # all combos, both genders
    python scripts/generate_flagship.py --gender male   # only male
    python scripts/generate_flagship.py --only mannin   # prefix filter on combo id
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

from generate_character import API, CHAR_DIR, build_prompt, out_stem  # noqa: E402

COMBOS_FILE = REPO / "src" / "generated" / "character_combos.yaml"


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--gender", choices=["male", "female", "both"], default="both")
    p.add_argument("--only", help="prefix match on combo id")
    p.add_argument("--overwrite", action="store_true")
    p.add_argument("--steps", type=int, default=30)
    p.add_argument("--cfg", type=float, default=7.5)
    p.add_argument("--sampler", default="dpmpp_2m")
    p.add_argument("--width", type=int, default=768)
    p.add_argument("--height", type=int, default=1024)
    p.add_argument("--seed", type=int, default=-1)
    p.add_argument("--api", default=API)
    args = p.parse_args()

    combos = yaml.safe_load(open(COMBOS_FILE))["flagship"]
    if args.only:
        combos = [c for c in combos if c["id"].startswith(args.only)]

    genders = ["male", "female"] if args.gender == "both" else [args.gender]
    CHAR_DIR.mkdir(exist_ok=True)

    jobs: list[tuple[dict, str]] = [(c, g) for c in combos for g in genders]
    skipped = 0
    todo: list[tuple[dict, str]] = []
    for c, g in jobs:
        out = CHAR_DIR / f"{out_stem(c['race'], g, c['class_id'], c['faction'])}.png"
        if out.exists() and not args.overwrite:
            skipped += 1
            continue
        todo.append((c, g))

    print(f"plan: generate={len(todo)} skip={skipped} (of {len(jobs)} total)")
    if not todo:
        return

    t0 = time.time()
    for i, (c, g) in enumerate(todo, 1):
        stem = out_stem(c["race"], g, c["class_id"], c["faction"])
        out = CHAR_DIR / f"{stem}.png"
        prompt, negative = build_prompt(c["race"], g, c["class_id"], c["faction"])
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
            png_b64 = r.json()["images"][0]
            if "," in png_b64:
                png_b64 = png_b64.split(",", 1)[1]
            out.write_bytes(base64.b64decode(png_b64))
            dt = time.time() - t_start
            size = out.stat().st_size
            elapsed = time.time() - t0
            eta = (elapsed / i) * (len(todo) - i)
            print(f"[{i}/{len(todo)}] {stem:<60} {dt:5.1f}s  {size // 1024:>5}KB  ETA {eta/60:.1f}m")
        except Exception as e:
            print(f"[{i}/{len(todo)}] {stem} FAILED: {e}")


if __name__ == "__main__":
    main()
