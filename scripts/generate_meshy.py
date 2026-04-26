#!/usr/bin/env python3
"""Submit text-to-image jobs to Meshy.ai and download the rendered PNGs.

Auth: reads MESHY_API_KEY from .env at the repo root (or the environment).

Usage:

    # check auth (lists recent text-to-image tasks; doesn't spend credits)
    python3 scripts/generate_meshy.py --ping

    # one-off inline prompt
    python3 scripts/generate_meshy.py --prompt "wide establishing landscape, late-medieval Burgundian river valley..." --aspect 3:2 --count 2

    # generate from one zone's prompt: field
    python3 scripts/generate_meshy.py --zone dalewatch_marches

    # one hub
    python3 scripts/generate_meshy.py --hub harriers_rest --aspect 3:2

    # all hubs of a zone (and the zone itself)
    python3 scripts/generate_meshy.py --zone dalewatch_marches --all-hubs

Outputs land under assets/meshy/<slug>/ as image_1.png .. image_N.png plus
the raw API JSON. A CSV at assets/meshy/_log.csv records every run.
"""
from __future__ import annotations

import argparse
import csv
import json
import os
import sys
import threading
import time
import urllib.error
import urllib.request
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

import yaml

# stdout is shared across threads — every print goes through this lock so
# lines from different jobs don't interleave mid-line.
_print_lock = threading.Lock()
def tprint(*a, **kw):
    with _print_lock:
        print(*a, **kw, flush=True)

# ─── paths + config ─────────────────────────────────────────────────────
REPO = Path(__file__).resolve().parents[1]
ENV = REPO / ".env"
OUT_DIR = REPO / "assets" / "meshy"
LOG_CSV = OUT_DIR / "_log.csv"
ZONES_DIR = REPO / "src" / "generated" / "world" / "zones"
BIOMES_DIR = REPO / "src" / "generated" / "world" / "biomes"
DUNGEONS_DIR = REPO / "src" / "generated" / "world" / "dungeons"

API_BASE = "https://api.meshy.ai/openapi"
# Meshy's text-to-image endpoint. Their REST shape mirrors the text-to-3d
# endpoint: POST returns {"result": "<task_id>"}, GET <endpoint>/<task_id>
# returns the task with status / image_urls when SUCCEEDED.
TEXT_TO_IMAGE = f"{API_BASE}/v1/text-to-image"

POLL_INTERVAL_SEC = 4
POLL_TIMEOUT_SEC = 60 * 6  # 6 minutes — images usually finish in 20–60s

VALID_ASPECTS = {"1:1", "3:2", "2:3"}
VALID_COUNTS = {1, 2, 3, 4}


# ─── env loader (no external dep) ───────────────────────────────────────
def load_env(path: Path) -> dict[str, str]:
    out: dict[str, str] = {}
    if not path.exists():
        return out
    for line in path.read_text(encoding="utf-8").splitlines():
        s = line.strip()
        if not s or s.startswith("#") or "=" not in s:
            continue
        k, _, v = s.partition("=")
        out[k.strip()] = v.strip().strip("'\"")
    return out


def get_api_key() -> str:
    env_kv = load_env(ENV)
    key = os.environ.get("MESHY_API_KEY") or env_kv.get("MESHY_API_KEY")
    if not key:
        sys.exit("error: MESHY_API_KEY not found in .env or environment")
    return key


# ─── HTTP helpers (urllib only — no requests dep) ───────────────────────
def http(method: str, url: str, *, headers: dict | None = None,
         body: dict | None = None, raw: bool = False) -> dict | list | bytes:
    data = None
    h = {"Accept": "application/json"}
    if headers:
        h.update(headers)
    if body is not None:
        data = json.dumps(body).encode("utf-8")
        h["Content-Type"] = "application/json"
    req = urllib.request.Request(url, data=data, method=method, headers=h)
    try:
        with urllib.request.urlopen(req, timeout=120) as r:
            payload = r.read()
            if raw:
                return payload
            return json.loads(payload) if payload else {}
    except urllib.error.HTTPError as e:
        msg = e.read().decode("utf-8", errors="replace")
        raise RuntimeError(f"HTTP {e.code} on {method} {url}\n{msg}") from e
    except urllib.error.URLError as e:
        raise RuntimeError(f"network error on {method} {url}: {e}") from e


