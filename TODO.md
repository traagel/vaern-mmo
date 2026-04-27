# Vaern TODO

Forward-looking slice list. For the full current state see `README.md`; for design rationale see `memory/`. For the ratified pre-alpha plan see `~/.claude/plans/set-and-prioritze-goals-delightful-mochi.md`.

---

## Map editor (post-Slice-9)

`vaern-editor` is a standalone Bevy binary at `crates/vaern-editor/`, sibling of `vaern-client`. Source-of-truth authoring for the same YAML + voxel data the runtime reads.

Implemented (run `cargo run --release -p vaern-editor -- --zone dalewatch_marches`):

- **Free-fly camera** + camera-relative chunk streaming with despawn-based eviction (`voxel/render_opt.rs::evict_chunks_outside_draw_distance` despawns out-of-range entities, `pending_respawn: HashSet<ChunkCoord>` tracks despawned coords for reliable re-spawn on return — never re-marks always-empty chunks).
- **HDR atmospheric pipeline matching the runtime client**: camera carries `Hdr + Tonemapping::TonyMcMapface + Bloom::NATURAL + Exposure::SUNLIGHT + Atmosphere::earthlike + DistanceFog`. Editor preview = runtime preview.
- **Environment panel** (left SidePanel) — time-of-day slider 0–24h with autoplay (configurable seconds/game-hour); fog enable + visibility + falloff mode + auto-or-manual color; atmosphere toggle; **draw distance slider 1–64 chunks** (logarithmic, ~32m–2km radius) with km/m readout + chunk count + perf warning.
- **Voxel sculpt toolkit (8 tools)**: Sphere / Smooth / Flatten / Ramp / Reset / Cylinder / Box / Stamp. Continuous drag stroke, falloff curves (Hard / Linear / Smooth), X/Z mirror plane with configurable origin, 4-slot brush presets (Ctrl+1..4 load, Ctrl+Shift+N save), Shift/Ctrl scroll-modifier for fine/coarse radius. Undo/redo (Ctrl+Z / Ctrl+Shift+Z) with twin ring buffers per-stroke. New brush types live in `vaern-voxel::edit` (Cylinder primitive, Falloff trait extension, FlattenBrush, RampBrush, ResetBrush, StampBrush, SmoothStroke).
- **Biome paint mode** — chunk-aligned XZ painting, 9-biome palette + 0–4 chunk-radius brush. `BiomeOverrideMap` (XZ → BiomeKey) persists to `src/generated/world/biome_overrides.bin` (bincode, deterministic ordering). Material swap is direct on chunk entities via `commands.entity(e).insert(MeshMaterial3d(handle))` because `attach_biome_material`'s `Added<ChunkRenderTag>` filter only fires once per entity.
- **Place mode** — palette-selected Poly Haven slug + LMB on ground spawns a new prop into the nearest hub.
- **Select / inspect / edit / delete** — LMB picks prop via scene-mesh AABB raycast.
- **Save → hub YAMLs + biome overrides + voxel edits** — `props:` array spliced into each touched hub's YAML via `serde_yaml::Value` (preserves all other fields). Round-trips into runtime client via the existing `vaern-client/src/scene/dressing.rs` reader.
- **Bundle splitting on disk** — `scripts/split_polyhaven_bundle.py` peels multi-mesh glTFs into per-piece glTFs sharing the original `.bin` + textures. Already run on `modular_fort_01` (22 piece slugs in catalog).
- **`scripts/wipe_dressing.py`** — strips `props:` from every hub YAML + `scatter:` from every zone core YAML + deletes `voxel_edits.bin`. Used to reset the world to a blank canvas for re-authoring.
- **Texture quality**: ground materials load normal + AO maps as **linear** (`is_srgb = false`) — sRGB decoding had warped the normal vectors and read as static-noise lighting. Sampler now uses `anisotropy_clamp: 16` + Repeat addressing; tile size 24u. Ground material is `perceptual_roughness: 1.0` (matte). `NotShadowCaster` on every chunk so the directional sun's cascade pass skips them.
- **Heightfield**: `EditorHeightfield` is a flat plane at `GROUND_BIAS_Y = 0.5` (half-voxel offset deliberate — places the iso-surface mid-cube so surface nets extracts a clean horizontal slab; values like `0.0` would land exactly on chunk-Y boundaries and produce degenerate vertices). No voronoi biome resolver in the editor — every unpainted chunk uses `BiomeKey::Marsh`. `BiomeOverrideMap` overrides per XZ.
- **FPS counter** in the toolbar — driven by `bevy::diagnostic::FrameTimeDiagnosticsPlugin` registered in `EditorPlugin`.

