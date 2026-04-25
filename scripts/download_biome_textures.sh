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
