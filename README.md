# Vaern

Solo-developer hardcore two-faction persistent-coop RPG. Rust · Bevy 0.18 · Lightyear 0.26 · bevy_egui 0.39. D&D 3.5-inspired mechanics. AI-assisted pipeline.

## Status

**Pre-alpha-shaped MMO that reads as a place.** Menu → character create (race / body / pillar) → live 3D world with a gear-driven Quaternius character mesh on a **PBR-dressed Dalewatch** (Poly Haven trees / rocks / shrubs / ground cover scattered across a 1200×1200u box, plus ~55 hand-authored hub props — castle door, iron gates, weapon rack, banners, well, fire pit, market barrels, vendor table) layered onto the voxel ground. Full combat-gear-loot loop, **currency economy with 10 capital-hub vendors**, **text chat with 5 channels + speech bubbles**, **party system with shared XP**, **player nameplates with V-toggle**, **5 side-quest givers in Dalewatch (Quartermaster Hayes / Captain Morwen / Innkeeper Bel / Smith Garrick / Mistress Pell)**, **mob level banding** (L1-2 keep, L3-4 mid, L5-6 outer, L7+ Drifter's-Lair direction) **with per-kind respawn timers** (commons 3 min, elites 10 min, named 30 min), **felt level progression** — kill XP scales by mob-vs-killer level (greys give 0, +5 reds give 1.5×), every level-up grants +1 pillar point to your committed pillar with a centered banner + screen flash, quests refuse if more than 3 levels above you. **Text emotes** (/wave /bow /sit /cheer /dance /point) ride the chat-bubble system. **Corpse-run death penalty** — die → respawn at home with 25% HP, your corpse stays at the death site for 10 minutes; walk back to it → full HP restored. Server-authoritative over UDP + netcode, multi-client, client prediction + interpolation + zone-scoped area-of-interest replication. **Hostable build:** netcode key resolved from `VAERN_NETCODE_KEY` (release rejects unset / all-zero / wrong-length), server bind via `--bind` / `VAERN_BIND` (default `0.0.0.0:27015`), client target via `--server` / `VAERN_SERVER`, server panics write a forensics report to `~/.local/share/vaern/server/crash_<unix_ts>.log`, **client auto-reconnects** with exponential backoff (1s → 2s → 4s → 8s, 5 attempts max) when the lightyear `Connected` marker is removed mid-game and replays the last successful credentials so a server bounce under `VAERN_REQUIRE_AUTH=1` resumes the session without a re-prompt; falls back to MainMenu on auth failure or exhausted attempts. **Server-side accounts (Slice 8e)** — SQLite at `~/.config/vaern/server/accounts.db` with bcrypt-hashed passwords; case-insensitive username + character-name uniqueness; client login/register/create-character UI behind `AppState::Authenticating` + `CharacterSelect`; `CharacterSummary` populated from `PersistedCharacter` so the roster shows real race/pillar/level. Gated by `VAERN_REQUIRE_AUTH=1` (default off so the dev loop keeps working without credentials). **5-tier per-pillar gear-reward ladder** on the main Dalewatch chain (`chain_dalewatch_first_ride` steps 4/6/7/8 + chain capstone) — Might gambeson → leather → mail → plate, Finesse leather → mail, Arcana cloth wool → silk → mageweave; full silhouette flips at the ArmorType-change tiers. **Quest polish (Slice 9)** — talk / deliver / investigate steps now turn in via authored NPC reply text + a contextual click-through button (e.g. "Take the leather kit") and grant their gear ladder rewards on the player's click; multi-kill objectives track `2/3` in the tracker and only advance on the final required kill; investigate steps spawn cyan `?` POI markers at landmark coordinates that the player F-presses to advance; server validates 5.0u proximity to the right NPC / waypoint before honoring `ProgressQuest`. **Drifter's Lair pseudo-dungeon (Slice 6, code-complete, awaits 2-client playtest)** — open-world spawn region anchored at zone-local `(470, 80)` east of Ford of Ashmere with 16 hand-authored boulders / dead trees / lanterns marking the threshold; **Master Drifter Halen** (mini-boss, L9) and **Grand Drifter Valenn** (capstone boss, L10) flanked by L8-L10 drifter brutes / acolytes / fanatics in 4-mob pulls. **Shared Need-Before-Greed-Pass loot rolls** — boss kills with ≥2 party members within `PARTY_SHARE_RADIUS=40u` spawn a `LootRollContainer` (no single owner) and broadcast `LootRollOpen` to every eligible client; per-item Need/Greed/Pass votes resolve via a pure `decide_roll_winner` (Need beats Greed beats Pass; ties d100; all-Pass = no winner); winner gets the item directly into inventory; 60s deadline auto-settles. Solo / out-of-radius kills bypass — existing single-owner `LootContainer` flow unchanged. **Open Need** — no pillar gating; any party member can roll Need on any item. **One-tier-above gear ladder** on Valenn drops: full 4-piece mithril plate (Might) / dragonscale leather (Finesse) / shadowsilk cloth (Arcana) at `quality: exceptional`; Halen drops one chest piece per pillar at steel/wyvern/mageweave + exceptional. Boss-drop bonus stacks on top of the existing chain-final reward (Slice 4e capstone set still lands deterministically on the chain step 10 turn-in). **374 workspace tests passing.**

What works end-to-end:
- **Main menu** with character create/select (egui). Race + **body** (Male / Female — cosmetic, client-local) + **pillar** pickers (Might / Finesse / Arcana — characters commit to a pillar only at creation; archetype / Order unlocks are deferred to an evolution path). Saved characters persist to `~/.config/vaern/characters.json`.
- **10 starter zones** (1 per race) populated on startup on a 2800u ring; each player spawns in their race's zone. **Dalewatch Marches** (Mannin starter) was redesigned to Classic-Elwynn scope: 4 hubs, 12 sub-zones / landmarks, 10-step main chain, 2 side chains, 20 side quests, 24 mob types in a 1200×1200u playable box. Other 9 zones still ~15 mobs, 2 hubs, 1 main chain, 5 resource nodes.
- **Server-authoritative biome voxel ground + atmospheric sky.** The world's ground is the `vaern-voxel` crate: a hand-rolled Surface Nets extractor (no external `fast-surface-nets` / `ndshape` / `glam` deps) over a sparse `HashMap<ChunkCoord, VoxelChunk>` store with 32³-content + 1-padding chunks (34³ f32 samples, ~157 KB), streamed around every active player on **both** server and client. 8 editable algorithm layers behind traits: `SdfField`, `VertexPlacement`, `NormalStrategy`, `QuadSplitter`, `IsoSurfaceExtractor`, `MeshSink`, `WorldGenerator`, `Brush`. Server uses `HeightfieldGenerator` to seed 5×3×5 chunks around each player; client streams 11×3×11 around the camera and adds world-XZ UVs + MikkTSpace tangents + a per-chunk `StandardMaterial` from a cached 9-biome CC0 ambientCG PBR table resolved via nearest-hub Voronoi (`vaern-client/src/voxel_biomes.rs`). **F10 crater flow is fully server-authoritative**: client sends `ServerEditStroke` → server validates (range ≤ 12u, distance ≤ 40u from requesting player) → applies `EditStroke::new(SphereBrush { mode: Subtract, .. })` → broadcasts up to 8 `VoxelChunkDelta`/tick to every client. Reconnecting clients are caught up via a per-link `PendingReconnectSnapshots` queue (4 chunks/tick/client) seeded from the server's `EditedChunks` set. Server player + NPC Y-snap and client predicted-player Y-snap all query `vaern_voxel::query::ground_y(store, x, z, top_y, descent)` (with `terrain::height` fallback for unseeded chunks), so craters affect physics the same way they affect visuals. Two load-bearing voxel crate fixes landed this slice: `ChunkShape::MESH_MIN = PADDING - 1` to close static chunk seams, and `chunks_containing_voxel` enumeration extended from `{-1, 0}` to `{-1, 0, +1}` to propagate halo writes correctly across chunk boundaries (without the latter, a textured "cap" floats over every carved crater). Sky is Bevy 0.18's `AtmospherePlugin` (procedural scattering) paired with `DistanceFog` (exp-squared, 1500u visibility), `Bloom::NATURAL`, `Tonemapping::TonyMcMapface`, `Exposure::SUNLIGHT`, `Hdr`. 46/46 voxel unit + e2e tests pass.
- **Voronoi hub biomes baked into the voxel ground.** Each hub YAML still declares a `biome: <key>` from the 9-biome palette (grass / grass_lush / mossy / dirt / snow / stone / scorched / marsh / rocky; all CC0 PBR from ambientCG). On client startup, `voxel_biomes::BiomeResolver` loads the world YAML and builds a nearest-hub table with a 900u influence radius per hub. At chunk seed time (both the camera streamer and the delta-receive path), the chunk's footprint center is resolved to a `BiomeKey`, cached in `ChunkBiomeMap`, and the material-attach system pulls the matching `StandardMaterial` from the lazy per-biome cache. Transitions are chunk-aligned (32u tiles) — smooth per-fragment blending would need a custom triplanar shader with per-vertex biome weights. The legacy `scene/hub_regions.rs` overlay-mesh plugin (with its 1024 floor patches + 7 road ribbons per zone) is retired and unregistered; the file stays as source-level reference for porting roads (which are NOT yet represented in the voxel ground — that's a follow-up).
- **PBR world dressing on top of the voxel ground.** 57-asset Poly Haven CC0 pack at `assets/polyhaven/`, downloaded via `scripts/download_polyhaven.py` (see `memory/reference_polyhaven_pipeline.md` for API gotchas — UA blocking + texture URL rewrite). `PolyHavenCatalog` resource in `vaern-assets` keys slug → category. Zone YAML carries `scatter:` rules (biome + density + min spacing + slope cap + hub exclusion + seed salt) and hub YAML carries `props:` lists (slug + offset + rotation + scale). `crates/vaern-client/src/scene/dressing.rs` runs a deterministic seeded Poisson-disk scatter inside each zone's 1200×1200u footprint, snaps Y to the voxel ground via `vaern_core::terrain::height`, and culls beyond 250u. `MAX_SCATTER_PER_ZONE=1500` is a safety cap. Dalewatch is dressed: ~55 authored hub props (castle door, iron gates, weapon rack, banners, well, fire pit, market barrels) + 5 scatter rules covering trees / rocks / dead wood / shrubs / ground cover. **Trees are saplings only** — Poly Haven's hero-tree photoscans are 100MB-905MB each and excluded from the scatter pack. **No collision on dressing props yet** (deferred). See `memory/project_world_dressing.md`.
- **Top-left unit frame** — race portrait, character name, level, HP / mana / stamina bars (amber while blocking), `BLOCKING` / `PARRY` tags when active, XP bar. Populated by server-pushed `PlayerStateSnapshot` (not replication — see architecture).
- **Mouse-look camera** — cursor locked + hidden in-game; mouse drives camera yaw/pitch; scroll for zoom. Hold **LeftAlt** to free the cursor for UI clicks. Cursor auto-frees whenever an egui panel is open.
- **Target lock (Tab-cycle)** — **Tab cycles** through combat NPCs within 40u, preferring those in the camera's front cone (falls back to nearest overall when none in front). QuestGivers are excluded. **Esc clears**. While locked, the player continuously turns toward the target (kinematic motion controller).
- **Combat shapes** (tuned per-ability in flavored YAML): `target`, `aoe_on_target`, `aoe_on_self`, `cone`, `line`, `projectile`. Friendly fire on — AoE hits party. Channeled cones/lines/projectiles snapshot their `range` onto `Casting`, so heavy attack (cast-time cone) doesn't sweep to infinity.
- **Hotbar (6 key-bound + 2 mouse-bound)** — keys 1-6 fire class kit; **LMB** = light auto-attack; **RMB** = heavy auto-attack. No GCD.
- **Cast bar** (bottom-center) for abilities with `cast_secs > 0` — school-colored fill.
- **Quest flow**: walk up to a gold "!" NPC → `F` → Accept → quest log (`L`) tracks. **Server hard-refuses accept if `chain.steps[0].level > player.level + 3`** (no entry appears in the log). Side-quest givers populated in Dalewatch — Quartermaster Hayes (capital) + Captain Morwen (Harrier's) + Innkeeper Bel (Ford) + Smith Garrick (Kingsroad) + Mistress Pell (Miller's), each at the NW 4u offset of their hub.
- **Mob level banding** — Dalewatch tiers mobs into bands by level: L1-2 cluster around the keep, L3-4 around Harrier's Rest + Kingsroad, L5-6 around Miller's + Ford, L7+ at a fixed (470, 80) "Drifter's Lair" anchor east of Ford. Per-rarity scatter radius (named 110u / elite 90u / common 70+jitter). Other zones still ring around zone origin (legacy procedural fallback). **Per-kind respawn timers**: combat=180s / elite=600s / named=1800s (was a flat 30s).
- **Felt level progression** — `level_xp_multiplier` curve scales kill XP by `mob_level - killer_level`: parity = 1.0×, +5 = 1.5× (cap), -3 = 0.5×, -6+ = 0.0× (grey). Each level-up grants +1 pillar point auto-targeted at your committed pillar (highest-cap, tie-break Might > Finesse > Arcana). Both kill and quest XP paths use the wrapped `grant_xp_with_levelup_bonus` so every level-up gives the bonus regardless of source. Client renders a centered "LEVEL UP / Level N" banner with 0.35s gold screen flash + 2.5s fade (egui).
- **XP + levels**: kills + quest steps grant XP. Level-up via `xp_curve.yaml`.
- **Pillar XP from play**: every ability cast grants pillar points to the ability's pillar (Might/Finesse/Arcana). HP auto-scales on pillar gain via `derive_primaries`.
- **NPC AI**: per-type aggro, threat-table targeting, roaming idle, leash-home. Slow-aware (chilled NPCs actually slow down).
- **NPC stats from bestiary** — creature_type resistances + armor_class physical/magic reduction fold into `CombinedStats` per mob, scaled by rarity (Combat 1.0× / Elite 1.25× / Named 1.5×). Fire dragons resist fire; plate knights resist blade.