Stubbed slots (file scaffold + plugin shells): scatter preview mode, transform gizmo.

**Performance notes**: at 12-chunk draw distance (~384m radius, 625 surface chunks) → ~35 FPS on a Ryzen 9 7950X3D + RX 7900 XTX in release mode. Frame time scales O(N²) with draw distance (one draw call per chunk; no LOD). The cap was raised to 64 chunks (~2km radius) but expect <2 FPS at the top end. Real "big landscape" rendering needs LOD / GPU instancing / async meshing — separate slice.

---

## Where we are today (2026-04-26)

Pre-alpha-shaped scaffold. Menu → login/register against the server-side SQLite account store → char create → **PBR-dressed Dalewatch** with scattered trees / rocks / shrubs + ~55 authored hub props → side-quest givers populated → mob level bands by hub → walk to NPC, F-press, **read authored turn-in dialogue + click contextual button** (talk/deliver) or F-press a cyan `?` waypoint marker (investigate) → kill mobs (XP scales by mob-vs-killer level: greys=0, +5 reds=1.5×; multi-kill objectives track `2/3` in the tracker) → level up (centered banner + screen flash + +1 pillar point auto-granted) → die → 25% HP at home, walk back to corpse for full restore → /wave at your friend → ride east of Ford of Ashmere to **Drifter's Lair** (zone-local 470, 80) → 4-mob pulls of L8-L10 drifters → kill **Halen** mini-boss (L9) and **Valenn** capstone boss (L10) → if grouped, vote Need/Greed/Pass on the mithril/dragonscale/shadowsilk + exceptional drops in a centered modal → repeat. Server bounce mid-session: client auto-reconnects with exponential backoff, replays cached credentials, resumes the session without a re-prompt. Multi-client, server-authoritative, zone-AoI replicated, **374 tests green** (4 pre-existing combat-test failures unchanged). Slice 6 is **code-complete + tests green; 2-client playtest pending.**

**Post-Slice-6 polish (2026-04-26):** retired legacy `scene/hub_regions.rs` (file deleted, mod dropped); animation pipeline grew motion-direction-aware locomotion (back-pedal via reversed `Walk_Loop` / `Jog_Fwd_Loop` since UAL has no `Walk_Bwd_Loop`), generic `Idle_Shield_Loop` block stance replacing sword-specific `Sword_Block`, sword combo rotation on physical attacks (`Sword_Attack` / `Regular_A` / `_B` / `_C`), `Spell_Simple_Shoot` magic resolve, and `Hit_Knockback` for damage ≥ 35 — all wired through a new `AnimContext` resource populated by `enrich_anim_context_from_cast_fired`. Camera `follow_camera` now ground-clamps via `vaern_voxel::query::ground_y` so steep-tilt angles can't push the camera through terrain or into a server-carved crater. Sideways strafe gap remains — UAL ships no `Walk_Left/Right` clips.

The pre-alpha goal hierarchy plan at `~/.claude/plans/set-and-prioritze-goals-delightful-mochi.md` decomposes into ~8 slices. **Status as of 2026-04-25**:

- ✅ Slice 1a — Poly Haven downloader (`scripts/download_polyhaven.py` with UA + texture-URL-rewrite fixes)
- ✅ Slice 1b — `PolyHavenCatalog` resource in `vaern-assets`
- ✅ Slice 1c — Zone YAML `scatter:` + hub `props:` schema + Dalewatch seed
- ✅ Slice 1d+1e — Scatter algorithm + runtime in `crates/vaern-client/src/scene/dressing.rs`
- ⏸ Slice 1f — Foliage card billboards (PBR atlas + facing system; deferred polish)
- ✅ Slice 2 — Side-quest giver spawn (5 new givers in Dalewatch)
- ✅ Slice 3 — Mob level banding + per-kind respawn timers
- ✅ Slice 4a — Level-scaled mob XP (`level_xp_multiplier`)
- ✅ Slice 4b — Pillar-point on level-up (`grant_xp_with_levelup_bonus`)
- ✅ Slice 4c — Level-up UI (`level_up_ui.rs`)
- ✅ Slice 4d — Level-gated quest accept (server hard-refuses if `quest.level > player.level + 3`)
- ✅ Slice 4e — Dalewatch gear reward ladder (`vaern-data::ItemReward` pillar-keyed; 5 tiers on `chain_dalewatch_first_ride` steps 4/6/7/8 + final; per-pillar Might gambeson→leather→leather→mail→plate, Finesse leather progression to mail, Arcana cloth wool→silk→mageweave)
- ⏸ Slice 5 polish — visual corpse marker on client + party-rez skill
- ✅ Slice 5 MVP — Corpse-run death penalty (server-only Corpse entity; 25% HP respawn; 3u proximity = full HP; 10min expiry)
- ✅ Slice 6 — Drifter's Lair pseudo-dungeon + shared Need-Before-Greed-Pass loot rolls (code-complete, awaits 2-client playtest). Halen rescaled L6→L9, Valenn rescaled L8→L10 (mob YAML + seed_dalewatch_redesign.py); chain step 7 level 6→8, step 10 level 8→10. 3 new L8-L10 drifter trash mobs (`drifter_brute`, `drifter_acolyte`, `drifter_fanatic`) seeded into `_roster.yaml`. Dungeon spec authored at `world/dungeons/drifters_lair/{core,bosses}.yaml` via `seed_dungeons.py` (idx 33). 16 hand-authored Poly Haven dressing props on Ford of Ashmere at offsets that resolve to zone-local (470, 80) — boulders, dead trees, tree stumps, root clusters, two lanterns marking the threshold. New `vaern-data::BossDrops` loader reads `world/dungeons/<id>/loot.yaml`; Drifter's Lair authored: Halen drops 3 chest pieces (steel/wyvern/mageweave + exceptional, 1 per pillar), Valenn drops 12-piece full sets per pillar (mithril plate / dragonscale leather / shadowsilk cloth + exceptional). New protocol messages `LootRollOpen` / `LootRollVote` / `LootRollResult` + `RollVote { Need, Greed, Pass }` enum + `LootRollItem`. New `vaern-server::loot_rolls` module: `LootRollContainer` component, pure `decide_roll_winner` (Need beats Greed beats Pass; ties roll d100; single uncontested vote auto-wins with `roll_value=255`; all-Pass / empty = no winner), `tick_roll_containers` (settles expired items + despawns when fully settled), `handle_loot_roll_votes` (drains `LootRollVote`, "first vote sticks", settles on all-voted). Modified `loot_io::spawn_loot_container_on_mob_death`: boss-tier (`NpcKind::Named`) + party of ≥2 within `PARTY_SHARE_RADIUS=40u` → spawns `LootRollContainer` + broadcasts `LootRollOpen` to every eligible link; otherwise existing single-owner `LootContainer` flow runs unchanged. Open Need (no pillar gate; user decision). New `vaern-client::roll_window` plugin: centered egui modal anchored CENTER_TOP, rarity-colored item names, `[Pass] [Greed] [Need]` buttons, 60s countdown (red < 10s), inline result row (`"Mart won — Need 87"`) for 3s before pruning, modal closes when all items prune. 18 new tests (3 boss_drops loader + 6 decide_roll_winner + 1 all_voted + 3 eligible_for_roll + 4 YAML guards + 1 net from mob/dungeon-count guard bumps). 356 → 374. Phases out of scope: voxel cave sculpting (post-alpha), instance/lockout (post-alpha), boss-phase mechanics, personal-loot fallback (rejected — Open Need chosen).
- ✅ Slice 7 phase 1 — Text emotes via chat-bubble path
- ⏸ Slice 7 phase 2 — Animation playback (UAL clip per emote; needs new replicated state)
- ✅ Slice 8a — Netcode key from `VAERN_NETCODE_KEY` env (release rejects unset/all-zero/wrong-length; debug falls back to dev key)
- ✅ Slice 8b — Configurable bind/connect (`--bind`/`VAERN_BIND` server, `--server`/`VAERN_SERVER` client)
- ✅ Slice 8c — Server panic handler writes `~/.local/share/vaern/server/crash_<ts>.log`
- ✅ Slice 8d — Client auto-reconnect with backoff (`AppState::Reconnecting` + 5-attempt 1→2→4→8s exponential backoff; reuses `OwnClientId` so server-side state lookup keys match)
- ✅ Slice 8e Phase 1 — Server-side AccountStore (rusqlite + bcrypt) + 6 auth protocol messages + AuthedAccount gating in `process_pending_spawns` (gated on `VAERN_REQUIRE_AUTH=1`, default off)
- ✅ Slice 8e Phase 2 MVP — Client AppState::Authenticating + CharacterSelect, login/register/create-character UI, server-driven roster
- ✅ Slice 8e Phase 3 — Reconnect re-auth via cached credentials (`AwaitingReconnectAuth` resource gates `send_hello_on_connect` / `reconnect_tick` / `detect_reconnected` until LoginResult round-trips; `drain_reconnect_auth_results` ships deferred ClientHello on success, drops to MainMenu on fail) + race/pillar/level populated in `CharacterSummary` from `PersistedCharacter` via new `build_character_summary` helper (orphan rows fall back to `?` placeholder rather than dropping). Migration cut — all current characters are throwaway test data.
- ✅ Slice 9 — Quest polish: talk / deliver / investigate steps now turn in via authored NPC reply text (`completion_text`) + contextual button (`completion_button`) on `chain_dalewatch_first_ride` steps 2/3/5/6/8/9; mid-chain talk button triggers Slice 4e Tier-2 (step 6 Fennick) + Tier-4 (step 8 Iwen) gear ladder on the player's click instead of relog. Multi-kill counter on `QuestLogProgress.kill_count` (persisted, broadcast as `kill_count` + `kill_count_required`); tracker UI renders `2/3`. Investigate/explore steps anchor to a new `LandmarkIndex` (loaded from per-zone `landmarks.yaml`); server spawns `QuestPoi` waypoint entities (`NpcKind::QuestPoi` + cyan `?` nameplate marker) at each landmark referenced by an active investigate step. Server validates 5.0u proximity + step-kind matching before honoring `ProgressQuest`; deliver also requires + consumes the item via `inventory.consume_matching` (the folio quest-item is narrative-implied today — `item_required` deferred until a quest-item base lands in the content registry). Validation logic extracted as a pure `decide_progress` helper. Old "Progress step" debug button gated behind `#[cfg(debug_assertions)]`. 7 new tests (decide_progress branches + landmark loader + completion_text guard). 349 → 356.