def auth_headers(key: str) -> dict[str, str]:
    return {"Authorization": f"Bearer {key}"}


# ─── prompt sources ─────────────────────────────────────────────────────
def slugify(s: str) -> str:
    return "".join(c if c.isalnum() else "_" for c in s.lower()).strip("_")[:60]


def find_zone_yaml(zone_id: str) -> dict:
    p = ZONES_DIR / zone_id / "core.yaml"
    if not p.exists():
        sys.exit(f"zone not found: {p}")
    with open(p) as f:
        return yaml.safe_load(f)


def find_biome_yaml(biome_id: str) -> dict:
    p = BIOMES_DIR / f"{biome_id}.yaml"
    if not p.exists():
        sys.exit(f"biome not found: {p}")
    with open(p) as f:
        return yaml.safe_load(f)


def all_biome_ids() -> list[str]:
    if not BIOMES_DIR.exists():
        return []
    return sorted(p.stem for p in BIOMES_DIR.glob("*.yaml") if not p.stem.startswith("_"))


def find_dungeon_yaml(dungeon_id: str) -> dict:
    p = DUNGEONS_DIR / dungeon_id / "core.yaml"
    if not p.exists():
        sys.exit(f"dungeon not found: {p}")
    with open(p) as f:
        return yaml.safe_load(f)


def find_dungeon_bosses(dungeon_id: str) -> list[dict]:
    p = DUNGEONS_DIR / dungeon_id / "bosses.yaml"
    if not p.exists():
        return []
    with open(p) as f:
        doc = yaml.safe_load(f) or {}
    return doc.get("bosses", []) or []


def find_boss(boss_id: str) -> dict:
    """Return the boss-doc. Searches all dungeons."""
    for entity in sorted(DUNGEONS_DIR.iterdir()):
        if not entity.is_dir() or entity.name.startswith("_"):
            continue
        bp = entity / "bosses.yaml"
        if not bp.exists():
            continue
        with open(bp) as f:
            doc = yaml.safe_load(f) or {}
        for b in doc.get("bosses", []) or []:
            if b.get("id") == boss_id:
                return b
    sys.exit(f"boss not found: {boss_id}")


def find_landmarks_for_zone(zone_id: str) -> list[dict]:
    p = ZONES_DIR / zone_id / "landmarks.yaml"
    if not p.exists():
        return []
    with open(p) as f:
        doc = yaml.safe_load(f) or {}
    return doc.get("landmarks", []) or []


def find_landmark(landmark_id: str) -> tuple[dict, str]:
    """Return (landmark-doc, zone_id). Searches all zones."""
    for zd in sorted(ZONES_DIR.iterdir()):
        if not zd.is_dir():
            continue
        lp = zd / "landmarks.yaml"
        if not lp.exists():
            continue
        with open(lp) as f:
            doc = yaml.safe_load(f) or {}
        for lm in doc.get("landmarks", []) or []:
            if lm.get("id") == landmark_id:
                return lm, zd.name
    sys.exit(f"landmark not found: {landmark_id}")


def find_hub_yaml(hub_id: str, zone_id: str | None = None) -> tuple[dict, str]:
    """Return (hub-doc, zone_id). zone_id auto-discovered if not given."""
    candidates: list[Path] = []
    if zone_id:
        candidates.append(ZONES_DIR / zone_id / "hubs" / f"{hub_id}.yaml")
    else:
        candidates.extend(ZONES_DIR.rglob(f"hubs/{hub_id}.yaml"))
    for p in candidates:
        if p.exists():
            with open(p) as f:
                return yaml.safe_load(f), p.parent.parent.name
    sys.exit(f"hub not found: {hub_id}")


