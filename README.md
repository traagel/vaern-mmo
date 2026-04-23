# Vaern

Solo-developer hardcore two-faction persistent-coop RPG. Rust · Bevy 0.18 · Lightyear 0.26 · bevy_egui 0.39. D&D 3.5-inspired mechanics. AI-assisted pipeline.

## Status

**Playable MMO scaffold with closed combat-gear-loot-prep loop, combat depth, and visible characters.** Menu → character create (race / body / pillar) → live 3D world with a gear-driven Quaternius character mesh playing UAL clips keyed off `AnimState` (idle / walk / run / cast-hold / sword-swing / block / hit / death), per-race zones, shaped abilities with YAML-driven status-effect riders (burning / chilled / bleeding / decay), Active Block + Parry stances backed by a stamina pool, kill mobs for XP + loot drops, equip gear that actually modifies combat **and visually swaps the body mesh** via a WoW-style paper doll with rarity-colored tooltips, gather materials from world resource nodes, quaff potions (healing / mana / stamina / damage + resist buffs) mid-fight from a 4-slot hotkey belt. Server-authoritative over UDP + netcode, multi-client, with client prediction + interpolation + zone-scoped area-of-interest replication.

What works end-to-end:
- **Main menu** with character create/select (egui). Race + **body** (Male / Female — cosmetic, client-local) + **pillar** pickers (Might / Finesse / Arcana — characters commit to a pillar only at creation; archetype / Order unlocks are deferred to an evolution path). Saved characters persist to `~/.config/vaern/characters.json`.
- **10 starter zones** (1 per race) populated on startup on a 2800u ring; each player spawns in their race's zone. **Dalewatch Marches** (Mannin starter) was redesigned to Classic-Elwynn scope: 4 hubs, 12 sub-zones / landmarks, 10-step main chain, 2 side chains, 20 side quests, 24 mob types in a 1200×1200u playable box. Other 9 zones still ~15 mobs, 2 hubs, 1 main chain, 5 resource nodes.
- **Procedural uneven terrain + atmospheric sky.** Ground is an 8000×8000u tessellated plane (320 cells/axis, ~25u cell) textured with a CC0 ambientCG grass PBR set (Color + NormalGL + AO) and displaced by two octaves of sine noise. Sky is Bevy 0.18's `AtmospherePlugin` (procedural scattering), paired with `DistanceFog` (exp-squared, 1500u visibility), `Bloom::NATURAL`, `Tonemapping::TonyMcMapface`, `Exposure::SUNLIGHT`, `Hdr`. Shared `vaern_core::terrain::height` function drives both the client mesh and server-side Y-snapping for players + NPCs so entities hug the hills.
- **Top-left unit frame** — race portrait, character name, level, HP / mana / stamina bars (amber while blocking), `BLOCKING` / `PARRY` tags when active, XP bar. Populated by server-pushed `PlayerStateSnapshot` (not replication — see architecture).
- **Mouse-look camera** — cursor locked + hidden in-game; mouse drives camera yaw/pitch; scroll for zoom. Hold **LeftAlt** to free the cursor for UI clicks. Cursor auto-frees whenever an egui panel is open.
- **Target lock (Tab-cycle)** — **Tab cycles** through combat NPCs within 40u, preferring those in the camera's front cone (falls back to nearest overall when none in front). QuestGivers are excluded. **Esc clears**. While locked, the player continuously turns toward the target (kinematic motion controller).
- **Combat shapes** (tuned per-ability in flavored YAML): `target`, `aoe_on_target`, `aoe_on_self`, `cone`, `line`, `projectile`. Friendly fire on — AoE hits party. Channeled cones/lines/projectiles snapshot their `range` onto `Casting`, so heavy attack (cast-time cone) doesn't sweep to infinity.
- **Hotbar (6 key-bound + 2 mouse-bound)** — keys 1-6 fire class kit; **LMB** = light auto-attack; **RMB** = heavy auto-attack. No GCD.
- **Cast bar** (bottom-center) for abilities with `cast_secs > 0` — school-colored fill.
- **Quest flow**: walk up to a gold "!" NPC → `F` → Accept → quest log (`L`) tracks.
- **XP + levels**: kills + quest steps grant XP. Level-up via `xp_curve.yaml`.
- **Pillar XP from play**: every ability cast grants pillar points to the ability's pillar (Might/Finesse/Arcana). HP auto-scales on pillar gain via `derive_primaries`.
- **NPC AI**: per-type aggro, threat-table targeting, roaming idle, leash-home. Slow-aware (chilled NPCs actually slow down).
- **NPC stats from bestiary** — creature_type resistances + armor_class physical/magic reduction fold into `CombinedStats` per mob, scaled by rarity (Combat 1.0× / Elite 1.25× / Named 1.5×). Fire dragons resist fire; plate knights resist blade.

