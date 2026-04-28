#!/usr/bin/env bash
# Download + extract CC0 PBR terrain biome textures from ambientCG.
#
# Idempotent: skips zips already in raw/ and biome folders already
# populated. Run after editing the BIOMES table below to grab any
# newly-added biome; `--force` re-downloads and re-extracts everything.
#
# Each biome ends up at:
#   assets/extracted/terrain/<biome_key>/<AmbientCGId>_2K-JPG_{Color,NormalGL,AmbientOcclusion,...}.jpg
#
# `hub_regions.rs::biome_table` consumes the extracted paths.

set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
ASSETS="$REPO/assets/extracted/terrain"
RAW="$ASSETS/raw"
mkdir -p "$ASSETS" "$RAW"

# biome_key  =>  ambientCG set id (2K-JPG resolution)
declare -a BIOMES=(
    "snow:Snow004"
    "stone:PavingStones004"
    "scorched:Ground063"
    "marsh:Ground059"
    "rocky:Rocks023"
    # Cartography-fidelity expansion (Apr 2026): cover the cartography
    # biome vocabulary (fields/forest/mountain/sand/mud/...) so the
    # editor's biome paint palette + the cartography→editor importer
    # can render each cartography region with a distinct PBR set.
    "forest:Ground067"          # brown rocky forest ground, leaf litter
    "mountain_rock:Rock030"     # natural grey cliff/rock face — replaces
                                #   the wrong PavingStones004 "mountain"
    "sand:Ground033"            # light sandy beach (editor brush only)
    "mud:Ground050"             # muddy ground with puddles — riverbanks,
                                #   ford crossings (editor brush only)
    # Farm-fidelity expansion: Concord river-valley pastoral identity —
    # Dalewatch is yeoman farms + mills + walled towns, every farmhouse
    # auto-scattered by cartography sat on generic Grass before this pass.
    "cropland:Ground041"        # dirt + leaves — dry harvested field
    "pasture:Grass006"          # lawn-grass — grazed pasture, distinct
                                #   from the dense GrassLush
    "cobblestone:PavingStones070" # OLD cobblestone — farmyards, mill
                                #   yards, threshing floors (PavingStones004
                                #   stays as smooth processed paving)
    "tilled_soil:Ground026"     # smooth flat clay mud — packed earth,
                                #   ploughed-look field
)

FORCE=0
if [[ "${1:-}" == "--force" ]]; then
    FORCE=1
    echo "[force] re-downloading + re-extracting every biome"
fi

for entry in "${BIOMES[@]}"; do
    key="${entry%%:*}"
    id="${entry##*:}"
    zip="${RAW}/${id}_2K-JPG.zip"
    dir="${ASSETS}/${key}"

    if [[ $FORCE -eq 0 && -d "$dir" ]] && \
       [[ -f "$dir/${id}_2K-JPG_Color.jpg" ]]; then
        echo "[skip] $key ($id) already extracted"
        continue
    fi

    if [[ $FORCE -eq 1 || ! -f "$zip" ]]; then
        echo "[fetch] $key ← https://ambientcg.com/get?file=${id}_2K-JPG.zip"
        curl --fail --location --silent --show-error \
            --output "$zip" \
            "https://ambientcg.com/get?file=${id}_2K-JPG.zip"
    else
        echo "[cached] $zip"
    fi

    echo "[extract] $key → $dir"
    rm -rf "$dir"
    mkdir -p "$dir"
    unzip -q -o "$zip" -d "$dir"
done

echo ""
echo "done. Biomes extracted:"
for entry in "${BIOMES[@]}"; do
    key="${entry%%:*}"
    id="${entry##*:}"
    count=$(ls "$ASSETS/$key" 2>/dev/null | wc -l)
    printf "  %-10s %-18s (%d files)\n" "$key" "$id" "$count"
done