# ─── core: submit + poll + download ─────────────────────────────────────
def submit_text_to_image(key: str, prompt: str, *,
                         negative: str | None = None,
                         aspect: str = "1:1",
                         count: int = 1,
                         ai_model: str = "nano-banana-pro",
                         kw_slug: str = "") -> str:
    if aspect not in VALID_ASPECTS:
        raise RuntimeError(f"invalid aspect {aspect}; expected one of {sorted(VALID_ASPECTS)}")
    if count not in VALID_COUNTS:
        raise RuntimeError(f"invalid count {count}; expected one of {sorted(VALID_COUNTS)}")
    body: dict = {
        "prompt": prompt,
        "aspect_ratio": aspect,
        "num_images": count,
        "ai_model": ai_model,
    }
    if negative:
        body["negative_prompt"] = negative
    tprint(f"  [{kw_slug}] submitting text-to-image (aspect {aspect}, x{count})...")
    res = http("POST", TEXT_TO_IMAGE, headers=auth_headers(key), body=body)
    if not isinstance(res, dict):
        raise RuntimeError(f"unexpected response shape: {type(res).__name__}: {res}")
    task_id = res.get("result") or res.get("id") or res.get("task_id")
    if not task_id:
        raise RuntimeError(f"no task id in response: {res}")
    tprint(f"  [{kw_slug}] task {task_id}")
    return task_id


def poll_task(key: str, task_id: str, *, kw_slug: str = "") -> dict:
    started = time.time()
    last_progress = -1
    url = f"{TEXT_TO_IMAGE}/{task_id}"
    while time.time() - started < POLL_TIMEOUT_SEC:
        info = http("GET", url, headers=auth_headers(key))
        if not isinstance(info, dict):
            raise RuntimeError(f"unexpected poll response shape: {info}")
        status = info.get("status", "?")
        progress = info.get("progress", 0)
        if progress != last_progress or status == "SUCCEEDED":
            tprint(f"  [{kw_slug}] [{int(time.time() - started):>3}s] {status} {progress}%")
            last_progress = progress
        if status == "SUCCEEDED":
            return info
        if status in ("FAILED", "EXPIRED", "CANCELED"):
            raise RuntimeError(f"task {task_id} ended in status {status}: "
                               f"{info.get('task_error') or info}")
        time.sleep(POLL_INTERVAL_SEC)
    raise RuntimeError(f"task {task_id} timed out after {POLL_TIMEOUT_SEC}s")


def download(url: str, dest: Path, *, kw_slug: str = "") -> int:
    dest.parent.mkdir(parents=True, exist_ok=True)
    payload = http("GET", url, raw=True)
    dest.write_bytes(payload)
    tprint(f"  [{kw_slug}] wrote {dest.relative_to(REPO)} ({len(payload):,} B)")
    return len(payload)


def extract_image_urls(info: dict) -> list[str]:
    """Meshy occasionally tweaks field names. Try the likely places."""
    urls: list[str] = []
    if isinstance(info.get("image_urls"), list):
        urls.extend(u for u in info["image_urls"] if isinstance(u, str))
    if isinstance(info.get("images"), list):
        for entry in info["images"]:
            if isinstance(entry, str):
                urls.append(entry)
            elif isinstance(entry, dict):
                u = entry.get("url") or entry.get("image_url")
                if u:
                    urls.append(u)
    if isinstance(info.get("result"), list):
        urls.extend(u for u in info["result"] if isinstance(u, str))
    return urls


def next_image_index(dest: Path) -> int:
    """Smallest N such that image_<N>.<ext> doesn't exist yet (1-indexed)."""
    if not dest.exists():
        return 1
    existing = {p.stem for p in dest.glob("image_*.*")}
    i = 1
    while f"image_{i}" in existing:
        i += 1
    return i


def save_outputs(slug: str, prompt: str, info: dict) -> Path:
    dest = OUT_DIR / slug
    dest.mkdir(parents=True, exist_ok=True)
    # Always overwrite the latest task.json + prompt.txt for the most recent run
    (dest / "task.json").write_text(json.dumps(info, indent=2))
    (dest / "prompt.txt").write_text(prompt + "\n")
    urls = extract_image_urls(info)
    if not urls:
        raise RuntimeError(
            f"no image urls in task {info.get('id')}; full task.json saved at "
            f"{(dest / 'task.json').relative_to(REPO)}"
        )
    base_idx = next_image_index(dest)
    for i, url in enumerate(urls):
        ext = ".png"
        clean = url.split("?", 1)[0].lower()
        for cand in (".png", ".jpg", ".jpeg", ".webp"):
            if clean.endswith(cand):
                ext = cand
                break
        download(url, dest / f"image_{base_idx + i}{ext}", kw_slug=slug)
    return dest


