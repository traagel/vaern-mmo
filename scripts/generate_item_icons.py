#!/usr/bin/env python3
"""Batch-generate item icons for every ItemBase in the compositional registry.

Walks `src/generated/items/bases/**/*.yaml`, calls the local SDXL server once
per base (deduped by `id`), writes each PNG to `icons/items/<base_id>.png`,
and appends a CSV audit row to `icons/items/generation_log.csv`.

The client's `ItemIconCache` reads exactly this layout at runtime — dropping
a PNG in `icons/items/` replaces the procedural fallback on next client
launch, no Rust changes required.

Prompt composition follows the same pattern as `generate_icons_batch.py`:
global composition + art tags from `icon_style.yaml`, plus per-kind motif /
material / palette fragments defined in this script (items don't have the
per-school/per-category motif YAMLs that abilities do).

Examples:

    # preview the full plan
    python scripts/generate_item_icons.py --dry-run

    # stage rollout: just weapons, cap to 5 sanity-check icons
    python scripts/generate_item_icons.py --only weapons --limit 5

    # full run with an SDXL VAE-safety pause every 30 images
    python scripts/generate_item_icons.py --sleep-every 30 --sleep-secs 10

    # regenerate one failing base
    python scripts/generate_item_icons.py --only minor_healing_potion --overwrite
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
GENERATED = REPO / "src" / "generated"
BASES_ROOT = GENERATED / "items" / "bases"
ICONS_DIR = REPO / "icons" / "items"
LOG_PATH = ICONS_DIR / "generation_log.csv"
API = "http://127.0.0.1:8080/sdapi/v1/txt2img"


def load_yaml(p: Path):
    with open(p) as f:
        return yaml.safe_load(f)


def flatten(*fragments: str) -> str:
    """Merge prose / csv fragments into a single comma-separated tag string."""
    out: list[str] = []
    for frag in fragments:
        if not frag:
            continue
        for part in frag.split(","):
            part = part.strip().rstrip(".").strip()
            if part:
                out.append(part)
    return ", ".join(out)


# ─── prompt tokens ───────────────────────────────────────────────────────────
#
# Keep the full prompt under SDXL's ~77-token CLIP window. Everything past
# that is ignored — including the subject duplication at the tail.
#
# Design: one global style anchor (~12 tokens), one subject anchor (2-3
# tokens), ONE per-kind motif line (~6-10 tokens), subject duplicated at
# tail for weighting. Total budget: ~25-30 tokens, leaving headroom for
# longer piece names like "Greater Cold Resistance Potion" (5 tokens).

# The style anchor pulls hard on "saturated signature color" + "inner
# glow" + "specular highlights" because the failure mode without them is
# a monochrome rim-light study (black-and-white ink-art drift). Avoid the
# words "silhouette" and "rim-lit subject" as positives — SDXL reads them
# literally and produces solid-color cutouts.
GLOBAL_STYLE = (
    "painterly fantasy game icon, hand-painted digital art, saturated signature color, "
    "strong specular highlights, inner glow, dramatic lighting, pure black background"
)
GLOBAL_NEGATIVE = (
    "text, watermark, letters, ui, border, multiple subjects, grid, collage, "
    "photograph, cluttered, silhouette, monochrome, black and white, ink drawing, "
    "sketch, lineart, flat shading, desaturated"
)

# Per-weapon-school silhouette anchor. The piece_name ("longsword",
# "halberd") already tells SDXL what the thing IS; this line adds
# material + grip-style cues.
WEAPON_SILHOUETTE = {
    "blade": "steel blade with leather grip",
    "blunt": "hafted weapon with heavy iron head",
    "dagger": "short steel blade with corded hilt",
    "spear": "long wooden haft with steel tip",
    "bow": "curved wooden recurve bow with taut string",
    "crossbow": "wooden crossbow with steel prod",
    "thrown": "balanced throwing weapon with iron tip",
    "unarmed": "studded leather hand wrap",
    "arcane": "ornate enchanted focus with glowing runes",
}

# Per-piece full overrides for bases whose piece_name SDXL's CLIP doesn't
# recognize strongly. For these, we replace BOTH the subject anchor and
# the motif — otherwise SDXL's weak concept for words like "flail" pulls
# the image toward a generic weapon default, ignoring the descriptive
# motif. Each entry is a single phrase that becomes both the subject
# anchor (duplicated for weighting) and the motif.
#
# Rule of thumb: use a noun SDXL actually knows ("ball-and-chain mace",
# not "flail") + a short anatomical description. No jargon ("prod",
# "siangham"). Treat the whole string as a self-sufficient prompt
# describing the single object.
#
# Add entries when sampled output doesn't match the intended weapon.
# Format: base_id → short phrase (~8-15 tokens).
PIECE_OVERRIDE = {
    # Blunt — SDXL's "flail" concept is weak → defaults to sword + chain.
    # Winner from bake-off: anchor on "morningstar" (a noun SDXL knows)
    # and qualify with the distinguishing features.
    "flail": "morningstar with spiked ball on chain, medieval flail",
    "morningstar": "spiked mace, heavy round spiked ball on short wooden haft",
    "greatclub": "huge wooden two-handed club with knobbed striking head",
    # Crossbows — "prod" is obscure; describe the horizontal-bow anatomy.
    "crossbow": "medieval crossbow, wooden stock with horizontal bow arms, loaded bolt",
    "hand_crossbow": "small pistol crossbow, one-handed wooden stock, loaded bolt",
    "heavy_crossbow": "heavy medieval crossbow, thick wooden stock with broad steel bow arms",
    "repeating_crossbow": "chinese repeating crossbow, vertical box magazine atop wooden stock",
    # Obscure daggers / exotic melee — lead with strong analog nouns.
    "kukri": "curved nepali fighting knife, forward-angled inward-curving blade",
    "kris": "wavy-bladed dagger, flame-shaped undulating blade, ornate wooden hilt",
    "sai": "three-pronged metal fighting fork, long central prong with two shorter side prongs",
    "siangham": "slim wooden stake weapon, pointed hardwood spike",
    # Thrown — obscure.
    "shuriken": "ninja throwing star, flat metal with multiple sharp radiating points",
    "dart": "small throwing dart, slim metal spike with feathered tail",
    # Arcane — prevent "generic wand" drift for specific casting foci.
    "orb": "glowing magical orb, floating glass crystal sphere, inner light",
    "runestaff": "wizard staff, long wooden staff topped with glowing crystal and carved runes",
    "wand": "magic wand, short ornate wooden rod with inlaid runes",
    "scepter": "ceremonial scepter, short golden rod with jeweled head",
}

# Per-armor-type material descriptor. Paired with the piece_name
# (Helmet / Pauldrons / Boots / Gauntlets / etc.) it gives SDXL enough
# to render a specific armor silhouette.
ARMOR_MATERIAL = {
    "cloth": "embroidered woven fabric in flowing folds",
    "gambeson": "quilted padded linen with diamond stitching",
    "leather": "tanned stitched hide with buckled straps",
    "mail": "interlinked steel chainmail with riveted links",
    "plate": "polished plate steel with rivet detail",
}

# Liquid color keyed off ConsumeEffect.kind. Drives bottle interior hue.
POTION_LIQUID = {
    "heal_hp": "red liquid with crimson glow",
    "heal_mana": "blue liquid with sapphire glow",
    "heal_stamina": "amber liquid with honey glow",
    "buff": "iridescent liquid with prismatic shimmer",
    "none": "clear liquid with faint shimmer",
}

# Per-kind negative add-on. Kept short — the global negative already
# covers the common drifts (text, ui, grid, photograph).
KIND_NEGATIVE = {
    "weapon": "wielder, character, person",
    "armor": "wearer, mannequin, character, full body",
    "shield": "wielder, character",
    "rune": "character, rune circle, cast spell",
    "consumable": "multiple bottles, shelf, scene, character",
    "material": "landscape, pile, scene, multiple samples",
    "reagent": "laboratory scene, character",
    "trinket": "jewelry collection, multiple trinkets",
    "quest": "character, scene",
    "currency": "treasure hoard, chest",
    "misc": "scene, landscape",
}


# ─── per-kind motif (single line, ~5-10 tokens) ──────────────────────────────


def material_motif(piece: str) -> str:
    """Pick a single-line motif for a crafting material based on keywords
    in the piece name. Falls through to a generic 'raw material sample'."""
    if "ingot" in piece:
        return "metal ingot bar with hammered surface"
    if "hide" in piece or "leather" in piece or "skin" in piece:
        return "folded tanned hide with stitched edge"
    if "cloth" in piece or "silk" in piece or "linen" in piece:
        return "folded bolt of woven cloth"
    if "ore" in piece:
        return "raw ore chunk with mineral facets"
    if "herb" in piece or "root" in piece or "leaf" in piece or "weed" in piece:
        return "bundled dried herb sprig"
    if "dust" in piece or "powder" in piece:
        return "small pile of fine magical powder"
    if "gem" in piece or "crystal" in piece:
        return "cut faceted gemstone with inner light"
    if "scale" in piece:
        return "single iridescent creature scale"
    return "raw material sample"


def kind_motif(base: dict) -> str:
    """One comma-separated motif line per item kind. ~5-10 tokens.
    The piece_name carries most of the subject specificity; this adds
    material + pose cues.

    PIECE_OVERRIDE bases bypass this path entirely — handled in
    build_prompt, which skips the piece_name anchor for them."""
    kind = base["kind"]
    ktype = kind["type"]
    piece = base["piece_name"].lower()

    if ktype == "weapon":
        school = kind.get("school", "")
        return WEAPON_SILHOUETTE.get(school, "forged fantasy weapon")

    if ktype == "armor":
        atype = kind.get("armor_type", "")
        return ARMOR_MATERIAL.get(atype, "fantasy armor piece")

    if ktype == "shield":
        return "heraldic wooden shield with metal rim and carved emblem"

    if ktype == "rune":
        school = kind.get("school", "")
        return f"carved stone disc with glowing {school} sigil"

    if ktype == "consumable":
        food_words = ("pie", "stew", "tack", "feast", "cheese", "bread", "jerky", "ration")
        if "scroll" in piece:
            return "rolled parchment scroll with wax seal"
        if any(w in piece for w in food_words):
            return "rustic tavern food in warm tones"
        effect = kind.get("effect", {}) or {}
        ekind = effect.get("kind", "none")
        return f"glass flask with cork stopper, {POTION_LIQUID.get(ekind, 'colored liquid')}"

    if ktype == "material":
        return material_motif(piece)

    if ktype == "reagent":
        return "small alchemical vial with label"

    if ktype == "trinket":
        return "small ornate fantasy trinket with inset gem"

    if ktype == "quest":
        return "aged parchment with wax seal and emblem"

    if ktype == "currency":
        return "stack of embossed fantasy coins"

    return "single fantasy item prop"


# ─── prompt builder ──────────────────────────────────────────────────────────


def build_prompt(base: dict) -> tuple[str, str]:
    """Compose a tight ~25-token prompt.

    Default shape: <global_style>, <piece>, <kind_motif>, <piece>
    The piece_name duplication at head + tail is the standard SDXL
    poor-man's weighting trick — emphasizes the subject noun.

    PIECE_OVERRIDE shape: <global_style>, <override>, <override>
    Overridden bases skip the piece_name anchor entirely because the
    piece word itself (e.g. "flail") is the drift source — SDXL's
    weak concept for it pulls toward a generic default regardless of
    the motif. The override phrase is a self-sufficient subject
    description using words SDXL knows, duplicated for weighting.
    """
    base_id = base["id"]
    piece = base["piece_name"].lower()
    ktype = base["kind"]["type"]

    if base_id in PIECE_OVERRIDE:
        override = PIECE_OVERRIDE[base_id]
        prompt = flatten(GLOBAL_STYLE, override, override)
    else:
        prompt = flatten(GLOBAL_STYLE, piece, kind_motif(base), piece)
    negative = flatten(GLOBAL_NEGATIVE, KIND_NEGATIVE.get(ktype, ""))
    return prompt, negative


# ─── base iteration ──────────────────────────────────────────────────────────


def iter_bases():
    """Yield (base_id, base_dict) across every items/bases YAML.

    The directory has:
      bases/armor/{cloth,gambeson,leather,mail,plate}.yaml
      bases/{weapons,shields,consumables,runes,materials}.yaml
    """
    for yaml_path in sorted(BASES_ROOT.rglob("*.yaml")):
        doc = load_yaml(yaml_path)
        if not doc or "bases" not in doc:
            continue
        for base in doc["bases"]:
            yield base["id"], base


def filter_bases(pairs, only: str | None):
    if not only:
        yield from pairs
        return
    for bid, base in pairs:
        ktype = base["kind"]["type"]
        # Match: exact id, id prefix, or kind type shortcut ("weapons", "armor")
        if (
            bid == only
            or bid.startswith(only)
            or ktype == only.rstrip("s")  # "weapons" → kind "weapon"
            or ktype == only
        ):
            yield bid, base


# ─── generation ──────────────────────────────────────────────────────────────


def generate_one(base_id: str, base: dict, params: dict, api: str) -> tuple[Path, int]:
    prompt, negative = build_prompt(base)
    body = {"prompt": prompt, "negative_prompt": negative, **params}
    r = requests.post(api, json=body, timeout=600)
    r.raise_for_status()
    payload = r.json()
    png_b64 = payload["images"][0]
    if "," in png_b64:
        png_b64 = png_b64.split(",", 1)[1]
    out = ICONS_DIR / f"{base_id}.png"
    out.write_bytes(base64.b64decode(png_b64))
    info = {}
    try:
        info = json.loads(payload.get("info", "{}"))
    except (json.JSONDecodeError, TypeError):
        pass
    real_seed = int(info.get("seed", -1)) if info else -1
    return out, real_seed


def main() -> None:
    p = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    p.add_argument(
        "--only",
        help="filter: exact base_id, base_id prefix, or kind type ('weapons', 'armor', 'consumables', 'runes', 'shields', 'materials', 'trinket', ...)",
    )
    p.add_argument("--limit", type=int, default=None, help="stop after N new icons")
    p.add_argument(
        "--sample",
        type=int,
        default=None,
        help="pick N ids evenly strided across the filtered list",
    )
    p.add_argument("--dry-run", action="store_true", help="list ids without generating")
    p.add_argument(
        "--overwrite", action="store_true", help="regenerate even if png exists"
    )
    p.add_argument(
        "--print-prompt",
        action="store_true",
        help="print each generated prompt to stdout (debug drift)",
    )
    p.add_argument("--steps", type=int, default=30)
    p.add_argument("--cfg", type=float, default=8.5)
    p.add_argument("--sampler", default="dpmpp_2m")
    p.add_argument("--size", type=int, default=1024)
    p.add_argument("--seed", type=int, default=-1)
    p.add_argument("--api", default=API)
    p.add_argument(
        "--sleep-every",
        type=int,
        default=0,
        help="pause for --sleep-secs every N generations to let SDXL VAE recover (OOM mitigation)",
    )
    p.add_argument("--sleep-secs", type=float, default=8.0)
    args = p.parse_args()

    ICONS_DIR.mkdir(parents=True, exist_ok=True)

    params = {
        "steps": args.steps,
        "width": args.size,
        "height": args.size,
        "cfg_scale": args.cfg,
        "sampler_name": args.sampler,
        "seed": args.seed,
    }

    all_pairs = list(filter_bases(iter_bases(), args.only))
    pending = [
        (bid, base)
        for bid, base in all_pairs
        if args.overwrite or not (ICONS_DIR / f"{bid}.png").exists()
    ]
    skipped = len(all_pairs) - len(pending)

    if args.sample:
        stride = max(1, len(pending) // args.sample)
        todo = pending[::stride][: args.sample]
    elif args.limit:
        todo = pending[: args.limit]
    else:
        todo = pending

    print(f"plan: generate={len(todo)} skip={skipped} matched={len(all_pairs)}")
    if args.dry_run:
        for bid, base in todo:
            ktype = base["kind"]["type"]
            print(f"{bid:<40} [{ktype}] {base['piece_name']}")
            if args.print_prompt:
                prompt, negative = build_prompt(base)
                print(f"  + prompt ({len(prompt)} chars): {prompt}")
                print(f"  - negative: {negative}")
                print()
        return
    if not todo:
        print("nothing to do")
        return

    first_run = not LOG_PATH.exists()
    with open(LOG_PATH, "a", newline="") as lf:
        w = csv.writer(lf)
        if first_run:
            w.writerow(
                ["timestamp", "base_id", "kind", "seed", "elapsed_s", "size_bytes", "status"]
            )

        batch_start = time.time()
        for i, (bid, base) in enumerate(todo, 1):
            ktype = base["kind"]["type"]
            t0 = time.time()
            ts = time.strftime("%Y-%m-%dT%H:%M:%S")
            try:
                if args.print_prompt:
                    prompt, negative = build_prompt(base)
                    print(f"  + {bid} prompt: {prompt}")
                out, real_seed = generate_one(bid, base, params, args.api)
                dt = time.time() - t0
                size = out.stat().st_size
                w.writerow([ts, bid, ktype, real_seed, f"{dt:.2f}", size, "ok"])
                lf.flush()
                avg = (time.time() - batch_start) / i
                eta_s = avg * (len(todo) - i)
                print(
                    f"[{i}/{len(todo)}] {bid:<40} {ktype:<11} {dt:5.1f}s  {size // 1024:>5}KB  ETA {eta_s/60:.1f}m"
                )
            except KeyboardInterrupt:
                w.writerow([ts, bid, ktype, -1, 0, 0, "interrupted"])
                lf.flush()
                print("interrupted")
                return
            except Exception as e:
                dt = time.time() - t0
                w.writerow([ts, bid, ktype, -1, f"{dt:.2f}", 0, f"error: {e}"])
                lf.flush()
                print(f"[{i}/{len(todo)}] {bid}  FAILED: {e}")

            if args.sleep_every and i % args.sleep_every == 0 and i < len(todo):
                print(f"  -- VAE rest: sleeping {args.sleep_secs}s --")
                time.sleep(args.sleep_secs)

    print(f"done: {len(todo)} icons in {(time.time() - batch_start)/60:.1f}m")


if __name__ == "__main__":
    main()
