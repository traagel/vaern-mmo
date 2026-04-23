#!/usr/bin/env python3
"""Convert Meshtint's Polygonal Fantasy Pack from its shipped FBX + PSD format
into Bevy-friendly GLB + PNG under `assets/extracted/meshtint/`.

Pipeline:
  1. Unzip `assets/zips/Polygonal Fantasy Pack 1.4.zip` into a temp dir.
  2. Flatten every PSD under `Textures/` into a sibling PNG (ImageMagick).
  3. Invoke `fbx2gltf` (Godot fork, 0.13.1+) once per FBX to produce GLBs.
  4. Run `clean_gltf_attrs.py` over the output (safety net).

Tools required on PATH: `fbx2gltf` (Godot fork) and `magick` (ImageMagick 7).

Key conversion flags:
  -b                       GLB (single file, embedded textures)
  --khr-materials-unlit    match Meshtint's flat-shaded look
  --skinning-weights 4     cap influences at Bevy's supported 4 per vertex
  --normalize-weights 1    re-normalize weights after cap
  --long-indices auto      use uint32 only when the mesh actually needs them
"""

import shutil
import subprocess
import sys
import zipfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent
ZIP_PATH = REPO / "assets/zips/Polygonal Fantasy Pack 1.4.zip"
RAW_DIR = REPO / "assets/extracted/_meshtint_raw"
OUT_DIR = REPO / "assets/extracted/meshtint"

# (subpath within the extracted pack, destination subfolder under OUT_DIR)
CATEGORIES = [
    ("FBX/Environment", "environment"),
    ("FBX/Male",        "male"),
    ("FBX/Female",      "female"),
    ("FBX/Weapons",     "weapons"),
    ("Animations",      "animations"),
]

FBX2GLTF_FLAGS = [
    "-b",
    "--pbr-metallic-roughness",  # extract PBR channels (incl. baseColorTexture) from FBX
    "--khr-materials-unlit",     # render flat-shaded (matches Meshtint authoring style)
    "--skinning-weights", "4",
    "--normalize-weights", "1",
    "--long-indices", "auto",
]


def sanitise(name: str) -> str:
    """FBX filenames use spaces and '@' (animation take marker). Normalize for
    Bevy asset paths where spaces are awkward."""
    return (
        name.replace(" ", "_")
            .replace("@", "_at_")
            .replace("(", "")
            .replace(")", "")
    )


def extract_zip() -> Path:
    if not ZIP_PATH.exists():
        sys.exit(f"error: {ZIP_PATH} not found")
    if RAW_DIR.exists() and any(RAW_DIR.iterdir()):
        print(f"[1/4] raw already extracted at {RAW_DIR.relative_to(REPO)}/")
    else:
        RAW_DIR.mkdir(parents=True, exist_ok=True)
        print(f"[1/4] extracting {ZIP_PATH.name}…")
        with zipfile.ZipFile(ZIP_PATH) as zf:
            zf.extractall(RAW_DIR)

    roots = [p for p in RAW_DIR.iterdir() if p.is_dir()]
    if len(roots) != 1:
        sys.exit(f"error: unexpected RAW_DIR layout: {roots}")
    return roots[0]


def flatten_psds(textures_dir: Path) -> None:
    psds = sorted(textures_dir.glob("*.psd"))
    if not psds:
        print("[2a/4] no PSDs found in Textures/, skipping")
        return
    todo = [p for p in psds if not p.with_suffix(".png").exists()]
    if not todo:
        print(f"[2a/4] all {len(psds)} PSDs already flattened to PNG")
        return
    print(f"[2a/4] flattening {len(todo)}/{len(psds)} PSDs → PNG…")
    failures = 0
    for psd in todo:
        png = psd.with_suffix(".png")
        r = subprocess.run(
            ["magick", str(psd), "-flatten", str(png)],
            capture_output=True, text=True,
        )
        if r.returncode != 0:
            print(f"  FAIL {psd.name}: {r.stderr.strip() or r.stdout.strip()}")
            failures += 1
    if failures:
        print(f"  WARN: {failures} PSD(s) failed; corresponding materials may render untextured")