**Character rendering (own player):**
- **Quaternius modular character mesh** drives the own-player avatar. Body / legs / arms / feet / head-piece spawned slot-by-slot under the player entity, each independently colored.
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
- `Tab` — cycle target (40u max range, prefers NPCs in the camera's front cone, QuestGivers excluded)
- `Esc` — clear current target
- `I` — toggle inventory + paper doll (auto-frees cursor while open)
- `C` — toggle character / stat screen
- `G` — loot nearest container within 5 units (opens loot window)
- `H` — harvest nearest resource node within 3.5 units (mining / herbalism / logging)
- `LeftAlt` — hold to free cursor for UI clicks + disable mouse-look
- `F` — talk to nearest quest-giver (≤5u range, gold "!" nameplate)
- `K` — toggle spellbook · `L` — toggle quest log · `☰` top-right — logout / quit

## Architecture

Workspace of sixteen crates + modular client + modular server:

```
crates/
├── vaern-core/       abstract types: Pillar, ClassPosition, Morality, Faction, School,
│                     DamageType (12 variants)
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
│                     belt_io / resource_nodes / aoi (zone-room AoI replication)
├── vaern-client/     bin: DefaultPlugins + 18 focused modules (see below)
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
├── nameplates.rs    world-space HP plates + floating damage numbers + "!" quest-givers
├── hud.rs           compass strip
├── quests.rs        loads chain YAMLs, drains QuestLogSnapshot
├── interact.rs      [F] quest-giver dialogue, [L] quest log
├── inventory_ui.rs  [I] inventory + equipment window, ClientContent registry loader,
│                    right-click consumable → bind to belt menu
├── belt_ui.rs       4-slot consumable belt strip (keys 7/8/9/0), binding snapshot
├── loot_ui.rs       [G] loot window + pending-loot gizmo markers
├── stat_screen.rs   [C] character stats (pillars + CombinedStats breakdown)
├── harvest_ui.rs    [H] resource-node markers + harvest-proximity input
└── diagnostic.rs    periodic snapshot + connect/disconnect + cast-fired logs
```

**Dependency graph (roughly):**
- `vaern-core` → nothing
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

- Server-authoritative over UDP via lightyear + netcode. Shared 32-byte private key (zeroed — replace before public exposure).
- **Replicated components:** `Transform` (prediction + linear/slerp interpolation), `Health`, `ResourcePool`, `Casting` (MapEntities), `Experience`, `PlayerRace`, `PlayerTag`, `DisplayName`, `NpcKind`, `QuestGiverHub`, `ProjectileVisual`, `NodeKind`, `NodeState`, **`AnimState`**.
- **Messages** (grouped):
  - *Combat*: `ClientHello` (C→S), `CastIntent` (C→S, MapEntities), `StanceRequest` (C→S: `SetBlock(bool)` / `ParryTap`), `CastFired { caster, target, school, damage }` (S→C, MapEntities — `caster` field drives client-side own-player attack flash), `HotbarSnapshot` (S→C).
  - *Quests*: `AcceptQuest`/`AbandonQuest`/`ProgressQuest` (C→S), `QuestLogSnapshot` (S→C).
  - *State*: `PlayerStateSnapshot` (S→C every tick — HP/pool/XP/cast + pillar scores/caps/banked XP × 3 + stamina + is_blocking + is_parrying).
  - *Inventory + equip*: `InventorySnapshot`, `EquippedSnapshot` (S→C on change, not per tick); `EquipRequest`, `UnequipRequest` (C→S).
  - *Loot*: `PendingLootsSnapshot` (S→C on `PendingLootsDirty` flag, not per tick), `LootWindowSnapshot` / `LootClosedNotice` (S→C), `LootOpenRequest` / `LootTakeRequest` / `LootTakeAllRequest` (C→S).
  - *Harvest*: `HarvestRequest` (C→S, MapEntities). Node state flows through component replication.
- **Area-of-interest replication** — one lightyear `Room` per starter zone. NPCs + resource nodes carry `NetworkVisibility` and join their zone's room at spawn; each client's link migrates between rooms as its player crosses zones. Players + projectiles stay globally visible. Pre-AoI, 603 NPCs × 60Hz Transform replication saturated the kernel UDP buffer on localhost and caused NPC rubber-banding. `RoomPlugin` must be added explicitly — it's not in lightyear's `SharedPlugins`.
- **Prediction:** own player on a `Predicted` copy; `buffer_wasd_input` → `ActionState<Inputs>` with `camera_yaw_mrad` bundled.
- **Own-player state via message, not replication.** Lightyear 0.26 gives the owning client only a `Predicted` copy — filter `(With<Replicated>, Without<Predicted>)` matches zero. HP/pool/XP/cast/stamina/stance + inventory + equipped + pending-loots all push via per-tick messages instead. Dynamic insertion/removal of predicted components (e.g. `AnimOverride`) is also unreliable on the Predicted copy, so **own-player transient animation flashes (Attacking / Hit) are driven client-side from the `CastFired` message** — server sets the flash, sends `CastFired { caster, … }`, client inspects `caster == own_player` and stamps `AnimState::Attacking` + a local `AnimOverride` to hold the swing for its full UAL clip duration.
- **`CastFired` local relay.** Lightyear's `MessageReceiver::receive()` drains on read. Multiple consumers (vfx impact flashes, nameplate damage numbers, animation flash driver, diagnostic logger) would compete over the single queue. A single `relay_cast_fired` system is the sole `MessageReceiver<CastFired>` reader and re-emits every received message as a Bevy-local `CastFiredLocal` (via `MessageWriter`). All downstream consumers read `MessageReader<CastFiredLocal>` — Bevy's per-system cursor tracking lets every subscriber see every message.
- **Loot containers are server-only entities** (not replicated). Clients see them only through `PendingLootsSnapshot` summaries owned by the top-threat player. Kills → one container per player-threat → loot is personal.
- **Respawnable component** on players resets HP/position/pool instead of despawning.
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

**106+ tests pass** across: class position invariants, combat parity (GCD-aware), stats-aware damage pipeline (armor/resist/crit/weapon-roll), YAML loads (schools/archetypes/abilities/world/bestiary/races/dungeons), item composition resolver (bases × materials × qualities × affixes), affix validation (applies_to, unknown id, soulbind propagation), loot drop distribution (combat vs named rarity skew, shard-only-never-random invariant), inventory stack merging, equipment slot validation (rune-in-focus, shield-in-offhand, two-hander-displaces-offhand), economy vendor pricing, profession skill clamps + node tier gates, NPC stat derivation from bestiary.

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

### Quest + content gaps
- [x] Dalewatch Marches redesigned to Classic-Elwynn scope — 12 sub-zones, 4 hubs, 10-step main chain, 2 side chains, 20 side quests, 24 mob types, 1200×1200u playable box.
- [ ] Hand-curate remaining 9 starter chains (hearthkin, sunward_elen, …) with `npcs:` registries like dalewatch.
- [ ] **Side-quest givers don't actually spawn.** Only `chain.npcs:` entries become world NPCs; the 20 side-quest givers in Dalewatch are orphan. Need to extend the side-quest YAML schema with `npcs:` and merge into the quest-giver spawn pass. Without this, the capital shows 1 giver instead of the budgeted 6.
- [ ] Auto-advance talk/investigate/deliver objectives (kill-step auto-advance works).
- [ ] Quest state persistence — server QuestLog is in-memory.
- [ ] Gold / item quest rewards — currently XP only; currency pool + item drops pending.
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

### Infrastructure / polish
- [x] **Area-of-interest replication** — zone-scoped lightyear rooms; only the player's current zone replicates.
- [x] **Per-tick broadcast spam** — InventorySnapshot / EquippedSnapshot / PendingLootsSnapshot gated on change.
- [x] **Server tick-rate logger** — Hz + max-frame telemetry every second.
- [x] **Own-player character mesh** — Quaternius modular outfit driven by equipped armor; gender picker in char-create.
- [x] **Own-player animation** — UAL clip pipeline via AnimState + cast_school + mainhand weapon. Transient one-shot swings hold until clip finishes.
- [x] Ground pipeline — tessellated 8000u plane with procedural sine displacement + CC0 grass PBR set (`assets/extracted/terrain/grass002/`); shared `vaern_core::terrain::height` drives both mesh generation and entity Y-snap.
- [x] Atmosphere + fog + bloom + tonemapping + HDR — procedural `AtmospherePlugin` sky, `FogFalloff::from_visibility_squared(1500.0)`, `Bloom::NATURAL`, `Tonemapping::TonyMcMapface`, `Exposure::SUNLIGHT`.
- [x] Loot container visual — `assets/extracted/props/Bag.gltf` mesh (replaces the yellow gizmo sphere).
- [x] Quest-giver humanoid skins — hashed fallback picks one of 12 Quaternius archetypes from the display name when the mob map has no explicit entry.
- [ ] Replace zeroed netcode private key before public exposure.
- [ ] Server-side character persistence (currently client-local JSON).
- [ ] Zone transitions / portals / dungeon entry UI (32 dungeon YAMLs exist, not instanced).
- [ ] Ground mesh / fancy visuals for resource nodes (still gizmo spheres).
- [ ] HDRI-based skybox + IBL (3 Poly Haven `.hdr` files downloaded; needs equirectangular → cubemap bake for Bevy's `Skybox` + `EnvironmentMapLight`). Procedural `Atmosphere` covers the sky for now.
- [ ] Player-follow / tiled ground (ground is a finite 8000u plane centered on world origin; content past ±4000u would reveal the edge).
- [ ] PPO balance trainer in `vaern-sim`.
- [ ] **Remote player + NPC mesh** — only own player is Quaternius so far. Remote players + NPCs still render as cuboids. Needs server-broadcast of minimal equipment summaries + animation-state routing per-entity.
- [ ] **Weapon overlay on Quaternius rig** — character swings an empty hand. Meshtint has `WeaponOverlay` + `WeaponGrips` calibration; Quaternius needs equivalent grips YAML.
- [ ] **Clip per weapon / ability category** — `Sword_Attack` is used for every physical cast. UAL has `Sword_Regular_A/B/C + Combo` for multi-stage swings; bow needs a separate clip set (none ship in UAL).

### Known rough edges
- `Casting` + `AnimOverride` components are registered for prediction but dynamic insertion on the own-player Predicted copy is unreliable in lightyear 0.26. Cast bar + transient-anim flashes are driven by `PlayerStateSnapshot` / `CastFiredLocal` messages instead.
- Auto-attack light/heavy specs are hardcoded blade cones; should branch on equipped weapon school.
- NPCs don't have their own CombinedStats-derived melee damage yet — raw `attack_damage` on the spawn slot.
- Starter gear + hotbar are pillar-keyed (Might / Finesse / Arcana × 1 kit each). Archetype-specific kits land with the archetype-unlock path.
- Paper doll is two columns of slot buttons; no real character silhouette yet.
- Character gender is client-local only; no server-side storage or replication.

## World & lore

`src/world_theory.yaml` contains the original design: Vaern island-continent geography, Concord (Veyr, defenders) vs Rend (Hraun, arrivals), race list, 4-layer mystery-revelation system, hardcore death design. **Deprecated sections** in that file: `classes`, `multiclass_system`, `build_totals` — superseded by the class position system above.

## Memory

Claude Code persistent memory at `~/.claude/projects/-home-mart-git-rust-mmo-project/memory/`. Encodes design principles, working context, and non-obvious architectural decisions established across sessions.
