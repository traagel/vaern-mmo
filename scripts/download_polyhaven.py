#!/usr/bin/env python3
"""
Download a curated pack of PBR world-dressing assets from Poly Haven (CC0).

Assets land under assets/polyhaven/{slug}/ with a .gltf + .bin + textures/,
ready for Bevy's GLTF loader. Run with `python3 scripts/download_polyhaven.py`.

Idempotent: skips files already present with matching size.
"""
from __future__ import annotations
import json
import sys
import urllib.request
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

API = "https://api.polyhaven.com"
ROOT = Path(__file__).resolve().parents[1] / "assets" / "polyhaven"
RES = "1k"
UA = "Mozilla/5.0 (X11; Linux x86_64) vaern-asset-fetch/1.0"

CURATED = {
    # NOTE: Poly Haven trees are photoscans. Hero meshes (pine_tree_01,
    # fir_tree_01, island_tree_*, tree_small_02) are 90 MB - 905 MB each
    # and not usable for density scatter. Included here only: saplings
    # small enough to scatter. For hero trees, download selectively into
    # assets/polyhaven_hero/ by hand.
    "trees": [
        "pine_sapling_small",       # ~16 MB
        "fir_sapling",              # ~20 MB
        "fir_sapling_medium",       # ~70 MB — borderline, use sparsely
    ],
    "dead_wood": [
        "dead_tree_trunk", "dead_tree_trunk_02",
        "tree_stump_01", "tree_stump_02",
        "pine_roots", "root_cluster_01", "single_root",
        "dry_branches_medium_01",
    ],
    "rocks": [
        "boulder_01", "rock_07", "rock_09",
        "rock_face_01", "rock_face_02",
        "rock_moss_set_01", "rock_moss_set_02",
        "stone_01", "mountainside", "coast_rocks_01",
    ],
    "ground_cover": [
        "grass_medium_01", "grass_medium_02", "grass_bermuda_01",
        "moss_01", "fern_02", "dandelion_01",
    ],
    "shrubs_flowers": [
        "shrub_01", "shrub_02", "shrub_03", "shrub_04",
        "celandine_01",
    ],
    "hub_props": [
        "wooden_barrels_01", "wooden_crate_02",
        "wooden_bucket_01", "wooden_bucket_02",
        "wooden_bowl_01", "wooden_lantern_01",
        "Lantern_01", "vintage_oil_lamp", "wooden_candlestick",
        "lantern_chandelier_01",
        "treasure_chest",
        "WoodenTable_01", "WoodenChair_01",
        "large_castle_door", "large_iron_gate",
        "modular_fort_01", "stone_fire_pit",
        "spinning_wheel_01", "horse_statue_01",
    ],
    "weapon_rack_dressing": [
        "katana_stand_01",
        "antique_estoc", "kite_shield",
        "ornate_medieval_dagger", "ornate_medieval_mace",
        "ornate_war_hammer",
    ],
}


def _open(url: str, timeout: int = 30):
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    return urllib.request.urlopen(req, timeout=timeout)


def fetch_json(url: str) -> dict:
    with _open(url, timeout=30) as r:
        return json.loads(r.read())


def download(url: str, dest: Path, expected_size: int | None) -> str:
    dest.parent.mkdir(parents=True, exist_ok=True)
    if dest.exists() and expected_size and dest.stat().st_size == expected_size:
        return "skip"
    with _open(url, timeout=60) as r, dest.open("wb") as f:
        while chunk := r.read(65536):
            f.write(chunk)
    return "ok"