_log_lock = threading.Lock()
def append_log(slug: str, prompt: str, task_id: str, dest: Path,
               aspect: str, count: int) -> None:
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    with _log_lock:
        new = not LOG_CSV.exists()
        with open(LOG_CSV, "a", newline="") as f:
            w = csv.writer(f)
            if new:
                w.writerow(["timestamp", "slug", "task_id", "aspect", "count", "dest", "prompt"])
            w.writerow([time.strftime("%Y-%m-%dT%H:%M:%S"), slug, task_id,
                        aspect, count, str(dest.relative_to(REPO)), prompt])


# ─── high-level entry points ────────────────────────────────────────────
def run_one(key: str, slug: str, prompt: str, *,
            negative: str | None = None, aspect: str = "1:1",
            count: int = 1, ai_model: str = "nano-banana-pro") -> None:
    tprint(f"→ [{slug}] {prompt[:90]}{'…' if len(prompt) > 90 else ''}")
    task_id = submit_text_to_image(key, prompt, negative=negative,
                                   aspect=aspect, count=count,
                                   ai_model=ai_model, kw_slug=slug)
    info = poll_task(key, task_id, kw_slug=slug)
    dest = save_outputs(slug, prompt, info)
    append_log(slug, prompt, task_id, dest, aspect, count)
    tprint(f"  ✓ [{slug}] {dest.relative_to(REPO)}")


def cmd_ping(key: str) -> int:
    res = http("GET", f"{TEXT_TO_IMAGE}?page_size=5", headers=auth_headers(key))
    items = res.get("result", res) if isinstance(res, dict) else res
    print("auth OK; recent text-to-image tasks:")
    if isinstance(items, list):
        if not items:
            print("  (no prior tasks on this key yet)")
        for t in items[:5]:
            tid = (t or {}).get("id", "?")
            st = (t or {}).get("status", "?")
            pr = ((t or {}).get("prompt") or "")[:64]
            print(f"  {tid:>40}  {st:>10}  {pr}")
    else:
        print(json.dumps(items, indent=2)[:1500])
    return 0


# ─── job collection ─────────────────────────────────────────────────────
CREDITS_PER_JOB = 9  # observed for nano-banana-pro at 1:1


def slug_has_image(slug: str) -> bool:
    """True if assets/meshy/<slug>/ already has any image_*.<ext>."""
    d = OUT_DIR / slug
    if not d.exists():
        return False
    for ext in (".png", ".jpg", ".jpeg", ".webp"):
        if any(d.glob(f"image_*{ext}")):
            return True
    return False


def collect_zone_jobs(zone_id: str) -> list[tuple[str, str, str | None]]:
    z = find_zone_yaml(zone_id)
    prompt = z.get("prompt")
    neg = z.get("negative_prompt")
    # overlay prose.yaml's `zone:` block — only fills gaps
    pp = ZONES_DIR / zone_id / "prose.yaml"
    if pp.exists():
        prose = yaml.safe_load(pp.read_text(encoding="utf-8")) or {}
        zp = prose.get("zone") or {}
        prompt = prompt or zp.get("prompt")
        neg = neg or zp.get("negative_prompt")
    if prompt:
        return [(f"{zone_id}__zone", prompt, neg)]
    return []


