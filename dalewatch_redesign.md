# Dalewatch Marches — Starter Zone Redesign

Design document for expanding the Mannin (human) starter zone to WoW-Classic
Elwynn-Forest scope: ~12 named sub-zones, a 10-step spine chain, 2 side
chains, ~20 side quests, and a ~600×600u initial playable area (stretch
target 1200×1200u once the ground plane grows).

Status: **design-only**. No YAML written yet. Approve or redline this doc and
I'll bulk-generate the files via a Python seed script.

---

## 1. Existing state (what we're building on)

- **Zone file** `src/generated/world/zones/dalewatch_marches/core.yaml`
  - tier: starter · level_range 1–5 · starter_race: mannin · faction_a
  - quest_count_target 50 · 15 unique mob types · ~9h solo play budget
- **Hubs** (2): `dalewatch_keep` (capital, 6 givers), `miller_crossing` (outpost, 3)
- **Main chain**: `chain_dalewatch_first_ride` — 5 steps, ends on Corlen the Drifter
- **Mobs**: wolf/boar/stag/heron/otter/bear/duck + juveniles (11 beasts), drifter
  shade, drifter mage warband, raid stray warband, named drifter mage
- **Zone geography**: all content clustered within ~45u of the zone origin.
  Effective playable space ≈ 90u × 90u.

## 2. Target scope

| Axis | Current | This pass | Stretch |
|---|---|---|---|
| Sub-zones (named areas) | 2 | **12** | 12 |
| Hubs (with quest givers) | 2 | **4** | 4 |
| Main-chain steps | 5 | **10** | 10 |
| Side chains | 0 | **2** | 2 |
| Side quests | ~7 | **~20** | ~20 |
| Filler quests | 34 | ~12 | ~12 |
| Total quests | 46 | **~50** | ~50 |
| Unique mob types | 15 | **22** | 22 |
| Named mobs (bosses) | 1 | **5** | 5 |
| Zone internal diameter | ~90u | **~600u** | ~1200u |

Stretch column assumes a later pass that grows the ground plane and zone-ring
radius. This pass keeps the ground plane at 2200u and fits dalewatch within
its current sector.

---

## 3. World context + the "why" for the player

### 3.1 The Concord

The **Concord** is a federation of Mannin city-states in the western dales —
the Mannish equivalent of a confederate kingdom. Walled towns, yeoman farms,
river-driven mills. Not a monarchy; a council of house-banners under a
Speaker. Light on heroic mythos, heavy on law, roadwardens, and grain
rotation. Think late-medieval Burgundian or post-imperial frontier.

Opposing them to the east: the **Pact** (`faction_b`). Skarn, Kharun, and
Gravewrought under a blood-oath confederation. Never openly at war with the
Concord, but the border moves.

### 3.2 Dalewatch County

The Marches are the Concord's eastern frontier. Fertile river valley, but
the Ford of Ashmere is the natural invasion corridor and everyone knows it.
The county seat is **Dalewatch Keep**, garrisoned by the **Warden Corps** —
Concord light cavalry, mixed yeoman/minor-gentry, drawn here young and
deployed fast.

The Wardens ride, they don't garrison. "They ride the line so the plough
stays in the field." Their oath is to the road, not the crown.

### 3.3 The present crisis (three converging threads)

1. **Drifters.** Rogue Concord magi. Three years ago the Academies issued
   the **Censure** — a purge of hedge-practitioners who'd been teaching
   unsanctioned blood-working. Most surrendered. Some fled east. They've
   been organizing in the fens, and some are consorting with something
   older than the Concord. They call it **the Wake**.

2. **Pact scouts.** Probing parties at the Ford of Ashmere. Not an invasion
   — yet. But the Warden Corps is thin because veterans are being pulled
   east to reinforce the Ford, which is why new recruits are being sworn in
   fast enough to still have dirt on their boots.

3. **Brine-shades.** Aberrations in the Blackwash Fens — the deep marsh
   beyond the Reed-Brake. Nobody knows what they are. The drifter Corlen
   (existing main-chain boss) was consorting with one. Killing him might
   have annoyed the nest, not cleared it.

### 3.4 Why the player cares

You've come to Dalewatch to swear the **Warden's Oath**. The Corps is
shorthanded. They put you on patrol before the ink dries. You don't fight
for a flag — you fight for the mill-roads, the ford-towns, and the specific
grandmother in Miller's Crossing who made you stew last week.

