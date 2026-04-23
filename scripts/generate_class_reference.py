#!/usr/bin/env python3
"""Generate one reference portrait per class on a canonical race (default Mannin /
Concord). Produces a class-kit preview set for the web UI's Classes tab.

Filenames: characters/<race>.class_NN.<faction>.<gender>.png

Usage:
    python scripts/generate_class_reference.py                # 15 classes x both genders on mannin/faction_a
    python scripts/generate_class_reference.py --race kharun --faction faction_b --gender male
"""
from __future__ import annotations

import argparse
import base64
import sys
import time
from pathlib import Path

import requests

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO / "scripts"))

from generate_character import API, CHAR_DIR, build_prompt, out_stem  # noqa: E402

NUM_CLASSES = 15


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--race", default="mannin", help="canonical race for the class-kit reference")
    p.add_argument("--faction", default="faction_a")
    p.add_argument("--gender", choices=["male", "female", "both"], default="both")
    p.add_argument("--overwrite", action="store_true")
    p.add_argument("--only", type=int, default=None, help="only generate this class_id")
    p.add_argument("--steps", type=int, default=30)
    p.add_argument("--cfg", type=float, default=7.5)
    p.add_argument("--sampler", default="dpmpp_2m")
    p.add_argument("--seed", type=int, default=-1)
    p.add_argument("--width", type=int, default=768)
    p.add_argument("--height", type=int, default=1024)
    p.add_argument("--api", default=API)
    args = p.parse_args()

    genders = ["male", "female"] if args.gender == "both" else [args.gender]
    CHAR_DIR.mkdir(exist_ok=True)

    ids = [args.only] if args.only is not None else list(range(NUM_CLASSES))
    jobs: list[tuple[int, str]] = [(cid, g) for cid in ids for g in genders]
    todo: list[tuple[int, str]] = []
    skipped = 0
    for cid, g in jobs:
        stem = out_stem(args.race, g, cid, args.faction)
        if (CHAR_DIR / f"{stem}.png").exists() and not args.overwrite:
            skipped += 1
            continue
        todo.append((cid, g))

    print(f"plan: generate={len(todo)} skip={skipped} (race={args.race} faction={args.faction})")
    if not todo:
        return

    t0 = time.time()
    for i, (cid, g) in enumerate(todo, 1):
        stem = out_stem(args.race, g, cid, args.faction)
        out = CHAR_DIR / f"{stem}.png"
        prompt, negative = build_prompt(args.race, g, cid, args.faction)
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
            print(f"[{i}/{len(todo)}] {stem:<60} {dt:5.1f}s  {size // 1024:>5}KB  ETA {eta/60:.1f}m")
        except Exception as e:
            print(f"[{i}/{len(todo)}] {stem} FAILED: {e}")


if __name__ == "__main__":
    main()