def collect_hub_jobs(zone_id: str) -> list[tuple[str, str, str | None]]:
    out: list[tuple[str, str, str | None]] = []
    hubs_dir = ZONES_DIR / zone_id / "hubs"
    if not hubs_dir.exists():
        return out
    for hp in sorted(hubs_dir.glob("*.yaml")):
        with open(hp) as f:
            hub = yaml.safe_load(f) or {}
        if hub.get("prompt"):
            out.append((f"{zone_id}__{hub['id']}", hub["prompt"], hub.get("negative_prompt")))
    # also overlay any prose.yaml hub prompts
    pp = ZONES_DIR / zone_id / "prose.yaml"
    if pp.exists():
        prose = yaml.safe_load(pp.read_text(encoding="utf-8")) or {}
        for hid, hd in (prose.get("hubs") or {}).items():
            if hd and hd.get("prompt") and not any(slug == f"{zone_id}__{hid}" for slug, _, _ in out):
                out.append((f"{zone_id}__{hid}", hd["prompt"], hd.get("negative_prompt")))
    return out


def collect_landmark_jobs(zone_id: str) -> list[tuple[str, str, str | None]]:
    out: list[tuple[str, str, str | None]] = []
    for lm in find_landmarks_for_zone(zone_id):
        if lm.get("prompt"):
            out.append((f"{zone_id}__{lm['id']}", lm["prompt"], lm.get("negative_prompt")))
    return out


def collect_dungeon_jobs(zone_id: str | None = None) -> list[tuple[str, str, str | None]]:
    out: list[tuple[str, str, str | None]] = []
    if not DUNGEONS_DIR.exists():
        return out
    for entity in sorted(DUNGEONS_DIR.iterdir()):
        if not entity.is_dir() or entity.name.startswith("_"):
            continue
        core = entity / "core.yaml"
        if not core.exists():
            continue
        with open(core) as f:
            d = yaml.safe_load(f) or {}
        if zone_id and d.get("zone") != zone_id:
            continue
        # prose.yaml override
        prose = entity / "prose.yaml"
        prompt = d.get("prompt")
        neg = d.get("negative_prompt")
        if prose.exists():
            pd = yaml.safe_load(prose.read_text(encoding="utf-8")) or {}
            dp = pd.get("dungeon") or {}
            prompt = prompt or dp.get("prompt")
            neg = neg or dp.get("negative_prompt")
        if prompt:
            out.append((f"dungeon__{d['id']}", prompt, neg))
    return out


def collect_boss_jobs(dungeon_id: str | None = None) -> list[tuple[str, str, str | None]]:
    out: list[tuple[str, str, str | None]] = []
    for entity in sorted(DUNGEONS_DIR.iterdir()):
        if not entity.is_dir() or entity.name.startswith("_"):
            continue
        if dungeon_id and entity.name != dungeon_id:
            continue
        bp = entity / "bosses.yaml"
        bosses_yaml: dict = {}
        if bp.exists():
            bosses_yaml = {b.get("id"): b for b in
                           (yaml.safe_load(bp.read_text(encoding="utf-8")) or {}).get("bosses") or []}
        prose_path = entity / "prose.yaml"
        prose_bosses: dict = {}
        if prose_path.exists():
            prose_bosses = (yaml.safe_load(prose_path.read_text(encoding="utf-8")) or {}).get("bosses") or {}
        # union of ids
        for bid in sorted(set(bosses_yaml) | set(prose_bosses)):
            b = bosses_yaml.get(bid, {})
            pp = prose_bosses.get(bid, {}) or {}
            prompt = b.get("prompt") or pp.get("prompt")
            neg = b.get("negative_prompt") or pp.get("negative_prompt")
            if prompt:
                out.append((f"boss__{bid}", prompt, neg))
    return out


def collect_biome_jobs() -> list[tuple[str, str, str | None]]:
    out: list[tuple[str, str, str | None]] = []
    for bid in all_biome_ids():
        biome = find_biome_yaml(bid)
        if biome.get("prompt"):
            out.append((f"biome__{bid}", biome["prompt"], biome.get("negative_prompt")))
    return out


def all_zone_ids() -> list[str]:
    return sorted(p.name for p in ZONES_DIR.iterdir()
                  if p.is_dir() and not p.name.startswith("_"))