**Character rendering (all humanoids):**
- **Quaternius modular character mesh** drives the own-player avatar, every remote player, and every humanoid NPC (quest-givers, bandits, cultists) — all on the same UE-Mannequin skeleton and all driven by the shared UAL `AnimationGraph`. Body / legs / arms / feet / head-piece spawned slot-by-slot under the player entity, each independently colored.
- **Gear-driven outfit.** `outfit_from_equipped(OwnEquipped, ContentRegistry)` maps each primary armor slot's `ArmorType` to a Quaternius outfit family + color variant: cloth→Wizard, gambeson→Peasant V2, leather→Ranger, mail→KnightCloth V3, plate→Knight V2. **Unequipped = Peasant BaseColor** — the reserved "naked rags" identity, never used by any equipped armor. Mail vs plate differ by body mesh (KnightCloth vs Knight) AND color (V3 vs V2).
- **Respawn on change.** A `sync_own_player_visual` system watches `OwnEquipped`; on any change that alters the resolved `QuaterniusOutfit` it despawns the old mesh and spawns a new one. Equipping a ring or trinket is a no-op visually.
- **UAL animation pipeline.** `MeshtintAnimationCatalog::scan` at startup loads both UAL GLBs (86 Quaternius clips). `install_character_animation_player` (per scene) wires an `AnimationPlayer` on every Quaternius child (body, legs, arms, feet, head piece, Superhero head split, hair, beard) pointing at a shared `AnimationGraph`. UAL is authored on the UE Mannequin skeleton — same as Quaternius — so no retargeting.
- **AnimState → UAL clip driver.** `drive_own_player_animation` walks the Quaternius subtree every frame, reads the own player's `AnimState` + snapshot's `cast_school` + mainhand weapon school, and picks a clip: `Idle_Loop` (unarmed) / `Sword_Idle` (armed) / `Walk_Loop` / `Jog_Fwd_Loop` / `Sword_Attack` (Attacking flash) / `Sword_Block` (Blocking) / `Hit_Chest` (Hit flinch) / `Death01` / `Spell_Simple_Idle_Loop` (magic cast) or `Sword_Idle` (physical windup). **Transient clips (Attacking, Hit) play one-shot and hold until `ActiveAnimation::is_finished()` — even after the server-side 250ms `AnimOverride` has already reverted to Idle** — so the full swing reads on-screen.

