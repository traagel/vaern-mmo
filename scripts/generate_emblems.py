#!/usr/bin/env python3
"""Generate emblem images for factions (heraldic crest) and schools (arcane sigil).

Output:
    emblems/faction_a.png, emblems/faction_b.png
    emblems/school_<name>.png   (for each of 26 schools)

Usage:
    python scripts/generate_emblems.py                    # both, skip existing
    python scripts/generate_emblems.py --only factions
    python scripts/generate_emblems.py --overwrite
"""
from __future__ import annotations

import argparse
import base64
import time
from pathlib import Path

import requests
import yaml

REPO = Path(__file__).resolve().parents[1]
GENERATED = REPO / "src" / "generated"
EMBLEMS_DIR = REPO / "emblems"
API = "http://127.0.0.1:8080/sdapi/v1/txt2img"

FACTION_STYLE = (
    "heraldic emblem, ornate faction crest, symmetric symbolic design, "
    "circular medallion composition, painterly fantasy, high-detail illustration, "
    "centered subject on pure black background, rim-lit embossed metalwork, "
    "ornate border ring, symbolic iconography"
)

SCHOOL_STYLE = (
    "abstract magical school sigil, glowing arcane rune-work, symmetric centered motif, "
    "painterly fantasy, high-detail icon illustration, pure black background, "
    "mystical aura, embossed sigil, concentric rune-rings"
)

INSTITUTION_STYLE = (
    "institutional heraldic seal, ornate order-crest, symmetric ceremonial design, "
    "wax-pressed medallion composition, painterly fantasy, high-detail illustration, "
    "centered subject on pure black background, rim-lit embossed metalwork, "
    "symbolic iconography befitting an order or chapter"
)

NEGATIVE = (
    "text, letters, numbers, watermark, signature, logo, "
    "multiple subjects, concept sheet, grid, collage, "
    "realistic photograph, photography, "
    "human figure, character, face, armor, weapon, cluttered, cropped, border frame"
)


def load(p: Path):
    with open(p) as f:
        return yaml.safe_load(f)


def faction_prompt(faction: dict) -> str:
    v = faction["visual"]
    return (
        f"{FACTION_STYLE}. {v['pitch']}. "
        f"Heraldry: {v.get('heraldry', '')}. "
        f"Palette: {v.get('palette', '')}. "
        f"Materials: {v.get('material_accents', '')}"
    )


def school_prompt(school: dict) -> str:
    s = school["icon_style"]
    return (
        f"{SCHOOL_STYLE}. {school['name']} school emblem, {school.get('tag', '')} school. "
        f"Motif: {s['motif']}. "
        f"Silhouette: {s['silhouette']}. "
        f"Palette: {s['palette']}. "
        f"Material: {s['material']}"
    )


def institution_prompt(inst: dict) -> str:
    a = inst.get("aesthetic") or {}
    return (
        f"{INSTITUTION_STYLE}. {inst.get('name', '')} — {inst.get('tradition', '')}. "
        f"Motif: {a.get('motif', '')}. "
        f"Palette: {a.get('palette', '')}. "
        f"Scene: {a.get('pitch', '')}"
    )


def load_entity(entity_dir: Path) -> dict | None:
    """Merge core.yaml with sibling <name>.yaml files (same as build_web_data)."""
    if not entity_dir.is_dir():
        return None
    core = entity_dir / "core.yaml"
    if not core.exists():
        return None
    merged: dict = load(core) or {}
    for f in sorted(entity_dir.glob("*.yaml")):
        if f.name == "core.yaml":
            continue
        content = load(f)
        if content is None:
            continue
        merged[f.stem] = content
    return merged


def run_one(prompt: str, out: Path, steps: int, cfg: float, size: int, api: str):
    body = {
        "prompt": prompt,
        "negative_prompt": NEGATIVE,
        "steps": steps,
        "width": size,
        "height": size,
        "cfg_scale": cfg,
        "sampler_name": "dpmpp_2m",
        "seed": -1,
    }
    r = requests.post(api, json=body, timeout=600)
    r.raise_for_status()
    b64 = r.json()["images"][0]
    if "," in b64:
        b64 = b64.split(",", 1)[1]
    out.write_bytes(base64.b64decode(b64))


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--only", choices=["factions", "schools", "institutions"], default=None)
    p.add_argument("--overwrite", action="store_true")
    p.add_argument("--steps", type=int, default=30)
    p.add_argument("--cfg", type=float, default=7.5)
    p.add_argument("--size", type=int, default=1024)
    p.add_argument("--api", default=API)
    args = p.parse_args()

    EMBLEMS_DIR.mkdir(exist_ok=True)
    jobs: list[tuple[str, dict, Path]] = []

    if args.only != "schools":
        for fp in sorted((GENERATED / "factions").glob("*.yaml")):
            if fp.stem.startswith("_"):
                continue
            d = load(fp)
            if "visual" not in d:
                continue
            out = EMBLEMS_DIR / f"{d['id']}.png"
            if out.exists() and not args.overwrite:
                continue
            jobs.append(("faction", d, out))

    if args.only not in ("factions", "institutions"):
        for pillar_dir in sorted((GENERATED / "schools").iterdir()):
            if not pillar_dir.is_dir():
                continue
            for sp in sorted(pillar_dir.glob("*.yaml")):
                d = load(sp)
                if "icon_style" not in d:
                    continue
                out = EMBLEMS_DIR / f"school_{d['name']}.png"
                if out.exists() and not args.overwrite:
                    continue
                jobs.append(("school", d, out))

    if args.only not in ("factions", "schools"):
        idir = GENERATED / "institutions"
        if idir.exists():
            for entity in sorted(idir.iterdir()):
                if not entity.is_dir() or entity.name.startswith("_"):
                    continue
                d = load_entity(entity)
                if not d or not d.get("aesthetic"):
                    continue
                out = EMBLEMS_DIR / f"institution_{d['id']}.png"
                if out.exists() and not args.overwrite:
                    continue
                jobs.append(("institution", d, out))

    print(f"plan: generate={len(jobs)}")
    if not jobs:
        return
    t0 = time.time()
    for i, (kind, d, out) in enumerate(jobs, 1):
        if kind == "faction":
            prompt = faction_prompt(d)
            name = d["id"]
        elif kind == "institution":
            prompt = institution_prompt(d)
            name = d["id"]
        else:
            prompt = school_prompt(d)
            name = d["name"]
        t_start = time.time()
        try:
            run_one(prompt, out, args.steps, args.cfg, args.size, args.api)
            dt = time.time() - t_start
            size_kb = out.stat().st_size // 1024
            elapsed = time.time() - t0
            eta = (elapsed / i) * (len(jobs) - i)
            print(f"[{i}/{len(jobs)}] {kind}:{name:<22} {dt:5.1f}s  {size_kb:>5}KB  ETA {eta/60:.1f}m")
        except Exception as e:
            print(f"[{i}/{len(jobs)}] {kind}:{name} FAILED: {e}")


if __name__ == "__main__":
    main()