**Recommended next**: 2-client playtest of Slice 6 to confirm the runtime flow (LootRollOpen broadcast, vote routing, settlement, content-registry resolution of mithril/dragonscale/shadowsilk into real tooltips + paper-doll silhouettes, solo-bypass regression, out-of-radius eligibility). If the playtest surfaces bugs, fix forward. After Slice 6 confirms green in playtest, the remaining ~5-7 sessions are polish (Slice 1f foliage cards, Slice 5 visual corpse marker + party-rez, Slice 7 phase 2 emote anim playback, the `quest_folio` deliver-item base) and the pre-alpha is shippable.

---

## Pre-alpha Steam readiness

"Pre-alpha" here means: 2-3 friends install a private build, sign in with local accounts, land in **Dalewatch** (Mannin-only spawn — other races + zones disabled in char-create until post-alpha), play an 8-10 hour coop arc to L10, clear Drifter's Lair, die and walk back, log out, log back in tomorrow with state intact.

User explicit decisions (`set-and-prioritze-goals-delightful-mochi.md`):
- **Race scope**: Mannin only at char-create for pre-alpha lore coherence. Other 4 Concord races (Hearthkin / Sunward Elen / Firland / Wyrling) gated with "coming soon"; one-flag flip to enable.
- **Zone scope**: Dalewatch only. Other 9 zones stay in-tree but are disabled in zone-select.
- **Account system**: local username + password (SQLite + bcrypt) for pre-alpha. Steam auth deferred.

Organized by blocker severity. A ❌ is a hard blocker for pre-alpha. A ⚠️ is required for the "MMO feel" claim to hold. A ✅ has landed.

### Tier 1 — Hard blockers (ship is unshippable without these)

