# syntax=docker/dockerfile:1.6
#
# Vaern compendium web image.
#
# Stage 1 (Bun + cwebp): validates web/data.json, minifies app.js, and
# transcodes every PNG/JPG under icons/, emblems/, characters/, assets/meshy/
# to WebP at sensible per-tree dimensions. Source images on disk total
# ~1.7 GB; the optimized output is roughly 150–200 MB. The .png references
# in app.js + data.json are rewritten to .webp by the same pass.
#
# Stage 2 (nginx): serves the optimized bundle. Nothing else is shipped —
# the heavy source PNGs never enter the final image.
#
# Build (from the repo root):
#     docker build -f docker/web/Dockerfile -t traagel/vaern-mmo-web:latest .

# ─── Stage 1: Bun builder + cwebp ────────────────────────────────────────
FROM oven/bun:1-alpine AS builder
RUN apk add --no-cache libwebp-tools
WORKDIR /work

# Site-identity overrides — pass via --build-arg to spin a parody/sister
# variant. Defaults reproduce the canonical Vaern build.
#   --build-arg BASE_PATH=/lexi-returns/
#   --build-arg SITE_TITLE="NEW WORLD 2: LEXI RETURNS"
#   --build-arg SITE_PRETTY="New World 2: Lexi Returns"
#   --build-arg SITE_SUBTITLE="Compendium · Lexi · Returns"
#   --build-arg SITE_TAGLINE_SPLASH="The sequel nobody asked for."
ARG BASE_PATH=/
ARG SITE_TITLE=VAERN
ARG SITE_PRETTY=Vaern
ARG SITE_SUBTITLE="Compendium · Two factions · One island"
ARG SITE_TAGLINE_SPLASH="Two factions. One island."
ENV BASE_PATH=$BASE_PATH \
    SITE_TITLE=$SITE_TITLE \
    SITE_PRETTY=$SITE_PRETTY \
    SITE_SUBTITLE=$SITE_SUBTITLE \
    SITE_TAGLINE_SPLASH=$SITE_TAGLINE_SPLASH

# Static site (HTML pages + JS/CSS/data; both index.html splash and
# compendium.html data browser get staged through the builder).
COPY web/index.html      ./web/index.html
COPY web/compendium.html ./web/compendium.html
COPY web/styles.css      ./web/styles.css
COPY web/data.json       ./web/data.json
COPY web/app.js          ./web/app.js

# Asset trees — each is its own COPY so a change in one doesn't bust the
# others' layer cache.
COPY icons        ./icons
COPY emblems      ./emblems
COPY characters   ./characters
COPY assets/meshy ./assets/meshy

# Build script
COPY docker/web/build.ts ./build.ts

# Validate + minify + optimize → /work/dist
RUN bun run build.ts

# ─── Stage 2: nginx static server ────────────────────────────────────────
FROM nginx:alpine

# Drop the default config and replace with our gzip + cache rules.
RUN rm /etc/nginx/conf.d/default.conf
COPY docker/web/nginx.conf /etc/nginx/conf.d/default.conf

# The optimized bundle is the only thing the final image carries.
COPY --from=builder /work/dist /usr/share/nginx/html

EXPOSE 80