**Combat depth:**
- **Timed status effects** — `StatusEffects(Vec<StatusEffect>)` component on any combat-capable entity. Variants: `Dot`, `Stance`, `Slow`, `StatMods { damage_mult_add, resist_adds[12] }`. `compute_damage` reads StatMods on both caster (offensive mult bonus) and target (per-channel resist add); consumables push StatMods as timed buffs. Refresh-on-reapply semantics; auto-removes when empty.
- **YAML-driven effect riders** — flavored ability variants can declare `applies_effect: { id, duration_secs, kind: dot|slow, dps, tick_interval, speed_mult }`. On a successful hit (parry-negated hits skip the rider), the target gets the effect. Seeded: fire→burning (3 dps/6s), frost→chilled (0.6 speed_mult/3s), shadow→decay (2 dps/5s), blood→bleeding (4 dps/8s) at tiers 25 + 50.
- **Active Block (Q hold)** — drains 15 stamina/sec, 60% frontal / 25% flank / 0% rear damage reduction. Breaks when stamina hits zero.
- **Active Parry (E tap)** — 0.35s window, 20 stamina per successful consume (free to miss). Fully negates both damage and any rider debuff. First hit in-window triggers, window closes.
- **Stamina pool** — 100/100, 12/sec regen. Separate component from mana. Block refused if pool empty.
- **Slow-aware movement** — both players and NPCs move at `speed_mult × base` while chilled. Strongest slow wins (doesn't stack).

**Animation state:**
- **Replicated `AnimState`** on players + NPCs — `Idle / Walking / Running / Casting / Blocking / Attacking / Hit / Dead`. Derived every FixedUpdate tick from Transform delta + Casting + StatusEffects + Health.
- **Transient Attacking + Hit flashes** on every CastEvent — 250ms `AnimOverride` freeze prevents the derive loop from clobbering the flash.
- **Visible in nameplates** — every NPC + player nameplate shows its current state in a small grey `[running]` / `[blocking]` / `[attacking]` tag.

**Gear & item system (Model B — compositional):**
- Items are composed at runtime from four orthogonal tables: **bases** (piece shape), **materials** (substance + stat mults), **qualities** (craft roll), **affixes** (stat deltas). 222 bases + 25 materials + 7 qualities + 27 affixes → ~5,000+ resolvable combinations.
- **Affixes** roll on world drops (weight-pool filtered by tier + base kind), stack as prefix ("Enchanted") + suffix ("of Warding") in the resolved name. 5 shard-only affixes (weight 0, soulbinds on apply) — reserved for boss-token imprint flow.
- **Materials** carry per-channel `resist_adds` — silver vs necrotic/radiant, dragonscale vs fire, shadowsilk vs radiant penalty. Real mechanical differentiation at the material level.
- **Runes** — caster magical-ward gear in `EquipSlot::Focus`. Drain mana via negative mp5 in exchange for heavy per-channel magical resist. Magic-tank build enabled.
- Rarity = affix slot count (Common 0, Legendary 4). Pre-rolled drops leave 1 slot open for crafter polish.

**Inventory + equipment UI** (toggled by `I`):
- 30-slot inventory grid (3 × 10) with stack merging (keyed on full `ItemInstance` identity).
- 20-slot paper doll on the right — 11 armor slots (head→feet) in the left sub-column, 9 accessory/weapon/focus slots on the right.
- **Rarity-colored item names** (WoW palette: grey / white / green / blue / purple / orange).
- **Hover tooltip cards** — bold name in rarity color, rarity + kind line, nonzero stats only (armor/weapon damage/crit/haste/block/mp5/fortune), per-channel resists, soulbound tag in gold italic, weight.
- Left-click inventory item → auto-equips to default slot (gear) or **consumes** (potions, elixirs, food) via `ConsumeItemRequest`.
- Right-click paper-doll slot → unequips.
- Two-hander displaces offhand; Focus rejects non-runes; armor slot-id validation.

**Consumables**:
- Every `Consumable` base carries a YAML-authored `ConsumeEffect`: `HealHp` / `HealMana` / `HealStamina` (clamp-add to the target pool) or `Buff { id, duration_secs, damage_mult_add, resist_adds[12] }` (pushes a timed `StatusEffect::StatMods`).
- Potions ship with real amounts — Minor Healing +40 HP, Major Healing +450 HP, same pattern for mana and stamina.
- Elixirs of Might / Finesse / Arcana: +15% damage for 5 min. Giant's Elixir: +25% damage for 2 min. `compute_damage` folds `status_damage_bonus` across all active StatMods into the caster's damage multiplier — buffs stack additively.
- **Warding Elixir**: +15 resist across all 12 channels for 5 min (broad defensive buff).
- **Per-channel Resist Potions** (24 bases — 12 channels × lesser/greater): +30/+60 on the named channel for 3 min. `compute_damage` folds `status_resist_bonus` onto `resist_total[dt]` before the mitigation curve, capped at the shared 80% resist ceiling. Prep-before-boss loop — bring the right resist potion for the encounter.

**Consumable belt (keys 7/8/9/0)**:
- 4-slot potion strip below the hotbar, owned by `ConsumableBelt` on the server. Bindings store the `ItemInstance` template (not an inventory index), so they survive stack rearrangement.
- **Bind**: right-click a potion in the inventory panel → "Bind to Slot 1/2/3/4". Rebinding over a slot overwrites.
- **Fire**: press `7 8 9 0` to quaff the bound potion. Server searches inventory for a matching stack, applies the `ConsumeEffect`, decrements one charge. Binding persists on empty-inventory so you can replenish.
- Strip shows bound name + `×count` (grey when zero matching stacks remain).

**Stat screen** (toggled by `C`):
- Live `CombinedStats` fold — pillars + gear → derived primaries + armor + 12 resist channels + utility. MP5 flags "(rune drain)" when negative.
- Pillar progress bars showing banked XP toward next pillar point.

**Combat reads stats**:
- Caster: weapon min/max dmg roll, melee/spell mult scaling, crit roll against `total_crit_pct` → ×1.5 multiplier.
- Target: `armor / (armor + 200)` mitigation, per-channel resist via school→DamageType → `resist_total[channel] × 0.005` (80% cap, supports negative for vulnerability amplification).
- All three damage sites wired: instant resolution, channeled cast completion, projectile hit.

**Loot flow**:
- Mob dies → `vaern-loot` rolls rarity curve + base + material + affixes → server spawns a `LootContainer` entity at the corpse position, owned by top-threat player.
- Client sees yellow gizmo marker → walks within 5u → `G` opens loot window.
- Click items individually or "Take all" → server moves to inventory, updates snapshot. Container auto-despawns after 5 min or when emptied.

**Gathering** (v1 foundation):
- 5 resource nodes per starter zone (copper veins, stanchweed patches, pine trees, etc.) as replicated entities with `NodeKind` + `NodeState`.
- `H` proximity-harvests nearest Available node within 3.5u → material enters inventory → node flips to Harvested → respawns after 60s (tier 1).
- 4 gathering professions wired (Mining, Herbalism, Skinning, Logging); 7 crafting professions typed but no recipes yet.

**Currency loop** (closed earn→spend):
- **`PlayerWallet { copper: u64 }`** component on every player; `saturating_add` credit + `try_debit` spend + `can_afford` check. Lives in `vaern-economy`.
- **Mob kills drop coin** in addition to items. `DropTable.coin_range: (u32, u32)` scaled by `(material_tier, NpcTier)`: combat 2-10c at T1 → 8-46c at T6; elite 15-50c → 63-210c; named 100-300c → 420-1260c. Coin rolls independently of item drop_chance — a no-item kill still pays. Credited directly to the top-threat player's wallet (no loot-container dance for coin).
- **Quest rewards pay copper** — step `gold_reward_copper` on progress + chain `gold_bonus_copper` on completion. `apply_kill_objectives` pays the step reward on auto-advance.
- **`WalletSnapshot { copper: u64 }`** S→C broadcast only on `Changed<PlayerWallet>` (idle ticks cost nothing).
- **Gold displayed in the inventory panel** (press `I`) under the Inventory heading, formatted as `"12g 34s 56c"` via `format_copper_as_gsc`. **Not** in the combat unit frame — currency is an inventory concern.
- **Persisted** as `PersistedCharacter.wallet_copper: u64` with `#[serde(default)]` for legacy saves.

**Vendor NPCs** (10 per starter-zone capital):
- **One general-goods vendor per capital hub** (Merchant Kell at Dalewatch Keep, Merchant Seyla at Shadegrove Spire, etc.), seeded from `src/generated/vendors.yaml`. Each stocks ~12 items: minor potions (healing / mana / stamina), food (bread / hardtack / rations), a scroll of recall, linen cloth shirt / trousers / cowl, and two copper weapons sized to the region's flavor.
- **`NpcKind::Vendor`** variant; cool-blue nameplate color; excluded from Tab-targeting; non-combat.
- **F within 5u opens the vendor window** (two tabs: Buy / Sell). Buy tab lists stock with `vendor_buy_price` server-computed prices; click "Buy Xc" — server debits wallet, adds item to inventory, decrements stock if `VendorSupply::Limited(n)`. Sell tab iterates the player's own inventory, shows `vendor_sell_price` (60% spread) per sellable stack; click "Sell Xc" — server credits wallet, removes the item. Soulbound / `no_vendor` items show a grey "(no sale)" row.
- **Auto-close** when the player walks out of range (5u) via `VendorClosedNotice`.
- **`VendorIdTag`** stamped by a startup pass (`tag_new_vendors`) so wire ids stay stable across reconnects.

**Chat system**:
- **Five channels**: Say (20u proximity), Zone (whole-zone AoI room), Whisper (single recipient by display name), Party (cross-zone party members), System (server-authored banners).
- **Enter** opens input bar; type + Enter sends. Prefix parser: no prefix = Say, `/s /say`, `/z /zone`, `/p /party`, `/w /whisper /tell /msg <name>`. Unknown `/foo` falls through to Say.
- **Party commands** (`/invite /inv /leave /disband /kick`) intercept before chat parsing — same input bar.
- **Emotes**: `/wave /bow /sit /cheer /dance /point` translate client-side into a Say-channel send with body `*waves.*` / etc. Bubble + history both render. Animation playback (UAL clip per emote) is post-pre-alpha scope.
- **Rate-limited** at 5 messages/sec per sender (rolling 1s window). Truncated to 256 chars. Server authoritative on `from` (reads sender's `DisplayName` — clients never stamp their own name).
- **Speech bubbles** render above the speaker's head on Say and Zone only — Whisper and Party stay private visually. 5s lifetime with a 1s fade, truncated to 72 chars with an ellipsis, one-bubble-per-speaker policy (new bubble replaces old). Bubble anchors at `head + 2.8u`; nameplate at `+2.1u` — they stack without overlap.
- **`ChatInputFocused`** resource: while the input has keyboard focus, movement (WASD), targeting (Tab/Esc), combat (1-6, LMB/RMB), stances (Q/E), spellbook (K), and nameplate-toggle (V) are all suppressed. Cooldowns still tick; mouse-look still streams — matches the standard MMO-client model.
- **History**: 50-line ring in the bottom-left overlay. Channel colors: Say white, Zone mint-green, Party cool-blue, Whisper magenta (received) / pink (echo), System yellow.

**Party system** (strict-coop core):
- **Invite by display name** — `/invite Brenn` (or `/inv Brenn`). Server validates target exists, target isn't already in a party, your party has room (max 5). Target gets `PartyIncomingInvite`; UI pops an Accept/Decline modal at center-top. 60s invite TTL server-side.
- **Party frame** top-left below the unit frame — one row per member with name + level + HP bar + `[L]` leader tag + Leave button. Rebuilt from `PartySnapshot` (broadcast on join/leave/kick/disband; dirty-set gated — not per-tick).
- **Leave / kick / disband**: `/leave` or Leave button leaves the party; `/kick <name>` (leader-only); party auto-disbands when size drops below 2 (every ex-member gets `PartyDisbandedNotice`, their `PlayerPartyId` component is stripped). Leader-leave promotes `members[0]` before the size check.
- **Shared XP** — `vaern-server::xp::award_xp_on_mob_death` splits XP across every party member within `PARTY_SHARE_RADIUS = 40u` of the killer. Killer gets full base reward; each partner in range gets a `per` share with small-group multiplier: `1.0× / 0.7× / 0.55× / 0.45× / 0.38×` for 1/2/3/4/5 sharers. Total group payout rises with party size (5-party ≈ 1.9× solo) so grouping pays, but never scales linearly to 5×.
- **Party chat** (`/p <msg>`) routes through `ChatChannel::Party` — `chat_io` queries `PartyTable` and ships to every member's link across zones. Same 5/sec rate limit as other channels.

**Nameplates**:
- **Every entity with `Health`** gets a nameplate — players, quest-givers, vendors, combat mobs. Projected each frame from world-space (`head + 2.1u`) to screen.
- **Label = `DisplayName`** for both players and NPCs. Pillar label only shown for anonymous spawns (empty `DisplayName` from headless test clients).
- **60u culling** (`NAMEPLATE_MAX_RANGE`) — distant plates hide before projection so dense crowds don't become letter soup.
- **V toggles** on/off via `NameplatesVisible` resource (gated on chat focus). When off, also hides chat bubbles.
- **Color by kind**: players + combat mobs white, quest-givers gold, vendors cool-blue, elites violet, named pink. `"!"` quest marker renders above quest-giver plates.
- **State tag** under HP bar reads `[idle]` / `[running]` / `[blocking]` / `[attacking]` etc. — live from replicated `AnimState`.

## Quick start

```bash
# One server, any number of clients
./target/debug/vaern-server          # terminal 1
./target/debug/vaern-client          # terminal 2 — goes through the menu
VAERN_CLIENT_ID=1001 ./target/debug/vaern-client    # terminal 3 — second client

# Or the dev-fast script that skips the menu via env vars
./scripts/run-multiplayer.sh
```

**In-game controls**
- `WASD` — camera-relative movement (W = forward relative to camera)
- Mouse — camera yaw/pitch (cursor locked); scroll — zoom
- **LMB** — light attack (fast cone, 0.5s cd)
- **RMB** — heavy attack (0.4s windup cone, 1.5s cd)
- `1-6` — hotbar abilities (no GCD — chain freely with LMB/RMB)
- `7 8 9 0` — consumable belt (bound potions, quaffable mid-fight)
- **Q** — hold for Active Block (drains stamina; 60% frontal damage reduction)
- **E** — tap for Active Parry (0.35s window, 20 stamina on successful negate)
- `Tab` — cycle target (40u max range, prefers NPCs in the camera's front cone, QuestGivers + Vendors excluded)
- `Esc` — clear current target / close focused panel
- `I` — toggle inventory + paper doll (wallet shown under Inventory heading)
- `C` — toggle character / stat screen
- `G` — loot nearest container within 5 units (opens loot window)
- `H` — harvest nearest resource node within 3.5 units (mining / herbalism / logging)
- `LeftAlt` — hold to free cursor for UI clicks + disable mouse-look
- `F` — talk to nearest quest-giver OR open nearest vendor's buy/sell window (≤5u range)
- `K` — toggle spellbook · `L` — toggle quest log · `☰` top-right — logout / quit
- **`V` — toggle nameplates + chat bubbles on/off** (gated on chat focus — typing "V" in chat is safe)
- **`Enter`** — open chat input. Type + Enter sends to the selected channel. `Esc` cancels.
  - No prefix = `/say` (20u proximity). `/z` = zone. `/p` = party. `/w <name>` = whisper.
  - `/invite <name>`, `/leave`, `/kick <name>` are party commands (also work from the same input bar).
  - `/wave /bow /sit /cheer /dance /point` are emotes — render as third-person bubble text ("Brenn waves.").
  - **While the chat input has focus, WASD / hotbar / Tab / Q / E / K / V are all suppressed** — typing "W" doesn't walk.
- `F10` — **debug voxel stomp** — carve a 6u-radius sphere crater in the voxel world at the camera's forward focus.

## Architecture

Workspace of seventeen crates + modular client + modular server:

```
crates/
├── vaern-core/       abstract types: Pillar, ClassPosition, Morality, Faction, School,
│                     DamageType (12 variants), terrain height field, Voronoi
│                     partition + Catmull-Rom spline utilities (voronoi.rs)
├── vaern-voxel/      chunked SDF voxel world (hand-rolled, not fast-surface-nets):
│                     sdf/ (trait + Sphere/BoxSdf/Capsule/Plane + Union/Subtract/
│                     Intersect/SmoothUnion/SmoothSubtract), chunk/ (32³ content +
│                     1 padding = 34³ samples per chunk, sparse HashMap store,
│                     DirtyChunks), mesh/ (IsoSurfaceExtractor + VertexPlacement +
│                     NormalStrategy + QuadSplitter + MeshSink — 4 swappable
│                     algorithm layers), edit/ (Brush + EditStroke with halo sync),
│                     generator/ (HeightfieldGenerator bridges terrain::height),
│                     query/ (ground_y + raycast against ChunkStore),
│                     replication/ (ChunkDelta FullSnapshot | SparseWrites,
│                     version-numbered + replay-safe), plugin.rs (VoxelCorePlugin
│                     resources-only for server, VoxelMeshPlugin for client, combined
│                     VaernVoxelPlugin). 46 unit+e2e tests pass.
├── vaern-data/       YAML loaders: schools, classes, abilities, flavored, bestiary,
│                     races, world, dungeons, quest chains
├── vaern-protocol/   SharedPlugin: lightyear registration, channels, every network
│                     message + replicated component; depends on most domain crates
├── vaern-combat/     Bevy plugin: Health, Stamina, abilities + AbilityShape, Casting,
│                     Projectile. damage.rs: stat-aware compute_damage + apply_stances
│                     (parry full-negate + block angle math). effects.rs: StatusEffects
│                     (Dot/Stance/Slow/StatMods), tick_status_effects. anim.rs: AnimState
│                     (replicated) + derive_anim_state + transient Attacking/Hit flash.
├── vaern-character/  Experience, PlayerRace, XpCurve (leaf)
├── vaern-stats/      Pillar identity + 3-tier stat pool: PillarScores/Caps/Xp,
│                     DerivedPrimaries, SecondaryStats (armor + [f32;12] resists +
│                     weapon dmg + crit/haste/block + mp5), TertiaryStats,
│                     CombinedStats (Component, denormalized onto entities)
├── vaern-items/      Compositional item model (Model B). ItemBase × Material ×
│                     Quality × Affix → ResolvedItem. ContentRegistry holds the
│                     four part tables; resolve() folds instance → display + stats.
│                     Shared enums: ArmorType/ArmorLayer/BodyZone/WeaponGrip/Rarity
├── vaern-economy/    Vendor pricing math over ResolvedItem: buy/sell spread, market
│                     floor, GoldSinkKind ledger enum
├── vaern-equipment/  20-slot paper doll (+ Focus for runes); Equipped stores
│                     slot → ItemInstance. validate_slot_for_item takes &ResolvedItem;
│                     two-hander↔offhand displacement, rune slot gating
├── vaern-inventory/  PlayerInventory component: fixed-capacity slot grid, stack
│                     merging keyed on full (base, material, quality, affixes) tuple,
│                     add/take/take_all
├── vaern-loot/       Drop tables + roll_drop. Rarity emerges from material+quality;
│                     quality biased by table's rarity_curve. Affix pool filtered by
│                     base kind + tier + weight>0. Shard-only affixes never random-roll
├── vaern-professions/Profession enum (11 variants), ProfessionSkills component,
│                     NodeKind (15 mining/herb/logging nodes), NodeState
├── vaern-server/     bin: UDP + modular — data / connect / npc / quests / xp /
│                     player_state / combat_io / movement / util / starter_gear /
│                     stats_sync / inventory_io / loot_io / consume_io /
│                     belt_io / resource_nodes / aoi (zone-room AoI replication) /
│                     voxel_world (authoritative chunk store + streamer + edit
│                     validator + delta broadcast + reconnect catch-up pipeline) /
│                     wallet_io (broadcast WalletSnapshot on Changed<PlayerWallet>) /
│                     vendor_io (open/buy/sell handlers + stable VendorIdTag) /
│                     chat_io (Say/Zone/Whisper/Party/System routing + rate limit) /
│                     party_io (PartyTable + invite/accept/leave/kick + dirty-set
│                     snapshot broadcast; shared-XP scaling inlined in xp.rs) /
│                     respawn (corpse-run death penalty: spawn server-only
│                     Corpse entity at death pos, 25% HP respawn, walk-back
│                     restoration at 3u proximity, 10-min expiry; CorpseOnDeath
│                     marker makes shared apply_deaths skip players)
├── vaern-client/     bin: DefaultPlugins + 22 focused modules (see below) incl.
│                     vendor_ui, chat_ui, party_ui wired for the MMO-feel slice
├── vaern-sim/        bin: headless deterministic sim — reserved for PPO balance training
├── vaern-assets/     shared Bevy plugin: two character-rendering stacks
│                     + shared animation pipeline + NamedRegions cache.
│                       · meshtint — Meshtint Polygonal Fantasy Pack:
│                         MeshtintCharacterBundle, OutfitPieces visibility,
│                         BodyOverlay + WeaponOverlay spawn, WeaponGrips YAML,
│                         MeshtintPieceTaxonomy, hair/eye flat-material
│                         overrides, mirrored-normal fix, Visibility::Inherited
│                         auto-insert, palette swap.
│                       · quaternius — Quaternius modular outfits on the UE
│                         Mannequin skeleton: spawn_quaternius_character +
│                         QuaterniusOutfit (body/legs/arms/feet/head/hair/
│                         beard per-slot, all optional), ColorVariant runtime
│                         texture swap, HideNonHeadRegions for the Superhero
│                         split that provides eyes/eyebrows/face.
│                       · animation — rig-tagged AnimationClipSrc catalog,
│                         shared AnimationGraph, AnimatedRig marker, auto-
│                         install of AnimationPlayer + AnimationTargetId on
│                         any animated character (Meshtint or Quaternius).
│                         UAL1 + UAL2 clips drive Quaternius natively.
└── vaern-museum/     two bins, both consumers of vaern-assets:
                      - vaern-museum — single-mannequin composer. Rig toggle
                        (Meshtint / Quaternius). Meshtint view: outfit / body
                        overlays / weapon grip / palette + skin/hair/eye
                        presets. Quaternius view: per-slot sliders (body /
                        legs / arms / feet / head piece / hair / beard) +
                        3-colour palette picker. Shared animation panel on
                        both — pick any UAL clip; syncs across every
                        AnimationPlayer under the character.
                      - vaern-atlas — one-shot spawn of every Meshtint variant
                        (base pieces × 2 genders + 26 overlay rows + all weapons)
                        with taxonomy labels billboarded via egui. Authoring tool
                        for taxonomy classification.

```

**Client modules** (all gated on `AppState::InGame`; main.rs is ~80 lines):
```
src/
├── main.rs          App bootstrap + plugin registration only
├── shared.rs        marker components, attach_mesh / attach_character
├── menu.rs          egui main menu · character create/select (race + body + pillar) · in-game ☰ logout
├── net.rs           lightyear client entity + ClientHello (with race_id)
├── scene.rs         mouse-look camera (cursor lock, LeftAlt/panel-open = free-look),
│                    3D ground/light, own-player Quaternius mesh spawn
│                    (outfit_from_equipped + respawn on OwnEquipped change),
│                    CastFiredLocal relay + AnimState overlay (snapshot flags)
│                    + anim driver (UAL clip keyed on AnimState + cast school)
├── input.rs         WASD + motion-controller yaw, LMB/RMB + 1-6 cast intents,
│                    Tab = cycle target / Esc = clear
├── hotbar_ui.rs     egui hotbar + spellbook + icon cache, emits CastAttempted events
├── attack_viz.rs    shape telegraph flashes + projectile mesh rendering
├── unit_frame.rs    top-left player frame (portrait/name/L#/HP/XP)
├── combat_ui.rs     Bevy-native cast bar + target frame + swing flash
├── vfx.rs           impact flashes, cast-beam gizmos, gold target ring
├── nameplates.rs    world-space HP plates (DisplayName label, 60u cull, V-toggle) +
│                    floating damage numbers + "!" quest-givers + chat speech bubbles
│                    (5s lifetime, fade, 72-char truncate, 1-per-speaker)
├── hud.rs           compass strip
├── quests.rs        loads chain YAMLs, drains QuestLogSnapshot
├── interact.rs      [F] quest-giver dialogue, [L] quest log
├── inventory_ui.rs  [I] inventory + equipment window + wallet line (amber, under
│                    Inventory heading), ClientContent registry loader, right-click
│                    consumable → bind to belt menu
├── vendor_ui.rs     [F] vendor Buy/Sell window (5u proximity), NearbyVendor detect,
│                    ActiveVendor snapshot, wallet header, local sell-price display
├── chat_ui.rs       Enter-opens input + bottom-left history (50-line ring) + prefix
│                    parser (/s /z /p /w) + ChatInputFocused gate that suppresses
│                    WASD/hotbar/Tab/Q/E/K/V while typing + ChatBubbleEvent emit
├── party_ui.rs      Party frame (name + level + HP bar + [L] leader + Leave btn),
│                    invite popup (Accept/Decline), party-command parser
│                    (/invite /inv /leave /disband /kick)
├── belt_ui.rs       4-slot consumable belt strip (keys 7/8/9/0), binding snapshot
├── loot_ui.rs       [G] loot window + pending-loot gizmo markers
├── stat_screen.rs   [C] character stats (pillars + CombinedStats breakdown)
├── harvest_ui.rs    [H] resource-node markers + harvest-proximity input
├── voxel_biomes.rs  BiomeResolver: loads world YAML, builds nearest-hub biome
│                    table (900u influence radius), answers BiomeKey queries
│                    per chunk footprint. BiomeKey::textures() maps to a CC0
│                    ambientCG PBR set.
├── voxel_demo.rs    Voxel ground plugin: streams 11×3×11 chunk cube around
│                    camera, attaches world-XZ UVs + MikkTSpace tangents +
│                    per-biome StandardMaterial on new chunk entities (with
│                    refresh_uvs_on_remesh for edit-driven mesh swaps). F10
│                    sends a server-authoritative ServerEditStroke; server
│                    broadcasts ChunkDeltas back via apply_server_chunk_deltas.
├── level_up_ui.rs  Centered "LEVEL UP / Level N" banner + 0.35s gold
│                    screen-flash overlay + 2.5s fade. Watches OwnPlayerState
│                    .snap.xp_level for upward transitions; first snapshot
│                    just records the persisted level (no banner on resync).
├── scene/dressing.rs  Loads world YAML on enter-game, walks each zone's
│                    `scatter:` rules + each hub's `props:` list, spawns
│                    SceneRoot-per-asset tagged `GameWorld` for teardown.
│                    Deterministic Poisson scatter via splitmix64 hash of
│                    (zone_id, biome, category, seed_salt). 250u distance
│                    cull; MAX_SCATTER_PER_ZONE safety cap.
└── diagnostic.rs    periodic snapshot + connect/disconnect + cast-fired logs
```

**Dependency graph (roughly):**
- `vaern-core` → nothing
- `vaern-voxel` → core (bridges terrain::height via HeightfieldGenerator; bevy 0.18, serde, thiserror only — no fast-surface-nets / ndshape / glam)
- `vaern-combat` → core + stats (for CombinedStats lookup in compute_damage)
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

- Server-authoritative over UDP via lightyear + netcode. Shared 32-byte private key resolved at boot via `vaern_protocol::config::resolve_netcode_key`: release builds require `VAERN_NETCODE_KEY` (hex) and reject all-zero / wrong-length; debug builds fall back to a zero dev key with a warning. Server bind from `--bind <addr>` / `VAERN_BIND` (default `0.0.0.0:27015`), client target from `--server <addr>` / `VAERN_SERVER` (default `127.0.0.1:27015`).
- **Replicated components:** `Transform` (prediction + linear/slerp interpolation), `Health`, `ResourcePool`, `Casting` (MapEntities), `Experience`, `PlayerRace`, `PlayerTag`, `DisplayName`, `NpcKind`, `QuestGiverHub`, `ProjectileVisual`, `NodeKind`, `NodeState`, **`AnimState`**.
- **Messages** (grouped):
  - *Combat*: `ClientHello` (C→S), `CastIntent` (C→S, MapEntities), `StanceRequest` (C→S: `SetBlock(bool)` / `ParryTap`), `CastFired { caster, target, school, damage }` (S→C, MapEntities — `caster` field drives client-side own-player attack flash), `HotbarSnapshot` (S→C).
  - *Quests*: `AcceptQuest`/`AbandonQuest`/`ProgressQuest` (C→S), `QuestLogSnapshot` (S→C).
  - *State*: `PlayerStateSnapshot` (S→C every tick — HP/pool/XP/cast + pillar scores/caps/banked XP × 3 + stamina + is_blocking + is_parrying).
  - *Inventory + equip*: `InventorySnapshot`, `EquippedSnapshot` (S→C on change, not per tick); `EquipRequest`, `UnequipRequest` (C→S).
  - *Loot*: `PendingLootsSnapshot` (S→C on `PendingLootsDirty` flag, not per tick), `LootWindowSnapshot` / `LootClosedNotice` (S→C), `LootOpenRequest` / `LootTakeRequest` / `LootTakeAllRequest` (C→S).
  - *Harvest*: `HarvestRequest` (C→S, MapEntities). Node state flows through component replication.
  - *Voxel edits*: `ServerEditStroke { center, radius, mode }` (C→S); `VoxelChunkDelta(ChunkDelta)` (S→C). Server applies via `EditStroke::new(SphereBrush).apply()` and broadcasts up to 8 deltas/tick; reconnecting clients get catch-up snapshots at 4/tick from the server's persistent `EditedChunks` set.
  - *Wallet + vendors*: `WalletSnapshot { copper: u64 }` (S→C on `Changed<PlayerWallet>` only — not per-tick). `VendorOpenRequest { vendor }` (C→S, MapEntities), `VendorBuyRequest { vendor_id, listing_idx }` (C→S), `VendorSellRequest { vendor_id, inventory_idx }` (C→S). `VendorWindowSnapshot { vendor_id, vendor_name, listings }` (S→C on open + after buy + on stock change), `VendorClosedNotice { vendor_id }` (S→C when player walks out of 5u range).
  - *Chat*: `ChatSend { channel, text, whisper_target? }` (C→S). `ChatMessage { channel, from, to, text, timestamp_unix }` (S→C). Server stamps `from` from the sender's `DisplayName` — clients never stamp their own. Rate-limited at 5 msg/sec/sender on a rolling 1s window.
  - *Party*: `PartyInviteRequest { target_name }` / `PartyInviteResponse { party_id, accept }` / `PartyLeaveRequest` / `PartyKickRequest { target_name }` (C→S). `PartyIncomingInvite { party_id, from_name }` / `PartySnapshot { party_id, leader_name, members: Vec<PartyMember> }` / `PartyDisbandedNotice { party_id }` (S→C). Snapshot broadcast is dirty-set gated (join/leave/kick/disband) — not per-tick; HP staleness between snapshots is acceptable at pre-alpha.
- **Area-of-interest replication** — one lightyear `Room` per starter zone. NPCs + resource nodes carry `NetworkVisibility` and join their zone's room at spawn; each client's link migrates between rooms as its player crosses zones. Players + projectiles stay globally visible. Pre-AoI, 603 NPCs × 60Hz Transform replication saturated the kernel UDP buffer on localhost and caused NPC rubber-banding. `RoomPlugin` must be added explicitly — it's not in lightyear's `SharedPlugins`.
- **Prediction:** own player on a `Predicted` copy; `buffer_wasd_input` → `ActionState<Inputs>` with `camera_yaw_mrad` bundled.
- **Own-player state via message, not replication.** Lightyear 0.26 gives the owning client only a `Predicted` copy — filter `(With<Replicated>, Without<Predicted>)` matches zero. HP/pool/XP/cast/stamina/stance + inventory + equipped + pending-loots all push via per-tick messages instead. Dynamic insertion/removal of predicted components (e.g. `AnimOverride`) is also unreliable on the Predicted copy, so **own-player transient animation flashes (Attacking / Hit) are driven client-side from the `CastFired` message** — server sets the flash, sends `CastFired { caster, … }`, client inspects `caster == own_player` and stamps `AnimState::Attacking` + a local `AnimOverride` to hold the swing for its full UAL clip duration.
- **`CastFired` local relay.** Lightyear's `MessageReceiver::receive()` drains on read. Multiple consumers (vfx impact flashes, nameplate damage numbers, animation flash driver, diagnostic logger) would compete over the single queue. A single `relay_cast_fired` system is the sole `MessageReceiver<CastFired>` reader and re-emits every received message as a Bevy-local `CastFiredLocal` (via `MessageWriter`). All downstream consumers read `MessageReader<CastFiredLocal>` — Bevy's per-system cursor tracking lets every subscriber see every message.
- **Loot containers are server-only entities** (not replicated). Clients see them only through `PendingLootsSnapshot` summaries owned by the top-threat player. Kills → one container per player-threat → loot is personal.
- **Respawnable component** on players resets HP/position/pool instead of despawning. Players ALSO carry `CorpseOnDeath`, which makes the shared `apply_deaths` skip them; server-only `respawn::apply_player_corpse_run` is the sole player-death handler — captures pre-teleport position, spawns a server-only `Corpse { owner, position, remaining_secs }` entity, sets HP to 25% of max instead of full, teleports to `Respawnable.home`. `respawn::tick_corpses` ticks lifetime, restores HP to max + despawns when owner walks within 3u (`CORPSE_RECOVERY_RADIUS`), and despawns expired corpses (10-min `CORPSE_LIFETIME_SECS`) or corpses whose owner has logged out. **No client-visible corpse marker yet** — players navigate back from memory of their death position; visual marker is a follow-up.
- **Server tick-rate logger** — prints `[tick] 60 Hz  avg_frame=16.72ms  max_frame=16.74ms` each second; catches Update-loop stretch that would cause client rubber-banding.

### Combat model

- **Abilities are entities** with `AbilitySpec` (damage, cooldown_secs, cast_secs, resource_cost, school, threat_multiplier, **range, shape, aoe_radius, cone_half_angle_deg, line_width, projectile_speed, projectile_radius, applies_effect**), `AbilityCooldown`, `Caster`.
- **Shapes**: `Target`, `AoeOnTarget`, `AoeOnSelf`, `Cone`, `Line`, `Projectile`. Friendly fire on.
- **No GCD** — per-ability cooldowns only; abilities interleave freely with LMB/RMB.
- **Projectiles** server-simulated in `FixedUpdate::tick_projectiles` with swept-sphere collision.
- **Channeled casts snapshot `range`** onto `Casting` so cones/lines/projectiles that resolve mid-combat stay bounded (heavy attack doesn't sweep to infinity).
- **Stats-aware damage pipeline** (`vaern-combat::damage::compute_damage` → `apply_stances`):
  - Caster: weapon min/max dmg roll (physical schools), `(melee_mult + spell_mult) × 0.5` global multiplier, crit roll against `total_crit_pct` → ×1.5. Reads caster's `CombinedStats` component if present.
  - Target: armor mitigation `armor / (armor + 200)`, per-channel resist `resist_total[dt] × 0.005` (capped 80%, supports negative values for vulnerability amplification).
  - **Stance layer** (`apply_stances`): active Parry → full negate (damage → 0, consumes the parry, debits stamina); active Block → frontal/flank/rear damage reduction based on the caster→target hit angle.
  - **Rider effects**: if `final_damage > 0` and the ability has `applies_effect`, attach the DoT / Slow to the target. Parried / blocked-to-zero hits don't apply riders.
  - School → DamageType lookup covers physical (blade→slashing, blunt→bludgeoning, etc.) and magical (fire/cold/light/shadow/frost/arcane/etc.) schools identically.
  - Called at all three damage sites via the unified `resolve_hit` helper: instant `select_and_fire`, channeled `progress_casts` completion, `tick_projectiles` hit.
  - Missing `CombinedStats` on either side falls through to raw damage — legacy NPC balance intact for entities without stats wired.
- **NPC stats from bestiary**: `npc_combined_stats(creature_type, armor_class, NpcKind)` derives armor (inverse of the mitigation formula from `physical_reduction`) + per-channel resists (magical base from `magic_reduction` + per-school bumps from creature_type `resistances`). Rarity mult: Combat 1.0× / Elite 1.25× / Named 1.5×.
- **Pillar XP on cast**: every `CastEvent` credits XP to the caster's pillar via `GameData.schools` lookup; dedupe by (caster, ability) per frame so AoEs don't multiply. `sync_hp_max_to_pillars` updates Health.max on pillar gain, preserving HP-fraction so growth isn't a free heal.
- **CombinedStats denormalization**: server system `sync_combined_stats` watches `Changed<Equipped> | Changed<PillarScores>`, resolves every equipped `ItemInstance`, folds `SecondaryStats` + `DerivedPrimaries` + (zeroed) `TertiaryStats` into `CombinedStats` as a Component.
- NPC AI: per-mob `AggroRange` + `LeashRange` (8u common / 11u elite / 14u named), threat-table target selection, `RoamState` wander, leash warp-home + HP reset on over-extend.

### Target lock + motion controller

Camera and player rotation use a kinematic motion controller:

- **Target selection**: `Tab` cycles through combat NPCs within 40u, preferring those in the camera's front cone (80° half-angle). Falls back to nearest-overall in-range if none in front. Filters out `NpcKind::QuestGiver`. `Escape` clears. A stale target (despawned NPC) is cleared next frame.
- **Smooth follow**: while a target IS held, camera yaw + mesh rotation drift toward it via a kinematic motion controller (brake-plan velocity capped by √(2·a·d)). Mouse yaw suppressed; pitch still mouse-driven.
- **Motion params** (in `input.rs`): `IDLE_TURN_RATE = 0.3 rad/s`, `CAST_TURN_RATE = 12 rad/s`, `TURN_ACCEL = 20 rad/s²`.
- **On cast** (any `CastAttempted`): velocity kicks to `min(brake_peak, CAST_TURN_RATE)` — a ~0.26s swoosh on 180°, not a teleport.

### Status effects + stances + stamina

- **`StatusEffects(Vec<StatusEffect>)` on every combat-capable entity.** Variants: `Dot { damage_per_tick, school, threat_multiplier }`, `Stance(Block | Parry)`, `Slow { speed_mult }`, `StatMods { damage_mult_add }` (reserved). Refresh-on-reapply semantics; `tick_status_effects` in Update decrements, fires DoT ticks (emits CastEvent so threat + damage numbers work), drains stamina for active Block, auto-removes the component when empty.
- **Active Block (Q hold)** — client sends `StanceRequest::SetBlock(true/false)` on press/release. Drains 15 stamina/s. Damage reduction scales by hit angle: 60% frontal → 25% flank → 0% rear. Stance breaks when stamina = 0. Refused if pool already empty.
- **Active Parry (E tap)** — `StanceRequest::ParryTap` opens a 0.35s window. First hit in-window fully negates (damage → 0) and also blocks any rider debuff. Consumes 20 stamina **on the negate**, not on the tap — free to miss. Parry wins over Block when both active.
- **`Stamina { current, max, regen_per_sec }`** — new component, separate from `ResourcePool` (mana). Players: 100/100, 12/s regen. Exposed to own-player via `PlayerStateSnapshot.stamina_current/max + is_blocking + is_parrying`. Stamina bar renders below the mana bar in the unit frame.
- **YAML-driven effect riders**: flavored ability variants accept `applies_effect: { id, duration_secs, kind: dot|slow, dps, tick_interval, speed_mult }`. Parsed as `FlavoredEffect` in vaern-data (mirror type to avoid a bevy dep there), converted to `EffectSpec` in `apply_flavored_overrides`. Seeded: fire→burning, frost→chilled, shadow→decay, blood→bleeding at tiers 25 + 50.
- **Slow-aware movement**: `StatusEffects::move_speed_mult()` returns the strongest (lowest) `Slow.speed_mult`; player movement and NPC chase/roam multiply their step by it. Slows don't stack — deepest wins.

### Animation state

- **`AnimState` enum replicated**: `Idle / Walking / Running / Casting / Blocking / Attacking / Hit / Dead`.
- **`derive_anim_state` in FixedUpdate** — priority `Dead > Blocking > Casting > Running > Walking > Idle` from Transform-delta speed + Casting + StatusEffects + Health. XZ-projected speed thresholds: walk = 0.5 u/s, run = 3.0 u/s.
- **Transient flashes**: `mark_attack_and_hit` reads each `CastEvent` — flashes the caster to `Attacking`, flashes the target to `Hit` (only when `damage > 0` and target ≠ caster). Paired with `AnimOverride { remaining_secs: 0.25 }` so derive doesn't clobber the flash before it's visible. `tick_anim_override` removes the override when expired.
- **Visualized** as a small grey `[idle]` / `[casting]` / `[running]` etc. tag under every nameplate (`nameplates.rs` reads the replicated `AnimState`).

### Gear & loot flow

1. **Mob dies** → server rolls drop via `vaern-loot::roll_drop` against `DropTable::for_npc(kind, tier)`. Rarity emerges from rolled material + quality, not forced ahead of time.
2. **Server spawns** a `LootContainer` entity at mob position, owned by top-threat player. Not replicated; carries contents + despawn timer.
3. **Client** receives `PendingLootsSnapshot` per tick → pulsing yellow gizmo at each position.
4. **Walk in range (5u)** → press `G` → `LootOpenRequest` → `LootWindowSnapshot` → egui window with items.
5. **Click** an item or "Take all" → `LootTakeRequest`/`LootTakeAllRequest` → server moves stack to `PlayerInventory` → broadcasts updated `InventorySnapshot` + `LootWindowSnapshot`. Full-inventory items stay in container.
6. **Container auto-despawns** at 5 min or when empty (sends `LootClosedNotice` so window auto-closes).

### Item resolution pipeline (`ContentRegistry::resolve`)

Given `ItemInstance { base_id, material_id, quality_id, affixes }`:

1. Look up `ItemBase`, `Quality`, optional `Material`. Unknown id → `ResolveError::UnknownBase/Material/Quality`.
2. Validate pairing: `base.armor_type ∈ material.valid_for` / `material.weapon_eligible` / `material.shield_eligible`. Fail → `InvalidPairing`.
3. Resolve affixes: look up each by id, check `applies_to` matches base kind. Fail → `UnknownAffix` / `InvalidAffix`.
4. Compute `weight_kg`, `rarity` (material.base_rarity + quality.rarity_offset clamped), `stats` (base kind's scaling × material × quality, then per-affix `stat_delta` folded).
5. Compose display name: `{quality} {prefixes*} {material} {piece} {suffixes*}`. Compose id: `{quality?}_{material?}_{base}+{affixes...}`.
6. Soulbound = base.soulbound OR any applied affix's `soulbinds: true` — boss-shard affixes flip to BoP on imprint.

### World & data

All design data is YAML under `src/generated/`, compiled from Python seed scripts (see `scripts/seed_*.py`). Bulk writes ≥15 files always go through a seed script, never per-file edits.

```
src/generated/
├── archetypes/         15 class positions (barycentric M/A/F triangle)
├── abilities/          per-pillar/category ability tiers (25/50/75/100)
├── flavored/           school-flavored variants + per-ability stat overrides
├── schools/            27 schools with morality + pillar
├── factions/           faction-gating rules
├── races/              10 playable races with creature_type refs
├── bestiary/           11 creature_types + 10 armor_classes (mob inheritance root)
├── institutions/ + archetypes/*/orders/   flavored Order system
├── items/              composition tables for the runtime resolver
│   ├── bases/{armor,weapons,shields,runes,consumables,materials}
│   ├── materials.yaml  25 substances (copper → adamantine, linen → voidcloth,
│   │                   leathers, gambesons)
│   ├── qualities.yaml  7 craft-roll tiers (crude → masterful)
│   └── affixes.yaml    27 affixes (11 suffix, 6 elemental banes, 5 prefixes,
│                       5 shard-only soulbinding)
└── world/
    ├── world.yaml + progression/
    ├── biomes/, continents/, zones/<id>/, dungeons/<id>/
```

**Item seeder** (`scripts/seed_items.py`) is a package — `scripts/items/{armor,weapons,shields,runes,consumables,crafting,materials,qualities,affixes}.py` — each module owns its table + `seed()` function. Orchestrator wipes `src/generated/items/` and calls them all.

Totals: **28 zones · 79 hubs · 612 mobs · 32 dungeons · 105 bosses · 30 quest chains (28 main + 2 side) · 11 creature_types · 15 class kits · 222 item bases · 25 materials · 7 qualities · 27 affixes**.

**Quest schema** (chain YAML): hand-curated chains have an `npcs:` registry naming each contact + their hub + dialogue; steps reference NPCs by id (e.g. `npc: warden_telyn`). Procedural chains still work via `target_hint` parsing at the capital hub.

**Chain hand-curation status**: `dalewatch_marches` (mannin/human) is the showcase zone — fully hand-curated, Classic-Elwynn-scale redesign. 10-step main chain (Warden Telyn → Brother Fennick → Scout Iwen → Sergeant Rook → Grand Drifter Valenn) + 2 side chains (Grain Thief / Grove Keeper) + 20 side quests across 4 hubs (Dalewatch Keep, Harrier's Rest, Kingsroad Waypost, Miller's Crossing, Ford of Ashmere). Underlying story in `dalewatch_redesign.md`. Other 9 starter zones use procedural target_hints until curated.

**Hub placement schema**: hub YAMLs accept an optional `offset_from_zone_origin: { x, z }` for big-zone layouts (honored by the server spawn loader). Zones without this field keep the legacy 8u-radius tight layout. Non-hub sub-zones are declared in a per-zone `landmarks.yaml` for reference (not server-loaded today — used by `investigate`-step `location:` targets as display hints).

## Design principles

- **Abstract first, flavor second.** Math (class position, capability tiers, school mechanics) is faction-neutral. Flavor (faction names, order affiliations, player-facing class names) is a separable layer.
- **Math-first, sim-validated balance.** Combat simulator will use PPO-trained rotations to validate class parity. Outcome equivalence, not hand-tuning.
- **Mechanical vs narrative identity.** ~30 sim profiles are the balance budget. Flavor variants (Orders, race skins, named identities) are unlimited on top.
- **Strict morality gating.** No oxymorons (no undead priests). Evil schools → evil faction; good → good; neutral → both. Each mechanical role has ≥1 morally-accessible school per faction.
- **Hybrid-first classes.** Most classes are dual-role-capable; pure tank/heal/DPS are "advanced cooperative" designated.
- **Strict coop, no solo content.** Target: close-friend / household groups. Every activity requires ≥2 players. Combat is continuous action-style (New World reference), not tick-based.
- **Bestiary inheritance.** Every mob and playable race references a `creature_type` (beast / humanoid / undead / demon / aberration / elemental / construct / fey / giant / dragonkin / living_construct). HP scaling, default armor, resistances, school affinities all inherit from the type. Validator catches "light-devotion ashwolf" / "poison golem" incoherence.

## Class position system

Every character sits at a position in a quantized barycentric triangle:

- **Might** — physical: armor, weapons, endurance, threat
- **Arcana** — magical: spells, rituals, wards, control
- **Finesse** — cunning: stealth, precision, evasion, crafting

Each pillar ∈ {0, 25, 50, 75, 100}, summing to 100. **15 valid positions**. Internal labels (Fighter, Paladin, Cleric, Druid, Wizard, Sorcerer, Warlock, Bard, Rogue, Ranger, Monk, Barbarian, Duskblade, Mystic, Warden) are dev-facing only; player-facing names come from faction/Order flavor.

## Testing

```bash
cargo test --workspace
```

**374 tests pass** across the prior coverage (class position invariants, combat parity (GCD-aware), stats-aware damage pipeline, YAML loads, item composition, affix validation, loot drops, inventory stacking, equipment slot validation, economy / wallet, profession skills, NPC stat derivation, party split-XP, chat rate-limit + parser, persistence round-trip) plus the slice 1-9 additions (PolyHavenCatalog / dressing / scatter / side-quest givers / mob banding / level XP curve / emote parser / corpse-run / netcode-key / panic-handler / auto-reconnect / SQLite accounts / quest polish), plus Slice 6: **boss-drop loader** (3 — Valenn 12-piece, Halen 3-piece, unknown-mob = none), **`decide_roll_winner`** (6 — need beats greed, single-need auto-win, single-greed when no need, tied-need d100, tied-greed d100, all-pass = no winner, empty = no winner), **`RollItemState::all_voted`** (1), **`eligible_for_roll`** (3 — in-radius partners + killer, killer-not-in-party, non-party-in-radius), **YAML guards** (Halen L9, Valenn L10, drifters_lair dungeon yaml, step 10 targets Valenn at L10).

4 pre-existing `vaern-combat` failures (`attacker_kills_dummy`, `resource_gate_delays_kill`, `parity.rs` × 2) all stem from `apply_deaths` being moved to the server-only schedule — the `common::headless_app` test harness loads only the shared `CombatPlugin` which has `detect_deaths` without its follow-up despawn. Unrelated to runtime gameplay.

Re-seed items:
```bash
python3 scripts/seed_items.py
```

## Open TODOs

### Design
- [ ] **Faction naming** — bind `faction_a` / `faction_b` placeholders to Concord / Rend
- [ ] **Order system delivery** — in-world organizations that teach schools; how you join
- [ ] **Progression mechanics** — how characters move between class positions
- [ ] **Numeric balance** — damage, CDs, cast times, resistance multipliers (sim-driven)
- [ ] **Race × class modifiers** — small racial tweaks on class stats
- [ ] **Blood counterpart beyond devotion** — audit remaining evil-school mechanical gaps

### MMO-feel (pre-alpha Tier-1)
- [x] **Currency loop** — `PlayerWallet` + coin drops scaled by mob rarity + tier, quest gold payout on step/chain complete, `WalletSnapshot` on change, wallet UI under Inventory heading. Persisted as `PersistedCharacter.wallet_copper`.
- [x] **Live vendor NPCs** — 10 general-goods vendors at starter-zone capitals, `NpcKind::Vendor` + `VendorStock` + F-interact Buy/Sell window, seeded from `src/generated/vendors.yaml`.
- [x] **Text chat** — Say (20u) / Zone (AoI room) / Whisper (by name) / Party (cross-zone) / System channels, rate-limited at 5/sec, 256-char truncate, server-authoritative `from`.
- [x] **Party system v1** — invite/accept/leave/kick by name, `PartyTable` + dirty-set snapshot broadcast, party frame top-left with member HP, shared XP within 40u (1.0/0.7/0.55/0.45/0.38× scaling), party chat cross-zone.
- [x] **Player nameplates** — `DisplayName` label (not pillar), 60u culling, V-toggle, chat-input-aware gating.
- [x] **Chat bubbles** — 5s speech balloons above speakers on public channels only (Say + Zone), 1s fade, 72-char truncate.
- [x] **World dressing** (Slice 1) — Poly Haven PBR scatter + ~55 authored Dalewatch hub props. See `memory/project_world_dressing.md`.
- [x] **Mob level banding** (Slice 3) — Dalewatch tiers L1-2/3-4/5-6/7+ to keep / mid / outer / Drifter's Lair. Per-kind respawn (commons 3min / elites 10min / named 30min).
- [x] **Felt level progression** (Slice 4a-c) — `level_xp_multiplier` curve (greys=0, +5 reds=1.5×), pillar-point bonus on every level-up, centered "LEVEL UP" banner + screen flash.
- [x] **Text emotes** (Slice 7) — `/wave /bow /sit /cheer /dance /point` ride the chat-bubble system. Animation playback per emote is post-pre-alpha.
- [x] **Death penalty** (Slice 5) — corpse-run MVP: 25% HP respawn, walk back to corpse for full restore, 10-min expiry. Visual marker + party-rez are post-MVP.
- [x] **Drifter's Lair pseudo-dungeon** (Slice 6, code-complete + tests green, awaits 2-client playtest) — open-world spawn region at zone-local `(470, 80)` east of Ford of Ashmere; Halen (L9 mini-boss) + Valenn (L10 capstone) flanked by 3 new L8-L10 drifter trash mobs in 4-mob pulls; new `LootRollContainer` + `LootRollOpen`/`Vote`/`Result` protocol drives Need-Before-Greed-Pass when ≥2 party members are within `PARTY_SHARE_RADIUS=40u`; one-tier-above gear ladder on Valenn (mithril/dragonscale/shadowsilk + exceptional, 4-piece per pillar) on top of Slice 4e chain capstone reward; solo / out-of-radius kills bypass to existing single-owner flow.
- [ ] **Shipping hardening** (Slice 8) — env netcode key + configurable bind + panic handler + auto-reconnect + local SQLite accounts.

### Quest + content gaps
- [x] Dalewatch Marches redesigned to Classic-Elwynn scope — 12 sub-zones, 4 hubs, 10-step main chain, 2 side chains, 20 side quests, 24 mob types, 1200×1200u playable box.
- [x] Gold / item quest rewards — `gold_reward_copper` + `gold_bonus_copper` wired to the wallet on step / chain completion.
- [x] **Side-quest givers spawn** (Slice 2). `vaern-data::HubSideQuests` schema has optional `giver:` block; server walks each zone's `quests/side/<hub>.yaml` and spawns one giver NPC per declared block. Dalewatch seeded with 5 (Hayes / Morwen / Bel / Garrick / Pell). Other zones still rely on procedural target-hint fallback.
- [x] **Level-gated quest accept** (Slice 4d). Server hard-refuses `AcceptQuest` if `chain.steps[0].level > player.level + 3`.
- [ ] Hand-curate remaining 9 starter chains (hearthkin, sunward_elen, …) with `npcs:` registries like dalewatch — **out of pre-alpha scope** (Mannin-only spawn).
- [ ] Auto-advance talk/investigate/deliver objectives (kill-step auto-advance works).
- [x] Quest state persistence — server `QuestLog` now persists via `PersistedCharacter.quest_log`.
- [ ] Quest item rewards (Slice 4e) — only XP + gold today; rolled-item rewards pending. **Blocks Slice 6.**
- [ ] Multi-kill objectives (`count > 1`) — currently advance on first kill.

### Gear / loot / crafting next steps
- [ ] **Boss shard items** — `ItemKind::Shard { affix_id }` droppable by specific bosses, consumable at a crafter rite to imprint the shard's affix onto an item with open slots (converts to BoP). Finishes the unified loot+craft design.
- [ ] **Crafter rite + recipe system** — NPC interaction to apply shards, reroll affixes, fill slots, rarify. Recipes YAML per profession; consumes materials + reagents; skill-driven quality roll.
- [ ] **Gathering polish** — skill gains on harvest, tool requirement (pickaxe/skinning knife), world-authored node placements per zone (currently hardcoded 5/zone).
- [ ] **Crafting professions wired** — Alchemy first (potions from herbs, riding the new StatusEffect buff infra), then Blacksmithing / Leatherworking / Tailoring / Enchanting / Jewelcrafting / Bowyery.
- [ ] **Order tier sets** — per-order materials ("Frostsilver") + rite-only acquisition + unique set-bonus mechanics (not stat tags — new gameplay verbs). Decoupled from random loot.
- [ ] **Item icons** — keyed by `base_id`, same pipeline as hotbar icons. Tooltip cards + rarity colors are in.
- [ ] **Drag-and-drop** inventory ↔ paper doll.

### Combat depth
- [x] **DoTs / status effects** — `StatusEffects` infra + YAML-driven riders (fire/frost/shadow/blood seeded). Slow-aware movement.
- [x] **Active Block / Active Parry stances** — Q/E bindings; stance-aware damage pipeline; parry blocks rider debuffs.
- [x] **Animation state** — replicated `AnimState` + derive + transient flash on attack / hit.
- [x] **Haste → cooldown/cast reduction** — `vaern_stats::formula::cast_speed_scale(h) = 1/(1+h/100)` scales cast + cooldown at cast start in `select_and_fire`.
- [x] **Generic buffs (StatMods)** — `compute_damage` folds `status_damage_bonus` across active `StatMods`; consumables with `ConsumeEffect::Buff` push timed StatMods onto the user. Elixirs of Might/Finesse/Arcana/Giants seeded.
- [ ] **Threat decoupled from damage** — `threat_multiplier` exists but scales off damage; tanks should hold aggro while dealing less damage.
- [ ] **Ability-category shape tuning** — `might/offense` hand-tuned; rest fall back to defaults.

### Voxel world
- [x] **`vaern-voxel` crate landed** — chunked SDF + hand-rolled Surface Nets, 8 swappable algorithm layers, 46/46 tests.
- [x] **Client streaming + F10 stomp** — voxels stream around the camera, F10 issues a server-authoritative edit stroke.
- [x] **Server-authoritative edits** — `ValidatedEditStroke` pipeline on server: `validate_edit_requests` gates by radius + range + live-player, `apply_validated_edits` runs `EditStroke<SphereBrush>` against the authoritative `ChunkStore` and marks `EditedChunks`.
- [x] **`ChunkDelta` replication** — `VoxelChunkDelta(ChunkDelta)` (S→C) ships up to 8 chunks/tick live, plus per-client reconnect catch-up at 4 chunks/tick via `PendingReconnectSnapshots`.
- [x] **Retire the legacy ground plane** — 8000u grass plane deleted; `scene/hub_regions.rs` overlay unregistered. Voxels are the only ground.
- [x] **Server Y-snap via voxel query** — server `movement` + `npc::ai` and client `predicted_player_movement` all call `vaern_voxel::query::ground_y` with `terrain::height` fallback for unseeded chunks.
- [x] **Biome-aware voxel materials** — `BiomeResolver` + per-biome cached `StandardMaterial`s with world-XZ UVs + MikkTSpace tangents. 9 CC0 ambientCG sets, chunk-aligned transitions.
- [x] **Seam closure** — `ChunkShape::MESH_MIN = PADDING - 1` closes static +side chunk seams; `chunks_containing_voxel` enumeration `{-1, 0, +1}` closes dynamic seams after edits (no more textured cap over carved craters).
- [ ] **Chunk eviction** — earlier per-frame distance evictor made the whole 3D scene go dark when enabled (unknown render-pipeline interaction). Disabled. Memory grows monotonically on both server and client until this is root-caused.
- [ ] **Zone-scoped delta broadcast** — today every `VoxelChunkDelta` goes to every client; wiring lightyear Room scope by chunk zone would reduce bandwidth with multi-zone concurrent play.
- [ ] **Sparse delta encoding** — broadcast uses `ChunkDelta::full_snapshot` (~150 KB/chunk). `encode_delta(old, new, writes)` exists in the crate but needs per-sample write tracking through `EditStroke`.
- [ ] **Roads on voxel ground** — the `hub_regions.rs` Catmull-Rom + wiggle helper isn't ported yet. A "dirt-road" biome override along each road path would bake roads into the same biome pipeline.
- [ ] **Teardown** — chunk entities don't carry `GameWorld`, so they persist across logout into the next in-game session.
- [ ] **F10 bandwidth / re-mesh lag** — a few-tick visual delay between stomp and the textured cap despawning. Not a bug, just network RTT + `MESHING_BUDGET=64/frame` chunks draining. Raise the budget (or async mesher) if this becomes annoying.

### Infrastructure / polish
- [x] **Area-of-interest replication** — zone-scoped lightyear rooms; only the player's current zone replicates.
- [x] **Per-tick broadcast spam** — InventorySnapshot / EquippedSnapshot / PendingLootsSnapshot gated on change.
- [x] **Server tick-rate logger** — Hz + max-frame telemetry every second.
- [x] **Own-player character mesh** — Quaternius modular outfit driven by equipped armor; gender picker in char-create.
- [x] **Own-player animation** — UAL clip pipeline via AnimState + cast_school + mainhand weapon. Transient one-shot swings hold until clip finishes.
- [x] Ground pipeline — chunked SDF voxel world (`vaern-voxel`) streamed around the camera, seeded from `vaern_core::terrain::height`. Replaces the retired tessellated grass plane. Shared heightmap still drives server-side Y-snap for players + NPCs.
- [x] Atmosphere + fog + bloom + tonemapping + HDR — procedural `AtmospherePlugin` sky, `FogFalloff::from_visibility_squared(1500.0)`, `Bloom::NATURAL`, `Tonemapping::TonyMcMapface`, `Exposure::SUNLIGHT`.
- [x] Loot container visual — `assets/extracted/props/Bag.gltf` mesh (replaces the yellow gizmo sphere).
- [x] Quest-giver humanoid skins — hashed fallback picks one of 12 Quaternius archetypes from the display name when the mob map has no explicit entry.
- [ ] Replace zeroed netcode private key before public exposure.
- [x] Server-side character persistence — `PersistedCharacter` JSON at `~/.config/vaern/server/characters/<uuid>.json`; 5s wall-clock flush + save-on-disconnect observer; inventory, equipped, belt, pillars, quest log, wallet, position all survive. See `memory/project_server_persistence.md`.
- [ ] Zone transitions / portals / dungeon entry UI (32 dungeon YAMLs exist, not instanced).
- [ ] Ground mesh / fancy visuals for resource nodes (still gizmo spheres).
- [ ] HDRI-based skybox + IBL (3 Poly Haven `.hdr` files downloaded; needs equirectangular → cubemap bake for Bevy's `Skybox` + `EnvironmentMapLight`). Procedural `Atmosphere` covers the sky for now.
- [ ] Player-follow / tiled ground (ground is a finite 8000u plane centered on world origin; content past ±4000u would reveal the edge).
- [ ] PPO balance trainer in `vaern-sim`.
- [x] **Remote player + NPC Quaternius mesh** — all visible characters (own, remote, humanoid NPCs) render as Quaternius on the UE-Mannequin skeleton and share the UAL animation pipeline.
- [x] **Weapon overlay on Quaternius rig** — `QuaterniusWeaponOverlay` attaches MEGAKIT props (Sword_Bronze / Axe_Bronze / Table_Knife / Shield_Wooden / Pickaxe_Bronze) to the `hand_r` / `hand_l` bones via `assets/quaternius_weapon_grips.yaml`. Own + remote players derive from equipped gear; humanoid NPCs derive from their `NpcAppearance.archetype` (knights carry sword + shield, nobles sword, rangers knife, peasants axe, wizards empty-handed). MEGAKIT only ships 5 props so bow/staff/wand slots still render empty — asset expansion is a separate slice.
- [ ] **Clip per weapon / ability category** — `Sword_Attack` is used for every physical cast. UAL has `Sword_Regular_A/B/C + Combo` for multi-stage swings; bow needs a separate clip set (none ship in UAL).

### Known rough edges
- `Casting` + `AnimOverride` components are registered for prediction but dynamic insertion on the own-player Predicted copy is unreliable in lightyear 0.26. Cast bar + transient-anim flashes are driven by `PlayerStateSnapshot` / `CastFiredLocal` messages instead.
- Auto-attack light/heavy specs are hardcoded blade cones; should branch on equipped weapon school.
- NPCs don't have their own CombinedStats-derived melee damage yet — raw `attack_damage` on the spawn slot.
- Starter gear + hotbar are pillar-keyed (Might / Finesse / Arcana × 1 kit each). Archetype-specific kits land with the archetype-unlock path.
- Paper doll is two columns of slot buttons; no real character silhouette yet.
- Character gender is client-local only; no server-side storage or replication.
- Party HP updates between snapshots rely on join/leave/kick to re-broadcast — a future 500ms heartbeat would keep frame bars live during combat.
- Own player's Replicated + Predicted copies both spawn their own nameplate (double plate over own head in third-person); fix by filtering out own entity on spawn.
- 4 pre-existing `vaern-combat` test failures (`attacker_kills_dummy`, `resource_gate_delays_kill`, parity tests) from `apply_deaths` living on the server-only schedule — the headless test harness runs only the shared `CombatPlugin`. Unrelated to gameplay runtime.

## World & lore

`src/world_theory.yaml` contains the original design: Vaern island-continent geography, Concord (Veyr, defenders) vs Rend (Hraun, arrivals), race list, 4-layer mystery-revelation system, hardcore death design. **Deprecated sections** in that file: `classes`, `multiclass_system`, `build_totals` — superseded by the class position system above.

## Memory

Claude Code persistent memory at `~/.claude/projects/-home-mart-git-rust-mmo-project/memory/`. Encodes design principles, working context, and non-obvious architectural decisions established across sessions.