#### Infrastructure
- ✅ **Real netcode private key** — `VAERN_NETCODE_KEY` (hex, 32 bytes) resolved at boot in `vaern-protocol::config::resolve_netcode_key`. Release rejects unset / all-zero / wrong-length with `exit 2`; debug builds warn and fall back to all-zero dev key.
- ✅ **Dedicated server deployment** — `--bind <addr>` CLI flag + `VAERN_BIND` env; default `0.0.0.0:27015`. Systemd unit / Docker image / real host (Hetzner/OVH EU box) is the deployment task that follows.
- ✅ **Client-side server picker or hardcoded prod server** — `--server <addr>` CLI flag + `VAERN_SERVER` env; default loopback for dev. (No menu picker yet — env/flag is sufficient for pre-alpha tester onboarding via launch script.)
- ✅ **Crash handlers + auto-reconnect** — Slice 8c: `crash::install` writes `~/.local/share/vaern/server/crash_<unix_ts>.log` with panic message, location, thread, captured backtrace, git_sha (`VAERN_GIT_SHA` env), and chains to the default panic hook. Slice 8d: client auto-reconnect with exponential backoff (1s → 2s → 4s → 8s, 5 attempts max) on lightyear `Remove<Connected>`; reuses `OwnClientId` so server-side state lookup keys match across the reconnect.
- ✅ **Account identity beyond client-local JSON** — Slice 8e Phase 1 + 2 + 3 shipped: server-side SQLite at `~/.config/vaern/server/accounts.db` with bcrypt-hashed passwords; case-insensitive username + character-name uniqueness; client login/register/create-character UI behind `AppState::Authenticating` + `CharacterSelect`; reconnect re-auth replays `CachedCredentials` automatically under `VAERN_REQUIRE_AUTH=1` (Phase 3); `CharacterSummary` populated from `PersistedCharacter` so the roster shows real race/pillar/level. Local-JSON migration was cut — existing characters are throwaway test data.
- ⏸ **Steam integration** — deferred to full alpha (was ❌, now post-pre-alpha per user decision).

#### Content floor (the "something to do" floor)
- ✅ **Currency loop** — `PlayerWallet` + coin drops scaled by mob rarity + tier, quest gold payout on step / chain complete, `WalletSnapshot` on change, wallet UI under Inventory heading. Persisted as `PersistedCharacter.wallet_copper`.
- ✅ **Live vendor NPCs** — 10 general-goods vendors at starter-zone capitals.
- ✅ **Death penalty** — corpse-run MVP (Slice 5): 25% HP respawn, walk-back-to-corpse for full restore, 10-min expiry.
- ✅ **Text chat** — Say / Zone / Whisper / Party / System with bubbles.
- ✅ **Party / group system** — invite / accept / leave / kick / shared XP / cross-zone party chat.

### Tier 2 — MMO feel (required to credibly call it an MMO)

- ⏸ **9 hand-curated starter chains** — Dalewatch only is full starter-scale; pre-alpha decision is Dalewatch-only so other 9 are not pre-alpha-blocking. Post-alpha content stream.
- ✅ **Side-quest giver spawn fix** — Slice 2 shipped. 5 new givers in Dalewatch (Quartermaster Hayes / Captain Morwen / Innkeeper Bel / Smith Garrick / Mistress Pell). Other zones still rely on procedural target-hint fallback.
- ✅ **Drifter's Lair pseudo-dungeon (Slice 6)** — the L10 capstone. Open-world spawn region anchored at zone-local (470, 80) east of Ford of Ashmere; Halen (L9 mini-boss) + Valenn (L10 capstone) flanked by L8-L10 drifter brutes/acolytes/fanatics in 4-mob pulls; 16 Poly Haven props at the anchor mark the threshold; one-tier-above gear ladder on bosses (mithril/dragonscale/shadowsilk + exceptional 4-piece per pillar on Valenn; chest pieces at steel/wyvern/mageweave + exceptional on Halen) on top of the existing Slice 4e chain capstone reward. Code-complete, 18 new tests green; **2-client playtest pending**.
- ⏸ **World boss + zone-level elite content** — out of pre-alpha scope.
- ⏸ **Banking / shared stash** — out of pre-alpha scope (30-slot inventory suffices for L1→L10 arc).
- ⏸ **Zone portals UI** — moot for single-zone pre-alpha.
- ✅ **Quest item rewards (Slice 4e)** — `vaern-data::ItemReward` (pillar-keyed) on `QuestStep` + `QuestChainFinalReward`; server `grant_item_rewards` injects into kill-step + chain-complete + talk-progress paths. 5-tier ladder on `chain_dalewatch_first_ride`: T1 single-piece material upgrade @ step 4, T2 full-set ArmorType flip @ step 6, T3 single piece @ step 7, T4 second silhouette flip @ step 8, T5 capstone full set @ chain final.
- ✅ **Multi-kill objectives (`count > 1`)** — Slice 9 shipped. `QuestLogProgress.kill_count` bumps per matching mob death; advance gated on `kill_count >= objective.count.max(1)`; reset on advance; persisted across logout. Tracker UI renders `2/3` suffix when `kill_count_required > 1`.
- ✅ **Click-through turn-ins for talk / investigate / deliver steps** — Slice 9 shipped. Players walk to the right NPC (or cyan `?` waypoint), F-press, read the authored `completion_text`, click `Take the leather kit` / `Hand it over` / `Continue`. Server validates 5.0u proximity + step-kind + (for deliver) inventory match. Replaced the dev "Progress step" button (now gated `#[cfg(debug_assertions)]`).
- ✅ **Emotes (Slice 7 phase 1)** — `/wave /bow /sit /cheer /dance /point` via chat-bubble path. Animation playback per emote is phase 2, deferred.
- ✅ **Nameplate overhead names for players** — DisplayName label, 60u culling, V-toggle.

