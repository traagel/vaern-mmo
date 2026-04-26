#!/usr/bin/env bash
#
# Build + push a Vaern web image to Docker Hub.
#
# Usage:
#     scripts/push-web.sh                          # vaern variant, tag latest
#     scripts/push-web.sh v0.4.0                   # vaern, tag v0.4.0 + latest
#     scripts/push-web.sh --variant lexi           # parody build, served at /lexi-returns/
#     scripts/push-web.sh --variant lexi v0.1.0    # parody, tag v0.1.0 + latest
#     scripts/push-web.sh --no-push                # build only
#     scripts/push-web.sh --no-cache               # force clean rebuild
#
# Variants (the `--variant` flag selects identity overrides):
#     vaern   (default)  → traagel/vaern-mmo-web         · base /
#     lexi               → traagel/vaern-mmo-web-lexi    · base /lexi-returns/
#                          wordmark: "NEW WORLD 2: LEXI RETURNS"
#
# Requires: docker CLI logged in (`docker login -u traagel`).
#
# Env overrides:
#     IMAGE=…                  # override the repo name for any variant
#     PLATFORMS=linux/amd64,linux/arm64  # buildx target platforms
set -euo pipefail

VARIANT="vaern"
NO_PUSH=0
NO_CACHE=0
TAG=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --variant)  VARIANT="${2:?--variant requires a value}"; shift 2 ;;
        --variant=*) VARIANT="${1#--variant=}"; shift ;;
        --no-push)  NO_PUSH=1; shift ;;
        --no-cache) NO_CACHE=1; shift ;;
        -h|--help)
            sed -n '2,22p' "$0" | sed 's/^# \?//'
            exit 0 ;;
        -*) echo "unknown flag: $1" >&2; exit 2 ;;
        *)  TAG="$1"; shift ;;
    esac
done
TAG="${TAG:-latest}"

# ── variant config ──
case "$VARIANT" in
    vaern)
        DEFAULT_IMAGE="traagel/vaern-mmo-web"
        BUILD_ARGS=()  # all defaults
        ;;
    lexi)
        DEFAULT_IMAGE="traagel/vaern-mmo-web-lexi"
        BUILD_ARGS=(
            --build-arg "BASE_PATH=/lexi-returns/"
            --build-arg "SITE_TITLE=NEW WORLD 2: LEXI RETURNS"
            --build-arg "SITE_PRETTY=New World 2: Lexi Returns"
            --build-arg "SITE_SUBTITLE=Compendium · Lexi · Returns"
            --build-arg "SITE_TAGLINE_SPLASH=The sequel everybody asked for."
        )
        ;;
    *)
        echo "✗ unknown variant: $VARIANT  (expected: vaern, lexi)" >&2
        exit 2 ;;
esac

IMAGE="${IMAGE:-$DEFAULT_IMAGE}"
PLATFORMS="${PLATFORMS:-linux/amd64,linux/arm64}"

# ── locate repo root ──
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

# ── pre-flight ──
if [[ ! -f web/data.json ]]; then
    echo "✗ web/data.json missing — run scripts/build_web_data.py first" >&2
    exit 1
fi

# ── tags to apply ──
TAGS=("$IMAGE:$TAG")
if [[ "$TAG" != "latest" ]]; then
    TAGS+=("$IMAGE:latest")
fi
TAG_ARGS=()
for t in "${TAGS[@]}"; do
    TAG_ARGS+=("-t" "$t")
done

CACHE_ARGS=()
[[ $NO_CACHE -eq 1 ]] && CACHE_ARGS+=("--no-cache")

# ── multi-arch via buildx (requires push, since multi-arch images can't be
# loaded into the local docker daemon by default) ──
USE_BUILDX=0
if [[ "$PLATFORMS" == *,* ]] || [[ "$NO_PUSH" -eq 0 ]]; then
    if docker buildx inspect >/dev/null 2>&1; then
        USE_BUILDX=1
    fi
fi

echo
echo "── web image build ──"
echo "  variant:   $VARIANT"
echo "  image:     $IMAGE"
echo "  tag(s):    ${TAGS[*]}"
echo "  platforms: $PLATFORMS"
echo "  push:      $([[ $NO_PUSH -eq 1 ]] && echo no || echo yes)"
echo "  buildx:    $([[ $USE_BUILDX -eq 1 ]] && echo yes || echo no)"
if (( ${#BUILD_ARGS[@]} > 0 )); then
    echo "  build-args:"
    for ba in "${BUILD_ARGS[@]}"; do [[ "$ba" != "--build-arg" ]] && echo "    $ba"; done
fi
echo

if [[ $USE_BUILDX -eq 1 ]]; then
    PUSH_FLAG=()
    [[ $NO_PUSH -eq 0 ]] && PUSH_FLAG+=("--push") || PUSH_FLAG+=("--load")
    # buildx --load only works for single-platform builds
    if [[ $NO_PUSH -eq 1 && "$PLATFORMS" == *,* ]]; then
        echo "✗ multi-arch + --no-push isn't possible (docker daemon can't"   >&2
        echo "  load multi-arch indexes). Either drop multi-arch with"        >&2
        echo "  PLATFORMS=linux/amd64 ./scripts/push-web.sh --no-push, or"    >&2
        echo "  push instead."                                                >&2
        exit 2
    fi
    docker buildx build \
        --platform "$PLATFORMS" \
        -f docker/web/Dockerfile \
        "${BUILD_ARGS[@]}" \
        "${TAG_ARGS[@]}" \
        "${CACHE_ARGS[@]}" \
        "${PUSH_FLAG[@]}" \
        .
else
    docker build \
        -f docker/web/Dockerfile \
        "${BUILD_ARGS[@]}" \
        "${TAG_ARGS[@]}" \
        "${CACHE_ARGS[@]}" \
        .
    if [[ $NO_PUSH -eq 0 ]]; then
        for t in "${TAGS[@]}"; do
            docker push "$t"
        done
    fi
fi

echo
echo "✓ done"
for t in "${TAGS[@]}"; do
    echo "  $t"
done
