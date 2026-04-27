# Vaern

Solo-developer hardcore two-faction persistent-coop RPG.

**Stack:** Rust · Bevy 0.18 · Lightyear 0.26 · bevy_egui 0.39
**Mechanics:** D&D 3.5-inspired
**Pipeline:** AI-assisted

---

## Status

Pre-alpha-shaped MMO that reads as a place. **496 workspace tests passing.**

### TL;DR

- Menu → character create (race / body / pillar) → live 3D world with a gear-driven Quaternius character mesh.
- PBR-dressed Dalewatch on a chunked SDF voxel ground (Poly Haven trees / rocks / shrubs / ground cover scattered across 1200×1200u, plus ~55 hand-authored hub props).
- Full combat → gear → loot → currency → vendor loop, with chat, parties, shared XP, nameplates, emotes, corpse-run death penalty, level banding, side quests, and a code-complete pseudo-dungeon (Drifter's Lair).
- Server-authoritative over UDP via lightyear + netcode. Multi-client. Client prediction + interpolation + zone-scoped AoI replication. Hostable build with env-driven config, panic forensics, auto-reconnect, and SQLite-backed accounts.

### World & rendering

- **10 starter zones** (1 per race) on a 2800u ring; each player spawns in their race's zone.
- **Dalewatch Marches** (Mannin starter) is the showcase zone — full starter-scale scope: 4 hubs, 12 sub-zones, 10-step main chain, 2 side chains, 20 side quests, 24 mob types in a ~2×2 km box. Other 9 zones still ~15 mobs / 2 hubs / 1 chain / 5 nodes.
- **Voxel ground** — `vaern-voxel` crate, hand-rolled Surface Nets, 32³+1-padding chunks streamed around every active player on both server and client. 8 swappable algorithm layers. Server seeds 5×3×5 around each player; client streams 11×3×11 around the camera with world-XZ UVs + MikkTSpace tangents.
- **Voronoi hub biomes** — 9-biome CC0 ambientCG PBR table; nearest-hub resolver with 900u influence radius; chunk-aligned transitions (32u tiles).
- **F10 voxel stomp is fully server-authoritative** — client `ServerEditStroke` → server validates (≤12u radius, ≤40u from sender) → applies sphere-subtract brush → broadcasts up to 8 `VoxelChunkDelta`/tick. Reconnecting clients catch up at 4 chunks/tick.
- **Sky** — Bevy 0.18 `AtmospherePlugin` (procedural scattering) + `DistanceFog` (1500u visibility) + `Bloom::NATURAL` + `Tonemapping::TonyMcMapface` + `Exposure::SUNLIGHT` + `Hdr`.
- **PBR world dressing** — 57-asset Poly Haven CC0 pack, deterministic seeded Poisson scatter per zone, 250u distance cull, 1500-prop safety cap. Dalewatch dressed with ~55 authored hub props + 5 scatter rules. Trees are saplings only (hero-tree photoscans excluded). No collision on dressing props yet.
- **Cartography (parchment SVG maps)** — `vaern-cartography` crate + uv-shebang seed scripts (`balance_world_layout.py`, `seed_connections.py`, `seed_geography.py`, `audit_quest_landmarks.py`). Voronoi-tessellated continent (28 zone cells clipped to a hand-authored coastline), sub-Voronoi biome pockets keyed off landmark names (croft → fields, grove → forest, fen → marsh, etc.), tier-density procedural farmhouse glyphs clustered along roads/rivers, auto-generated dirt-path spurs from every off-road landmark/hub to the nearest road. Three CLI bins: `vaern-validate` (cross-file rules incl. graph reachability + level-band gap), `vaern-render-zone <id>`, `vaern-render-world`. Byte-deterministic across runs (golden test enforced).

### Character rendering

All humanoids (own player, remote players, humanoid NPCs) are Quaternius modular meshes on the UE Mannequin skeleton, driven by a shared UAL `AnimationGraph`.

- **Gear-driven outfit.** `outfit_from_equipped` maps each primary armor slot's `ArmorType` to a Quaternius outfit family + color: cloth→Wizard, gambeson→Peasant V2, leather→Ranger, mail→KnightCloth V3, plate→Knight V2. Unequipped = Peasant BaseColor (the reserved "naked rags" identity).
- **Respawn on change.** `sync_own_player_visual` watches `OwnEquipped`; resolution change → despawn old mesh, spawn new. Rings / trinkets are a no-op visually.
- **AnimState → UAL clip driver** — three sibling drivers (own / remote / NPC) consume `AnimState` + cast school + mainhand school each frame:

  | State | Clip |
  |---|---|
  | Idle | `Idle_Loop` (unarmed) / `Sword_Idle` (armed) |
  | Walking / Running | `Walk_Loop` / `Jog_Fwd_Loop` (speed=+1.0 forward, -1.0 reverse-played for back-pedal — UAL has no `Walk_Bwd_Loop`) |
  | Casting | `Sword_Idle` (physical) / `Spell_Simple_Idle_Loop` (magic) |
  | Blocking | `Idle_Shield_Loop` (weapon-agnostic) |
  | Attacking | `Sword_Attack` → `Sword_Regular_A/B/C` round-robin (physical) / `Spell_Simple_Shoot` (non-physical) |
  | Hit | `Hit_Chest` (<35 dmg) / `Hit_Knockback` (≥35 dmg) |
  | Dead | `Death01` |

- **Transient clips (Attacking, Hit) play one-shot and hold until `ActiveAnimation::is_finished()`** — even after the server-side 250ms `AnimOverride` reverts to Idle — so the full swing reads on-screen. `AnimSlot` distinguishes Play / AdjustSpeed / Hold for forward↔reverse flips.

### Combat

- **Mouse-look camera** — cursor locked + hidden in-game; LeftAlt frees cursor for UI clicks. Cursor auto-frees whenever an egui panel opens. Camera is ground-clamped via `vaern_voxel::query::ground_y`.
- **Target lock** — Tab cycles combat NPCs within 40u, prefers camera's front cone. Esc clears. Locked players continuously turn toward their target.
- **Combat shapes** (per-ability YAML): `target`, `aoe_on_target`, `aoe_on_self`, `cone`, `line`, `projectile`. Friendly fire on. Channeled cones/lines/projectiles snapshot `range` onto `Casting` so heavy attack doesn't sweep to infinity.
- **Hotbar** — 6 keybind + 2 mouse-bound (LMB light auto-attack, RMB heavy). No GCD.
- **Cast bar** — bottom-center, school-colored, for abilities with `cast_secs > 0`.
- **Stats-aware damage pipeline** — caster: weapon roll × global mult × crit roll. Target: armor mitigation `armor / (armor + 200)` + per-channel resist `resist_total[dt] × 0.005` (80% cap, supports negative for vulnerability).
- **NPC stats from bestiary** — creature_type resists + armor_class reductions fold into `CombinedStats`, scaled by rarity (Combat 1.0× / Elite 1.25× / Named 1.5×).
- **NPC AI** — per-type aggro, threat-table targeting, roaming idle, leash-home. Slow-aware.

### Combat depth

- **Timed status effects** — `StatusEffects(Vec<StatusEffect>)` on every combat-capable entity. Variants: `Dot`, `Stance`, `Slow`, `StatMods { damage_mult_add, resist_adds[12] }`. `compute_damage` reads StatMods on both sides; consumables push timed StatMods. Refresh-on-reapply.
- **YAML-driven effect riders** — flavored ability variants declare `applies_effect: { id, duration_secs, kind, dps, tick_interval, speed_mult }`. Parry-negated hits skip the rider. Seeded: fire→burning, frost→chilled, shadow→decay, blood→bleeding at tiers 25 + 50.
- **Active Block (Q hold)** — drains 15 stamina/s, 60% frontal / 25% flank / 0% rear damage reduction. Breaks at zero stamina.
- **Active Parry (E tap)** — 0.35s window, 20 stamina on consume (free to miss). Fully negates damage and rider debuff.
- **Stamina pool** — 100/100, 12/s regen. Separate from mana.
- **Slow-aware movement** — both players and NPCs move at `speed_mult × base` while chilled. Strongest slow wins; doesn't stack.

### Animation state

- **Replicated `AnimState`** on players + NPCs. Derived every FixedUpdate from Transform delta + Casting + StatusEffects + Health.
- **Transient flashes** — every CastEvent triggers a 250ms `AnimOverride` to prevent the derive loop from clobbering the flash.
- **Visible in nameplates** — small grey `[running]` / `[blocking]` / `[attacking]` tag.

### Gear & item system (Model B — compositional)

- Items are composed at runtime from four orthogonal tables: **bases** (piece shape) × **materials** (substance + stat mults) × **qualities** (craft roll) × **affixes** (stat deltas).
- **222 bases · 25 materials · 7 qualities · 27 affixes → ~5,000+ resolvable combinations.**
- **Affixes** roll on world drops (weight-pool filtered by tier + base kind), stack as prefix ("Enchanted") + suffix ("of Warding") in the resolved name. 5 shard-only affixes (weight 0, soulbinds on apply) reserved for boss-token imprint.
- **Materials** carry per-channel `resist_adds` — silver vs necrotic/radiant, dragonscale vs fire, shadowsilk vs radiant penalty.
- **Runes** — caster magical-ward gear in `EquipSlot::Focus`. Drain mana via negative mp5 in exchange for heavy magical resist.
- Rarity = affix slot count (Common 0 → Legendary 4). Pre-rolled drops leave 1 slot open for crafter polish.

### Inventory + equipment UI (`I`)

- 30-slot inventory grid (3×10) with stack merging keyed on full `ItemInstance` identity.
- 20-slot paper doll on the right — 11 armor slots (head→feet) + 9 accessory/weapon/focus.
- **Rarity-colored item names** (genre-standard palette: grey / white / green / blue / purple / orange).
- **Hover tooltip cards** — bold name in rarity color, rarity + kind line, nonzero stats, per-channel resists, soulbound tag in gold italic, weight.
- Left-click → auto-equip (gear) or consume (potions/elixirs/food).
- Right-click paper-doll slot → unequip.
- Two-hander displaces offhand; Focus rejects non-runes; armor slot-id validation.

### Consumables

- Every `Consumable` base carries a YAML-authored `ConsumeEffect`: `HealHp` / `HealMana` / `HealStamina` (clamp-add) or `Buff { id, duration_secs, damage_mult_add, resist_adds[12] }` (timed StatMods).
- Real amounts — Minor Healing +40 HP, Major Healing +450 HP. Same pattern for mana/stamina.
- **Elixirs** — Might / Finesse / Arcana: +15% damage 5min. Giant's: +25% damage 2min. Stack additively.
- **Warding Elixir** — +15 resist across all 12 channels for 5min.
- **Per-channel Resist Potions** (24 bases, 12 channels × lesser/greater): +30/+60 for 3min. Capped at the 80% resist ceiling. Prep-before-boss loop.

### Consumable belt (keys 7/8/9/0)

- 4-slot strip below the hotbar, owned by `ConsumableBelt` on the server. Bindings store the `ItemInstance` template (not an inventory index) so they survive stack rearrangement.
- **Bind**: right-click a potion → "Bind to Slot 1/2/3/4".
- **Fire**: 7/8/9/0 quaffs the bound potion. Server applies the `ConsumeEffect`, decrements one charge.
- Strip shows bound name + `×count` (grey when zero stacks remain).

### Stat screen (`C`)

- Live `CombinedStats` fold — pillars + gear → derived primaries + armor + 12 resist channels + utility.
- MP5 flags "(rune drain)" when negative.
- Pillar progress bars showing banked XP toward the next pillar point.

### Loot flow

- Mob dies → `vaern-loot` rolls rarity curve + base + material + affixes → `LootContainer` at corpse, owned by top-threat player.
- Client sees yellow gizmo → walks within 5u → `G` opens loot window.
- Click items individually or "Take all" → server moves to inventory. Container auto-despawns at 5min or empty.

### Currency loop (closed earn → spend)

- **`PlayerWallet { copper: u64 }`** component on every player. Lives in `vaern-economy`.
- **Mob kills drop coin** in addition to items, scaled by `(material_tier, NpcTier)`: combat 2-10c → 8-46c at T6, elite 15-50c → 63-210c, named 100-300c → 420-1260c. Independent of item drop_chance — a no-item kill still pays. Credited directly to the top-threat player's wallet.
- **Quest rewards pay copper** — step `gold_reward_copper` on progress + chain `gold_bonus_copper` on completion.
- **`WalletSnapshot`** S→C on `Changed<PlayerWallet>` only.
- **Gold displayed in the inventory panel** under the Inventory heading as `"12g 34s 56c"` — *not* in the unit frame (currency is an inventory concern).
- **Persisted** as `PersistedCharacter.wallet_copper: u64` with `#[serde(default)]` for legacy saves.

### Vendor NPCs (10 per starter capital)

- One general-goods vendor per capital hub (Merchant Kell at Dalewatch Keep, Merchant Seyla at Shadegrove Spire, etc.), seeded from `src/generated/vendors.yaml`. ~12 items each: minor potions, food, scroll of recall, linen cloth, two copper weapons.
- **`NpcKind::Vendor`** — cool-blue nameplate; excluded from Tab-targeting; non-combat.
- **F within 5u opens Buy/Sell window.** Buy tab uses server-computed `vendor_buy_price`; Sell tab uses `vendor_sell_price` (60% spread). Soulbound / `no_vendor` items show grey "(no sale)".
- Auto-close on walk-out (5u) via `VendorClosedNotice`.
- **`VendorIdTag`** stamped at startup so wire ids stay stable across reconnects.

### Chat (`Enter`)

- **Five channels**: Say (20u proximity), Zone (whole-zone AoI room), Whisper (by display name), Party (cross-zone), System.
- **Prefix parser**: no prefix = Say; `/s /say`, `/z /zone`, `/p /party`, `/w /whisper /tell /msg <name>`. Unknown `/foo` → Say.
- **Party commands** (`/invite /inv /leave /disband /kick`) intercept before chat parsing.
- **Emotes**: `/wave /bow /sit /cheer /dance /point` translate to a Say-channel send with body `*waves.*` / etc. Animation playback is post-pre-alpha.
- **Rate-limited** — 5 msg/sec/sender (rolling 1s window), 256-char truncate. Server-authoritative `from`.
- **Speech bubbles** — render above speakers on Say + Zone only. 5s lifetime, 1s fade, 72-char ellipsis truncate, one-bubble-per-speaker. Anchored at `head + 2.8u`; nameplate at `+2.1u`.
- **`ChatInputFocused`** suppresses WASD / Tab / Esc / 1-6 / LMB/RMB / Q/E / K / V while typing.
- **History** — 50-line ring bottom-left. Channel colors: Say white, Zone mint-green, Party cool-blue, Whisper magenta (received) / pink (echo), System yellow.

### Party system v1 (strict-coop)

- **Invite by display name** — `/invite Brenn`. Server validates target exists, isn't already partied, party has room (max 5). 60s invite TTL.
- **Party frame** top-left under the unit frame — name + level + HP bar + `[L]` leader tag + Leave button. Rebuilt from `PartySnapshot` (broadcast on join/leave/kick/disband; dirty-set gated, not per-tick).
- **Leave / kick / disband** — `/leave` or button; `/kick <name>` (leader-only); auto-disband when size drops below 2. Leader-leave promotes `members[0]` first.
- **Shared XP** — splits across party members within `PARTY_SHARE_RADIUS = 40u` of the killer. Killer gets full base; partners get small-group multiplier `1.0 / 0.7 / 0.55 / 0.45 / 0.38×` for 1/2/3/4/5 sharers. Total payout rises with party size (5-party ≈ 1.9× solo) but never linearly to 5×.
- **Party chat** routes through `ChatChannel::Party` — cross-zone, same 5/sec limit.

### Nameplates (`V` toggle)

- Every entity with `Health` gets a nameplate, projected from `head + 2.1u`.
- **Label = `DisplayName`** for both players and NPCs. Pillar label only for anonymous spawns.
- **60u culling** so dense crowds don't become letter soup.
- **V toggles** on/off (gated on chat focus). When off, also hides chat bubbles.
- **Color by kind** — players + combat mobs white, quest-givers gold, vendors cool-blue, elites violet, named pink. `"!"` quest marker over quest-giver plates.
- **State tag** under HP bar reads `[idle]` / `[running]` / `[blocking]` / `[attacking]` — live from replicated `AnimState`.

### Quest flow

- Walk up to a gold "!" NPC → `F` → Accept → quest log (`L`).
- **Server hard-refuses accept if `chain.steps[0].level > player.level + 3`** — no entry appears in the log.
- **5 side-quest givers in Dalewatch** — Quartermaster Hayes (capital), Captain Morwen (Harrier's), Innkeeper Bel (Ford), Smith Garrick (Kingsroad), Mistress Pell (Miller's). Each at the NW 4u offset of their hub.
- **Quest polish (Slice 9)** — talk / deliver / investigate steps now turn in via authored NPC reply text + a contextual click-through button (e.g. "Take the leather kit") and grant gear-ladder rewards on the player's click.
- **Multi-kill objectives** track `2/3` in the tracker and only advance on the final required kill.
- **Investigate steps** spawn cyan `?` POI markers at landmark coordinates that the player F-presses to advance.
- **Server validates 5.0u proximity** to the right NPC / waypoint before honoring `ProgressQuest`.

### Mob level banding

- **Dalewatch tiers** mobs by level: L1-2 around the keep, L3-4 around Harrier's Rest + Kingsroad, L5-6 around Miller's + Ford, L7+ at the fixed `(470, 80)` Drifter's Lair anchor east of Ford.
- Per-rarity scatter radius (named 110u / elite 90u / common 70+jitter).
- Other zones still ring around zone origin (legacy procedural fallback).
- **Per-kind respawn timers**: combat=180s / elite=600s / named=1800s (was a flat 30s).

### Felt level progression

- **`level_xp_multiplier`** scales kill XP by `mob_level - killer_level`: parity = 1.0×, +5 = 1.5× (cap), -3 = 0.5×, -6+ = 0.0× (grey).
- Each level-up grants **+1 pillar point** auto-targeted at your committed pillar (highest-cap, tie-break Might > Finesse > Arcana).
- Both kill and quest XP paths flow through `grant_xp_with_levelup_bonus` so every level-up gives the bonus.
- Client renders a centered "LEVEL UP / Level N" banner with 0.35s gold flash + 2.5s fade.

### Gear-reward ladder

- **5-tier per-pillar ladder** on the main Dalewatch chain (`chain_dalewatch_first_ride` steps 4/6/7/8 + chain capstone):
  - **Might**: gambeson → leather → mail → plate
  - **Finesse**: leather → mail
  - **Arcana**: cloth wool → silk → mageweave
- Full silhouette flips at the ArmorType-change tiers.

### Drifter's Lair pseudo-dungeon (Slice 6, code-complete, awaits 2-client playtest)

- **Open-world spawn region** anchored at zone-local `(470, 80)` east of Ford of Ashmere.
- **16 hand-authored boulders / dead trees / lanterns** mark the threshold.
- **Master Drifter Halen** (mini-boss, L9) and **Grand Drifter Valenn** (capstone boss, L10), flanked by L8-L10 drifter brutes / acolytes / fanatics in 4-mob pulls.
- **Shared Need-Before-Greed-Pass loot rolls** — boss kills with ≥2 party members within `PARTY_SHARE_RADIUS=40u` spawn a `LootRollContainer` (no single owner) and broadcast `LootRollOpen` to every eligible client. Per-item Need/Greed/Pass votes resolve via pure `decide_roll_winner` (Need beats Greed beats Pass; ties d100; all-Pass = no winner). Winner gets the item directly. 60s deadline auto-settles. Solo / out-of-radius kills bypass to the existing single-owner `LootContainer` flow.
- **Open Need** — no pillar gating; any party member can roll Need on any item.
- **One-tier-above gear ladder on Valenn** — full 4-piece mithril plate (Might) / dragonscale leather (Finesse) / shadowsilk cloth (Arcana) at `quality: exceptional`.
- **Halen** drops one chest piece per pillar at steel / wyvern / mageweave + exceptional.
- Boss-drop bonus stacks on top of the existing chain-final reward (Slice 4e capstone set still lands deterministically on chain step 10).

### Death penalty (corpse-run)

- Die → respawn at home with **25% HP**.
- Your corpse stays at the death site for **10 minutes**.
- Walk back to it (3u proximity) → full HP restored.
- Visual marker is post-MVP — players navigate from memory of their death position.

### Server-side accounts (Slice 8e)

- **SQLite** at `~/.config/vaern/server/accounts.db` with bcrypt-hashed passwords.
- **Case-insensitive uniqueness** on username + character name.
- **Client login/register/create-character UI** behind `AppState::Authenticating` + `CharacterSelect`.
- **`CharacterSummary` populated from `PersistedCharacter`** so the roster shows real race/pillar/level.
- **Gated by `VAERN_REQUIRE_AUTH=1`** (default off so the dev loop keeps working without credentials).

### Map editor (`vaern-editor`)

Standalone Bevy binary, sibling of `vaern-client`. Authoring tool for the same world data the runtime reads — saved edits round-trip into the live game.

```bash
cargo run -p vaern-editor -- --zone dalewatch_marches
```

- **Free-fly camera** (WASD + Q/E + RMB-look + scroll-speed) over the active zone.
- **Voxel terrain sculpt** — Mode 3: LMB carves (Subtract), Shift+LMB raises (Union), inspector slider for radius (0.5–32u). Uses the same `EditStroke::apply` pipeline as the runtime F10 stomp.
- **Asset placement** — Mode 2 + palette: pick a Poly Haven slug from the left panel, LMB on ground spawns a new prop into the nearest hub at hub-local offset.
- **Selection + edit** — Mode 1: LMB picks a prop via scene-mesh AABB raycast (handles big stretched assets like castle doors). Inspector edits offset / rotation / scale / Y-override; Delete button or Delete key removes.
- **Biome paint** (Mode 4) — sub-cell brush at 8m resolution (4×4 cells per chunk). LMB-drag continuous paint, `[`/`]` resize, `B` paint mode, `E` erase (revert to default Marsh), `I` arm eyedropper. Inspector picks shape (Circle/Square), falloff, biome. Brush footprint shown as immediate-mode gizmo on the terrain. 9-channel per-vertex weights blend in a custom `ExtendedMaterial<StandardMaterial, BiomeBlendExt>` shader — every chunk uses one shared material handle, no per-vertex flat-interp variance, no chunk-boundary color lines.
- **Diagnostic panel** — left "Environment" panel exposes: live `ChunkStore` size + dirty queue + in-flight async tasks + render entities + drawn-after-frustum-cull count + FPS / frame time + per-system µs timings (rolling 1s window, sorted by mean) + isolation toggles (hide-chunks / skip-eviction / skip-streamer / disable-biome-blend / debug-viz mode).
- **Performance** — chunk seed and attribute attach are rate-limited to keep frame time bounded during fill (256 seeds / 16 mesh tasks / unbounded attribute pass per frame). `VoxelChunk` is now sparse (`Uniform(f32)` vs `Dense(Box<[f32]>)`) — air/solid stack chunks shrink from 157 KB to 4 bytes. Async meshing runs on `AsyncComputeTaskPool`. Default draw distance is 16 chunks (~512m radius); slider goes to 64.
- **Save** — toolbar button writes both:
  - `src/generated/world/voxel_edits.bin` — bincode `Vec<ChunkDelta>` of every chunk that diverged from the heightfield baseline.
  - `src/generated/world/biome_overrides.bin` — `OverridesFileV2`: sub-cell-keyed biome paint state with N=4 sub-cells per chunk. Legacy V1 (per-chunk-XZ) files auto-upscale on load.
  - `src/generated/world/zones/<zone>/hubs/<hub>.yaml` — the `props:` array spliced into each touched hub's YAML via `serde_yaml::Value` (preserves all other fields).
- **Round-trip to runtime** — server reads `voxel_edits.bin` on Startup and registers every chunk in the existing `EditedChunks` set, so connecting clients receive the deltas through the established `queue_reconnect_snapshots` path. Hub YAML edits land via the existing client `OnEnter(InGame)` reader. Biome overrides are editor-only for now (runtime client still uses the legacy single-`StandardMaterial`-per-chunk path).

**Bundle splitting** — `scripts/split_polyhaven_bundle.py` peels a multi-mesh Poly Haven glTF into one glTF per top-level node, sharing the original `.bin` + textures via relative URIs. Already run on `modular_fort_01` (22 piece slugs in the catalog: tower_round + thick/thin walls + walkways + stairs).

**Open scars**:
- `voxel_edits.bin` can balloon (saw 1.3 GB in one session) because `diff_against_generator` walks every chunk in `ChunkStore` rather than only chunks that have actually been edited. Reset via `rm src/generated/world/{voxel_edits,biome_overrides}.bin`.
- **Random crash** in current build — symptom unconfirmed (no stack trace captured yet). To repro: launch editor, load a saved world with edits, fly around / sculpt / paint — crash strikes intermittently. Diagnostic logs in the console (loader summary + streamer first-frame + `halo-synced N pairs`) capture the load path, but the actual fault path is still unknown.

**Stubbed** (slots reserved): scatter preview, voxel undo for biome paint (snapshot infra is in place), transform gizmo, splatmap upgrade for biome blend.

### Hostable build (Slice 8)

- **Netcode key** resolved from `VAERN_NETCODE_KEY` (release rejects unset / all-zero / wrong-length).
- **Server bind** via `--bind` / `VAERN_BIND` (default `0.0.0.0:27015`).
- **Client target** via `--server` / `VAERN_SERVER`.
- **Server panics** write a forensics report to `~/.local/share/vaern/server/crash_<unix_ts>.log`.
- **Client auto-reconnect** with exponential backoff (1s → 2s → 4s → 8s, 5 attempts max) when the lightyear `Connected` marker is removed mid-game. Replays the last successful credentials so a server bounce under `VAERN_REQUIRE_AUTH=1` resumes without a re-prompt. Falls back to MainMenu on auth failure or exhausted attempts.

---

## Quick start

```bash
# One server, any number of clients
./target/debug/vaern-server                         # terminal 1
./target/debug/vaern-client                         # terminal 2 — goes through the menu
VAERN_CLIENT_ID=1001 ./target/debug/vaern-client    # terminal 3 — second client

# Or the dev-fast script that skips the menu via env vars
./scripts/run-multiplayer.sh
```

### In-game controls

**Movement & camera**
- `WASD` — camera-relative movement (W = forward relative to camera)
- Mouse — camera yaw/pitch (cursor locked); scroll — zoom
- `LeftAlt` — hold to free cursor for UI clicks + disable mouse-look

**Combat**
- `LMB` — light attack (fast cone, 0.5s cd)
- `RMB` — heavy attack (0.4s windup cone, 1.5s cd)
- `1-6` — hotbar abilities (no GCD; chain freely with LMB/RMB)
- `7 8 9 0` — consumable belt (bound potions, quaffable mid-fight)
- `Q` — hold for Active Block (drains stamina; 60% frontal damage reduction)
- `E` — tap for Active Parry (0.35s window, 20 stamina on successful negate)
- `Tab` — cycle target (40u, prefers camera front cone, QuestGivers + Vendors excluded)
- `Esc` — clear current target / close focused panel

**Panels & interaction**
- `I` — inventory + paper doll (wallet shown under Inventory heading)
- `C` — character / stat screen
- `K` — spellbook
- `L` — quest log
- `G` — loot nearest container within 5u
- `H` — harvest nearest resource node within 3.5u
- `F` — talk to nearest quest-giver OR open nearest vendor's Buy/Sell window (≤5u)
- `V` — toggle nameplates + chat bubbles (gated on chat focus — typing "V" in chat is safe)
- `☰` top-right — logout / quit

**Chat**
- `Enter` — open chat input. Type + Enter sends. `Esc` cancels.
  - No prefix = `/say` (20u). `/z` = zone. `/p` = party. `/w <name>` = whisper.
  - `/invite <name>`, `/leave`, `/kick <name>` — party commands (also work from this input).
  - `/wave /bow /sit /cheer /dance /point` — emotes (third-person bubble: "Brenn waves.").
  - **While the chat input has focus, WASD / hotbar / Tab / Q / E / K / V are all suppressed.**

**Debug**
- `F10` — voxel stomp: carve a 6u-radius sphere crater at the camera's forward focus.

---

## Architecture

Workspace of eighteen crates + modular client + modular server + standalone editor.

### Crates

```
crates/
├── vaern-core/       Pillar, ClassPosition, Morality, Faction, School,
│                     DamageType (12 variants), terrain height field,
│                     Voronoi partition + Catmull-Rom spline (voronoi.rs)
├── vaern-voxel/      chunked SDF voxel world (hand-rolled, not fast-surface-nets)
├── vaern-data/       YAML loaders: schools, classes, abilities, flavored,
│                     bestiary, races, world, dungeons, quest chains
├── vaern-protocol/   SharedPlugin: lightyear registration, channels, every
│                     network message + replicated component
├── vaern-combat/     Bevy plugin: Health, Stamina, abilities + AbilityShape,
│                     Casting, Projectile, damage.rs, effects.rs, anim.rs
├── vaern-character/  Experience, PlayerRace, XpCurve (leaf)
├── vaern-stats/      Pillar identity + 3-tier stat pool, CombinedStats
├── vaern-items/      Compositional model: ItemBase × Material × Quality ×
│                     Affix → ResolvedItem; ContentRegistry
├── vaern-economy/    Vendor pricing math; GoldSinkKind ledger enum
├── vaern-equipment/  20-slot paper doll (+ Focus); validate_slot_for_item
├── vaern-inventory/  PlayerInventory: slot grid, stack merging
├── vaern-loot/       Drop tables + roll_drop; rarity emerges from material
│                     + quality; affix pool filtered by base + tier
├── vaern-professions/Profession enum (11), ProfessionSkills, NodeKind (15)
├── vaern-server/     UDP server: data / connect / npc / quests / xp /
│                     player_state / combat_io / movement / starter_gear /
│                     stats_sync / inventory_io / loot_io / consume_io /
│                     belt_io / resource_nodes / aoi / voxel_world /
│                     wallet_io / vendor_io / chat_io / party_io / respawn
├── vaern-client/     DefaultPlugins + 22 focused modules (see below)
├── vaern-sim/        headless deterministic sim — reserved for PPO training
├── vaern-assets/     shared Bevy plugin: Meshtint + Quaternius + UAL animation
├── vaern-museum/     two bins: vaern-museum (composer) + vaern-atlas (taxonomy)
└── vaern-editor/     standalone Bevy authoring tool: voxel sculpt, prop placement,
                      hub YAML write-back, voxel-delta save-to-disk → runtime load
```

#### `vaern-voxel` detail

`sdf/` (Sphere/BoxSdf/Capsule/Plane + Union/Subtract/Intersect/SmoothUnion/SmoothSubtract) · `chunk/` (32³+1 padding = 34³ samples, sparse `HashMap` store, sparse `VoxelChunk` storage with `Uniform(f32)` / `Dense(Box<[f32]>)` enum, `DirtyChunks`) · `mesh/` (4 swappable algorithm layers: IsoSurfaceExtractor + VertexPlacement + NormalStrategy + QuadSplitter + MeshSink) · `edit/` (Brush + EditStroke with halo sync) · `generator/` (HeightfieldGenerator bridges `terrain::height`) · `query/` (ground_y + raycast) · `replication/` (ChunkDelta FullSnapshot | SparseWrites, version-numbered + replay-safe) · `perf/` (per-system frame-time profiler). Async meshing on `AsyncComputeTaskPool`. **87 tests pass.**

Two load-bearing fixes: `ChunkShape::MESH_MIN = PADDING - 1` to close static chunk seams; `chunks_containing_voxel` enumeration extended from `{-1, 0}` to `{-1, 0, +1}` so halo writes propagate across chunk boundaries (without it, a textured "cap" floats over every carved crater).

#### `vaern-server::respawn` detail

Corpse-run death penalty: spawn server-only `Corpse` entity at death pos, 25% HP respawn, walk-back restoration at 3u proximity, 10-min expiry. `CorpseOnDeath` marker makes the shared `apply_deaths` skip players.

### Client modules

All gated on `AppState::InGame`; `main.rs` is ~80 lines.

```
src/
├── main.rs          App bootstrap + plugin registration only
├── shared.rs        marker components, attach_mesh / attach_character
├── menu.rs          egui main menu · char create/select · ☰ logout
├── net.rs           lightyear client entity + ClientHello (race_id)
├── scene.rs         mouse-look camera, 3D ground/light, own-player mesh,
│                    CastFiredLocal relay, AnimState overlay + driver
├── input.rs         WASD + motion-controller yaw, LMB/RMB + 1-6 cast,
│                    Tab cycle / Esc clear
├── hotbar_ui.rs     egui hotbar + spellbook + icon cache
├── attack_viz.rs    shape telegraph flashes + projectile mesh rendering
├── unit_frame.rs    top-left player frame (portrait/name/L#/HP/XP)
├── combat_ui.rs     Bevy-native cast bar + target frame + swing flash
├── vfx.rs           impact flashes, cast-beam gizmos, gold target ring
├── nameplates.rs    world-space HP plates (DisplayName label, 60u cull,
│                    V-toggle) + floating damage numbers + "!" quest-giver
│                    markers + chat speech bubbles
├── hud.rs           compass strip
├── quests.rs        loads chain YAMLs, drains QuestLogSnapshot
├── interact.rs      [F] quest-giver dialogue, [L] quest log
├── inventory_ui.rs  [I] inventory + equipment + wallet line
├── vendor_ui.rs     [F] vendor Buy/Sell window, NearbyVendor detect
├── chat_ui.rs       Enter input + 50-line history + prefix parser +
│                    ChatInputFocused gate + ChatBubbleEvent emit
├── party_ui.rs      Party frame + invite popup + party-command parser
├── belt_ui.rs       4-slot consumable belt strip (keys 7/8/9/0)
├── loot_ui.rs       [G] loot window + pending-loot gizmo markers
├── stat_screen.rs   [C] character stats (pillars + CombinedStats)
├── harvest_ui.rs    [H] resource-node markers + harvest-proximity
├── voxel_biomes.rs  BiomeResolver: nearest-hub biome table
├── voxel_demo.rs    Voxel ground plugin: streams 11×3×11 chunk cube,
│                    attaches per-biome StandardMaterial, F10 stomp
├── level_up_ui.rs   Centered "LEVEL UP" banner + screen-flash overlay
├── scene/dressing.rs  Loads world YAML, walks scatter rules + props,
│                     deterministic Poisson scatter (splitmix64)
└── diagnostic.rs    periodic snapshot + connect/disconnect logs
```

### Dependency graph (roughly)

- `vaern-core` → nothing
- `vaern-voxel` → core (bridges `terrain::height` via HeightfieldGenerator; bevy 0.18, serde, thiserror only — no fast-surface-nets / ndshape / glam)
- `vaern-combat` → core + stats
- `vaern-character` → core (leaf)
- `vaern-stats` → core (leaf)
- `vaern-items` → core + stats (re-exports SecondaryStats for affix stat_delta)
- `vaern-economy` → items
- `vaern-equipment` → items
- `vaern-inventory` → items
- `vaern-loot` → items + combat (for NpcKind)
- `vaern-professions` → bevy + serde only (leaf)
- `vaern-protocol` → everything above
- `vaern-server` / `vaern-client` / `vaern-sim` → all of the above + data

### Networking model

- Server-authoritative over UDP via lightyear + netcode.
- **Shared 32-byte private key** resolved at boot via `vaern_protocol::config::resolve_netcode_key`: release builds require `VAERN_NETCODE_KEY` (hex) and reject all-zero / wrong-length; debug builds fall back to a zero dev key with a warning.
- **Server bind** from `--bind <addr>` / `VAERN_BIND` (default `0.0.0.0:27015`). **Client target** from `--server <addr>` / `VAERN_SERVER` (default `127.0.0.1:27015`).

#### Replicated components

`Transform` (prediction + linear/slerp interpolation), `Health`, `ResourcePool`, `Casting` (MapEntities), `Experience`, `PlayerRace`, `PlayerTag`, `DisplayName`, `NpcKind`, `QuestGiverHub`, `ProjectileVisual`, `NodeKind`, `NodeState`, `AnimState`.

#### Messages

- **Combat**: `ClientHello` (C→S), `CastIntent` (C→S, MapEntities), `StanceRequest` (C→S: `SetBlock(bool)` / `ParryTap`), `CastFired { caster, target, school, damage }` (S→C, MapEntities), `HotbarSnapshot` (S→C).
- **Quests**: `AcceptQuest` / `AbandonQuest` / `ProgressQuest` (C→S), `QuestLogSnapshot` (S→C).
- **State**: `PlayerStateSnapshot` (S→C every tick — HP/pool/XP/cast + pillar scores/caps/banked XP × 3 + stamina + is_blocking + is_parrying).
- **Inventory + equip**: `InventorySnapshot`, `EquippedSnapshot` (S→C on change); `EquipRequest`, `UnequipRequest` (C→S).
- **Loot**: `PendingLootsSnapshot` (S→C on `PendingLootsDirty` flag), `LootWindowSnapshot` / `LootClosedNotice` (S→C), `LootOpenRequest` / `LootTakeRequest` / `LootTakeAllRequest` (C→S).
- **Harvest**: `HarvestRequest` (C→S, MapEntities). Node state via component replication.
- **Voxel edits**: `ServerEditStroke { center, radius, mode }` (C→S); `VoxelChunkDelta(ChunkDelta)` (S→C). Server applies via `EditStroke::new(SphereBrush).apply()`; broadcasts up to 8 deltas/tick; reconnecting clients catch up at 4/tick.
- **Wallet + vendors**: `WalletSnapshot` (S→C on `Changed<PlayerWallet>` only). `VendorOpenRequest` / `VendorBuyRequest` / `VendorSellRequest` (C→S). `VendorWindowSnapshot` / `VendorClosedNotice` (S→C).
- **Chat**: `ChatSend { channel, text, whisper_target? }` (C→S). `ChatMessage { channel, from, to, text, timestamp_unix }` (S→C). Server stamps `from` from sender's `DisplayName`. Rate-limited 5/sec on rolling 1s window.
- **Party**: `PartyInviteRequest` / `PartyInviteResponse` / `PartyLeaveRequest` / `PartyKickRequest` (C→S). `PartyIncomingInvite` / `PartySnapshot` / `PartyDisbandedNotice` (S→C). Snapshot broadcast is dirty-set gated, not per-tick.

#### Area-of-interest replication

One lightyear `Room` per starter zone. NPCs + resource nodes carry `NetworkVisibility` and join their zone's room at spawn; each client's link migrates between rooms as its player crosses zones. Players + projectiles stay globally visible. Pre-AoI, 603 NPCs × 60Hz Transform replication saturated the kernel UDP buffer on localhost and caused NPC rubber-banding. **`RoomPlugin` must be added explicitly** — it's not in lightyear's `SharedPlugins`.

#### Prediction & own-player state

- Own player on a `Predicted` copy; `buffer_wasd_input` → `ActionState<Inputs>` with `camera_yaw_mrad` bundled.
- **Own-player state via message, not replication.** Lightyear 0.26 gives the owning client only a `Predicted` copy — filter `(With<Replicated>, Without<Predicted>)` matches zero. HP/pool/XP/cast/stamina/stance + inventory + equipped + pending-loots all push via per-tick messages.
- Dynamic insertion/removal of predicted components (e.g. `AnimOverride`) is also unreliable on the Predicted copy, so **own-player transient animation flashes are driven client-side from the `CastFired` message** — server sets the flash, sends `CastFired { caster, … }`, client inspects `caster == own_player` and stamps `AnimState::Attacking` + a local `AnimOverride`.
- **`CastFired` local relay.** Lightyear's `MessageReceiver::receive()` drains on read. A single `relay_cast_fired` system is the sole `MessageReceiver<CastFired>` reader and re-emits via `MessageWriter` as a Bevy-local `CastFiredLocal`. All downstream consumers (vfx, nameplates, animation, diagnostics) read `MessageReader<CastFiredLocal>`.

#### Other

- **Loot containers are server-only** (not replicated). Clients see them only through `PendingLootsSnapshot` summaries owned by the top-threat player.
- **Respawnable component** on players resets HP/position/pool instead of despawning. Players carry `CorpseOnDeath`, which makes the shared `apply_deaths` skip them — `respawn::apply_player_corpse_run` is the sole player-death handler.
- **Server tick-rate logger** prints `[tick] 60 Hz  avg_frame=16.72ms  max_frame=16.74ms` each second; catches Update-loop stretch.

### Combat model

- **Abilities are entities** with `AbilitySpec` (damage, cooldown_secs, cast_secs, resource_cost, school, threat_multiplier, range, shape, aoe_radius, cone_half_angle_deg, line_width, projectile_speed, projectile_radius, applies_effect), `AbilityCooldown`, `Caster`.
- **Shapes**: `Target`, `AoeOnTarget`, `AoeOnSelf`, `Cone`, `Line`, `Projectile`. Friendly fire on.
- **No GCD** — per-ability cooldowns only.
- **Projectiles** server-simulated in `FixedUpdate::tick_projectiles` with swept-sphere collision.
- **Channeled casts snapshot `range`** onto `Casting` so cones/lines/projectiles stay bounded.

#### Stats-aware damage pipeline (`vaern-combat::damage`)

`compute_damage` → `apply_stances`:

1. **Caster**: weapon min/max dmg roll (physical schools), `(melee_mult + spell_mult) × 0.5` global multiplier, crit roll against `total_crit_pct` → ×1.5. Reads caster's `CombinedStats` if present.
2. **Target**: armor mitigation `armor / (armor + 200)`, per-channel resist `resist_total[dt] × 0.005` (capped 80%, supports negative for vulnerability amplification).
3. **Stance layer** (`apply_stances`): active Parry → full negate (damage → 0, consumes parry, debits stamina); active Block → frontal/flank/rear damage reduction based on caster→target hit angle.
4. **Rider effects**: if `final_damage > 0` and the ability has `applies_effect`, attach the DoT / Slow. Parried / blocked-to-zero hits don't apply riders.
5. School → DamageType lookup covers physical (blade→slashing, blunt→bludgeoning, etc.) and magical (fire/cold/light/shadow/frost/arcane/etc.) identically.
6. Called at all three damage sites via `resolve_hit`: instant `select_and_fire`, channeled `progress_casts` completion, `tick_projectiles` hit.
7. Missing `CombinedStats` on either side falls through to raw damage.

**NPC stats**: `npc_combined_stats(creature_type, armor_class, NpcKind)` derives armor (inverse of mitigation formula from `physical_reduction`) + per-channel resists (magical base from `magic_reduction` + per-school bumps). Rarity mult: Combat 1.0× / Elite 1.25× / Named 1.5×.

**Pillar XP on cast**: every `CastEvent` credits XP to the caster's pillar via `GameData.schools` lookup; dedupe by (caster, ability) per frame so AoEs don't multiply. `sync_hp_max_to_pillars` updates Health.max on pillar gain, preserving HP-fraction.

**CombinedStats denormalization**: `sync_combined_stats` watches `Changed<Equipped> | Changed<PillarScores>`, resolves every equipped `ItemInstance`, folds `SecondaryStats` + `DerivedPrimaries` + (zeroed) `TertiaryStats` into `CombinedStats` as a Component.

**NPC AI**: per-mob `AggroRange` + `LeashRange` (8u common / 11u elite / 14u named), threat-table target selection, `RoamState` wander, leash warp-home + HP reset on over-extend.

### Target lock + motion controller

- **Target selection**: `Tab` cycles combat NPCs within 40u, prefers camera's front cone (80° half-angle). Falls back to nearest-overall in-range. Filters out `NpcKind::QuestGiver`. `Escape` clears. Stale targets (despawned) clear next frame.
- **Smooth follow**: while locked, camera yaw + mesh rotation drift toward target via a kinematic motion controller (brake-plan velocity capped by √(2·a·d)). Mouse yaw suppressed; pitch still mouse-driven.
- **Motion params** (`input.rs`): `IDLE_TURN_RATE = 0.3 rad/s`, `CAST_TURN_RATE = 12 rad/s`, `TURN_ACCEL = 20 rad/s²`.
- **On cast** (any `CastAttempted`): velocity kicks to `min(brake_peak, CAST_TURN_RATE)` — a ~0.26s swoosh on 180°, not a teleport.

### Status effects + stances + stamina

- **`StatusEffects(Vec<StatusEffect>)`** on every combat-capable entity. Variants: `Dot { damage_per_tick, school, threat_multiplier }`, `Stance(Block | Parry)`, `Slow { speed_mult }`, `StatMods { damage_mult_add }`. Refresh-on-reapply. `tick_status_effects` decrements, fires DoT ticks, drains Block stamina, auto-removes the component when empty.
- **Active Block (Q hold)** — `StanceRequest::SetBlock(true/false)` on press/release. Drains 15 stamina/s. 60% frontal → 25% flank → 0% rear. Breaks at zero stamina. Refused if pool already empty.
- **Active Parry (E tap)** — `StanceRequest::ParryTap` opens 0.35s window. First in-window hit fully negates and blocks rider debuff. Consumes 20 stamina **on the negate**, not on the tap. Parry wins over Block when both active.
- **`Stamina { current, max, regen_per_sec }`** — separate from `ResourcePool` (mana). Players: 100/100, 12/s. Exposed via `PlayerStateSnapshot.stamina_current/max + is_blocking + is_parrying`.
- **YAML-driven effect riders** — flavored variants accept `applies_effect: { id, duration_secs, kind: dot|slow, dps, tick_interval, speed_mult }`. Parsed as `FlavoredEffect` in vaern-data, converted to `EffectSpec` in `apply_flavored_overrides`. Seeded: fire→burning, frost→chilled, shadow→decay, blood→bleeding at tiers 25 + 50.
- **Slow-aware movement**: `StatusEffects::move_speed_mult()` returns the strongest (lowest) `Slow.speed_mult`. Doesn't stack — deepest wins.

### Animation state

- **`AnimState`** enum replicated: `Idle / Walking / Running / Casting / Blocking / Attacking / Hit / Dead`.
- **`derive_anim_state` in FixedUpdate** — priority `Dead > Blocking > Casting > Running > Walking > Idle` from Transform-delta speed + Casting + StatusEffects + Health. XZ-projected speed thresholds: walk = 0.5 u/s, run = 3.0 u/s.
- **Transient flashes**: `mark_attack_and_hit` reads each `CastEvent` — flashes caster to `Attacking`, target to `Hit` (only when `damage > 0` and target ≠ caster). Paired with `AnimOverride { remaining_secs: 0.25 }`. `tick_anim_override` removes when expired.
- **Visualized** as a small grey `[idle]` / `[casting]` / `[running]` etc. tag under every nameplate.

### Gear & loot flow

1. **Mob dies** → server rolls drop via `vaern-loot::roll_drop` against `DropTable::for_npc(kind, tier)`. Rarity emerges from rolled material + quality.
2. **Server spawns** a `LootContainer` at mob position, owned by top-threat player. Not replicated; carries contents + despawn timer.
3. **Client** receives `PendingLootsSnapshot` per tick → pulsing yellow gizmo at each position.
4. **Walk in range (5u)** → `G` → `LootOpenRequest` → `LootWindowSnapshot` → egui window.
5. **Click** an item or "Take all" → `LootTakeRequest` / `LootTakeAllRequest` → server moves stack to `PlayerInventory` → broadcasts updated `InventorySnapshot` + `LootWindowSnapshot`. Full-inventory items stay in container.
6. **Container auto-despawns** at 5min or when empty (sends `LootClosedNotice`).

### Item resolution pipeline (`ContentRegistry::resolve`)

Given `ItemInstance { base_id, material_id, quality_id, affixes }`:

1. Look up `ItemBase`, `Quality`, optional `Material`. Unknown id → `ResolveError::UnknownBase/Material/Quality`.
2. Validate pairing: `base.armor_type ∈ material.valid_for` / `material.weapon_eligible` / `material.shield_eligible`. Fail → `InvalidPairing`.
3. Resolve affixes: look up by id, check `applies_to` matches base kind. Fail → `UnknownAffix` / `InvalidAffix`.
4. Compute `weight_kg`, `rarity` (material.base_rarity + quality.rarity_offset clamped), `stats` (base kind's scaling × material × quality, then per-affix `stat_delta` folded).
5. Compose display name: `{quality} {prefixes*} {material} {piece} {suffixes*}`. Compose id: `{quality?}_{material?}_{base}+{affixes...}`.
6. Soulbound = base.soulbound OR any applied affix's `soulbinds: true`.

### World & data

All design data is YAML under `src/generated/`, compiled from Python seed scripts (see `scripts/seed_*.py`). **Bulk writes ≥15 files always go through a seed script, never per-file edits.**

```
src/generated/
├── archetypes/         15 class positions (barycentric M/A/F triangle)
├── abilities/          per-pillar/category ability tiers (25/50/75/100)
├── flavored/           school-flavored variants + per-ability stat overrides
├── schools/            27 schools with morality + pillar
├── factions/           faction-gating rules
├── races/              10 playable races with creature_type refs
├── bestiary/           11 creature_types + 10 armor_classes
├── institutions/ + archetypes/*/orders/   flavored Order system
├── items/              composition tables for the runtime resolver
│   ├── bases/{armor,weapons,shields,runes,consumables,materials}
│   ├── materials.yaml  25 substances (copper → adamantine, linen → voidcloth)
│   ├── qualities.yaml  7 craft-roll tiers (crude → masterful)
│   └── affixes.yaml    27 affixes (11 suffix, 6 elemental banes, 5 prefixes,
│                       5 shard-only soulbinding)
└── world/
    ├── world.yaml + progression/
    ├── biomes/, continents/, zones/<id>/, dungeons/<id>/
```

**Item seeder** (`scripts/seed_items.py`) is a package — `scripts/items/{armor,weapons,shields,runes,consumables,crafting,materials,qualities,affixes}.py` — each module owns its table + `seed()`.

**Totals**: 28 zones · 79 hubs · 612 mobs · 32 dungeons · 105 bosses · 30 quest chains (28 main + 2 side) · 11 creature_types · 15 class kits · 222 item bases · 25 materials · 7 qualities · 27 affixes.

**Quest schema** (chain YAML): hand-curated chains have an `npcs:` registry naming each contact + their hub + dialogue; steps reference NPCs by id (e.g. `npc: warden_telyn`). Procedural chains still work via `target_hint` parsing at the capital hub.

**Chain hand-curation status**: `dalewatch_marches` (mannin/human) is the showcase zone — fully hand-curated. Other 9 starter zones use procedural target_hints until curated.

**Hub placement schema**: hub YAMLs accept an optional `offset_from_zone_origin: { x, z }` for big-zone layouts. Zones without it keep the legacy 8u-radius tight layout. Non-hub sub-zones live in `landmarks.yaml` (used as display hints for `investigate`-step `location:` targets).

---

## Design principles

- **Abstract first, flavor second.** Math (class position, capability tiers, school mechanics) is faction-neutral. Flavor (faction names, order affiliations, player-facing class names) is a separable layer.
- **Math-first, sim-validated balance.** Combat simulator will use PPO-trained rotations to validate class parity. Outcome equivalence, not hand-tuning.
- **Mechanical vs narrative identity.** ~30 sim profiles are the balance budget. Flavor variants (Orders, race skins, named identities) are unlimited on top.
- **Strict morality gating.** No oxymorons (no undead priests). Evil schools → evil faction; good → good; neutral → both. Each mechanical role has ≥1 morally-accessible school per faction.
- **Hybrid-first classes.** Most classes are dual-role-capable; pure tank/heal/DPS are "advanced cooperative" designated.
- **Strict coop, no solo content.** Target: close-friend / household groups. Every activity requires ≥2 players. Combat is continuous action-style (New World reference), not tick-based.
- **Bestiary inheritance.** Every mob and playable race references a `creature_type` (beast / humanoid / undead / demon / aberration / elemental / construct / fey / giant / dragonkin / living_construct). HP scaling, default armor, resistances, school affinities all inherit from the type. Validator catches "light-devotion ashwolf" / "poison golem" incoherence.

---

## Class position system

Every character sits at a position in a quantized barycentric triangle:

- **Might** — physical: armor, weapons, endurance, threat
- **Arcana** — magical: spells, rituals, wards, control
- **Finesse** — cunning: stealth, precision, evasion, crafting

Each pillar ∈ `{0, 25, 50, 75, 100}`, summing to 100. **15 valid positions.**

Internal labels (Fighter, Paladin, Cleric, Druid, Wizard, Sorcerer, Warlock, Bard, Rogue, Ranger, Monk, Barbarian, Duskblade, Mystic, Warden) are dev-facing only; player-facing names come from faction/Order flavor.

---

## Testing

```bash
cargo test --workspace
```

**496 tests pass.** Coverage: class position invariants, combat parity (GCD-aware), stats-aware damage pipeline, YAML loads, item composition, affix validation, loot drops, inventory stacking, equipment slot validation, economy / wallet, profession skills, NPC stat derivation, party split-XP, chat rate-limit + parser, persistence round-trip; plus the slice 1-9 additions (PolyHavenCatalog / dressing / scatter / side-quest givers / mob banding / level XP curve / emote parser / corpse-run / netcode-key / panic-handler / auto-reconnect / SQLite accounts / quest polish); plus Slice 6:

- **boss-drop loader** (3) — Valenn 12-piece, Halen 3-piece, unknown-mob = none
- **`decide_roll_winner`** (6) — need beats greed, single-need auto-win, single-greed when no need, tied-need d100, tied-greed d100, all-pass = no winner, empty = no winner
- **`RollItemState::all_voted`** (1)
- **`eligible_for_roll`** (3) — in-radius partners + killer, killer-not-in-party, non-party-in-radius
- **YAML guards** — Halen L9, Valenn L10, drifters_lair dungeon yaml, step 10 targets Valenn at L10

**4 pre-existing `vaern-combat` failures** (`attacker_kills_dummy`, `resource_gate_delays_kill`, `parity.rs` × 2) all stem from `apply_deaths` being moved to the server-only schedule — the `common::headless_app` test harness loads only the shared `CombatPlugin` which has `detect_deaths` without its follow-up despawn. **Unrelated to runtime gameplay.**

Re-seed items:

```bash
python3 scripts/seed_items.py
```

---

## Open TODOs

### Design

- [ ] **Faction naming** — bind `faction_a` / `faction_b` placeholders to Concord / Rend
- [ ] **Order system delivery** — in-world organizations that teach schools; how you join
- [ ] **Progression mechanics** — how characters move between class positions
- [ ] **Numeric balance** — damage, CDs, cast times, resistance multipliers (sim-driven)
- [ ] **Race × class modifiers** — small racial tweaks on class stats
- [ ] **Blood counterpart beyond devotion** — audit remaining evil-school mechanical gaps

### MMO-feel (pre-alpha Tier-1)

- [x] **Currency loop** — `PlayerWallet` + coin drops + quest gold + `WalletSnapshot` on change. Persisted as `PersistedCharacter.wallet_copper`.
- [x] **Live vendor NPCs** — 10 general-goods vendors at starter capitals.
- [x] **Text chat** — Say (20u) / Zone (AoI room) / Whisper / Party / System; rate-limited 5/sec; 256-char truncate; server-authoritative `from`.
- [x] **Party system v1** — invite/accept/leave/kick by name, dirty-set snapshot broadcast, party frame with member HP, shared XP within 40u, party chat cross-zone.
- [x] **Player nameplates** — `DisplayName`, 60u culling, V-toggle, chat-input-aware gating.
- [x] **Chat bubbles** — 5s speech balloons on Say + Zone only, 1s fade, 72-char truncate.
- [x] **World dressing** (Slice 1) — Poly Haven scatter + ~55 authored Dalewatch hub props.
- [x] **Mob level banding** (Slice 3) — Dalewatch L1-2/3-4/5-6/7+ tiers; per-kind respawn 3min/10min/30min.
- [x] **Felt level progression** (Slice 4a-c) — `level_xp_multiplier` curve, pillar-point on level-up, "LEVEL UP" banner + flash.
- [x] **Text emotes** (Slice 7) — `/wave /bow /sit /cheer /dance /point` ride chat-bubbles.
- [x] **Death penalty** (Slice 5) — corpse-run MVP: 25% HP respawn, walk back for full restore, 10-min expiry.
- [x] **Drifter's Lair pseudo-dungeon** (Slice 6, code-complete + tests green, awaits 2-client playtest).
- [ ] **Shipping hardening** (Slice 8) — env netcode key + configurable bind + panic handler + auto-reconnect + local SQLite accounts.

### Quest + content gaps

- [x] Dalewatch Marches redesigned to full starter-scale scope.
- [x] Gold / item quest rewards — `gold_reward_copper` + `gold_bonus_copper` wired.
- [x] **Side-quest givers spawn** (Slice 2). Dalewatch seeded with 5 (Hayes / Morwen / Bel / Garrick / Pell).
- [x] **Level-gated quest accept** (Slice 4d). Server hard-refuses if `chain.steps[0].level > player.level + 3`.
- [ ] Hand-curate remaining 9 starter chains — **out of pre-alpha scope** (Mannin-only spawn).
- [ ] Auto-advance talk/investigate/deliver objectives (kill-step works).
- [x] Quest state persistence — server `QuestLog` persists via `PersistedCharacter.quest_log`.
- [ ] Quest item rewards (Slice 4e) — only XP + gold today; rolled-item rewards pending. **Blocks Slice 6.**
- [ ] Multi-kill objectives (`count > 1`) — currently advance on first kill.

### Gear / loot / crafting next steps

- [ ] **Boss shard items** — `ItemKind::Shard { affix_id }` droppable by specific bosses, consumable at a crafter rite to imprint the shard's affix onto an item with open slots (converts to BoP).
- [ ] **Crafter rite + recipe system** — apply shards, reroll affixes, fill slots, rarify. Recipes YAML per profession.
- [ ] **Gathering polish** — skill gains on harvest, tool requirement, world-authored node placements per zone.
- [ ] **Crafting professions wired** — Alchemy first, then Blacksmithing / Leatherworking / Tailoring / Enchanting / Jewelcrafting / Bowyery.
- [ ] **Order tier sets** — per-order materials ("Frostsilver") + rite-only acquisition + unique set-bonus mechanics.
- [ ] **Item icons** — keyed by `base_id`, same pipeline as hotbar icons.
- [ ] **Drag-and-drop** inventory ↔ paper doll.

### Combat depth

- [x] **DoTs / status effects** — `StatusEffects` infra + YAML riders (fire/frost/shadow/blood seeded). Slow-aware movement.
- [x] **Active Block / Active Parry stances** — Q/E bindings; stance-aware damage pipeline; parry blocks rider debuffs.
- [x] **Animation state** — replicated `AnimState` + derive + transient flash on attack/hit.
- [x] **Haste → cooldown/cast reduction** — `vaern_stats::formula::cast_speed_scale(h) = 1/(1+h/100)`.
- [x] **Generic buffs (StatMods)** — consumables push timed StatMods. Elixirs of Might/Finesse/Arcana/Giants seeded.
- [ ] **Threat decoupled from damage** — `threat_multiplier` exists but scales off damage; tanks should hold aggro while dealing less.
- [ ] **Ability-category shape tuning** — `might/offense` hand-tuned; rest fall back to defaults.

### Voxel world

- [x] **`vaern-voxel` crate landed** — 8 swappable algorithm layers, sparse `VoxelChunk` storage, async meshing, 87/87 tests.
- [x] **Client streaming + F10 stomp** — voxels stream around camera, F10 issues server-authoritative edit.
- [x] **Server-authoritative edits** — `ValidatedEditStroke` pipeline.
- [x] **`ChunkDelta` replication** — up to 8 chunks/tick live + 4/tick reconnect catch-up.
- [x] **Retire the legacy ground plane** — 8000u plane + `scene/hub_regions.rs` overlay deleted.
- [x] **Server Y-snap via voxel query** — server `movement` + `npc::ai` and client `predicted_player_movement` all call `vaern_voxel::query::ground_y` with `terrain::height` fallback.
- [x] **Biome-aware voxel materials** — `BiomeResolver` + per-biome cached `StandardMaterial`s. 9 CC0 ambientCG sets.
- [x] **Seam closure** — `ChunkShape::MESH_MIN = PADDING - 1` + `chunks_containing_voxel` `{-1, 0, +1}`.
- [ ] **Chunk eviction** — earlier per-frame distance evictor made the whole 3D scene go dark when enabled (unknown render-pipeline interaction). Disabled. Memory grows monotonically until root-caused.
- [ ] **Zone-scoped delta broadcast** — today every `VoxelChunkDelta` goes to every client.
- [ ] **Sparse delta encoding** — broadcast uses `ChunkDelta::full_snapshot` (~150 KB/chunk). `encode_delta(old, new, writes)` exists in the crate but needs per-sample write tracking through `EditStroke`.
- [ ] **Roads on voxel ground** — recoverable from `git log -- crates/vaern-client/src/scene/hub_regions.rs`; would port as a "dirt-road" biome override along each road path.
- [ ] **Teardown** — chunk entities don't carry `GameWorld`, so they persist across logout.
- [ ] **F10 bandwidth / re-mesh lag** — few-tick visual delay between stomp and textured cap despawning. Just network RTT + `MESHING_BUDGET=64/frame` draining.

### Infrastructure / polish

- [x] **Area-of-interest replication** — zone-scoped lightyear rooms.
- [x] **Per-tick broadcast spam** — InventorySnapshot / EquippedSnapshot / PendingLootsSnapshot gated on change.
- [x] **Server tick-rate logger** — Hz + max-frame telemetry every second.
- [x] **Own-player character mesh** — Quaternius modular outfit driven by equipped armor; gender picker in char-create.
- [x] **Own-player animation** — UAL clip pipeline. Transient one-shot swings hold until clip finishes.
- [x] **Ground pipeline** — chunked SDF voxel world streamed around camera.
- [x] **Atmosphere + fog + bloom + tonemapping + HDR**.
- [x] **Loot container visual** — `assets/extracted/props/Bag.gltf`.
- [x] **Quest-giver humanoid skins** — hashed fallback picks one of 12 Quaternius archetypes.
- [ ] Replace zeroed netcode private key before public exposure.
- [x] **Server-side character persistence** — `PersistedCharacter` JSON; 5s wall-clock flush + save-on-disconnect observer.
- [ ] Zone transitions / portals / dungeon entry UI (32 dungeon YAMLs exist, not instanced).
- [ ] Ground mesh / fancy visuals for resource nodes (still gizmo spheres).
- [ ] HDRI-based skybox + IBL (3 Poly Haven `.hdr` files downloaded; needs equirectangular → cubemap bake).
- [ ] Player-follow / tiled ground (ground is a finite 8000u plane; content past ±4000u would reveal the edge).
- [ ] PPO balance trainer in `vaern-sim`.
- [x] **Remote player + NPC Quaternius mesh** — all visible characters render as Quaternius on the UE-Mannequin skeleton.
- [x] **Weapon overlay on Quaternius rig** — `QuaterniusWeaponOverlay` attaches MEGAKIT props to `hand_r` / `hand_l` bones via `assets/quaternius_weapon_grips.yaml`. MEGAKIT only ships 5 props so bow/staff/wand still render empty.
- [ ] **Clip per weapon / ability category** — `Sword_Attack` is used for every physical cast. UAL has `Sword_Regular_A/B/C + Combo`; bow needs a separate clip set (none ship in UAL).

### Known rough edges

- `Casting` + `AnimOverride` components are registered for prediction but dynamic insertion on the own-player Predicted copy is unreliable in lightyear 0.26. Cast bar + transient-anim flashes are driven by `PlayerStateSnapshot` / `CastFiredLocal` messages instead.
- Auto-attack light/heavy specs are hardcoded blade cones; should branch on equipped weapon school.
- NPCs don't have their own CombinedStats-derived melee damage yet — raw `attack_damage` on the spawn slot.
- Starter gear + hotbar are pillar-keyed (Might / Finesse / Arcana × 1 kit each). Archetype-specific kits land with the archetype-unlock path.
- Paper doll is two columns of slot buttons; no real character silhouette yet.
- Character gender is client-local only; no server-side storage or replication.
- Party HP updates between snapshots rely on join/leave/kick to re-broadcast — a future 500ms heartbeat would keep frame bars live during combat.
- Own player's Replicated + Predicted copies both spawn their own nameplate (double plate over own head in third-person); fix by filtering out own entity on spawn.
- 4 pre-existing `vaern-combat` test failures from `apply_deaths` living on the server-only schedule. Unrelated to gameplay runtime.

---

## World & lore

`src/world_theory.yaml` contains the original design: Vaern island-continent geography, Concord (Veyr, defenders) vs Rend (Hraun, arrivals), race list, 4-layer mystery-revelation system, hardcore death design.

**Deprecated sections** in that file: `classes`, `multiclass_system`, `build_totals` — superseded by the class position system above.

---

## Compendium (static web browser)

Standalone static site at `web/` that browses the entire design corpus —
**10 races · 28 zones (with hubs + landmarks) · 9 biomes · 33 dungeons (with all 107 bosses) · 15 institutions (with 89 orders) · 27 schools · 436 spells** — with hash-routed detail pages, faction-tinted vertical row layouts, and generated atmospheric images per zone / hub / landmark / dungeon / boss / race.

### Data flow

```
src/generated/{races,factions,biomes,zones,dungeons,...}/**/*.yaml
        +
src/generated/world/{zones,dungeons}/<id>/prose.yaml          ← description + prompt overlay
        +
assets/meshy/<slug>/image_*.png                                ← generated landscape / portrait shots
        ↓  scripts/build_web_data.py
web/data.json                                                  ← single ~860 KB blob the SPA fetches
        ↓  fetch + render in browser
web/{index.html, compendium.html, app.js, styles.css}
```

`build_web_data.py` overlays `prose.yaml` (description / prompt / vibe) onto the canonical `core.yaml` for each entity, attaches matching `assets/meshy/` image paths, and emits `web/data.json`. Re-run after any YAML edit.

### Image generation (Meshy.ai)

`scripts/generate_meshy.py` orchestrates Meshy's text-to-image API (`nano-banana-pro` at 1:1; `gpt-image-2` would unlock 3:2 / 2:3 but is account-gated).

**Slug convention:**

```
biome__<id>           biome establishing shot
<zone>__zone          zone establishing shot
<zone>__<hub_id>      hub establishing shot
<zone>__<landmark_id> landmark establishing shot
dungeon__<id>         dungeon interior shot
boss__<id>            boss portrait
race__<id>__<gender>  race portrait
```

Bulk flags scope by `--zone` / `--dungeon` when set, otherwise cover everything:

```bash
# auth check (free)
python3 scripts/generate_meshy.py --ping

# full world pass — 320+ jobs, ~2900 credits, idempotent (skips done slugs)
python3 scripts/generate_meshy.py --all --workers 8

# scoped runs
python3 scripts/generate_meshy.py --zone dalewatch_marches --all-hubs --all-landmarks
python3 scripts/generate_meshy.py --all-bosses --workers 8

# one-shot race portraits via dedicated script
python3 scripts/regen_race_portraits.py
```

Each job writes a PNG, an API-response `task.json`, and the literal `prompt.txt` to `assets/meshy/<slug>/`. `_log.csv` aggregates all runs. Reruns auto-skip slugs that already have an image (override with `--no-skip-existing`). 8 parallel workers cap at 1–3 minutes per Meshy job.

### Local viewing

```bash
# from the repo root, serving web/ as the doc root
python3 -m http.server -d web 8080
# → http://localhost:8080
```

Symlinks under `web/` (`icons → ../icons`, etc.) make the relative asset paths resolve regardless of whether the doc-root is `web/` or the repo root.

### Production image (Docker)

Two-stage build: **Bun** validates `data.json`, minifies `app.js`, and transcodes every PNG/JPG asset (1089 files, ~1.7 GB) to WebP via `cwebp` at per-tree max-side dimensions (icons 256, emblems 384, characters 768, meshy shots 1024, all q82). The `.png` references in app.js + data.json get `sed`-rewritten to `.webp`. **nginx:alpine** serves the result with gzip on text + 30-day immutable cache on images.

| | Source | Final | |
|---|---:|---:|---:|
| Bundle on disk | 1.7 GB | 32 MB | **57× smaller** |
| Image (uncompressed) | — | 92.5 MB | |
| Image (registry compressed) | — | **53 MB** | |

Two variants ship from the same Dockerfile via `--build-arg`:

| Variant | Image | URL prefix | Wordmark |
|---|---|---|---|
| `vaern` (default) | `traagel/vaern-mmo-web:latest` | `/` | `VAERN` |
| `lexi` (parody) | `traagel/vaern-mmo-web-lexi:latest` | `/lexi-returns/` | `NEW WORLD 2: LEXI RETURNS` |

```bash
# build + push (multi-arch via buildx by default)
docker login -u traagel
./scripts/push-web.sh                          # vaern · :latest
./scripts/push-web.sh v0.1.0                   # vaern · :v0.1.0 + :latest
./scripts/push-web.sh --variant lexi           # lexi · :latest
./scripts/push-web.sh --no-push                # local build only, single-arch

# run locally (either variant)
docker run -d --rm -p 8080:80 traagel/vaern-mmo-web:latest
docker run -d --rm -p 8081:80 traagel/vaern-mmo-web-lexi:latest
# lexi root path 302-redirects to /lexi-returns/
```

The lexi variant is fed identity overrides (`SITE_TITLE`, `SITE_PRETTY`, `SITE_TAGLINE_SPLASH`, `BASE_PATH=/lexi-returns/`) via Dockerfile ARGs and `build.ts` substitutes them into HTML + adds `<base href="/lexi-returns/">` + rewrites `world.setting_name` in `data.json` so the runtime overview heading also reads the new name. Same content, different deployment skin.

---

## Memory

Claude Code persistent memory at `~/.claude/projects/-home-mart-git-rust-mmo-project/memory/`. Encodes design principles, working context, and non-obvious architectural decisions established across sessions.