### Tier 3 — Economy and progression shelf-life (keeps players coming back past hour 5)

- **Alchemy as the first crafting profession** — potions already ride `ConsumeEffect::Buff`/`HealX`; authoring recipes is a data pass. Gathering → herbs → alchemy → consumable belt is a self-contained loop. Ship this before any other crafting pro.
- **Boss shard + crafter rite** — closes the loot+craft loop. Shard drops from bosses, consumed at a crafter NPC to imprint a soulbinding affix onto an item with open slots. Design is in `memory/project_gear_loot_system.md`; affixes are already tagged `soulbinds: true`.
- **Item icons keyed by `base_id`** — tooltips are text-only. Pipeline exists for hotbar icons (`scripts/generate_item_icons.py`, `icons/items/`); extend to cover every base.
- **Drag-and-drop inventory ↔ paper doll** — click-to-equip works, but drag is table-stakes for an inventory window.
- **Multiple starter gear kits per archetype** — all Might players look like peasants at level 1. Seed 3–5 archetype-flavored starter kits per pillar; pick randomly on char-create.
- **Tradeable mats between players (P2P trade window)** — not auction house yet. Trade-window protocol: both players confirm, atomic swap. Needed before crafting economy matters.
- **Ability unlock via trainer NPCs** — today all abilities unlock by pillar level. A "visit trainer, pay gold, learn rank 2 Firebolt" pass would give coin more purpose and anchor the capital hubs.
- **Reputation system (v0)** — at minimum, faction-bound reputation with Concord / Pact. +rep on faction quest completion, -rep on killing friendly NPCs. Display under the unit frame.

### Tier 4 — Nice-to-haves / clearly post-alpha

- Guild / clan system (design not yet written)
- Auction house / cross-realm market
- Mailbox / letters
- Duel / PvP flag
- Friends list + cross-zone `/who`
- Achievement / title system
- Durability as its own slice (if death penalty uses XP debt instead)
- Character deletion UI
- HDRI skybox bake (procedural `Atmosphere` is good enough)
- Foliage / grass billboards / decorative rocks
- PPO balance trainer in `vaern-sim`

### Tier 5 — Open technical debt (don't block pre-alpha, but will bite later)

