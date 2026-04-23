#!/usr/bin/env python3
"""Generate a character portrait from race [+ class] [+ faction] YAML pitches.

Composed prompt stays tight (~80 tokens) so the art style dominates SDXL's
~77-token attention window. Race pitch is always the subject; class pitch adds
kit (armor/weapon); faction pitch adds palette/heraldry.

Usage:
    python scripts/generate_character.py firland
    python scripts/generate_character.py mannin --class 0
    python scripts/generate_character.py mannin --class 0 --faction faction_a
    python scripts/generate_character.py mannin --gender female --class 0 --faction faction_a
"""
from __future__ import annotations

import argparse
import base64
from pathlib import Path

import requests
import yaml

REPO = Path(__file__).resolve().parents[1]
GENERATED = REPO / "src" / "generated"
CHAR_DIR = REPO / "characters"
API = "http://127.0.0.1:8080/sdapi/v1/txt2img"

STYLE = (
    "painterly fantasy character concept art, ArtStation illustration, "
    "single figure full body portrait, standing heroic pose, "
    "solid black studio background, dramatic rim light, face clearly visible"
)

NEGATIVE_BASE = (
    "multiple subjects, concept sheet, multiple views, inset panel, "
    "text, watermark, deformed, bad anatomy, cropped head, cropped feet, "
    "hood over face, face in shadow, colored background, gradient background, nude"
)


def load(p: Path):
    with open(p) as f:
        return yaml.safe_load(f)


def load_entity(entity_dir: Path) -> dict:
    """Merge core.yaml with sibling <name>.yaml files into one dict."""
    merged: dict = load(entity_dir / "core.yaml") or {}
    for f in sorted(entity_dir.glob("*.yaml")):
        if f.name == "core.yaml":
            continue
        content = load(f)
        if content is None:
            continue
        if f.stem == "specs" and isinstance(content, dict) and "specs" in content:
            merged["specs"] = content["specs"]
        else:
            merged[f.stem] = content
    return merged


def find_archetype_dir(class_id: int) -> Path:
    matches = sorted((GENERATED / "archetypes").glob(f"{class_id:02d}_*"))
    if not matches:
        raise SystemExit(f"no archetype dir for class_id {class_id}")
    return matches[0]


def build_prompt(race_id: str, gender: str, class_id: int | None, faction_id: str | None):
    race = load_entity(GENERATED / "races" / race_id)
    rv = race.get("visual") or {}
    race_pitch = rv.get("pitch") or rv.get("signature", "")
    if not race_pitch:
        raise SystemExit(f"race {race_id} has no visual.pitch")

    parts: list[str] = [STYLE]
    subject = f"{gender} {race_pitch}" if gender else race_pitch
    parts.append(subject)

    neg_extra: list[str] = []

    if class_id is not None:
        cls = load_entity(find_archetype_dir(class_id))
        cv = cls.get("visual") or {}
        if cv.get("pitch"):
            parts.append(f"wearing {cv['pitch']}")
        if cv.get("negative_tags"):
            neg_extra.append(cv["negative_tags"])

    if faction_id:
        fpath = GENERATED / "factions" / f"{faction_id}.yaml"
        if fpath.exists():
            fv = (load(fpath).get("visual") or {})
            if fv.get("pitch"):
                parts.append(fv["pitch"])
            if fv.get("negative_tags"):
                neg_extra.append(fv["negative_tags"])

    prompt = ", ".join(parts)
    negative = ", ".join([NEGATIVE_BASE, *neg_extra]) if neg_extra else NEGATIVE_BASE
    return prompt, negative


def out_stem(race_id: str, gender: str, class_id: int | None, faction_id: str | None) -> str:
    parts = [race_id]
    if class_id is not None:
        parts.append(f"class_{class_id:02d}")
    if faction_id:
        parts.append(faction_id)
    if gender:
        parts.append(gender)
    return ".".join(parts)


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("race_id")
    p.add_argument("--gender", choices=["male", "female", "none"], default="male")
    p.add_argument("--class", dest="class_id", type=int, default=None)
    p.add_argument("--faction", default=None)
    p.add_argument("--steps", type=int, default=30)
    p.add_argument("--cfg", type=float, default=7.5)
    p.add_argument("--sampler", default="dpmpp_2m")
    p.add_argument("--seed", type=int, default=-1)
    p.add_argument("--width", type=int, default=768)
    p.add_argument("--height", type=int, default=1024)
    p.add_argument("--api", default=API)
    args = p.parse_args()

    gender = "" if args.gender == "none" else args.gender
    prompt, negative = build_prompt(args.race_id, gender, args.class_id, args.faction)

    print(f"race: {args.race_id}  gender: {args.gender}  class: {args.class_id}  faction: {args.faction}")
    print(f"-- prompt ({len(prompt)} chars) --\n{prompt}\n")

    CHAR_DIR.mkdir(exist_ok=True)
    out = CHAR_DIR / f"{out_stem(args.race_id, args.gender if gender else '', args.class_id, args.faction)}.png"

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
    r = requests.post(args.api, json=body, timeout=600)
    r.raise_for_status()
    png_b64 = r.json()["images"][0]
    if "," in png_b64:
        png_b64 = png_b64.split(",", 1)[1]
    out.write_bytes(base64.b64decode(png_b64))
    print(f"wrote {out} ({out.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