# ─── main ───────────────────────────────────────────────────────────────
def main() -> int:
    p = argparse.ArgumentParser(description=__doc__,
                                formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--ping", action="store_true", help="check auth + list a few prior tasks")
    p.add_argument("--prompt", help="inline text prompt")
    p.add_argument("--slug", help="output dir slug (defaults to slugified prompt)")
    p.add_argument("--negative", help="negative prompt (optional)")
    p.add_argument("--aspect", default="1:1", choices=sorted(VALID_ASPECTS))
    p.add_argument("--count", type=int, default=1, choices=sorted(VALID_COUNTS),
                   help="number of images per job (1-4)")
    p.add_argument("--model", default="nano-banana-pro", help="Meshy AI model name")

    # selectors — single
    p.add_argument("--zone", help="zone id (single zone image, or scope for bulk flags)")
    p.add_argument("--hub", help="hub id (single hub)")
    p.add_argument("--landmark", help="landmark id (single landmark)")
    p.add_argument("--dungeon", help="dungeon id (single dungeon, or scope for --all-bosses)")
    p.add_argument("--boss", help="boss id (single boss)")
    p.add_argument("--biome", help="biome id (single biome)")

    # selectors — bulk (all are scoped by --zone/--dungeon when set)
    p.add_argument("--all-zones", action="store_true", help="every zone with a prompt")
    p.add_argument("--all-hubs", action="store_true",
                   help="every hub (scoped to --zone if set)")
    p.add_argument("--all-landmarks", action="store_true",
                   help="every landmark (scoped to --zone if set)")
    p.add_argument("--all-dungeons", action="store_true",
                   help="every dungeon (scoped to --zone if set)")
    p.add_argument("--all-bosses", action="store_true",
                   help="every boss (scoped to --dungeon if set)")
    p.add_argument("--all-biomes", action="store_true", help="every biome with a prompt")
    p.add_argument("--all", dest="all_everything", action="store_true",
                   help="zones + hubs + landmarks + dungeons + bosses + biomes (all of them)")

    # behaviour
    p.add_argument("--skip-existing", dest="skip_existing", action="store_true", default=None,
                   help="skip slugs that already have an image (default: ON in any --all-* mode)")
    p.add_argument("--no-skip-existing", dest="skip_existing", action="store_false",
                   help="overwrite-into-next-slot even when an image already exists")
    p.add_argument("--dry-run", action="store_true", help="show planned jobs and credit estimate; submit nothing")
    p.add_argument("--workers", type=int, default=8,
                   help="parallel job workers (default: 8)")

    args = p.parse_args()
    key = get_api_key()
    if args.ping:
        return cmd_ping(key)

    # Expand --all into the bulk flags
    if args.all_everything:
        args.all_zones = True
        args.all_hubs = True
        args.all_landmarks = True
        args.all_dungeons = True
        args.all_bosses = True
        args.all_biomes = True

    in_bulk_mode = any((args.all_everything, args.all_zones, args.all_hubs,
                       args.all_landmarks, args.all_dungeons, args.all_bosses,
                       args.all_biomes))
    skip_existing = args.skip_existing if args.skip_existing is not None else in_bulk_mode

    jobs: list[tuple[str, str, str | None]] = []

    # ─── inline prompt ───
    if args.prompt:
        jobs.append((args.slug or slugify(args.prompt), args.prompt, args.negative))

    # ─── single selectors ───
    if args.hub:
        hub, zid = find_hub_yaml(args.hub)
        if not hub.get("prompt"):
            sys.exit(f"hub {args.hub} has no prompt: field")
        jobs.append((f"{zid}__{args.hub}", hub["prompt"], hub.get("negative_prompt")))

    if args.landmark:
        lm, zid = find_landmark(args.landmark)
        if not lm.get("prompt"):
            sys.exit(f"landmark {args.landmark} has no prompt: field")
        jobs.append((f"{zid}__{args.landmark}", lm["prompt"], lm.get("negative_prompt")))

    if args.dungeon and not (args.all_bosses):
        d = find_dungeon_yaml(args.dungeon)
        if d.get("prompt"):
            jobs.append((f"dungeon__{args.dungeon}", d["prompt"], d.get("negative_prompt")))

    if args.boss:
        b = find_boss(args.boss)
        if not b.get("prompt"):
            sys.exit(f"boss {args.boss} has no prompt: field")
        jobs.append((f"boss__{args.boss}", b["prompt"], b.get("negative_prompt")))

    if args.biome:
        biome = find_biome_yaml(args.biome)
        if not biome.get("prompt"):
            sys.exit(f"biome {args.biome} has no prompt: field")
        jobs.append((f"biome__{args.biome}", biome["prompt"], biome.get("negative_prompt")))

    # ─── zone-scoped or full bulk ───
    zones_in_scope = [args.zone] if args.zone else all_zone_ids()

    if args.zone and not in_bulk_mode and not (args.hub or args.landmark):
        # plain --zone with no bulk flag: just the zone-level image
        jobs.extend(collect_zone_jobs(args.zone))

    if args.all_zones:
        for zid in zones_in_scope:
            jobs.extend(collect_zone_jobs(zid))
    if args.all_hubs:
        for zid in zones_in_scope:
            jobs.extend(collect_hub_jobs(zid))
    if args.all_landmarks:
        for zid in zones_in_scope:
            jobs.extend(collect_landmark_jobs(zid))
    if args.all_dungeons:
        # scoped to --zone if set, otherwise every dungeon everywhere
        if args.zone:
            jobs.extend(collect_dungeon_jobs(args.zone))
        else:
            jobs.extend(collect_dungeon_jobs(None))
    if args.all_bosses:
        # scoped to --dungeon if set, otherwise every boss everywhere
        jobs.extend(collect_boss_jobs(args.dungeon))
    if args.all_biomes:
        jobs.extend(collect_biome_jobs())

    # ─── dedupe slugs (preserve first occurrence) ───
    seen: set[str] = set()
    unique_jobs: list[tuple[str, str, str | None]] = []
    for slug, prompt, neg in jobs:
        if slug in seen:
            continue
        seen.add(slug)
        unique_jobs.append((slug, prompt, neg))
    jobs = unique_jobs

    # ─── filter skip-existing ───
    skipped: list[str] = []
    if skip_existing:
        kept: list[tuple[str, str, str | None]] = []
        for slug, prompt, neg in jobs:
            if slug_has_image(slug):
                skipped.append(slug)
            else:
                kept.append((slug, prompt, neg))
        jobs = kept

    if not jobs and not skipped:
        p.print_help()
        return 1

    total_credits = len(jobs) * args.count * CREDITS_PER_JOB
    print(f"planned {len(jobs)} job(s)  ·  aspect {args.aspect}  ·  count {args.count}  "
          f"·  est {total_credits} credits")
    if skipped:
        print(f"  ({len(skipped)} skipped — already have images; rerun with --no-skip-existing to redo)")
    for slug, prompt, neg in jobs[:30]:
        print(f"  - {slug}: {prompt[:72]}{'…' if len(prompt) > 72 else ''}")
    if len(jobs) > 30:
        print(f"  ... and {len(jobs) - 30} more")
    if args.dry_run:
        print("\ndry-run: nothing submitted")
        return 0

    print()
    workers = max(1, min(args.workers, len(jobs)))
    if workers == 1:
        # sequential path — keeps stack traces simple when debugging
        failed = 0
        for slug, prompt, neg in jobs:
            try:
                run_one(key, slug, prompt, negative=neg,
                        aspect=args.aspect, count=args.count, ai_model=args.model)
            except Exception as e:  # noqa: BLE001
                failed += 1
                tprint(f"  ✗ [{slug}] {e}")
        return 1 if failed else 0

    tprint(f"running {len(jobs)} jobs across {workers} workers")
    failed = 0
    with ThreadPoolExecutor(max_workers=workers) as pool:
        futs = {
            pool.submit(run_one, key, slug, prompt,
                        negative=neg, aspect=args.aspect, count=args.count,
                        ai_model=args.model): slug
            for slug, prompt, neg in jobs
        }
        for fut in as_completed(futs):
            slug = futs[fut]
            try:
                fut.result()
            except Exception as e:  # noqa: BLE001
                failed += 1
                tprint(f"  ✗ [{slug}] {e}")
    tprint(f"\ndone: {len(jobs) - failed} succeeded, {failed} failed")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
