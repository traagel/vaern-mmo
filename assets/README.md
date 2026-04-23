# Assets

3D asset packs (FBX / glTF / textures) for the game client and `vaern-museum` / `vaern-atlas`. Binaries under `zips/` and `extracted/` are **gitignored** — only this README, the conversion scripts, and the YAML calibration files are tracked. Get the zips separately and re-run the pipelines below.

## Layout

```
assets/
├── meshtint_weapon_grips.yaml                tracked — per-weapon grip calibration
├── meshtint_piece_taxonomy.yaml              tracked — human-readable names + tags per piece-node/overlay variant
├── zips/                                      gitignored — pristine source zips
│   ├── Polygonal Fantasy Pack 1.4.zip              ★ primary pack (Meshtint)
│   ├── Fantasy Props MegaKit[Standard].zip
│   ├── Modular Character Outfits - Fantasy[Standard].zip
│   ├── Universal Animation Library[Standard].zip
│   ├── Universal Animation Library 2[Standard].zip
│   └── Universal Base Characters[Standard].zip
└── extracted/                                 gitignored — organized for Bevy
    ├── meshtint/                                 ← currently used by museum + atlas
    │   ├── environment/   105 GLBs   low-poly props
    │   ├── female/        119 GLBs   base + 13 slot categories (Hair/Brow/Eyes/Mouth/Hat/Helmet/Headband/Earring/Necklace/Pauldron/Bracer/Poleyn)
    │   ├── male/          130 GLBs   same + Beard + Hair_Male
    │   ├── weapons/        49 GLBs
    │   ├── palettes/       14 PNGs   DS palette swaps (blue_gold, red_silver, …)
    │   └── animations/     11 GLBs   pose clips (Idle, Cast Spell, Warrior 01/02, Working, Talking…)
    ├── props/          (Infinigon MegaKit) — kept for reference, unreferenced by code
    ├── characters/     (Infinigon Superhero + Outfits + Hair) — ditto
    └── animations/     (Infinigon UAL1 + UAL2 + Mannequin_F) — ditto
```

## Tracked YAML calibration

- **`meshtint_weapon_grips.yaml`** — per-category `(translation, rotation_deg, flip_x/y/z)` offset + `attach: mainhand|offhand|back` for every weapon slot. Calibrated by hand-dial in the museum's weapon grip panel; loaded by `vaern_assets::WeaponGrips::load_yaml`.
- **`meshtint_piece_taxonomy.yaml`** — human-readable name + `kind` (bare/cloth/leather/plate/robe/priest/monk) + tags per base piece-node and body-overlay variant, per gender. Used for UI labels and future item-archetype → mesh filtering. Loaded by `vaern_assets::MeshtintPieceTaxonomy::load_yaml`.

## Primary pipeline — Meshtint Polygonal Fantasy Pack

Single-command conversion. Produces `assets/extracted/meshtint/*/**.glb` (all GLB, all textures embedded as PNG).

```bash
python3 scripts/convert_meshtint.py
```

What it does:

1. Unzip `zips/Polygonal Fantasy Pack 1.4.zip` into a temp dir.
2. Flatten every `Textures/*.psd` to `*.png` via ImageMagick.
3. **Binary-replace `.psd` → `.png` inside every FBX** so `fbx2gltf` finds the PNG siblings instead of embedding raw PSD bytes under `mimeType: image/unknown`. Both extensions are 4 bytes, so FBX's length-prefixed strings stay valid without structural surgery.
4. `fbx2gltf -b --pbr-metallic-roughness --khr-materials-unlit --skinning-weights 4 --normalize-weights 1 --long-indices auto` per FBX.
5. Run `scripts/clean_gltf_attrs.py` as a safety net (idempotent).

**Tools required on PATH:**

- `fbx2gltf` — the Godot fork. Install on Arch via `yay -S godot-fbx2gltf-bin` (installs to `/usr/bin/fbx2gltf` v0.13.1+).
- `magick` — ImageMagick 7 (PSD decoder).

## Bevy asset-path examples

```rust
// Props / weapons — no rig.
asset_server.load("extracted/meshtint/environment/Barrel_01.glb#Scene0");
asset_server.load("extracted/meshtint/weapons/Sword_01.glb#Scene0");

// Characters — layered composition (all pieces at the same transform).
asset_server.load("extracted/meshtint/male/Male_01.glb#Scene0");             // base
asset_server.load("extracted/meshtint/male/Helmet_01.glb#Scene0");           // overlay
asset_server.load("extracted/meshtint/male/Pauldron_03.glb#Scene0");         // overlay

// Animation source — register via vaern-assets' AnimationLibrary::add_source_renamed.
asset_server.load("extracted/meshtint/animations/Male_01_at_Pose_Idle.glb");
```

## Cleanup script

`scripts/clean_gltf_attrs.py` sweeps `assets/extracted/` and:

- **Strips unsupported vertex attributes.** Bevy 0.18 only reads POSITION / NORMAL / TANGENT / TEXCOORD_0 / TEXCOORD_1 / COLOR_0 / JOINTS_0 / WEIGHTS_0. Anything else (COLOR_N≥1, TEXCOORD_N≥2, custom DCC channels) is removed from each primitive's `attributes` dict.
- **Merges 8-bone skinning to top-4.** For files with `JOINTS_1`/`WEIGHTS_1`, pools all 8 influences per vertex, picks the top 4 by weight, renormalizes, rewrites `JOINTS_0`/`WEIGHTS_0` bytes in-place. Without this, Bevy silently drops influences 5-8 and skinning breaks on those vertices.

Idempotent. Safe to re-run after any asset change.

## Legacy: Infinigon pipeline

The repo previously used Infinigon's "Universal" series (Superhero base + Modular Outfits + UAL1/UAL2 + Fantasy Props MegaKit). These zips are still tracked in `zips/` and the extracted glTFs survive under `assets/extracted/{props,characters,animations}/` but aren't referenced anywhere in code.

If you want to re-extract:

```bash
# Extract each zip into a temp dir, then cherry-pick the Godot-glTF trees.
# Ran manually during initial exploration — no one-shot script for these packs.
# See the git history of assets/README.md for the earlier extraction recipes.
```

Optional region-split for the Infinigon base characters (head + torso + 2 arms + 2 legs):

```bash
python3 scripts/split_base_regions.py
```

Produces `Superhero_{Male,Female}_FullBody_Split.gltf` alongside the original. Not used by the current museum.

## Notes

- **Commercial license.** Each zip carries a "Standard" license (check each pack's `License.txt`). Do not commit the binaries to a public repo.
- **Normal-map convention.** Infinigon ships both OpenGL (Y-up green) and UE (Y-down) normals; keep the OpenGL-style ones — Bevy uses OpenGL conventions, matching Godot / Unity.
- **Untextured Meshtint materials.** 202/414 GLBs have materials with white `baseColorFactor` and no `baseColorTexture` — all 105 env props plus selective armor pieces. Meshtint authored these as palette-indexed UVs expecting Unity's importer to assign the shared `Colour Map 01.png`; fbx2gltf can't know. Pending fix documented in the asset-pipeline TODO memory.
