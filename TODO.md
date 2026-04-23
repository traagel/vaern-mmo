# Vaern TODO

Rolling slice list. Checked items landed. "Next" = my current recommendation order. See `memory/` for design rationale on each.

## Just landed (2026-04-21 session — combat depth + AoI + paper doll)

### Combat fixes (diagnosed from in-game reports)
- [x] **Tab targeting range-gated** — `MAX_TARGET_RANGE = 40u`, zones are 800u apart so no more cross-zone snaps.
- [x] **Tab targeting directional** — prefers NPCs within 80° front cone of camera; falls back to nearest-overall when none in front. Filters out `NpcKind::QuestGiver`.
- [x] **Channeled cone/line range bounded** — `Casting` gained a `range` field; `progress_casts` uses it instead of `f32::INFINITY`. Fixes heavy attack (RMB cone) sweeping all mobs in facing direction to infinity.

### Network / performance
- [x] **Server tick-rate logger** — `[tick] 60 Hz  avg_frame=16.72ms  max_frame=16.74ms` once per real second.
- [x] **Per-tick snapshot broadcasts gated on change** — `InventorySnapshot` + `EquippedSnapshot` now fire only on `Changed<PlayerInventory>|<Equipped>`. `PendingLootsSnapshot` gated on a `PendingLootsDirty` resource flipped by spawn / despawn / take handlers.
- [x] **Area-of-interest replication** — one lightyear `Room` per starter zone. NPCs + nodes opt in via `NetworkVisibility`; players + projectiles stay globally visible. Each client's link migrates between rooms as its player crosses zones. Fixes the localhost UDP overflow that caused NPC rubber-banding (603 entities × 60Hz × N clients was saturating the kernel buffer). See `memory/project_aoi_replication.md`.

