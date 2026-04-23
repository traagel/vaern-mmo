# Atmosphere Asset Wishlist

Shopping list for upgrading the scaffold's ground + sky to something actually readable.

## Current state (baseline)
- **Ground.** Flat 2200×2200u cuboid, solid color `(0.12, 0.14, 0.16)`, roughness 0.95, no texture. Gizmo grid overlay for spatial reference. Source: `crates/vaern-client/src/scene/ground.rs`.
- **Sky.** `ClearColor(0.05, 0.07, 0.10)` — solid dark blue. No skybox, no IBL.
- **Lighting.** One `DirectionalLight` @ 8_000 lx + `AmbientLight` brightness 80. No fog, no environment map. Source: `crates/vaern-client/src/scene/setup.rs`.
- **Art direction.** Stylized low-poly (Quaternius + Meshtint). Flat-shaded characters — textures must match, **not** photoreal scans.

## Minimum viable upgrade (one ground + one sky)

### 1. Stylized PBR ground set — CC0

Channels needed: BaseColor, Normal (OpenGL handedness), Roughness, AO. Height optional.
Resolution: **1K or 2K**. 4K is waste for this art style.
Format: PNG on download → bake to KTX2 for runtime. Bevy 0.18 loads KTX2 directly.

All links below are **direct downloads** (CC0, no account needed):

