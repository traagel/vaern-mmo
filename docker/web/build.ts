// Bun-driven build step for the Vaern compendium static site.
//
// Pipeline:
//  1. Validate web/data.json (parses; build fails on syntax error).
//  2. Minify web/app.js into dist/app.js via Bun.build.
//  3. Resize + transcode every image under icons/, emblems/, characters/,
//     assets/meshy/ to WebP. Output mirrors the input tree but with .webp
//     extensions, keyed by per-tree size profiles.
//  4. Rewrite ".png" → ".webp" in dist/app.js and dist/data.json so the
//     browser fetches the optimized files.
//
// Inputs at ./web/, ./icons/, ./emblems/, ./characters/, ./assets/meshy/.
// Output at ./dist/.

import { $ } from "bun";
import {
  readdirSync,
  statSync,
  mkdirSync,
  copyFileSync,
  existsSync,
  rmSync,
  writeFileSync,
  readFileSync,
  renameSync,
} from "node:fs";
import { join, dirname, relative } from "node:path";

const DIST = "./dist";

// ── site-identity overrides via env (set from Dockerfile ARGs) ──
//
// BASE_PATH:     /lexi-returns/     leading + trailing slash, "/" = no prefix
// SITE_TITLE:    NEW WORLD 2: LEXI RETURNS   uppercase wordmark text
// SITE_PRETTY:   New World 2: Lexi Returns   prose-form (used in <title>, data.json)
// SITE_SUBTITLE: Compendium · Two factions · One island   compendium tagline
const RAW_BASE = process.env.BASE_PATH || "/";
const NORM_BASE = RAW_BASE.endsWith("/") ? RAW_BASE : RAW_BASE + "/";
const FS_PREFIX = NORM_BASE === "/" ? "" : NORM_BASE.replace(/^\//, "").replace(/\/$/, "");
const SITE_TITLE = process.env.SITE_TITLE || "VAERN";
const SITE_PRETTY = process.env.SITE_PRETTY || "Vaern";
const SITE_SUBTITLE =
  process.env.SITE_SUBTITLE || "Compendium · Two factions · One island";
const SITE_TAGLINE_SPLASH =
  process.env.SITE_TAGLINE_SPLASH || "Two factions. One island.";

if (existsSync(DIST)) rmSync(DIST, { recursive: true, force: true });
mkdirSync(DIST, { recursive: true });

// ── 1. data.json validation ──
const dataPath = "./web/data.json";
const dataRaw = await Bun.file(dataPath).text();
try {
  JSON.parse(dataRaw);
} catch (e) {
  console.error(`✗ data.json failed to parse: ${(e as Error).message}`);
  process.exit(1);
}
const dataKB = (statSync(dataPath).size / 1024).toFixed(1);
console.log(`✓ data.json valid (${dataKB} KB)`);

// ── 2. app.js minify ──
const buildResult = await Bun.build({
  entrypoints: ["./web/app.js"],
  outdir: DIST,
  minify: true,
  target: "browser",
  naming: "[name].[ext]",
});
if (!buildResult.success) {
  for (const lg of buildResult.logs) console.error(lg);
  console.error("✗ app.js minify failed");
  process.exit(1);
}
const minSize = statSync(`${DIST}/app.js`).size;
const origSize = statSync("./web/app.js").size;
console.log(
  `✓ app.js minified  ${(origSize / 1024).toFixed(1)} KB → ` +
    `${(minSize / 1024).toFixed(1)} KB (${((minSize / origSize) * 100).toFixed(0)}%)`,
);

// ── transform-and-copy: every web/*.html with site-identity overrides ──
function transformHtml(text: string): string {
  let out = text;

  // Inject <base href="…"> right after <head> when serving under a prefix,
  // so JS-relative paths (fetch("data.json"), '<img src="icons/…">') resolve
  // under the prefix without having to rewrite app.js paths.
  if (NORM_BASE !== "/") {
    out = out.replace(
      /<head>/,
      `<head>\n<base href="${NORM_BASE}">`,
    );
  }

  // <title> tag — preserve "Compendium" suffix on the data-browser page,
  // collapse to bare site-pretty-name on the splash.
  out = out.replace(/<title>[^<]*<\/title>/, (m) => {
    const isCompendium = /Compendium/i.test(m);
    return isCompendium
      ? `<title>${SITE_PRETTY} — Compendium</title>`
      : `<title>${SITE_PRETTY} — A hardcore co-op MMO</title>`;
  });

  // Wordmark (splash + compendium header + compendium overview hero)
  out = out
    .replaceAll(`<h1>VAERN</h1>`, `<h1>${SITE_TITLE}</h1>`)
    .replaceAll(
      `<h2 id="overview-title">VAERN</h2>`,
      `<h2 id="overview-title">${SITE_TITLE}</h2>`,
    );

  // Tagline strings
  out = out
    .replace(
      /<div class="tagline">Compendium · Two factions · One island<\/div>/,
      `<div class="tagline">${SITE_SUBTITLE}</div>`,
    )
    .replace(
      /<p class="tagline">Two factions\. One island\.<\/p>/,
      `<p class="tagline">${SITE_TAGLINE_SPLASH}</p>`,
    );

  return out;
}

for (const ent of readdirSync("./web")) {
  if (ent.endsWith(".html")) {
    const src = readFileSync(`./web/${ent}`, "utf-8");
    writeFileSync(`${DIST}/${ent}`, transformHtml(src));
  }
}
copyFileSync("./web/styles.css", `${DIST}/styles.css`);

// ── 3. image optimization ──
//
// Profiles encode per-asset-tree max long side + WebP quality. The numbers
// match what the renderer actually displays on a retina screen, with a
// small headroom for hover-zoom and detail pages.

interface Profile {
  dir: string;
  max: number;
  quality: number;
}

const PROFILES: Profile[] = [
  { dir: "icons", max: 256, quality: 82 }, // rendered ~80px in the spell grid
  { dir: "emblems", max: 384, quality: 85 }, // rendered ~120px in the school cards
  { dir: "characters", max: 768, quality: 82 }, // legacy SDXL portraits, hero ~280px
  { dir: "assets/meshy", max: 1024, quality: 82 }, // zone heroes 360px + variant strips
];

const IMG_EXT = new Set([".png", ".jpg", ".jpeg"]);

function* walkImages(root: string): Generator<string> {
  if (!existsSync(root)) return;
  for (const ent of readdirSync(root, { withFileTypes: true })) {
    const p = join(root, ent.name);
    if (ent.isSymbolicLink()) continue; // skip web/* symlinks if they ever leak in
    if (ent.isDirectory()) yield* walkImages(p);
    else if (ent.isFile()) {
      const ext = p.slice(p.lastIndexOf(".")).toLowerCase();
      if (IMG_EXT.has(ext) || ext === ".webp") yield p;
    }
  }
}

const WORKERS = 8;
async function pool<T>(items: T[], worker: (item: T) => Promise<void>) {
  const queue = [...items];
  await Promise.all(
    Array(WORKERS)
      .fill(0)
      .map(async () => {
        while (queue.length) {
          const item = queue.shift();
          if (item !== undefined) await worker(item);
        }
      }),
  );
}

const t0 = Date.now();
let totalIn = 0;
let totalOut = 0;
let totalFiles = 0;

for (const { dir, max, quality } of PROFILES) {
  const inRoot = `./${dir}`;
  const outRoot = `${DIST}/${dir}`;
  if (!existsSync(inRoot)) {
    console.log(`  ${dir}: input missing, skipping`);
    continue;
  }
  mkdirSync(outRoot, { recursive: true });
  const files = [...walkImages(inRoot)];
  if (!files.length) continue;

  let dirIn = 0;
  let dirOut = 0;
  await pool(files, async (src) => {
    const rel = relative(inRoot, src);
    const ext = src.slice(src.lastIndexOf(".")).toLowerCase();
    const dst = join(
      outRoot,
      rel.replace(/\.(png|jpe?g|webp)$/i, ".webp"),
    );
    mkdirSync(dirname(dst), { recursive: true });

    if (ext === ".webp") {
      // already webp — copy through (still resize? we keep it as-is to avoid recompressing)
      copyFileSync(src, dst);
    } else {
      // cwebp resizes via -resize MAX 0 (max-width, preserve aspect). All
      // current sources are square or portrait so this caps the long side.
      await $`cwebp -quiet -q ${quality} -resize ${max} 0 ${src} -o ${dst}`;
    }
    dirIn += statSync(src).size;
    dirOut += statSync(dst).size;
  });

  totalIn += dirIn;
  totalOut += dirOut;
  totalFiles += files.length;
  console.log(
    `  ${dir.padEnd(14)}  ${String(files.length).padStart(4)} files  ` +
      `${(dirIn / 1024 / 1024).toFixed(0).padStart(4)} MB → ` +
      `${(dirOut / 1024 / 1024).toFixed(0).padStart(4)} MB  ` +
      `(${((dirOut / dirIn) * 100).toFixed(0)}%)`,
  );
}

const elapsed = ((Date.now() - t0) / 1000).toFixed(1);
console.log(
  `✓ ${totalFiles} images optimized: ` +
    `${(totalIn / 1024 / 1024).toFixed(0)} MB → ` +
    `${(totalOut / 1024 / 1024).toFixed(0)} MB ` +
    `(${((totalOut / totalIn) * 100).toFixed(0)}%) in ${elapsed}s`,
);

// ── 4. rewrite asset references + apply site-identity overrides ──
//
// The renderer + data.json reference .png paths; after transcoding we point
// everything at the .webp twins. Word-boundary anchor (\b) keeps us from
// touching strings like "image.pngabc" (none exist today, but be safe).

function pngToWebp(text: string): string {
  return text.replace(/\.png\b/gi, ".webp");
}

// app.js: png → webp + override the literal 'VAERN' fallback (line ~214).
const appPath = `${DIST}/app.js`;
let appText = pngToWebp(readFileSync(appPath, "utf-8"));
if (SITE_TITLE !== "VAERN") {
  appText = appText
    .replaceAll(`'VAERN'`, JSON.stringify(SITE_TITLE))
    .replaceAll(`"VAERN"`, JSON.stringify(SITE_TITLE));
}
writeFileSync(appPath, appText);

// data.json: png → webp + override world.setting_name (drives the
// compendium overview's heading at runtime via app.js).
const dataObj = JSON.parse(dataRaw);
if (SITE_PRETTY !== "Vaern" && dataObj.world) {
  dataObj.world.setting_name = SITE_PRETTY;
}
const newData = pngToWebp(JSON.stringify(dataObj));
JSON.parse(newData); // sanity-parse the rewritten JSON
writeFileSync(`${DIST}/data.json`, newData);

console.log("✓ rewrote .png → .webp in app.js and data.json");
if (SITE_TITLE !== "VAERN") {
  console.log(`✓ site identity: ${SITE_PRETTY} (wordmark "${SITE_TITLE}")`);
}

// ── 5. relocate under BASE_PATH if set ──
//
// We've been building into ./dist throughout. If the deployment lives at
// /lexi-returns/, we move everything into ./dist/lexi-returns/ and write a
// meta-refresh redirect at the bare ./dist/index.html so a request to /
// (via reverse proxy or default nginx index handling) auto-bounces to the
// real entry point.
if (FS_PREFIX) {
  const STAGED = `${DIST}.staged`;
  if (existsSync(STAGED)) rmSync(STAGED, { recursive: true, force: true });
  renameSync(DIST, STAGED);
  mkdirSync(DIST);
  renameSync(STAGED, `${DIST}/${FS_PREFIX}`);

  const redirectHtml = [
    `<!DOCTYPE html>`,
    `<html lang="en"><head>`,
    `<meta charset="UTF-8">`,
    `<title>${SITE_PRETTY}</title>`,
    `<meta http-equiv="refresh" content="0; url=${NORM_BASE}">`,
    `<meta name="robots" content="noindex">`,
    `</head><body>`,
    `<p>Redirecting to <a href="${NORM_BASE}">${NORM_BASE}</a>…</p>`,
    `</body></html>`,
  ].join("\n");
  writeFileSync(`${DIST}/index.html`, redirectHtml);

  console.log(`✓ relocated under ${NORM_BASE} (${DIST}/${FS_PREFIX}/)`);
  console.log(`✓ root redirect → ${NORM_BASE}`);
}

console.log(`build complete → ${DIST}`);
