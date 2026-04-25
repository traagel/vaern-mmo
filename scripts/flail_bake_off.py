#!/usr/bin/env python3
"""Cross-sample flail prompts × configs to find what actually works.

Each cell writes to icons/flail_test/<prompt_slug>__<config_slug>.png
so you can thumbnail-browse the grid. Negative prompt is held constant.

Usage:
    python3 scripts/flail_bake_off.py
"""
import base64
import pathlib
import time

import requests

API = "http://127.0.0.1:8080/sdapi/v1/txt2img"
OUT = pathlib.Path(__file__).resolve().parents[1] / "icons" / "flail_test"
OUT.mkdir(parents=True, exist_ok=True)

NEGATIVE = (
    "text, watermark, ui, multiple subjects, grid, collage, photograph, "
    "silhouette, monochrome, sketch, lineart, flat shading, wielder, character"
)

# ── prompt variants ── slug → prompt
PROMPTS = {
    "bare": "flail",
    "icon": "flail game icon",
    "wow": "flail, fantasy game icon, painterly, centered on black background",
    "descr": "medieval flail, iron ball on chain attached to wooden handle, fantasy weapon icon",
    "nomedieval": "ball and chain mace, spiked iron ball dangling from chain, fantasy weapon icon",
    "morningstarish": "morningstar with spiked ball on chain, medieval flail, fantasy icon",
    "anatomy": "weapon with short wooden handle and spiked iron ball connected by a chain, fantasy icon",
    "current": (
        "painterly fantasy game icon, hand-painted digital art, saturated signature color, "
        "strong specular highlights, inner glow, dramatic lighting, pure black background, "
        "ball and chain mace, spiked iron ball hanging from chain on a short wooden handle, "
        "ball and chain mace"
    ),
}

# ── config variants ── slug → params
CONFIGS = {
    "juggernaut_rec":   {"sampler_name": "dpmpp_2m", "steps": 30, "cfg_scale": 5.0},
    "dpmpp_sde_hi":     {"sampler_name": "dpmpp_sde", "steps": 40, "cfg_scale": 7.0},
    "euler_a_fast":     {"sampler_name": "euler_a", "steps": 20, "cfg_scale": 4.0},
    "base_sdxl_classic":{"sampler_name": "dpmpp_2m", "steps": 30, "cfg_scale": 8.5},
}

SIZE = 1024
SEED = 12345  # fixed seed → differences are prompt/config, not rng variance

total = len(PROMPTS) * len(CONFIGS)
i = 0
t0 = time.time()
for pslug, prompt in PROMPTS.items():
    for cslug, cfg in CONFIGS.items():
        i += 1
        out = OUT / f"{pslug}__{cslug}.png"
        body = {
            "prompt": prompt,
            "negative_prompt": NEGATIVE,
            "width": SIZE, "height": SIZE,
            "seed": SEED,
            **cfg,
        }
        print(f"[{i}/{total}] {pslug} × {cslug} … ", end="", flush=True)
        t = time.time()
        r = requests.post(API, json=body, timeout=600)
        r.raise_for_status()
        png = r.json()["images"][0]
        if "," in png:
            png = png.split(",", 1)[1]
        out.write_bytes(base64.b64decode(png))
        print(f"{time.time()-t:.1f}s  {out.stat().st_size//1024}KB")

print(f"\ndone in {(time.time()-t0)/60:.1f}m → {OUT}")