**ambientCG (zip includes every map):**
- [Grass002 — 2K JPG (37 MB)](https://ambientcg.com/get?file=Grass002_2K-JPG.zip) · [PNG 70 MB](https://ambientcg.com/get?file=Grass002_2K-PNG.zip) — painterly grass, 1.4 m tile
- [Grass004 — 2K JPG (38 MB)](https://ambientcg.com/get?file=Grass004_2K-JPG.zip) · [PNG 70 MB](https://ambientcg.com/get?file=Grass004_2K-PNG.zip) — alt painterly grass
- [Ground037 — 2K JPG (37 MB)](https://ambientcg.com/get?file=Ground037_2K-JPG.zip) — damp mossy woodland ground, 2.1 m tile
- [Ground048 — 2K JPG (37 MB)](https://ambientcg.com/get?file=Ground048_2K-JPG.zip) — dirt path variation

**Poly Haven (per-map zips; click the "2K" download button on the page):**
- [aerial_grass_rock](https://polyhaven.com/a/aerial_grass_rock) — 8K aerial rock + moss + grass overhead texture

Preview pages (to eyeball before downloading):
- ambientCG: https://ambientcg.com/view?id=Grass002 (swap id= param for the other names)
- Poly Haven: https://polyhaven.com/a/aerial_grass_rock

### 2. Sky HDRI — CC0

Format: `.hdr` equirectangular, 2K (1024×2048) ≈ 5 MB each.
Flow: download `.hdr` → convert to KTX2 cubemap + diffuse/specular IBL maps (Bevy's `EnvironmentMapLight` wants pre-baked). Tool: `cmft` or `IBLBaker`.

All CC0. **Direct download URLs (verified):**
- [kloofendal_48d_partly_cloudy_puresky — 2K .hdr](https://dl.polyhaven.org/file/ph-assets/HDRIs/hdr/2k/kloofendal_48d_partly_cloudy_puresky_2k.hdr) — bright partly-cloudy, neutral daytime. Page: https://polyhaven.com/a/kloofendal_48d_partly_cloudy_puresky
- [qwantani_puresky — 2K .hdr](https://dl.polyhaven.org/file/ph-assets/HDRIs/hdr/2k/qwantani_puresky_2k.hdr) — clear savanna sky with bright sun, for heartland/Pactmarch. Page: https://polyhaven.com/a/qwantani_puresky
- [satara_night — 2K .hdr](https://dl.polyhaven.org/file/ph-assets/HDRIs/hdr/2k/satara_night_2k.hdr) — starry night + Milky Way, for dusk/Ashweald. Page: https://polyhaven.com/a/satara_night

Browse more: https://polyhaven.com/hdris

URL pattern for grabbing more at 2K:
```
https://dl.polyhaven.org/file/ph-assets/HDRIs/hdr/2k/<slug>_2k.hdr
```
Swap `2k` for `4k`, `8k`, or `16k`; swap `hdr/...` for `exr/...` for EXR format.

Quick bulk-grab:
```bash
mkdir -p assets/extracted/sky/raw
cd assets/extracted/sky/raw
curl -LO https://dl.polyhaven.org/file/ph-assets/HDRIs/hdr/2k/kloofendal_48d_partly_cloudy_puresky_2k.hdr
curl -LO https://dl.polyhaven.org/file/ph-assets/HDRIs/hdr/2k/qwantani_puresky_2k.hdr
curl -LO https://dl.polyhaven.org/file/ph-assets/HDRIs/hdr/2k/satara_night_2k.hdr
```

### Code-only alternative (no downloads)
Bevy 0.18 has a built-in `AtmospherePlugin` that renders a physically-based sky procedurally. Cheaper, no asset pipeline needed, good placeholder until you pick HDRIs.

## Tune the material — mandatory

A single texture stretched across a 2200u plane looks atrocious. When you wire the ground material:
- Set `uv_transform` to repeat so **one tile ≈ 4–8m** in world space.
- Custom-build the ground mesh with tiled UVs instead of a `Cuboid`, or split the plane into a grid of smaller meshes.

## Atmospheric polish — code, no downloads

Drop these onto the camera for an outsize visual lift before any texture work lands:
- `DistanceFog` — `FogFalloff::ExponentialSquared`, density ~0.003, color = horizon tint of the HDRI.
- `Bloom` — intensity 0.15, threshold 0.9. Makes sun + specular reads sing.
- `Tonemapping::TonyMcMapface` or `AgX` — replaces default ACES, preserves color in sunsets.
- `ScreenSpaceAmbientOcclusion` — cheap, brings out creases on stylized models.

## Biome variants (second pass)

10 starter zones → group into 4–5 ground-material variants to avoid 10× asset duplication:
- Grass: Greenwood, Silverleaf, Heartland, Pactmarch, Pact Causeway
- Scorched: Ashweald, Firland Greenwood (late-day variant)
- Rocky: Irongate Pass, Frost Spine, Stoneguard Deep
- Marsh: Scrap Marsh, Barrow Coast, Scrap Flats
- Snow: Frost Spine (alt), Gravewatch Fields

Each biome = one material swap on the same cuboid, plus per-biome HDRI.

## Foliage (third pass, optional)

Once ground + sky read well, scatter foliage to break the flat plane:
- Grass billboard cards (ambientCG `GrassBlade` or Poly Haven) — GPU-instanced quads.
- Rocks — `assets/extracted/meshtint/environment/` already has several (`Rock_*.glb`).
- Dead trees for Ashweald — generate in Blender or pull from Kenney.nl nature packs.

## Disk budget (upper bound)

- One biome, 2K, 4 maps as KTX2: ~4 MB.
- One HDRI as pre-baked cubemap + IBL: ~3 MB.
- Five biomes + five skies: ~35 MB total. Trivial.

## File layout

```
assets/extracted/
├── terrain/
│   ├── grass/          basecolor.ktx2, normal.ktx2, roughness.ktx2, ao.ktx2
│   ├── dirt/
│   ├── rocky/
│   └── ...
└── sky/
    ├── overcast/       panorama.hdr, cubemap.ktx2, diffuse.ktx2, specular.ktx2
    ├── sunrise/
    └── night/
```

Load path example: `AssetServer::load("extracted/terrain/grass/basecolor.ktx2")`.

## Priority order

1. **AtmospherePlugin + DistanceFog + Tonemapping** — zero downloads, huge impact. Do this first.
2. **Ground PBR set** — one grass texture, wire with tiled UVs, pick Greenwood as test zone.
3. **HDRI sky** — one overcast sky, wire via `Skybox` + `EnvironmentMapLight`.
4. **Bloom + SSAO** — final polish.
5. **Biome variants + foliage** — later, when the scaffold feels atmospheric enough to justify content time.
