#!/usr/bin/env python3
"""Generate a single spell icon by composing the layered YAML prompt and calling
the local SDXL A1111-compatible server.

Prompt is built as comma-separated tags (SDXL-friendly), front-loaded with
composition anchors so the model does not drift into sheet/grid layouts.

Usage:
    python scripts/generate_icon.py <ability_id> [--steps N] [--cfg F] ...

ability_id = <pillar>.<category>.<tier>.<school>.<name>
  example:   arcana.damage.75.fire.fireball
"""
from __future__ import annotations

import argparse
import base64
import sys
from pathlib import Path

import requests
import yaml

REPO = Path(__file__).resolve().parents[1]
GENERATED = REPO / "src" / "generated"
ICONS_DIR = REPO / "icons"
API = "http://127.0.0.1:8080/sdapi/v1/txt2img"


def load(p: Path):
    with open(p) as f:
        return yaml.safe_load(f)


def flatten(*fragments: str) -> str:
    """Merge prose or csv fragments into a single comma-separated tag string,
    stripping stray whitespace and empty items."""
    out: list[str] = []
    for f in fragments:
        if not f:
            continue
        for part in f.split(","):
            part = part.strip().rstrip(".").strip()
            if part:
                out.append(part)
    return ", ".join(out)


def build_prompt(ability_id: str) -> tuple[str, str, dict, dict]:
    try:
        pillar, category, tier_s, school, _name = ability_id.split(".", 4)
    except ValueError:
        sys.exit(f"bad ability_id {ability_id!r} — expected pillar.category.tier.school.name")
    tier = int(tier_s)

    g_doc = load(GENERATED / "icon_style.yaml")
    s_doc = load(GENERATED / "schools" / pillar / f"{school}.yaml")
    c_doc = load(GENERATED / "abilities" / pillar / f"{category}.yaml")
    f_doc = load(GENERATED / "flavored" / pillar / f"{category}.yaml")

    ability = f_doc["variants"][tier][school]
    g = g_doc["global"]
    shape = c_doc["icon_shape"]
    style = s_doc["icon_style"]
    tier_word = g_doc["tier_words"][tier]

    pretty_name = ability["name"].replace("_", " ")
    prompt = flatten(
        g["composition_tags"],
        g["art_direction_tags"],
        pretty_name,
        shape["primary"],
        shape["tier_escalation"][tier],
        style["motif"],
        style["silhouette"],
        style["material"],
        f"palette: {style['palette']}",
        tier_word,
        ability["description"],
        g["lighting_tags"],
        g["detail_tags"],
        g["color_logic_tags"],
        pretty_name,
    )
    negative = flatten(g["negative_tags"], shape.get("negative_add", ""))
    return prompt, negative, ability, g_doc.get("sdxl_defaults", {})


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("ability_id")
    p.add_argument("--steps", type=int, default=None)
    p.add_argument("--cfg", type=float, default=None)
    p.add_argument("--sampler", default=None)
    p.add_argument("--seed", type=int, default=-1)
    p.add_argument("--size", type=int, default=None)
    p.add_argument("--api", default=API)
    args = p.parse_args()

    prompt, negative, ability, defaults = build_prompt(args.ability_id)
    steps   = args.steps   or defaults.get("steps", 30)
    cfg     = args.cfg     or defaults.get("cfg_scale", 8.5)
    sampler = args.sampler or defaults.get("sampler_name", "dpmpp_2m")
    size    = args.size    or defaults.get("width", 1024)

    print(f"ability: {ability['id']}  ({ability['name']})")
    print(f"-- prompt ({len(prompt)} chars) --\n{prompt}\n")
    print(f"-- negative --\n{negative}\n")
    print(f"-- params -- steps={steps} cfg={cfg} sampler={sampler} size={size}\n")

    ICONS_DIR.mkdir(exist_ok=True)
    out = ICONS_DIR / f"{args.ability_id}.png"

    body = {
        "prompt": prompt,
        "negative_prompt": negative,
        "steps": steps,
        "width": size,
        "height": size,
        "cfg_scale": cfg,
        "sampler_name": sampler,
        "seed": args.seed,
    }
    r = requests.post(args.api, json=body, timeout=300)
    r.raise_for_status()
    png_b64 = r.json()["images"][0]
    if "," in png_b64:
        png_b64 = png_b64.split(",", 1)[1]
    out.write_bytes(base64.b64decode(png_b64))
    print(f"wrote {out} ({out.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
