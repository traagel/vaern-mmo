#!/usr/bin/env python3
"""One-shot: regenerate race portraits via Meshy's gpt-image-2 at 2:3.

For each race in `src/generated/races/<id>/`, this composes a portrait prompt
from `core.yaml` + `visual.yaml` and submits one male and one female job.

Outputs land at `assets/meshy/race__<id>__<gender>/image_1.png` so the existing
`build_web_data.py:meshy_images_for(slug)` plumbing can pick them up under
the slug `race__<id>__<gender>`.

Usage:
    python3 scripts/regen_race_portraits.py [--dry-run] [--workers N]
                                             [--genders male,female]
                                             [--only mannin,skarn,...]
                                             [--no-skip-existing]
"""
from __future__ import annotations

import argparse
import sys
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

import yaml

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO / "scripts"))

from generate_meshy import (  # noqa: E402  (path-mutated import)
    OUT_DIR,
    get_api_key,
    run_one,
    slug_has_image,
    tprint,
)

RACES_DIR = REPO / "src" / "generated" / "races"

# Negative drift suppressors that suit gpt-image-2 portraits well.
DEFAULT_NEGATIVE = (
    "multiple figures, party, group, crowd, modern clothing, futuristic, "
    "sci-fi, neon, cyberpunk, anime, cartoon, watermark, text, signature, "
    "weapons drawn aggressively forward, distorted anatomy, extra limbs"
)


def load_race(race_id: str) -> tuple[dict, dict]:
    rdir = RACES_DIR / race_id
    if not rdir.is_dir():
        sys.exit(f"race not found: {rdir}")
    core = yaml.safe_load((rdir / "core.yaml").read_text(encoding="utf-8")) or {}
    visual_path = rdir / "visual.yaml"
    visual = yaml.safe_load(visual_path.read_text(encoding="utf-8")) if visual_path.exists() else {}
    return core, visual


def compose_prompt(race_id: str, gender: str, core: dict, visual: dict) -> str:
    """Natural-language portrait prompt for gpt-image-2.

    Front-loads the subject anchor, includes signature features, attire,
    hair-style, silhouette, and ends with style + framing cues.
    """
    pitch = (visual.get("pitch") or "").strip()
    signature = (visual.get("signature") or "").strip()
    body = (visual.get("body") or "").strip()
    features = (visual.get("features") or "").strip()
    hair = (visual.get("hair_style") or "").strip()
    attire = (visual.get("attire_baseline") or "").strip()
    silhouette = (visual.get("silhouette") or "").strip()

    pretty = race_id.replace("_", " ").title()
    pieces: list[str] = [
        f"Portrait of a {gender} {pretty}, single figure, isolated subject, three-quarter view from waist up",
    ]
    if pitch:
        pieces.append(pitch)
    if signature:
        pieces.append(signature)
    if body:
        pieces.append(body)
    if features:
        pieces.append(features)
    if hair:
        pieces.append(f"hair: {hair}")
    if attire:
        pieces.append(f"attire: {attire}")
    if silhouette:
        pieces.append(f"posture: {silhouette}")
    pieces.append(
        "painterly atmospheric character portrait, late-medieval fantasy aesthetic, "
        "soft directional lighting, neutral dim background, oil-painting feel, "
        "no other characters, no text, no watermark"
    )
    return ". ".join(p.rstrip(". ") for p in pieces if p) + "."


def build_jobs(race_ids: list[str], genders: list[str]) -> list[tuple[str, str, str]]:
    out: list[tuple[str, str, str]] = []
    for rid in race_ids:
        core, visual = load_race(rid)
        for g in genders:
            slug = f"race__{rid}__{g}"
            prompt = compose_prompt(rid, g, core, visual)
            negative = visual.get("negative_tags") or DEFAULT_NEGATIVE
            out.append((slug, prompt, negative))
    return out


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__,
                                formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--workers", type=int, default=8,
                   help="parallel job workers (default: 8)")
    p.add_argument("--genders", default="male,female",
                   help="comma-separated genders to render (default: male,female)")
    p.add_argument("--only", default="",
                   help="comma-separated race ids to include (default: all)")
    p.add_argument("--no-skip-existing", dest="skip_existing", action="store_false", default=True,
                   help="overwrite-into-next-slot even if an image already exists")
    p.add_argument("--model", default="gpt-image-2",
                   help="Meshy AI model (default: gpt-image-2)")
    p.add_argument("--aspect", default="2:3",
                   help="aspect ratio (default: 2:3 portrait)")
    p.add_argument("--count", type=int, default=1,
                   help="images per job (default: 1)")
    p.add_argument("--dry-run", action="store_true",
                   help="print planned jobs and exit")
    args = p.parse_args()

    key = get_api_key()
    genders = [g.strip() for g in args.genders.split(",") if g.strip()]
    if args.only:
        race_ids = [r.strip() for r in args.only.split(",") if r.strip()]
    else:
        race_ids = sorted(p.name for p in RACES_DIR.iterdir()
                          if p.is_dir() and not p.name.startswith("_"))

    jobs = build_jobs(race_ids, genders)

    skipped = []
    if args.skip_existing:
        kept = []
        for slug, prompt, neg in jobs:
            if slug_has_image(slug):
                skipped.append(slug)
            else:
                kept.append((slug, prompt, neg))
        jobs = kept

    print(f"planned {len(jobs)} job(s) · model {args.model} · aspect {args.aspect} · count {args.count}")
    if skipped:
        print(f"  ({len(skipped)} skipped — already have images; use --no-skip-existing to redo)")
    for slug, prompt, _ in jobs[:30]:
        print(f"  - {slug}: {prompt[:90]}{'…' if len(prompt) > 90 else ''}")
    if len(jobs) > 30:
        print(f"  ... and {len(jobs) - 30} more")

    if args.dry_run:
        print("\ndry-run: nothing submitted")
        return 0
    if not jobs:
        return 0

    workers = max(1, min(args.workers, len(jobs)))
    print(f"\nrunning across {workers} workers")
    failed = 0
    with ThreadPoolExecutor(max_workers=workers) as pool:
        futs = {
            pool.submit(run_one, key, slug, prompt,
                        negative=neg, aspect=args.aspect,
                        count=args.count, ai_model=args.model): slug
            for slug, prompt, neg in jobs
        }
        for fut in as_completed(futs):
            slug = futs[fut]
            try:
                fut.result()
            except Exception as e:  # noqa: BLE001
                failed += 1
                tprint(f"  ✗ [{slug}] {e}")
    tprint(f"\ndone: {len(jobs) - failed} succeeded, {failed} failed")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