def download_asset(slug: str) -> tuple[str, int, int, str]:
    """Download a Poly Haven asset: the .gltf plus its referenced .bin + textures/*.

    Poly Haven's /files API lists the .gltf but not the buffer or texture
    files it references — those are at sibling URLs on dl.polyhaven.org,
    reachable by resolving the URIs inside the .gltf against the .gltf's URL.

    Returns (slug, files_downloaded, files_skipped, status).
    """
    try:
        files = fetch_json(f"{API}/files/{slug}")
    except Exception as e:
        return (slug, 0, 0, f"ERR fetching manifest: {e}")

    gltf_section = files.get("gltf", {})
    if RES not in gltf_section:
        return (slug, 0, 0, f"ERR no {RES} gltf variant")
    variant = gltf_section[RES]

    gltf_info = variant.get("gltf") or variant.get("glTF") or {}
    gltf_url = gltf_info.get("url")
    if not gltf_url:
        return (slug, 0, 0, "ERR no gltf url in manifest")

    gltf_dest = ROOT / slug / Path(gltf_url).name
    dl = sk = 0
    try:
        r = download(gltf_url, gltf_dest, gltf_info.get("size"))
        if r == "ok":
            dl += 1
        else:
            sk += 1
    except Exception as e:
        return (slug, dl, sk, f"ERR {gltf_dest.name}: {e}")

    try:
        with gltf_dest.open("rb") as f:
            gltf_json = json.load(f)
    except Exception as e:
        return (slug, dl, sk, f"ERR parsing gltf: {e}")

    # Poly Haven hosts .bin buffers alongside the .gltf under Models/gltf/,
    # but textures live at Models/jpg/<res>/<slug>/<flat-name> — NOT under
    # the gltf's textures/ subdir. The .gltf references `textures/foo.jpg`
    # as a relative path, so we download each texture from the jpg/ host
    # path but save it to the textures/ subfolder to match the .gltf.
    gltf_dir_url = gltf_url.rsplit("/", 1)[0] + "/"
    # gltf_url: https://.../Models/gltf/<res>/<slug>/<slug>_<res>.gltf
    # tex_base: https://.../Models/jpg/<res>/<slug>/
    tex_dir_url = gltf_dir_url.replace("/Models/gltf/", "/Models/jpg/")
    asset_dir = gltf_dest.parent

    buffer_refs = [
        b.get("uri")
        for b in gltf_json.get("buffers", [])
        if b.get("uri") and not b["uri"].startswith("data:")
    ]
    image_refs = [
        i.get("uri")
        for i in gltf_json.get("images", [])
        if i.get("uri") and not i["uri"].startswith("data:")
    ]

    # .bin: sibling of the .gltf on the gltf/ host
    for ref in buffer_refs:
        file_url = gltf_dir_url + ref
        dest = asset_dir / ref
        try:
            r = download(file_url, dest, None)
            dl += 1 if r == "ok" else 0
            sk += 1 if r == "skip" else 0
        except Exception as e:
            return (slug, dl, sk, f"ERR {ref}: {e}")

    # textures: flat on jpg/ host, restored to textures/ subdir on disk
    for ref in image_refs:
        basename = ref.rsplit("/", 1)[-1]
        file_url = tex_dir_url + basename
        dest = asset_dir / ref
        try:
            r = download(file_url, dest, None)
            dl += 1 if r == "ok" else 0
            sk += 1 if r == "skip" else 0
        except Exception as e:
            return (slug, dl, sk, f"ERR {ref}: {e}")

    return (slug, dl, sk, "ok")


def verify_asset(slug: str) -> tuple[bool, list[str]]:
    """Check that a previously downloaded asset has all its referenced files on disk.

    Returns (ok, missing_refs).
    """
    asset_dir = ROOT / slug
    gltfs = list(asset_dir.glob("*.gltf"))
    if not gltfs:
        return False, [f"(no .gltf in {asset_dir})"]
    with gltfs[0].open("rb") as f:
        gltf_json = json.load(f)
    refs: list[str] = []
    refs += [b.get("uri") for b in gltf_json.get("buffers", []) if b.get("uri") and not b["uri"].startswith("data:")]
    refs += [i.get("uri") for i in gltf_json.get("images", []) if i.get("uri") and not i["uri"].startswith("data:")]
    missing = [r for r in refs if not (asset_dir / r).exists()]
    return (len(missing) == 0, missing)


def main() -> int:
    if len(sys.argv) > 1 and sys.argv[1] == "verify":
        all_slugs = [(c, s) for c, ss in CURATED.items() for s in ss]
        print(f"Verifying {len(all_slugs)} assets in {ROOT}")
        broken = 0
        for cat, slug in all_slugs:
            ok, missing = verify_asset(slug)
            if not ok:
                broken += 1
                print(f"  BROKEN {cat:20} {slug:30} missing={missing}")
        print()
        print(f"{len(all_slugs) - broken}/{len(all_slugs)} assets intact.")
        return 0 if broken == 0 else 1

    all_slugs = [(c, s) for c, ss in CURATED.items() for s in ss]
    print(f"Downloading {len(all_slugs)} assets from Poly Haven ({RES}, CC0) -> {ROOT}")
    print()

    results: list[tuple[str, str, int, int, str]] = []
    with ThreadPoolExecutor(max_workers=4) as ex:
        fut_to_slug = {ex.submit(download_asset, slug): (cat, slug) for cat, slug in all_slugs}
        for i, fut in enumerate(as_completed(fut_to_slug), 1):
            cat, slug = fut_to_slug[fut]
            _, dl, sk, status = fut.result()
            results.append((cat, slug, dl, sk, status))
            marker = "OK" if status == "ok" else status
            print(f"[{i:2}/{len(all_slugs)}] {cat:20} {slug:30} dl={dl:2} skip={sk:2} {marker}")

    ok = sum(1 for r in results if r[4] == "ok")
    err = sum(1 for r in results if r[4] != "ok")
    total_dl = sum(r[2] for r in results)
    print()
    print(f"Done. {ok} assets ok, {err} failed. {total_dl} files downloaded.")
    if err:
        print("Failures:")
        for r in results:
            if r[4] != "ok":
                print(f"  {r[1]}: {r[4]}")

    broken = sum(1 for _, slug in all_slugs if not verify_asset(slug)[0])
    print(f"Verification: {len(all_slugs) - broken}/{len(all_slugs)} assets have all referenced files on disk.")
    return 0 if err == 0 and broken == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