The main chain takes you from "new recruit investigating a missing boy"
(the existing arc) all the way to "swore the full oath, rode the strike on
the Blackwash, bears the Marches title". It is self-contained but also
seeds the next zone — the Ford crisis is the obvious bridge to whatever
comes next (heartland_ride or a Pact-side adventuring area).

---

## 4. Geography — the 12 sub-zones

Layout is roughly radial from Dalewatch Keep at the zone origin. Offsets are
from `zone_origin`; coordinates given as `(x, z)` in world units. Ring
positions are chosen so the roads form a rough Y shape: keep at center, west
road to Miller's, north-east road to Ashmere Ford, chapel + mine cluster
south.

| # | Sub-zone | Type | Offset (x, z) | Levels | Hub? |
|---|---|---|---|---|---|
| 1 | **Dalewatch Keep** | walled town | (0, 0) | 1–3 | ✅ capital |
| 2 | **Harrier's Rest** | chapel + hostel | (60, -100) | 1–3 | ✅ outpost |
| 3 | **Kingsroad Waypost** | patrol cabin | (-110, -30) | 2–4 | ✅ outpost |
| 4 | **Miller's Crossing** | ford town, mill | (-220, 20) | 3–5 | ✅ outpost (existing) |
| 5 | **Old Brenn's Croft** | remote farmstead | (-240, 120) | 2–4 | — landmark |
| 6 | **The Reed-Brake** | marshland | (-150, 200) | 3–5 | — landmark |
| 7 | **Thornroot Grove** | sacred forest | (100, 140) | 4–6 | — landmark |
| 8 | **Sidlow Cairn** | burial mounds | (180, -180) | 5–7 | — landmark |
| 9 | **Copperstep Mine** | abandoned mine | (150, -260) | 5–7 | — landmark |
| 10 | **Ford of Ashmere** | river crossing | (270, 40) | 6–8 | ✅ forward camp (NEW) |
| 11 | **Blackwash Fens** | deep marsh | (-50, 280) | 6–8 | — landmark |
| 12 | **The Drifter's Lair** | cave/dungeon | (20, 320) | 7–9 | — dungeon |

Four hubs total. The capital gains the most quest givers; Ashmere is a new
forward camp (outpost) that serves as the east-side launch pad for the
expanded main chain's second half.

### 4.1 Sub-zone vibes

**1. Dalewatch Keep** — stone curtain wall, packed muster yard, smell of
horse and forge. Banners of the Warden Corps. The "green" starting area:
nothing aggressive inside the walls; trainer NPCs; the main chain starts
here.

**2. Harrier's Rest** — wooden chapel to a minor road-saint; one-pot hostel
out back. Pilgrims and travelling smiths stop here before the bridge. Brother
Fennick runs the place. Road-facing, sheltered from the fens to the south.

**3. Kingsroad Waypost** — prefab patrol cabin halfway between the Keep and
Miller's. Courier relay point. Two bored deputies, a stove, a boot rack. The
bandit-ambush quests stage from here.

**4. Miller's Crossing** — existing. Stone bridge, grain mill, maybe ten
steadings. Old Brenn's flock grazes upriver.

**5. Old Brenn's Croft** — single farmhouse, three barns, sheep and the
occasional cow. Wolf packs come down from the ridge at night.

**6. The Reed-Brake** — shallow marsh on a river bend. Reed-cutter paths.
Corlen's old drifter camp was set up in an abandoned fisherman's hut. The
main chain's mid-point.

**7. Thornroot Grove** — old-growth standing stones and a spring. A keeper
tends it — half druid, half forester, wholly suspicious of outsiders. Beasts
get territorial when something's wrong with the grove.

**8. Sidlow Cairn** — low barrow-mounds on the ridge south-east of the Keep.
Old Mannin grave-sites. Local lore says the cairns "don't stay shut" — a
hint at future undead content but no Tier-1 undead this pass.

**9. Copperstep Mine** — failed copper vein, abandoned a decade. Drifter
cultists have reopened the upper shafts — not for ore but for something down
the deepest drift.

**10. Ford of Ashmere** — forward Warden camp at the strategic river
crossing. Earthworks, pickets, skirmish scouts trading rumors. Tangible
Pact pressure (skirmisher parties).

**11. Blackwash Fens** — the deep marsh beyond the Reed-Brake. Black water,
no roads, wrong smells. Brine-shade aberrations. This is where the main
chain's climax happens.

