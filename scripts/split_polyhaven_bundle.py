#!/usr/bin/env python3
"""Split a Poly Haven multi-mesh glTF bundle into one glTF per top-level
node, sharing the original `.bin` buffer + textures via relative URI
references (no asset duplication).

Some Poly Haven packs ship a single .gltf containing dozens of distinct
prop pieces (e.g. `modular_fort_01` packages 22 wall/tower/stair
sections). Treating that as one editor asset means the user can't place
individual fort pieces. This script peels each top-level node into its
own `<parent>__<piece>/<parent>__<piece>_1k.gltf`, with buffer + image
URIs rewritten as `../<parent>/<original>` so the original `.bin` and
textures stay in place — disk usage barely grows.

Usage:
    python3 scripts/split_polyhaven_bundle.py <slug>
    python3 scripts/split_polyhaven_bundle.py --list <slug>     # just print piece names
    python3 scripts/split_polyhaven_bundle.py modular_fort_01

Run from the repository root. Writes to `assets/polyhaven/<slug>__*/`.
Idempotent — re-running overwrites existing piece dirs.
"""
from __future__ import annotations

import argparse
import json
import sys
from copy import deepcopy
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
ASSET_ROOT = REPO_ROOT / "assets" / "polyhaven"


def load_bundle(slug: str) -> tuple[Path, dict]:
    src = ASSET_ROOT / slug / f"{slug}_1k.gltf"
    if not src.exists():
        raise FileNotFoundError(f"glTF not found at {src}")
    return src, json.loads(src.read_text())


def piece_slug(parent_slug: str, node_name: str | None, node_idx: int) -> str:
    """Sanitize the glTF node name into a flat slug suffix.

    Names like `modular_fort_01_wall_thick_corner_02` get the parent
    prefix stripped so the result is `modular_fort_01__wall_thick_corner_02`,
    not `modular_fort_01__modular_fort_01_wall_thick_corner_02`.
    """
    if not node_name:
        return f"{parent_slug}__node_{node_idx}"
    name = node_name.strip()
    prefix = parent_slug + "_"
    if name.startswith(prefix):
        name = name[len(prefix):]
    # Replace anything that isn't a-z 0-9 _ with _ to keep slugs filesystem-safe.
    cleaned = "".join(ch if (ch.isalnum() or ch == "_") else "_" for ch in name).lower()
    return f"{parent_slug}__{cleaned}"


def build_piece_gltf(parent_slug: str, full: dict, node_idx: int) -> dict:
    """Construct a piece-glTF document.

    Strategy: keep the global tables (nodes, meshes, materials, textures,
    images, samplers, buffers, bufferViews, accessors) intact. Only
    replace the `scenes` array with a single scene that points at the
    target node. Rewrite asset URIs (`.bin`, image files) to be
    relative to the original parent folder so we don't duplicate
    binaries.

    Unused entries in the global tables are inert as far as a glTF loader
    is concerned (they're never reached from any scene). Slight JSON-size
    bloat is the only cost; the heavy `.bin` is shared.
    """
    g = deepcopy(full)

    target_node = g["nodes"][node_idx]
    g["scenes"] = [
        {
            "name": target_node.get("name", f"node_{node_idx}"),
            "nodes": [node_idx],
        }
    ]
    g["scene"] = 0

    # Buffers: rewrite relative URIs to point at `../<parent>/<file>`.
    for buf in g.get("buffers", []):
        uri = buf.get("uri")
        if uri and not uri.startswith(("data:", "http://", "https://")):
            buf["uri"] = f"../{parent_slug}/{uri}"

    # Images: same treatment.
    for img in g.get("images", []):
        uri = img.get("uri")
        if uri and not uri.startswith(("data:", "http://", "https://")):
            img["uri"] = f"../{parent_slug}/{uri}"

    # Stamp asset.generator so the file is greppable.
    g.setdefault("asset", {})
    g["asset"]["generator"] = (
        f"vaern split_polyhaven_bundle.py from {parent_slug}_1k.gltf"
    )

    return g


def split(parent_slug: str, dry_run: bool = False) -> list[str]:
    """Split the bundle. Returns the new piece slugs."""
    src_path, full = load_bundle(parent_slug)
    scene = full["scenes"][full.get("scene", 0)]
    top_nodes: list[int] = scene.get("nodes", [])

    if not top_nodes:
        print(f"WARN: {parent_slug} has no top-level nodes in its default scene; nothing to split.", file=sys.stderr)
        return []

    new_slugs: list[str] = []
    for node_idx in top_nodes:
        node = full["nodes"][node_idx]
        slug = piece_slug(parent_slug, node.get("name"), node_idx)
        new_slugs.append(slug)
        if dry_run:
            continue

        piece = build_piece_gltf(parent_slug, full, node_idx)
        out_dir = ASSET_ROOT / slug
        out_dir.mkdir(parents=True, exist_ok=True)
        out_file = out_dir / f"{slug}_1k.gltf"
        out_file.write_text(json.dumps(piece, indent=2))

    return new_slugs


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("slug", help="Parent slug (e.g. modular_fort_01)")
    p.add_argument(
        "--list",
        dest="list_only",
        action="store_true",
        help="Print the piece slugs that would be produced, but don't write any files.",
    )
    args = p.parse_args()

    pieces = split(args.slug, dry_run=args.list_only)
    if args.list_only:
        for s in pieces:
            print(s)
        return 0

    print(f"Wrote {len(pieces)} piece glTFs under assets/polyhaven/")
    print()
    print("Add these to vaern-assets CURATED + vaern-data POLYHAVEN_CURATED_SLUGS:")
    print()
    for s in pieces:
        print(f'    ("{s}", PolyHavenCategory::HubProp),')
    return 0


if __name__ == "__main__":
    sys.exit(main())