def rewire_fbx_texture_refs(pack_root: Path) -> None:
    """Binary-replace every `.psd` reference inside each FBX with `.png`.

    Meshtint's FBXs are Kaydara binary format, but the texture paths are stored
    as plain ASCII length-prefixed strings inside. Because `.psd` and `.png`
    are both 4 bytes, the string byte counts (and therefore FBX's length
    prefixes) stay valid — no structural surgery needed.

    Without this step, `fbx2gltf` finds the PSD sibling on disk, embeds its
    bytes into the GLB, and sets `mimeType: image/unknown` (Bevy then fails
    to load the asset). After this rewrite, fbx2gltf resolves the matching
    PNG (already produced by `flatten_psds`) and embeds a valid image/png.
    """
    fbxs = list(pack_root.rglob("*.FBX")) + list(pack_root.rglob("*.fbx"))
    print(f"[2b/4] rewriting .psd → .png refs in {len(fbxs)} FBX file(s)…")
    touched = 0
    for fbx in fbxs:
        data = fbx.read_bytes()
        # Match both cases just in case Autodesk tools normalize to .PSD.
        new = data.replace(b".psd", b".png").replace(b".PSD", b".PNG")
        if new != data:
            fbx.write_bytes(new)
            touched += 1
    print(f"  patched {touched}/{len(fbxs)} FBX file(s)")


def convert_fbxs(pack_root: Path) -> None:
    fbx2gltf = shutil.which("fbx2gltf") or shutil.which("FBX2glTF")
    if not fbx2gltf:
        sys.exit("error: fbx2gltf not found on PATH. Install `godot-fbx2gltf-bin` from AUR.")

    OUT_DIR.mkdir(parents=True, exist_ok=True)
    total = 0
    converted = 0
    skipped = 0
    failed = 0

    print(f"[3/4] converting FBX → GLB via {fbx2gltf}…")
    for subpath, outname in CATEGORIES:
        in_dir = pack_root / subpath
        out_dir = OUT_DIR / outname
        if not in_dir.is_dir():
            print(f"  (missing) {in_dir.relative_to(pack_root)}")
            continue
        out_dir.mkdir(parents=True, exist_ok=True)

        fbxs = sorted(list(in_dir.glob("*.FBX")) + list(in_dir.glob("*.fbx")))
        print(f"\n== {outname}: {len(fbxs)} FBX ==")

        for i, fbx in enumerate(fbxs, 1):
            total += 1
            out_stem = sanitise(fbx.stem)
            out_path = out_dir / f"{out_stem}.glb"
            if out_path.exists():
                skipped += 1
                continue

            r = subprocess.run(
                [
                    fbx2gltf,
                    "-i", str(fbx),
                    "-o", str(out_dir / out_stem),   # fbx2gltf adds .glb
                    *FBX2GLTF_FLAGS,
                ],
                capture_output=True, text=True,
            )
            if r.returncode != 0 or not out_path.exists():
                failed += 1
                err = (r.stderr.strip() or r.stdout.strip())[:200]
                print(f"  [{i:3}/{len(fbxs)}] FAIL {fbx.name}: {err}")
            else:
                converted += 1
                print(f"  [{i:3}/{len(fbxs)}] {fbx.name} -> {outname}/{out_stem}.glb")

    print(
        f"\n  summary: {converted} converted, {skipped} skipped, "
        f"{failed} failed, {total} total"
    )


def clean_gltf_attrs() -> None:
    cleaner = HERE / "clean_gltf_attrs.py"
    if not cleaner.exists():
        print("[4/4] clean_gltf_attrs.py not found — skipping")
        return
    print(f"[4/4] sweeping glTF attributes in assets/extracted/…")
    subprocess.run([sys.executable, str(cleaner)], check=False)


def main() -> None:
    pack_root = extract_zip()
    flatten_psds(pack_root / "Textures")
    rewire_fbx_texture_refs(pack_root)
    convert_fbxs(pack_root)
    clean_gltf_attrs()
    print(f"\ndone. GLBs under {OUT_DIR.relative_to(REPO)}/")


if __name__ == "__main__":
    main()