- **Voxel chunk eviction** — memory grows monotonically; earlier naive evictor blacked out the scene. Unknown render-pipeline interaction. Will OOM on long sessions.
- **Voxel zone-scoped delta broadcast** — every `VoxelChunkDelta` goes to every client. Wire lightyear Room scope by chunk zone.
- **Sparse voxel delta encoding** — broadcast uses `ChunkDelta::full_snapshot` (~150 KB/chunk). `encode_delta` exists but needs per-sample write tracking through `EditStroke`.
- **Voxel chunk teardown on logout** — chunk entities don't carry `GameWorld`, so they persist into the next session.
- **Threat decoupled from damage** — today tanks must out-DPS to hold aggro. Move `threat_multiplier` to a per-ability flat-threat modifier independent of damage dealt.
- **NPC melee damage reads from equipped weapon, not scalar `attack_damage` field**.
- **Auto-attack branches on equipped weapon school** — currently hardcoded blade cones.
- **`Casting` + `AnimOverride` dynamic insertion on own-player Predicted copy is unreliable** — currently routed through `PlayerStateSnapshot` / `CastFiredLocal` messages. Known lightyear 0.26 limitation; watch for 0.27 fix.
- **Bow/staff/wand weapon models** — MEGAKIT only ships 5 props; ranged slots render empty.
- **More UAL attack clips per weapon category** — `Sword_Attack` used for every physical cast; UAL has A/B/C variants + bow set unused.

---

## Recommended slice ordering for pre-alpha (remaining work)

Slice 6 + Slice 8a-8d + Slice 4e + Slice 8e Phases 1+2+3 + Slice 9 shipped (Slice 6 awaits 2-client playtest). What's left:

1. **2-client playtest of Slice 6** — confirm `LootRollOpen` broadcast, vote routing, settlement timing, mithril/dragonscale/shadowsilk + exceptional resolve through `ContentRegistry` into real paper-doll silhouettes, solo-kill regression, out-of-radius eligibility. ~0.25 sessions if no bugs surface.
2. **Slice 1f — Foliage card billboards** — PBR atlas + facing system for carpet-grass density. Polish, not pre-alpha-blocking. ~2 sessions.
3. **Slice 7 phase 2 — Emote animation playback** — UAL clip per emote (Wave / Bow / Sit / etc) needs a new replicated `Emote(EmoteKind)` AnimState variant + transient override. ~1-2 sessions.
4. **Slice 5 polish — Visual corpse marker on client + party-rez skill** — pulsing gizmo at own-corpse position via `OwnCorpsesSnapshot`-style message; party-rez via new `ConsumeEffect::Revive`. ~1-2 sessions.
5. **Quest-item base for the deliver path** — author a `quest_folio` (or similar) item base in the content registry so step 5 deliver to Telyn can have a real `item_required` (today the folio is narrative-implied; server validates proximity only). ~0.2 sessions.

Total remaining: ~10-13 sessions to ship pre-alpha. Hand-curating the other 9 starter chains is post-pre-alpha (Mannin-only spawn for pre-alpha).

---

## Things NOT to do (from `memory/` — don't re-derive)

- Don't put pillar values on gear — gear is tactical, pillars are identity (`memory/project_stat_armor_system.md`)
- Don't move resists to tertiary — hardcore prep stays first-class (`memory/feedback_hardcore_prep.md`)
- Don't design class-specific crafts — every profession serves every class (`memory/feedback_crafting_economy.md`)
- Don't reintroduce GCD (`memory/project_mmo_architecture.md`)
- Don't query `(With<Replicated>, Without<Predicted>)` for own-player — use `PlayerStateSnapshot` (`memory/project_own_player_replication.md`)
- Don't hand-edit ≥15 similar YAMLs — use `scripts/seed_*.py` (`memory/feedback_bulk_writes.md`)
- Don't add passive parry on incoming hits — Block and Parry are both **active, mutually exclusive** stances
- Don't retrofit casual-MMO QoL onto hardcore-prep flow (`memory/feedback_hardcore_prep.md`)
- Don't say "compiles clean" after only `cargo check` — the binary is stale (`memory/feedback_check_vs_build.md`)

---

## Run recipes (unchanged)

```bash
cargo build -p vaern-server -p vaern-client
./target/debug/vaern-server                         # terminal 1
./target/debug/vaern-client                         # terminal 2 — full menu
VAERN_CLIENT_ID=1001 ./target/debug/vaern-client    # second client
./scripts/run-multiplayer.sh                        # build + server + 2 clients
```

```bash
cargo test --workspace
# expected: 106+ pass
```

```bash
python3 scripts/seed_items.py
```

---

## Archived session log

The per-session "Just landed" log for 2026-04-20 / 2026-04-21 sessions 2–8 lived here previously. That content is now summarized in `README.md` and the `memory/` files; reconstruct from `git log` if needed.