**12. The Drifter's Lair** — cave entrance in the fen, not quite a dungeon
instance (we don't have those yet) but a tight encounter space with the
final boss. Open-world pseudo-dungeon.

---

## 5. NPC roster

**Legend:** ⚫ existing · ⚪ new

### Dalewatch Keep (capital, 6 quest givers)

- ⚫ **Warden Telyn** — Captain of the Dalewatch. Main chain opener/closer.
- ⚪ **Dame Reyne Harrod** — Armory Steward. Sends you to cull wolves for hide.
- ⚪ **Marshal Orrin Vell** — Quartermaster. Boar tusks, supply runs.
- ⚪ **Corporal Mira Brask** — Recruit-master. "Find my three missing trainees."
- ⚪ **Courier Chief Waite** — Oversees mail relay. Letter-to-Ashmere quest.
- ⚪ **Lore-Keeper Anselm** — Archivist. Journal-translation mid-chain hook.

### Harrier's Rest (outpost, 2 quest givers)

- ⚪ **Brother Fennick** — chapel-keeper, part-time Concord translator.
  Mid-chain step 6 (decodes Corlen's journal).
- ⚪ **Sister Avel** — herbalist, tends the hostel. Sunweed gather quest.

### Kingsroad Waypost (outpost, 2 quest givers)

- ⚪ **Deputy Edric** — senior patroller, gruff. Bandit-ambush quest.
- ⚪ **Courier Relay Waite** (shared with Keep) — dispatches the lost-mail line.

### Miller's Crossing (outpost, 4 quest givers)

- ⚫ **Old Brenn** — existing shepherd, main chain.
- ⚪ **Miller Hadrin** — mill-keep. "Stolen grinding stones" quest.
- ⚪ **Deputy Warden Gresh** — ford-guard. Patrol quests.
- ⚪ **Fisherwoman Kest** — river-worker. Otter-cull quest.

### Ford of Ashmere (outpost, NEW — 3 quest givers)

- ⚪ **Sergeant Rook** — camp commander. Main chain step 9 staging.
- ⚪ **Scout Iwen** — ranger, tracks Pact movement. Main chain step 8 recognition.
- ⚪ **Engineer Tuck** — sent to Copperstep to survey. Side-hook for mine quests.

### Thornroot Grove (no hub, 1 quest giver)

- ⚪ **Keeper Anselm** — grove-warden. NOT the same as Lore-Keeper Anselm
  at the Keep (common Mannin name; note in lore that they're unrelated — or
  rename one). Side chain "Grove Keeper" hub.

Total NPCs: **14 unique** (3 existing + 11 new). Quest giver slots: 17
(capital 6 + outposts 2+2+4+3 = 17). Matches the hub counts.

### 5.1 Named mobs (bosses)

- ⚫ **Corlen the Drifter-Mage** — Reed-Brake, main chain step 4.
- ⚪ **Ashmane Alpha** — Thornroot Grove, level 5 wolf-pack leader.
- ⚪ **Pact Skirmisher Captain Vask** — Ford of Ashmere, level 7 raider.
- ⚪ **Master Drifter Halen** — Copperstep Mine, level 6 cultist leader.
- ⚪ **Grand Drifter Valenn + Brine-Shade Primarch** — Drifter's Lair, paired
  level 8 encounter. Final fight of expanded main chain.

---

## 6. New mob types

Current zone has 15 — we need 22. Adding 7:

| Mob ID | Kind | Rarity | Level | Location |
|---|---|---|---|---|
| `mob_dwm_beast_wolf_alpha` | beast (named) | named | 5 | Thornroot |
| `mob_dwm_warband_pact_scout` | humanoid | common | 6 | Ford of Ashmere |
| `mob_dwm_warband_pact_skirmisher` | humanoid | common | 7 | Ford |
| `mob_dwm_named_pact_captain` | humanoid | named | 7 | Ford |
| `mob_dwm_warband_drifter_cultist` | humanoid | common | 5 | Copperstep |
| `mob_dwm_named_drifter_master` | humanoid | named | 6 | Copperstep |
| `mob_dwm_exotic_brine_shade` | aberration | common | 6 | Blackwash |
| `mob_dwm_named_brine_primarch` | aberration | named | 8 | Drifter's Lair |
| `mob_dwm_named_drifter_valenn` | humanoid | named | 8 | Drifter's Lair |

That's 9 entries → 15 + 9 = 24 if we count all. Can trim 2 juveniles we
don't need, landing ~22. Acceptable overshoot either way; let's call it 22
target, land on 22–24.

---

## 7. Quest spines

### 7.1 Main chain — "The First Ride" (rework: 5 → 10 steps)

ID: `chain_dalewatch_first_ride` (extend existing).

| # | Step | Kind | Target / NPC | Level | XP |
|---|---|---|---|---|---|
| 1 | Report to Warden Telyn | talk | warden_telyn @ Keep | 1 | 520 |
| 2 | Ride to Old Brenn | talk | old_brenn @ Miller's | 1 | 620 |
| 3 | Follow the Bloodied Trail | investigate | dalewatch_reed_brake | 2 | 720 |
| 4 | Confront Corlen | kill | drifter_mage_named | 3 | 900 |
| 5 | Return Tam + Journal | deliver | warden_telyn | 3 | 1000 |
| 6 | **(NEW)** The Sealed Journal | talk | brother_fennick @ Harrier's Rest | 4 | 1100 |
| 7 | **(NEW)** Clear Copperstep | kill | master_drifter_halen | 6 | 1600 |
| 8 | **(NEW)** The Pact-Worked Focus | talk | scout_iwen @ Ashmere Ford | 7 | 1800 |
| 9 | **(NEW)** Strike the Blackwash | investigate | blackwash_fens | 8 | 2000 |
| 10 | **(NEW)** The Wake's Bones | kill | valenn + brine_primarch | 8 | 3000 |

Final reward (carries over, gets bumped):
- xp_bonus: 1200 → **3500**
- gold_bonus_copper: 500 → **1400**
- item_hint: "Warden's full cloak + Ashmere patrol-sigil + rider's steel belt"
- title_hint: "Rider of the Marches" (unchanged — it's already the right title)

Step 10's kill has two mob targets (count: 1 each). If the current schema
doesn't support multi-target kills, we either:
- (a) split step 10 into 10a (Valenn) and 10b (Primarch), bringing total to 11, or
- (b) make the Primarch a script-spawn that appears when Valenn dies (needs code).

Prefer (a) — zero code changes, keeps spine linear.

### 7.2 Side chain A — "The Grain Thief" (4 steps, Miller's Crossing)

Why it matters: sets up the minor criminal underclass in the county. Can
later reconnect to the drifter plot as a reveal in a follow-up zone. For
this pass, self-contained.

| # | Step | Kind | Target | Level |
|---|---|---|---|---|
| 1 | Meet Miller Hadrin | talk | miller_hadrin | 3 |
| 2 | Check the Granary | investigate | miller_granary | 3 |
| 3 | Track the Thief | kill | 5× raid_stray | 4 |
| 4 | Return Hadrin's Tally | deliver | miller_hadrin | 4 |

Final reward: 800 xp + 200 copper + "Miller's signed chit" flavor item.

### 7.3 Side chain B — "The Grove Keeper" (4 steps, Thornroot Grove)

Why it matters: introduces nature/druid-adjacent flavor and the "grove is
sick" seed — a hook for later zone threats (e.g., the Wake reaches beyond
the fens).

| # | Step | Kind | Target | Level |
|---|---|---|---|---|
| 1 | Speak with Sister Avel | talk | sister_avel | 4 |
| 2 | Gather Pilgrim's Herb | collect | 6× sunweed (item hint) | 4 |
| 3 | Escort to Thornroot | talk | keeper_anselm | 5 |
| 4 | The Grove's Alpha | kill | ashmane_alpha | 5 |

Final reward: 1000 xp + 250 copper + "Grove-blessed token".

### 7.4 Side quests (20 one-offs)

Distribution by hub:

**Dalewatch Keep (5)**
1. *Reyne's Armory* — kill 8 wolves (hide). L2. Giver: Dame Reyne Harrod.
2. *Quartermaster's Cache* — collect 10 boar tusks. L2. Giver: Marshal Orrin Vell.
3. *Stray Recruits* — find 3 recruits on the Kingsroad (talk). L3. Giver: Corporal Mira Brask.
4. *Letter to Ashmere* — courier (talk) to Sergeant Rook. L4. Giver: Courier Chief Waite.
5. *The Silent Bell* — investigate Harrier's Rest chapel. L3. Giver: Lore-Keeper Anselm.

**Harrier's Rest (4)**
6. *Tending the Pilgrims* — collect 6 sunweeds. L2. Giver: Sister Avel.
7. *Road-Beast Cull* — kill 10 wolves on the Kingsroad. L3. Giver: Brother Fennick.
8. *Brother's Favor* — deliver sanctified oil to Miller's Crossing. L3. Giver: Fennick.
9. *Corrupted Reeds* — gather 8 tainted reeds from the Brake. L4. Giver: Avel.

**Kingsroad Waypost (3)**
10. *Lost Mail* — recover 5 letter-cases (collect). L2. Giver: Deputy Edric.
11. *Bandit Ambush* — kill 6 raid strays. L3. Giver: Edric.
12. *Ridgeline Patrol* — investigate Sidlow Cairn. L5. Giver: Edric.

**Miller's Crossing (5)**
13. *Wolf Cull* — kill 12 wolves (the big one). L3. Giver: Deputy Gresh.
14. *Mill Stone Recovery* — kill 4 drifter mages (drop stones). L4. Giver: Miller Hadrin.
15. *Lost Flock* — talk to 5 stray herd-finders (or collect/return tokens). L3. Giver: Old Brenn.
16. *Fishing Trouble* — kill 8 otters. L2. Giver: Fisherwoman Kest.
17. *Deputy's Errand* — deliver to Sergeant Rook at the Ford. L5. Giver: Gresh.

**Ford of Ashmere (3)**
18. *Pact Patrol Kills* — kill 10 pact scouts. L6. Giver: Rook.
19. *Scout's Tokens* — collect 6 pact-worked charms. L7. Giver: Scout Iwen.
20. *The Engineer's Survey* — deliver survey bag to Engineer Tuck near Copperstep. L6. Giver: Rook.

Total: **20 side quests**. Combined with 10-step main chain and 2× 4-step
side chains (8 steps), that's 38 unique quests. Filler fills the gap to 50.

### 7.5 Filler bucket (~12)

Current filler bucket has 34 procedural quests. Keep the procedural generator
but cap at 12 for this zone — the handwritten content is the draw, filler is
there to round out the session-length target.

---

## 8. Scale plan

### 8.1 This pass — 600×600u

Zone origin stays where it is. Sub-zones placed at offsets up to ±300u from
zone_origin (table §4). Ground plane (2200u wide) still comfortably covers
the whole zone as long as zone_origin is within 800u of world origin.

NPC spawn code in `crates/vaern-server/src/npc/spawn.rs:100-140` computes
hub centers at `8 * (cos, sin)` of zone_origin. That table needs to read
from a new `hub.offset_from_zone_origin` field (or similar) so Harrier's
Rest actually lands at `(60, -100)` and not at `(8, 0)`. This is a small
schema + loader change — cheap to do.

### 8.2 Stretch — 1200×1200u (WoW Elwynn scale)

Requires two workspace changes:
- **Ground plane.** Grow from 2200u → 5000u (`crates/vaern-client/src/scene/ground.rs`).
- **Zone ring radius.** `data.zone_offsets` currently puts starter zones at
  800u radius (server data loader). For ~1200u-diameter zones, bump ring
  radius to 2500–3000u so zones don't overlap neighbors.

Stretch out of scope for this ticket — call it out as a follow-up so the
design is shippable at 600u without blocking on the plane/ring work.

---

## 9. Implementation plan

### 9.1 Files to write (by directory)

```
src/generated/world/zones/dalewatch_marches/
├── core.yaml                           (EDIT — bump hub_count 2→4)
├── hubs/
│   ├── dalewatch_keep.yaml             (EDIT — quest_givers 6, add offset field)
│   ├── miller_crossing.yaml            (EDIT — add offset field)
│   ├── harriers_rest.yaml              (NEW)
│   ├── kingsroad_waypost.yaml          (NEW)
│   └── ford_of_ashmere.yaml            (NEW)
├── mobs/
│   ├── _roster.yaml                    (EDIT — add 9 entries)
│   └── [9 new mob yamls]               (NEW)
├── quests/
│   ├── chains/
│   │   ├── chain_dalewatch_first_ride.yaml  (EDIT — expand to 10 steps)
│   │   ├── chain_dalewatch_grain_thief.yaml (NEW — 4 steps)
│   │   └── chain_dalewatch_grove_keeper.yaml (NEW — 4 steps)
│   ├── side/
│   │   └── [20 new side quests]        (NEW)
│   ├── filler.yaml                     (EDIT — trim to 12)
│   └── _summary.yaml                   (EDIT — new counts)
└── README.md                           (NEW — this design doc's pointer)
```

Total new files: ~35. Edits: ~5.

### 9.2 Schema additions

**Hub offset** — sub-zones need positions relative to zone origin:
```yaml
# hubs/harriers_rest.yaml
id: harriers_rest
zone: dalewatch_marches
name: Harrier's Rest
role: outpost
offset_from_zone_origin:
  x: 60.0
  z: -100.0
amenities: [innkeeper]
quest_givers: 2
```

Loader change (`crates/vaern-server/src/npc/spawn.rs`):
- Read `offset_from_zone_origin` from hub YAML.
- If present, use it; else fall back to the current radial layout.
- Backwards-compatible; other zones with no offset keep their current ring.

**Landmark entries** — sub-zones that are NOT hubs (Reed-Brake, Thornroot
Grove, etc.) need registered positions so `investigate` step `location:`
fields resolve. Proposal: new top-level `landmarks.yaml` in the zone dir:
```yaml
landmarks:
  - id: dalewatch_reed_brake
    name: The Reed-Brake
    offset_from_zone_origin: { x: -150.0, z: 200.0 }
  # …
```
Loader reads this in `data.rs` and exposes positions for the quest objective
resolver.

### 9.3 Generation approach

Per the `feedback_bulk_writes.md` memory — for ≥15 similar-shape files, I'll
write a one-shot Python seed script that materializes all the YAML from a
compact source table embedded in the script. Keeps the design and the output
in sync; rerunning regenerates the whole set.

Script path: `scripts/seed_dalewatch_redesign.py`. Operates as:
- Reads Python constants (hubs, mobs, NPCs, quest chains, side quests)
- Emits all YAMLs in idempotent form
- Refuses to overwrite unless `--force`
- Dry-run mode prints targets + sizes

### 9.4 Code changes (Rust)

- `crates/vaern-server/src/data.rs` — load `landmarks.yaml`; add
  `GameData::zone_landmarks()` lookup.
- `crates/vaern-server/src/npc/spawn.rs` — honor `offset_from_zone_origin`
  on hubs; place quest givers around their hub's absolute position.
- No combat/gameplay changes needed. No protocol changes.

### 9.5 Acceptance checks

After shipping:
- Server starts clean, logs `seeded N NPC spawn slots` where N ≈ current + ~20.
- All 4 hubs have the expected quest-giver count.
- Main chain has 10 steps, each with a valid `npc:` or `mob_id` reference.
- Filler count drops to 12.
- Quest objective target-hint strings resolve against the correct hubs / mobs.
- Console ping: no unresolved `archetype:` fallbacks on new NPC names (they
  get hashed archetypes per the recent quest-giver humanoid work).

---

## 10. Open design questions (call these out before YAML)

1. **Classic's "one boss one quest" vs Vaern's step-count.** WoW Elwynn had
   a sprawl of tiny 2-step quests. I've consolidated into a longer spine + 2
   side chains because the server's chain-renderer already works well with
   chains and I don't want to force 20 single-step quests. Redirect if you
   want more Elwynn-style one-offs instead.

2. **Pact scouts as humanoid appearance.** They should use a different
   humanoid archetype than the drifters (different faction). Proposal: hash
   them to `ranger_male` / `ranger_female`; drifters stay on `wizard_*`.
   Currently all humanoid NPCs without an explicit mesh map entry get a
   random archetype hashed from display name — works, but means a "Pact
   Scout" might render in wizard robes. If you want per-mob-ID archetype
   control, we add a `humanoid:` field to mob YAML.

3. **Sidlow Cairn undead tease.** I've specified no undead mobs this pass
   (the cairns are inert). The sub-zone is there as geography + one
   investigate-step quest. Want an undead mob now, or save it for a horror-
   themed zone later?

4. **The Drifter's Lair as pseudo-dungeon.** It's an open-world area with a
   named encounter, not an instance. Matches the current scaffold (no
   instancing yet). Confirm that's the target before I write it that way.

5. **Stretch dimensions now vs later.** 600u is shippable this pass and
   feels bigger than the current 90u. Going straight to 1200u means also
   growing the ground plane and zone ring in the same ticket. Pick.

---

**Next step:** approve or redline. When approved I'll write the Python seed
script, run it, and show you a diff of the generated YAMLs before anything
lands in `src/generated/world/`.