### Combat depth (status effects + stances + stamina + anim)
- [x] **`StatusEffects` infrastructure** in `vaern-combat/effects.rs` — `Vec<StatusEffect>` component, refresh-on-reapply semantics, self-removes when empty.
- [x] **`EffectKind::{Dot, Stance(Block|Parry), Slow, StatMods}`** — Block/Parry are modelled as stance variants, not bespoke components. Mutual exclusion by `id`.
- [x] **`Stamina` component** — 100/100, 12/s regen. Separate from `ResourcePool` (mana).
- [x] **Active Block (Q hold)** — 15/s drain, 60% frontal / 25% flank / 0% rear damage reduction. Stance breaks when stamina = 0.
- [x] **Active Parry (E tap)** — 0.35s negate window, 20 stamina per successful consume. Full damage negate + blocks rider debuffs. Free to miss.
- [x] **`apply_stances` + `resolve_hit`** — unified stance + rider-effect application at all three damage sites (select_and_fire / progress_casts / tick_projectiles).
- [x] **YAML-driven effect riders** — `applies_effect: { id, duration_secs, kind: dot|slow, dps, tick_interval, speed_mult }` on flavored variants. Mirror `FlavoredEffect` in vaern-data → `EffectSpec` at overlay time. Seeded fire→burning, frost→chilled, shadow→decay, blood→bleeding at tiers 25 + 50.
- [x] **Slow-aware movement** — `StatusEffects::move_speed_mult()` multiplies player + NPC movement step. Strongest slow wins (doesn't stack).

### Animation state
- [x] **`AnimState` enum replicated** — Idle / Walking / Running / Casting / Blocking / Attacking / Hit / Dead.
- [x] **`derive_anim_state`** in FixedUpdate — priority Dead > Blocking > Casting > Running > Walking > Idle. Speed thresholds 0.5 / 3.0 u/s.
- [x] **Transient Attacking + Hit flashes** — `mark_attack_and_hit` on CastEvent sets both sides, `AnimOverride { remaining_secs: 0.25 }` freezes derive for the flash.
- [x] **Nameplate state badge** — every nameplate shows `[idle]` / `[casting]` / `[blocking]` etc.

### UI — paper doll + tooltips + rarity colors
- [x] **Inventory panel rewritten** — 30 slots in 3×10 grid, 20 equipment slots in two sub-columns (11 armor / 9 jewelry+weapons+focus).
- [x] **Hover tooltip cards** — rarity-colored name, kind line, nonzero stats, per-channel resists, soulbound tag, weight.
- [x] **Rarity palette** — WoW-standard: Junk grey, Common white, Uncommon green, Rare blue, Epic purple, Legendary orange.
- [x] **Right-click to unequip** — replaces the old `[x]` button.

### Session next options

- [x] **Hotbar consumable belt (keys 7/8/9/0)** — 4-slot `ConsumableBelt` component + 4 protocol messages (`BindBeltSlotRequest` / `ClearBeltSlotRequest` / `ConsumeBeltRequest` / `ConsumableBeltSnapshot`). Slots store `ItemInstance` templates for stack-rearrangement stability. `belt_io` handles bind/clear/consume (reuses `consume_io::apply_consume_effect`) + broadcasts on change. Client `BeltUiPlugin` ships hotkey input + strip UI; inventory panel context menu binds on right-click. Tests: find_matching/total_matching/consume_matching/bind/clear + rebind override.
- [x] **Haste → real cast/cooldown reduction** — caster's `CombinedStats.total_haste_pct` folds through `formula::cast_speed_scale(h) = 1/(1+h/100)` and scales both `cd.remaining_secs` and `Casting::{remaining_secs, total_secs}` at cast start in `select_and_fire`. Snapshot-at-start; mid-cast stat changes don't retroactively speed up the current cast. Covered by 4 unit tests (formula) + 2 integration tests (`haste_shrinks_cast_time` + `haste_shrinks_cooldown`).
- [x] **Consumables + generic buffs** — `ItemKind::Consumable.effect: ConsumeEffect` (None / HealHp / HealMana / HealStamina / Buff{id,duration,damage_mult_add,resist_adds[12]}) is YAML-authored per base. Client inventory panel left-click sends `ConsumeItemRequest` for consumables; `consume_io` applies heals (clamp-to-max) / pushes `StatusEffect::StatMods` for buffs / decrements the stack. `compute_damage` sums `status_damage_bonus` across active StatMods into the caster multiplier, and `status_resist_bonus` into the target's `resist_total[dt]` before the mitigation curve (shared 80% cap). Potions seeded with real amounts; elixirs_of_{might,finesse,arcana,giants} apply damage buffs; Warding Elixir adds +15 across all 12 resist channels for 5 min; 24 per-channel resist potions add +30/+60 on their channel for 3 min.
- [x] **Per-pillar starter kits + pillar-only char-create** — `ClientHello.core_pillar: Pillar` (not `class_id`); `PlayerTag.core_pillar`; char-create picks Might / Finesse / Arcana; server seeds `PillarScores` with 25 in the chosen pillar + 5 in others (clamped by race caps); `starter_gear::build_starter_inventory_for_pillar` + `class_kits::build_starter_hotbar_by_pillar` produce 3 distinct starter loadouts. Archetype machinery (15 `KIT_SIGNATURES`, `build_hotbar_detailed`) preserved under `#[allow(dead_code)]` for the future archetype-unlock path. 12 new tests (4 pillar-score seed + 3 hotbar-per-pillar + 6 gear-per-pillar).
- [ ] **Item icons** — keyed by base_id, same pipeline as hotbar icons.
- [ ] **Drag-and-drop** inventory ↔ paper doll.
- [ ] **Boss shard + crafter rite** — close the unified loot+craft loop.
- [ ] **Alchemy recipes** — first crafting profession; potions ride the new StatusEffect infra for buffs.

## Just landed (2026-04-20 session 7 — inventory/equip protocol + UI)

- [x] **Protocol messages** — `EquipRequest` (C→S slot + inventory_idx), `UnequipRequest` (C→S slot), `InventorySnapshot` + `EquippedSnapshot` (S→C per-tick). Helper types `InventorySlotEntry` / `EquippedSlotEntry` carry raw `ItemInstance` tuples for cheap wire format. Registered in `SharedPlugin`.
- [x] **Server `inventory_io` module** — `broadcast_inventory_and_equipped` every tick; `handle_equip_requests` (take → equip → push prev/displaced back, restore on failure); `handle_unequip_requests` (unequip → inventory.add).
- [x] **Client `ClientContent` resource** — `ContentRegistry` loaded at startup from same `src/generated/items/` tree as server. Client resolves instances locally for display names; server sends raw instance tuples.
- [x] **Client state resources** — `OwnInventory` + `OwnEquipped` fed by the snapshot messages. Latest snapshot wins.
- [x] **Client UI** — `inventory_ui.rs` with single egui window toggled by `I`. Left: inventory list with `[idx] Name ×count`. Right: 20 equipped slots. Click inventory item → auto-select first valid slot via `default_slot_for(kind)` → send `EquipRequest`. Click `x` next to equipped slot → send `UnequipRequest`. No drag-drop yet (session 8).
- [x] **Workspace tests: 89 pass** (no new tests — protocol + UI work is runtime-verified).

### Session 8 (next — UI polish + per-class starters)

- [ ] **Paper doll visual layout** — replace the flat equipped list with a character silhouette + slot rings around it (WoW-style). Hover to tooltip, right-click to unequip.
- [ ] **Drag-and-drop** — drag from inventory slot to paper doll slot. Drag between inventory slots to rearrange.
- [ ] **Tooltip cards** — hover any slot → egui card with stat breakdown, rarity color, material flavor.
- [ ] **Icons** — reuse school-icon palette or author per-piece icons; map by base_id.
- [ ] **Class-specific starter kits** — fighter gets iron sword + bronze buckler + gambeson; mage gets wand + cloth robe + linen shirt; rogue gets dagger + dark leather + throwing knives. Edit `STARTER_KIT` table or move to per-race / per-class YAML.

## Just landed (2026-04-20 session 8 — affixes + loot drops)

- [x] **Affix layer in composition** — `Affix { id, display, position (Prefix/Suffix), applies_to, min_tier, max_tier, stat_delta, weight, soulbinds }`, loaded from `affixes.yaml`. Resolver validates applies_to, folds stat_delta into final stats, soulbinds propagate (`base.soulbound || any affix.soulbinds`).
- [x] **Display + id composition with affixes** — `"Masterful Enchanted Steel Longsword of the Eagle"` / id `masterful_steel_longsword+enchanted+of_the_eagle`.
- [x] **27 seeded affixes** — 11 universal suffixes (of_warding, of_striking…), 6 elemental banes, 5 prefixes (enchanted, sturdy…), 5 shard-only (of_the_frostwarden, of_the_flamecaller, of_worldserpent, of_the_first_ember, of_dawnkeeper — weight 0, soulbinds true).
- [x] **`vaern-loot` crate** — `DropTable { drop_chance, material_tier, base_kinds, rarity_curve }`, `roll_drop(&table, &reg, &mut rng)`, `DropTable::for_npc(kind, tier)` defaults. Rarity emerges from resolved material+quality so "what you roll" and "what player sees" always match. Shard-only affixes never random-roll.
- [x] **Server mob-death drops** — `loot_io::award_loot_on_mob_death` observer, top-threat player gets the instance directly in inventory. InventorySnapshot broadcast already handles client updates. `LootRng` resource seeded by wall-clock.
- [x] **Tests: 89 → 106** (11 affix tests, 6 loot tests — seeded for determinism, distribution checks on named vs combat, shard-affix-never-rolled invariant).

### Session 9 (next — bring gear to combat OR gathering/crafting)

- [ ] **Stats → combat** — the big "gear matters" slice. Weapon damage reads from weapon_min/max_dmg, incoming hits mitigate by armor + resist_total[damage_type], crit uses total_crit_pct.
- [ ] **Gathering professions** — Mining/Herbalism/Skinning/Logging. Resource nodes in zones. Profession skills.
- [ ] **Crafting professions** — Blacksmithing/Leatherworking/Tailoring/Alchemy/Enchanting/Jewelcrafting. Recipes, skill-driven quality rolls, station interact.
- [ ] **Ground item entities** — drop-then-F-to-pickup (replaces direct-to-inventory once party-loot-rules matter).

## Just landed (2026-04-20 session 6 — inventory foundation)

- [x] **`vaern-inventory` crate** — new leaf crate with `PlayerInventory` component (fixed-capacity slot grid) + `InventorySlot { instance, count }`. Stack-merge logic keyed on full tuple (base_id, material_id, quality_id, affixes) so enchanted variants don't merge with their mundane versions.
- [x] **Add / take / take_all / iter / total_weight_kg** — all resolve through `&ContentRegistry` so weight math reflects material multipliers.
- [x] **Starter gear module** (`vaern-server::starter_gear`) — universal kit: iron sword + bronze buckler + linen shirt + linen trousers + 5× minor healing + 5× minor mana + 1× flameguard rune. Single constant table; easy to edit per class later.
- [x] **`Equipped` + `PlayerInventory` attached on player spawn** — bundled into `gear` tuple in `connect::spawn_player`. Fresh players get starter kit in inventory; equipped slots start empty.
- [x] **Base shirt renamed** — `cloth_linen_shirt` → `cloth_shirt` so material composition produces "Linen Shirt" rather than "Linen Linen Shirt". Piece_name "Shirt" stays generic.
- [x] **Workspace tests: 80 → 89** (9 new inventory tests + 1 unchanged equipment/economy).

### Session 7 (next — inventory / equip protocol + paper doll UI)

- [ ] **Protocol messages** — `EquipRequest { slot, inventory_idx }` (C→S), `UnequipRequest { slot }` (C→S), `InventorySnapshot` + `EquippedSnapshot` (S→C). Register in SharedPlugin; tick-send on change.
- [ ] **Server handlers** — `handle_equip_request` (take from inventory → Equipped::equip → push previous back), `handle_unequip_request` (Equipped::unequip → inventory.add).
- [ ] **Client inventory panel (egui)** — 30-slot grid, item tooltips via resolved stats, drag source.
- [ ] **Client paper doll (egui)** — 20-slot body layout (head, shoulders, chest, wrists, hands, waist, legs, feet, neck, 2× ring, 2× trinket, mainhand, offhand, ranged, shirt, tabard, back, focus). Drag target + right-click-to-unequip.
- [ ] **Client content registry** — client needs `ContentRegistry` to resolve item tooltips. Either replicate parts tables on connect, or ship in a shared data blob.

## Just landed (2026-04-20 session 5 — consumer migration to ItemInstance)

- [x] **`Equipped` stores `slot → ItemInstance`** (was `slot → String`). All inspection helpers (equip/unequip/iter/get/totals) resolve through the registry on demand.
- [x] **`validate_slot_for_item(slot, &ResolvedItem)`** — equipment validation reads the resolved view, not the flat Item.
- [x] **Two-hander displacement works via instance lookup** — equip resolves mainhand to check grip before blocking offhand.
- [x] **Economy on `ResolvedItem`** — `vendor_buy_price / vendor_sell_price / market_floor` take the resolved view. Composed base_price flows through.
- [x] **Legacy deleted** — flat `Item` struct, `ItemRegistry`, `LoadItemsError`, `load_dir`/`load_tree` on old flat YAML, and 4 legacy-only tests all removed.
- [x] **Equipment + economy tests rewritten** — in-memory registry helpers compose minimal bases + `testmetal` material + `regular` quality; each test builds `ItemInstance` and exercises the full resolve → validate → equip path.
- [x] **Workspace tests: 83 → 80 pass** (4 legacy tests dropped, 1 new `unknown_base_resolves_to_error` added).

### Session 6 (next — inventory foundation)

- [ ] **`vaern-inventory` crate** — `PlayerInventory { instances: Vec<ItemInstance> }` component on server. Add/remove/iter methods. Stack semantics (stackable bases merge by base_id + material_id + quality_id match).
- [ ] **Equip/unequip message protocol** — C→S `EquipRequest { slot, inventory_idx }`, `UnequipRequest { slot }`. S→C `EquippedSnapshot` mirroring `PlayerStateSnapshot` pattern.
- [ ] **Starter gear grant** — on first spawn, drop a fighter/mage/rogue starter kit into `PlayerInventory` so dev clients have items to equip.
- [ ] No UI yet — session 7 handles paper doll + inventory panel in client.

## Just landed (2026-04-20 session 4 — Model B compositional items)

- [x] **Compositional item model** — bases × materials × qualities → resolver. `vaern-items::composition` new module. `ContentRegistry::resolve(ItemInstance) → ResolvedItem` folds the tuple into a display-ready item.
- [x] **Seeder rewrite** (`scripts/seed_items.py`) — emits small part tables instead of the flat cartesian expansion. **181 bases + 25 materials + 7 qualities on disk → ≈3,241 resolvable (base × material × quality) combos.**
- [x] **Material side** — 7 metals (copper→adamantine), 6 leathers (boarhide→dragonscale), 5 gambesons, 7 cloths (linen→voidcloth). Per-material `resist_adds` bake in material-specific effects (silver vs necrotic/radiant, dragonscale vs fire, shadowsilk vs necrotic with radiant penalty).
- [x] **Quality side** — 7 qualities (crude → masterful), each with `stat_mult` and `rarity_offset`. Orthogonal to material: a `masterful iron longsword` and a `crude mithril dagger` are both meaningful outcomes.
- [x] **Server integration** — `GameData.content: ContentRegistry` replaces `GameData.items`; loads bases/materials/qualities at startup.
- [x] **Tests** — 8 new resolver tests (load, compose, material effects, quality scaling, invalid pairing, id/display composition). Workspace total: 77 → 83.

### Session 5 (next — Model B consumer migration)

- [ ] **Migrate `Equipped` to `ItemInstance`** — slot → ItemInstance instead of slot → String. Update `validate_slot_for_item` to take a `&ResolvedItem` or walk registry. Equipment tests rewritten to compose instances.
- [ ] **Vendor pricing on `ResolvedItem`** — economy crate switches off legacy `Item` struct to `ResolvedItem`. Delete legacy `ItemRegistry` + flat `Item` YAML loader.
- [ ] **Loot roll → `ItemInstance`** — the whole point of Model B: loot tables pick (base, material, quality) under encounter-level + material-tier filters. Precursor to `vaern-loot`.

## Just landed (2026-04-20 session 3)

- [x] `scripts/seed_items.py` — procedural seeder: armor (type × slot × 6 tiers), weapons (school × variant × tier), shields, consumables, materials + reagents. **579 items.**
- [x] `src/generated/items/` tree — armor/{cloth,gambeson,leather,mail,plate}/*.yaml, weapons/<school>.yaml, shields.yaml, runes.yaml, consumables.yaml, materials.yaml
- [x] `ItemRegistry::load_tree` — recursive YAML walker; `GameData.items` loaded on server start
- [x] **Layered-armor layer mapping fix** — robes now `padding` (mage body garment, not undershirt); linen shirts in `under`; cloaks in `over`. Cloth spans 3 layers; leather spans 2; gambeson/mail/plate one each.
- [x] **`ArmorLayer::Ward`** — new outermost magical-absorb layer for the caster surface (Ward → Over → Plate → Chain → Padding → Under).
- [x] **`ItemKind::Rune { school }` + `EquipSlot::Focus`** — caster-only magical ward gear. 72 runes seeded (12 damage channels × 6 tiers).
- [x] **Negative mp5 as upkeep** — runes drain mana instead of needing a bespoke upkeep mechanic. "Magic-tank wizard" build trade: survive magical boss damage at the cost of casting throughput.
- [x] **`Item.stats: SecondaryStats`** — baseline stat roll field on Item. Runes populate; armor/weapons default to zeros until next slice.
- [x] Tests — runes carry magical absorb + drain; Focus slot accepts only Rune; total 77 workspace tests pass.

## Just landed (2026-04-20 session 2)

- [x] `vaern-core::DamageType` — 12-variant enum matching actual school `damage_type` values
- [x] `vaern-stats` crate — pillar identity + 3-tier gear stat pool
- [x] `vaern-items` crate — Item / SizeClass / Rarity / ItemKind + YAML registry loader
- [x] `vaern-items::ArmorType` + per-material resist profile (KCD damage-vs-armor math)
- [x] `vaern-items::ArmorLayer` + `BodyZone` (Phase A of layered armor)
- [x] `vaern-economy` crate — vendor buy/sell/market floor, GoldSinkKind ledger enum
- [x] `vaern-equipment` crate — 19-slot paper doll (Vanilla-parity incl. Shirt + Tabard)
- [x] Server integration: `PillarScores/Caps/Xp` attached on player spawn; race affinity → `PillarCaps`
- [x] Protocol: `PlayerStateSnapshot` gained 12 pillar fields (scores, caps, banked XP, XP-to-next × 3 pillars)
- [x] `GameData.races` loaded from `src/generated/races/<id>/core.yaml`

## Next slice options (pick one)

### Quick wins (~1 session each)

- [ ] **Client unit frame pillar bars** — read new `PlayerStateSnapshot` fields into `OwnPlayerState`, add three bars (Might / Finesse / Arcana) to the top-left unit frame. Data is already arriving; just unread.
- [ ] **Pillar XP hook on ability cast** — in `combat_io` or `select_and_fire`, call `award_pillar_xp()` with `XP_PER_ABILITY_CAST`, mapping school → pillar via `vaern-data` school registry. Players start earning pillar points from play immediately.
- [ ] **HP from pillars** — swap `Health::full(100.0)` in `connect::spawn_player` to `derive_primaries(&PillarScores::default()).hp_max`. Requires deciding what happens on level-up (current logic bumps `Health.max` directly; eventually replace with pillar-XP awards).

### Medium slices (~1-2 sessions)

- [ ] **Loot crate (`vaern-loot`)** — drop tables with TrinityCore-style mutually-exclusive groups, Need / Greed / Pass rolls on group-shared bags, quality thresholds. Registry now has 507 items to reference.
- [ ] **Item → SecondaryStats field** — extend `vaern-items::Item` with `stats: SecondaryStats`, extend seeder to roll armor/crit/resists from `StatBudget` per tier × rarity. Required before `combine()` can surface gear totals.
- [ ] **Equipment on player entity** — attach `Equipped` + `SecondaryStats + TertiaryStats` resources, wire `combine()` into broadcast so UI can show gear totals.
- [ ] **Item affix variants** — once SecondaryStats lives on Item, extend seeder with "of Fortitude / of the Bear / ..." affix suffixes to fan 507 base items → ~3-5k variants. Or defer affixes to instance-level roll in vaern-loot.

### Bigger slices (Phase B+ from layered-armor plan)

- [ ] **Layered armor Phase B** — expand `EquipSlot` enum from 19 flat slots to zone × layer matrix; `Equipped` enforces one item per (BodyZone, ArmorLayer) pair.
- [ ] **Layered damage resolution** — combat iterates armor layers front-to-back (Over → Plate → Chain → Padding → Under), each layer's `base_resist_profile()[damage_type] × coverage × tier_mult` reduces damage, residual passes through. Makes blade/blunt/spear mechanically distinct vs plate/mail/leather/cloth.
- [ ] **Active Block / Active Parry** — input handler (hold-key while shield equipped = Block stance; tap-timing with melee weapon = Parry stance; mutually exclusive). Stamina system needed first. See `memory/project_stat_armor_system.md` for stance semantics.
- [ ] **Encumbrance tiers** — total weight_kg vs carry_kg derived-from-Might → movement speed + stamina regen penalties. Layered armor's realism payoff.
- [ ] **Phase C content pipeline** — 10k+ layered armor items via seed scripts + paper doll UI with zone-diagram + layer stacks.
- [ ] **Phase D polish** — layered mesh rendering (top 2 layers visible), crafter specialization progression (tailor → mailer → armorer distinct skill trees).

## Open design questions (unresolved)

- [ ] **Sockets / gems** — deferred from item-system design. Decide yes/no before item content pipeline lands.
- [ ] **Durability** — ties into repair-as-gold-sink (memory says yes to hardcore-prep framing). Not yet modeled.
- [ ] **Proc effects on gear** — chance-on-hit / chance-on-cast. Need trigger metadata model, not scalar.
- [ ] **Set bonuses** — design lives on `ItemSet`, not modeled yet.
- [ ] **Level-up logic migration** — currently `Health.max` bumps directly on level-up. Pillar system expects pillar XP to drive stat gain instead. Coherent migration requires touching xp.rs, character-system code paths.
- [ ] **Class unlocks via journey** — earlier discussion: "becoming a class" via pillar thresholds + institution rep + quest trial chains (not char-create). Design floated, not locked.

## Things NOT to do (from memory — don't re-derive)

- Don't put pillar values on gear — gear is tactical, pillars are identity (see `memory/project_stat_armor_system.md`)
- Don't move resists to tertiary — hardcore prep stays first-class (`memory/feedback_hardcore_prep.md`)
- Don't design class-specific crafts — every profession serves every class (`memory/feedback_crafting_economy.md`)
- Don't reintroduce GCD (`memory/project_mmo_architecture.md`)
- Don't query `(With<Replicated>, Without<Predicted>)` for own-player — use `PlayerStateSnapshot` (`memory/project_own_player_replication.md`)
- Don't hand-edit ≥15 similar YAMLs — use `scripts/seed_*.py` (`memory/feedback_bulk_writes.md`)
- Don't add passive parry on incoming hits — Block and Parry are both **active, mutually exclusive** stances (see stance doc in `vaern-stats/src/lib.rs` module header)

## Run recipes (unchanged)

```bash
cargo build -p vaern-server -p vaern-client
./target/debug/vaern-server                         # terminal 1
./target/debug/vaern-client                         # terminal 2 — full menu
VAERN_CLIENT_ID=1001 ./target/debug/vaern-client    # second client
./scripts/run-multiplayer.sh                        # build + server + 2 clients
```

Tests:
```bash
cargo test --workspace
# expected: 106 pass
```

Re-seed items:
```bash
python3 scripts/seed_items.py
# emits src/generated/items/{armor,weapons,shields.yaml,consumables.yaml,materials.yaml}
```
