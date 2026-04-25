# Vaern TODO

Forward-looking slice list. For the full current state see `README.md`; for design rationale see `memory/`. For the ratified pre-alpha plan see `~/.claude/plans/set-and-prioritze-goals-delightful-mochi.md`.

---

## Where we are today (2026-04-25)

Pre-alpha-shaped scaffold. Menu → char create → **PBR-dressed Dalewatch** with scattered trees / rocks / shrubs + ~55 authored hub props → side-quest givers populated → mob level bands by hub → kill mobs (XP scales by mob-vs-killer level: greys=0, +5 reds=1.5×) → level up (centered banner + screen flash + +1 pillar point auto-granted) → die → 25% HP at home, walk back to corpse for full restore → /wave at your friend → repeat. Multi-client, server-authoritative, zone-AoI replicated, **309 tests green** (4 pre-existing combat-test failures unchanged).

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
- ⏸ Slice 6 — Drifter's Lair pseudo-dungeon + shared loot rolls (the L10 capstone)
- ✅ Slice 7 phase 1 — Text emotes via chat-bubble path
- ⏸ Slice 7 phase 2 — Animation playback (UAL clip per emote; needs new replicated state)
- ✅ Slice 8a — Netcode key from `VAERN_NETCODE_KEY` env (release rejects unset/all-zero/wrong-length; debug falls back to dev key)
- ✅ Slice 8b — Configurable bind/connect (`--bind`/`VAERN_BIND` server, `--server`/`VAERN_SERVER` client)
- ✅ Slice 8c — Server panic handler writes `~/.local/share/vaern/server/crash_<ts>.log`
- ✅ Slice 8d — Client auto-reconnect with backoff (`AppState::Reconnecting` + 5-attempt 1→2→4→8s exponential backoff; reuses `OwnClientId` so server-side state lookup keys match)
- ⏸ Slice 8e — Local SQLite accounts (server-side bcrypt + session token)

**Recommended next slice**: Slice 8e (local SQLite accounts, 2-3 sessions) — the last hard pre-alpha infrastructure blocker. Then Slice 6 dungeon (6-8 sessions).

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
- ❌ **Account identity beyond client-local JSON** — `~/.config/vaern/characters.json` is client-side. Pre-alpha decision: local username+password in server-side SQLite at `~/.config/vaern/server/accounts.db` with bcrypt. Server-enforced name uniqueness, one character list per account.
- ⏸ **Steam integration** — deferred to full alpha (was ❌, now post-pre-alpha per user decision).

#### Content floor (the "something to do" floor)
- ✅ **Currency loop** — `PlayerWallet` + coin drops scaled by mob rarity + tier, quest gold payout on step / chain complete, `WalletSnapshot` on change, wallet UI under Inventory heading. Persisted as `PersistedCharacter.wallet_copper`.
- ✅ **Live vendor NPCs** — 10 general-goods vendors at starter-zone capitals.
- ✅ **Death penalty** — corpse-run MVP (Slice 5): 25% HP respawn, walk-back-to-corpse for full restore, 10-min expiry.
- ✅ **Text chat** — Say / Zone / Whisper / Party / System with bubbles.
- ✅ **Party / group system** — invite / accept / leave / kick / shared XP / cross-zone party chat.

### Tier 2 — MMO feel (required to credibly call it an MMO)

- ⏸ **9 hand-curated starter chains** — Dalewatch only is Elwynn-scale; pre-alpha decision is Dalewatch-only so other 9 are not pre-alpha-blocking. Post-alpha content stream.
- ✅ **Side-quest giver spawn fix** — Slice 2 shipped. 5 new givers in Dalewatch (Quartermaster Hayes / Captain Morwen / Innkeeper Bel / Smith Garrick / Mistress Pell). Other zones still rely on procedural target-hint fallback.
- ⏸ **Drifter's Lair pseudo-dungeon (Slice 6)** — the L10 capstone. Plan: hub-external cave region with 4-mob pull cadence, 1 mini-boss, 1 boss tuned for 2-4 players, shared Need/Greed/Pass loot rolls, end-boss drops the L10 plate piece from Slice 4e's gear ladder. ~6-8 sessions.
- ⏸ **World boss + zone-level elite content** — out of pre-alpha scope.
- ⏸ **Banking / shared stash** — out of pre-alpha scope (30-slot inventory suffices for L1→L10 arc).
- ⏸ **Zone portals UI** — moot for single-zone pre-alpha.
- ✅ **Quest item rewards (Slice 4e)** — `vaern-data::ItemReward` (pillar-keyed) on `QuestStep` + `QuestChainFinalReward`; server `grant_item_rewards` injects into kill-step + chain-complete + talk-progress paths. 5-tier ladder on `chain_dalewatch_first_ride`: T1 single-piece material upgrade @ step 4, T2 full-set ArmorType flip @ step 6, T3 single piece @ step 7, T4 second silhouette flip @ step 8, T5 capstone full set @ chain final.
- ⏸ **Multi-kill objectives (`count > 1`)** — quests auto-advance on first kill even when count > 1. Counter exists in YAML but progress handler ignores it.
- ⏸ **Auto-advance for talk / investigate / deliver steps** — kill-step advance works; others require relog.
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

Slice 8a-8d + Slice 4e shipped — release builds reject an unset key, server bind is configurable, server panics land in a crash log, client auto-reconnects with exponential backoff after a server restart, and Dalewatch's `chain_dalewatch_first_ride` hands out a 5-tier per-pillar gear ladder that visibly flips silhouette twice (gambeson→leather→mail→plate for Might, leather→mail for Finesse, cloth material progression for Arcana). What's left:

1. **Slice 8e — Local SQLite accounts** — server-side bcrypt + session token at `~/.config/vaern/server/accounts.db`, replacing client-local `characters.json`. ~2-3 sessions.
2. **Slice 6 — Drifter's Lair pseudo-dungeon + shared loot rolls** — hub-external cave region, 4-mob pull cadence, mini-boss + boss tuned for 2-4 players, Need/Greed/Pass loot panel. End-boss drops the L10 plate piece. Slice 4e ladder ships steel/wyvern/mageweave at L8; Slice 6 boss should drop a step above (e.g. mithril or exceptional quality). ~6-8 sessions.
3. **Slice 1f — Foliage card billboards** — PBR atlas + facing system for carpet-grass density. Polish, not pre-alpha-blocking. ~2 sessions.
4. **Slice 7 phase 2 — Emote animation playback** — UAL clip per emote (Wave / Bow / Sit / etc) needs a new replicated `Emote(EmoteKind)` AnimState variant + transient override. ~1-2 sessions.
5. **Slice 5 polish — Visual corpse marker on client + party-rez skill** — pulsing gizmo at own-corpse position via `OwnCorpsesSnapshot`-style message; party-rez via new `ConsumeEffect::Revive`. ~1-2 sessions.
6. **Quest polish** — multi-kill objectives + auto-advance for talk/investigate/deliver. **Note**: Slice 4e Tier-2 (step 6) and Tier-4 (step 8) ladder rewards live on talk steps, so they don't fire until this lands or the player relogs. ~0.5-1 session.

Total remaining: ~13-19 sessions to ship pre-alpha. Hand-curating the other 9 starter chains is post-pre-alpha (Mannin-only spawn for pre-alpha).

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
