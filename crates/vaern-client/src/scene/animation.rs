//! Animation driving for own + remote players.
//!
//! The [`AnimState`] → UAL clip mapping lives here, alongside:
//!
//! * [`CastFiredLocal`] — the Bevy-local re-broadcast of the server's
//!   `CastFired` lightyear message. Many modules (vfx, nameplates,
//!   diagnostic, this one) subscribe to it. A single [`relay_cast_fired`]
//!   system is the sole reader of the lightyear `MessageReceiver`,
//!   republishing so every consumer gets every event (lightyear's
//!   receiver drains on read).
//! * [`drive_own_transient_anim_from_cast_fired`] — stamp own-player
//!   Attacking / Hit flashes client-side, dodging lightyear 0.26's
//!   unreliable dynamic-component replication for the Predicted copy.
//! * [`override_own_player_anim_state`] — overlay Blocking / Casting /
//!   Dead states from `PlayerStateSnapshot`, since StatusEffects +
//!   Casting + Health don't reliably reach the own Predicted copy.
//! * [`drive_own_player_animation`] /
//!   [`drive_remote_player_animation`] — walk each Quaternius subtree
//!   and retarget `AnimationPlayer`s to the clip keyed by `AnimState`.

use std::collections::HashMap;
use std::time::Duration;

use bevy::animation::graph::AnimationNodeIndex;
use bevy::animation::transition::AnimationTransitions;
use bevy::animation::AnimationPlayer;
use bevy::prelude::*;
use lightyear::prelude::client::*;
use lightyear::prelude::*;
use vaern_assets::MeshtintAnimations;
use vaern_combat::{AnimOverride, AnimState};
use vaern_equipment::EquipSlot;
use vaern_items::ItemKind;
use vaern_protocol::CastFired;

use crate::inventory_ui::{ClientContent, OwnEquipped};
use crate::menu::AppState;
use crate::shared::{Player, RemotePlayer};
use crate::unit_frame::OwnPlayerState;

use super::render::{NpcHumanoidVisual, PlayerVisual, RemoteVisual};
use crate::shared::Npc;

// --- public message ---------------------------------------------------------

/// Client-side broadcast of a server `CastFired`. The lightyear
/// `MessageReceiver<CastFired>` drains on read, so a single relay
/// system is the sole reader and re-emits via this Bevy-local message
/// so multiple consumers (animation flash driver, vfx, nameplates,
/// diagnostic) can each iterate independently without racing over the
/// queue.
#[derive(Message, Clone, Debug)]
pub struct CastFiredLocal(pub CastFired);

// --- internal tuning --------------------------------------------------------

const ANIM_CROSSFADE: Duration = Duration::from_millis(150);

/// Per-`AnimationPlayer` marker: which clip node was most recently
/// started + whether it was a one-shot. The drive systems use it to
/// refuse mid-swing interruptions and to avoid retriggering a clip
/// that's already playing on the same player.
///
/// `speed_centi` is the playback-speed multiplier × 100, quantized so
/// equality compares cheaply. `100` is the natural rate; `-100` plays
/// the clip in reverse — used to fake a back-pedal from `Walk_Loop`
/// since the UAL ships no `Walk_Bwd_Loop`. Storing the speed lets the
/// drivers flip direction without retriggering (and crossfading) the
/// same clip every frame.
#[derive(Component, Debug, Clone, Copy)]
struct AnimSlot {
    node: AnimationNodeIndex,
    transient: bool,
    speed_centi: i16,
}

#[inline]
fn quantize_speed(speed: f32) -> i16 {
    (speed * 100.0).clamp(-30_000.0, 30_000.0).round() as i16
}

/// Body-relative motion classification. Drives whether locomotion clips
/// play forward (`Forward`) or in reverse (`Backward`) so a player
/// pressing S to back-pedal isn't shown jogging forward.
///
/// The UAL ships no strafe clips, so lateral motion is folded into
/// `Forward` — `Walk_Loop` is the least-bad fallback for sideways WASD
/// + camera-relative movement. When dedicated `Walk_Left/Right` clips
/// land, extend this enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MotionDir {
    Forward,
    Backward,
}

/// Project (curr − prev) onto the body-forward XZ vector. Returns
/// `Backward` only when motion clearly opposes facing
/// (dot ≤ −0.5); otherwise (forward, sideways, or stationary) returns
/// `Forward`. Sub-millimeter deltas — common between FixedUpdate ticks
/// when running this in `Update` — also fall through to `Forward` so
/// stopped-but-still-Walking holds don't flicker direction.
fn motion_dir_xz(prev: Vec3, curr: Vec3, rotation: Quat) -> MotionDir {
    let delta = curr - prev;
    let motion = Vec2::new(delta.x, delta.z);
    if motion.length_squared() < 1.0e-4 {
        return MotionDir::Forward;
    }
    let motion_dir = motion.normalize();
    let fwd = rotation * Vec3::NEG_Z;
    let fwd_xz = Vec2::new(fwd.x, fwd.z);
    if fwd_xz.length_squared() < 1.0e-4 {
        return MotionDir::Forward;
    }
    let dot = motion_dir.dot(fwd_xz.normalize());
    if dot <= -0.5 {
        MotionDir::Backward
    } else {
        MotionDir::Forward
    }
}

/// Resource-shaped motion-direction cache, one entry per animatable
/// entity. `Local<MotionTracker>` is per-system; we keep separate
/// trackers per driver so own / remote / NPC don't fight over a shared
/// HashMap.
#[derive(Default)]
struct MotionTracker {
    last_pos: HashMap<Entity, Vec3>,
}

impl MotionTracker {
    /// Sample `entity` at `pos`, returning the direction implied by the
    /// delta from the previous sample (or `Forward` on first sight).
    fn sample(&mut self, entity: Entity, pos: Vec3, rotation: Quat) -> MotionDir {
        let prev = self.last_pos.insert(entity, pos).unwrap_or(pos);
        motion_dir_xz(prev, pos, rotation)
    }

    /// Drop tracker entries for entities that didn't appear this tick,
    /// so despawned characters don't leak.
    fn retain<F: FnMut(Entity) -> bool>(&mut self, mut keep: F) {
        self.last_pos.retain(|e, _| keep(*e));
    }
}

/// Damage threshold above which the target plays the heavy
/// `Hit_Knockback` reaction instead of the default `Hit_Chest`. Picked
/// at the bottom of "this hurt" — most light-attack hits land in
/// 8-25 dmg, heavy hits + crits land in 35+. Tune as combat math
/// shifts.
const HIT_KNOCKBACK_DAMAGE: f32 = 35.0;

/// Per-entity attack/hit clip override populated from `CastFiredLocal`
/// before the animation drivers run. Lets the clip picker tell:
///
/// - a sword swing from a magic resolve (school-aware → physical
///   randomized among 4 sword combos, magic → `Spell_Simple_Shoot`),
/// - a glancing hit from a heavy hit (damage threshold →
///   `Hit_Chest` or `Hit_Knockback`).
///
/// Entries are sticky for the duration of an attack/hit transient
/// (one-shot clips hold via `AnimSlot.transient` + `is_finished`); a
/// fresh `CastFired` overwrites the entry, so rapid-fire attacks
/// naturally rotate variants. Despawned-entity entries leak harmlessly
/// — fine at pre-alpha scale; revisit if it ever shows up in a profile.
#[derive(Resource, Default)]
struct AnimContext {
    attack_clip: HashMap<Entity, &'static str>,
    hit_clip: HashMap<Entity, &'static str>,
}

/// Round-robin physical-attack clip picker. Counter increments per
/// physical CastFired so back-to-back light attacks don't show the
/// same swing — A → Regular_A → Regular_B → Regular_C → A → …
fn pick_physical_attack_clip(counter: u32) -> &'static str {
    match counter % 4 {
        0 => "UAL_Sword_Attack",
        1 => "UAL_Sword_Regular_A",
        2 => "UAL_Sword_Regular_B",
        _ => "UAL_Sword_Regular_C",
    }
}

/// Read `CastFiredLocal` events and populate [`AnimContext`] for every
/// driver downstream this frame.
///
/// Picks per event:
/// - **caster.attack_clip** — physical school → next entry in the
///   `Sword_Attack` / `Sword_Regular_A` / `_B` / `_C` rotation;
///   non-physical → `Spell_Simple_Shoot` so a magic resolve reads as
///   a cast finish, not a sword swing.
/// - **target.hit_clip** — `Hit_Knockback` for damage ≥
///   [`HIT_KNOCKBACK_DAMAGE`], else `Hit_Chest`. Self-hits are
///   skipped (DoT ticks, channeled self-AoEs).
///
/// Has its own `MessageReader<CastFiredLocal>` cursor — Bevy lets
/// every reader see every message, so this co-exists with the
/// existing own-player flash driver without draining its queue.
fn enrich_anim_context_from_cast_fired(
    mut ctx: ResMut<AnimContext>,
    mut reader: MessageReader<CastFiredLocal>,
    mut counter: Local<u32>,
) {
    for CastFiredLocal(cf) in reader.read() {
        let attack_clip = if is_physical_school(&cf.school) {
            *counter = counter.wrapping_add(1);
            pick_physical_attack_clip(*counter)
        } else {
            "UAL_Spell_Simple_Shoot"
        };
        ctx.attack_clip.insert(cf.caster, attack_clip);

        if cf.damage > 0.0 && cf.target != cf.caster {
            let hit_clip = if cf.damage >= HIT_KNOCKBACK_DAMAGE {
                "UAL_Hit_Knockback"
            } else {
                "UAL_Hit_Chest"
            };
            ctx.hit_clip.insert(cf.target, hit_clip);
        }
    }
}

// --- plugin -----------------------------------------------------------------

pub struct AnimationPlugin;

impl Plugin for AnimationPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<CastFiredLocal>()
            .init_resource::<AnimContext>()
            .add_systems(
                Update,
                (
                    // Relay must run before any CastFiredLocal reader so
                    // the Bevy message is populated this tick.
                    relay_cast_fired,
                    // Both consumers read CastFiredLocal independently
                    // (separate reader cursors). The enricher must land
                    // before any driver so attack/hit overrides are
                    // visible on the same tick the transient state
                    // flashes.
                    (
                        drive_own_transient_anim_from_cast_fired,
                        enrich_anim_context_from_cast_fired,
                        override_own_player_anim_state,
                        drive_own_player_animation,
                        drive_remote_player_animation,
                        drive_npc_humanoid_animation,
                    )
                        .chain()
                        .after(relay_cast_fired),
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

// --- CastFired relay --------------------------------------------------------

/// Sole reader of the lightyear `MessageReceiver<CastFired>`. Re-emits
/// every incoming cast as a Bevy-local [`CastFiredLocal`] message so
/// all downstream consumers can subscribe independently.
fn relay_cast_fired(
    mut receivers: Query<&mut MessageReceiver<CastFired>, With<Client>>,
    mut out: MessageWriter<CastFiredLocal>,
) {
    let Ok(mut rx) = receivers.single_mut() else {
        return;
    };
    for ev in rx.receive() {
        out.write(CastFiredLocal(ev));
    }
}

// --- own-player state overrides --------------------------------------------

/// Drive own-player transient flashes (Attacking / Hit) client-side
/// from `CastFired` messages instead of leaning on lightyear 0.26
/// replication of the dynamic `AnimOverride` component, which races
/// `derive_anim_state` on the Predicted copy of the own player.
///
/// When the message says *we* swung (`caster == own_player`), stamp
/// `AnimState::Attacking` + insert an `AnimOverride` locally. When the
/// message says *we were hit* (`target == own_player` with damage and
/// caster ≠ us), stamp `Hit`. The existing `Without<AnimOverride>`
/// filters on `derive_anim_state` and [`override_own_player_anim_state`]
/// then naturally hold the flash for its 250ms duration — same
/// mechanism the server uses.
fn drive_own_transient_anim_from_cast_fired(
    mut reader: MessageReader<CastFiredLocal>,
    mut player: Query<(Entity, &mut AnimState), With<Player>>,
    mut commands: Commands,
) {
    let Ok((player_entity, mut state)) = player.single_mut() else {
        // Can't resolve own player yet — drop the events.
        reader.read().for_each(|_| ());
        return;
    };

    for CastFiredLocal(ev) in reader.read() {
        let own_swung = ev.caster == player_entity;
        let own_hit = ev.target == player_entity && ev.caster != player_entity && ev.damage > 0.0;

        if own_swung {
            if *state != AnimState::Attacking {
                *state = AnimState::Attacking;
            }
            commands
                .entity(player_entity)
                .insert(AnimOverride { remaining_secs: 0.25 });
        } else if own_hit {
            if *state != AnimState::Hit {
                *state = AnimState::Hit;
            }
            commands
                .entity(player_entity)
                .insert(AnimOverride { remaining_secs: 0.25 });
        }
    }
}

/// Stamp the own player's `AnimState` from the authoritative
/// `PlayerStateSnapshot` flags before the animation driver reads it.
///
/// Why this exists: `vaern_combat::derive_anim_state` runs in
/// `FixedUpdate` on the client too, but for the own player it's only
/// partially correct. `StatusEffects` is not replicated, so the
/// `"blocking"` check always sees an empty list; `Casting` and `Health`
/// aren't reliably on the predicted copy in lightyear 0.26. Result:
/// speed-derived states (Idle / Walking / Running) work, but
/// Blocking / Casting / Dead never trigger.
///
/// The snapshot already carries `is_blocking / is_parrying /
/// is_casting / hp_current`, so we overlay those here. Priority mirrors
/// `derive_anim_state`: Dead → Blocking → Casting → (whatever speed
/// produced).
fn override_own_player_anim_state(
    state: Res<OwnPlayerState>,
    // `Without<AnimOverride>`: skip the override entirely while the
    // server is asserting a transient state (Attacking / Hit). The
    // snapshot's `is_casting` flag lingers for one tick after the
    // cast completes, which would otherwise clobber the Attacking
    // flash right after it lands.
    mut q: Query<&mut AnimState, (With<Player>, Without<AnimOverride>)>,
) {
    let Ok(mut anim) = q.single_mut() else {
        return;
    };
    let snap = &state.snap;
    let new = if snap.hp_max > 0.0 && snap.hp_current <= 0.0 {
        AnimState::Dead
    } else if snap.is_parrying || snap.is_blocking {
        AnimState::Blocking
    } else if snap.is_casting {
        AnimState::Casting
    } else {
        // Leave the speed-derived state (Idle / Walking / Running)
        // that `derive_anim_state` produced in FixedUpdate.
        return;
    };
    if *anim != new {
        *anim = new;
    }
}

// --- clip selection ---------------------------------------------------------

/// A school id is a *physical* (weapon) school if melee/bow — anything
/// that should animate like a bodily strike, not a spellcast. Keep in
/// lockstep with `vaern_server::class_kits::default_range_for_school`.
fn is_physical_school(school: &str) -> bool {
    matches!(
        school,
        "blade"
            | "blunt"
            | "shield"
            | "dagger"
            | "spear"
            | "claw"
            | "fang"
            | "unarmed"
            | "bow"
            | "crossbow"
    )
}

/// True when the currently-equipped mainhand is a physical weapon.
/// Used to pick weapon-armed idle / casting poses instead of unarmed
/// clips when the player is holding a sword / mace / etc.
fn mainhand_is_physical(equipped: &OwnEquipped, content: &ClientContent) -> bool {
    let Some(inst) = equipped.slots.get(&EquipSlot::MainHand) else {
        return false;
    };
    let Ok(resolved) = content.0.resolve(inst) else {
        return false;
    };
    match &resolved.kind {
        ItemKind::Weapon { school, .. } => is_physical_school(school),
        _ => false,
    }
}

/// Resolved clip pick: which UAL clip to play and at what speed.
///
/// `speed = 1.0` is the natural rate; `-1.0` plays the clip in reverse
/// to fake a back-pedal from forward locomotion (`Walk_Loop`,
/// `Jog_Fwd_Loop`) since the UAL has no `Walk_Bwd_Loop`.
#[derive(Clone, Copy, Debug)]
struct ClipChoice {
    key: &'static str,
    speed: f32,
}

/// Map the current `AnimState` (+ context) onto a UAL clip + playback
/// speed. All keys are prefixed `UAL_` by
/// [`vaern_assets::MeshtintAnimationCatalog::scan`].
///
/// `attack_override` / `hit_override` are populated by
/// [`enrich_anim_context_from_cast_fired`] from the same CastFired
/// stream the server publishes. They let the Attacking / Hit branches
/// pick variant clips (sword combo rotation, magic resolve, heavy
/// knockback) when the data is available, with stable defaults
/// otherwise so the driver still works during the brief tick window
/// when AnimState arrives before the matching CastFired (or for NPC
/// targets we never get a CastFired for).
///
/// Notes on the non-trivial branches:
/// - `Idle` is weapon-aware: holding a blade keeps `Sword_Idle`,
///   otherwise the unarmed `Idle_Loop`.
/// - `Casting` splits by school so a 0.4s melee windup (RMB heavy,
///   `school == "blade"`) holds a sword-ready pose instead of a
///   spellcaster's floaty loop.
/// - `Blocking` uses the generic `Idle_Shield_Loop` defensive stance
///   rather than the previous `Sword_Block` clip — that one only
///   reads correctly with a sword, so unarmed / staff / bow wielders
///   looked frozen mid-strike. The shield idle is weapon-agnostic and
///   plays the left-arm-up defensive pose without needing a shield
///   mesh attached.
/// - `Walking` / `Running` flip to reverse playback (`speed = -1.0`)
///   when the entity's motion opposes its facing direction. There are
///   no strafe clips in the UAL, so lateral motion still plays
///   `Walk_Loop` forward — accept the limitation until strafe clips
///   land.
fn anim_state_clip_choice(
    state: AnimState,
    cast_school: &str,
    armed: bool,
    motion: MotionDir,
    attack_override: Option<&'static str>,
    hit_override: Option<&'static str>,
) -> ClipChoice {
    let locomotion_speed = if matches!(motion, MotionDir::Backward) {
        -1.0
    } else {
        1.0
    };
    match state {
        AnimState::Idle => ClipChoice {
            key: if armed { "UAL_Sword_Idle" } else { "UAL_Idle_Loop" },
            speed: 1.0,
        },
        AnimState::Walking => ClipChoice {
            key: "UAL_Walk_Loop",
            speed: locomotion_speed,
        },
        AnimState::Running => ClipChoice {
            key: "UAL_Jog_Fwd_Loop",
            speed: locomotion_speed,
        },
        AnimState::Casting => ClipChoice {
            key: if is_physical_school(cast_school) {
                // Physical melee/ranged windup — hold a weapon-ready
                // pose. The actual strike plays on cast completion via
                // AnimState::Attacking.
                "UAL_Sword_Idle"
            } else {
                "UAL_Spell_Simple_Idle_Loop"
            },
            speed: 1.0,
        },
        AnimState::Blocking => ClipChoice {
            key: "UAL_Idle_Shield_Loop",
            speed: 1.0,
        },
        AnimState::Attacking => ClipChoice {
            key: attack_override.unwrap_or("UAL_Sword_Attack"),
            speed: 1.0,
        },
        AnimState::Hit => ClipChoice {
            key: hit_override.unwrap_or("UAL_Hit_Chest"),
            speed: 1.0,
        },
        AnimState::Dead => ClipChoice {
            key: "UAL_Death01",
            speed: 1.0,
        },
    }
}

/// `Attacking` and `Hit` are one-shot swings; every other state is a
/// sustained loop (idle / walk / run / cast-hold / block-hold / dead).
/// Transient clips play through even after the server-side AnimState
/// reverts — otherwise the 250ms override drops us back to Idle before
/// the swing has had time to read on-screen.
fn is_transient_anim(state: AnimState) -> bool {
    matches!(state, AnimState::Attacking | AnimState::Hit)
}

// --- animation players ------------------------------------------------------

/// Walk every `AnimationPlayer` under the own-player's Quaternius child
/// and drive it to the clip keyed by the current `AnimState`. Runs
/// each frame so (a) state transitions retarget and (b) freshly-spawned
/// children (after a gear-change respawn) pick up the current clip
/// without a separate init path. Mid-playing one-shots hold the slot
/// until their `ActiveAnimation::is_finished()`, at which point the
/// desired loop (usually Idle) crossfades in.
///
/// Motion direction is sampled from the predicted Transform so a
/// back-pedal (S key) plays `Walk_Loop` in reverse instead of jogging
/// forward. The `Local<MotionTracker>` keeps `last_pos` per entity so
/// despawn → respawn (gear change) doesn't carry a stale sample.
fn drive_own_player_animation(
    player_q: Query<(Entity, &AnimState, &Transform, &PlayerVisual), With<Player>>,
    children_q: Query<&Children>,
    anim_q: Query<(
        Entity,
        &mut AnimationPlayer,
        &mut AnimationTransitions,
        Option<&AnimSlot>,
    )>,
    anims: Res<MeshtintAnimations>,
    own_state: Res<OwnPlayerState>,
    equipped: Res<OwnEquipped>,
    content: Option<Res<ClientContent>>,
    ctx: Res<AnimContext>,
    mut tracker: Local<MotionTracker>,
    commands: Commands,
) {
    if !anims.is_ready() {
        return;
    }
    let Ok((entity, state, tf, visual)) = player_q.single() else {
        tracker.last_pos.clear();
        return;
    };
    let Some(root) = visual.child else {
        return;
    };
    let armed = content
        .as_deref()
        .map(|c| mainhand_is_physical(&equipped, c))
        .unwrap_or(false);
    let motion = tracker.sample(entity, tf.translation, tf.rotation);
    tracker.retain(|e| e == entity);
    let choice = anim_state_clip_choice(
        *state,
        &own_state.snap.cast_school,
        armed,
        motion,
        ctx.attack_clip.get(&entity).copied(),
        ctx.hit_clip.get(&entity).copied(),
    );
    let Some(desired_node) = anims.node_for(choice.key) else {
        return;
    };
    retarget_subtree(
        root,
        desired_node,
        is_transient_anim(*state),
        choice.speed,
        &children_q,
        anim_q,
        commands,
    );
}

/// Humanoid-NPC sibling of [`drive_remote_player_animation`]. Same
/// logic, different source component — reads `AnimState` + the
/// tracking [`NpcHumanoidVisual::child`] populated by
/// `render_replicated_npcs` when it spawns the Quaternius character
/// for a humanoid NPC. NPC beasts use the EverythingLibrary static
/// mesh path and have no `AnimationPlayer` to drive, so they're not
/// in this query.
fn drive_npc_humanoid_animation(
    npc_q: Query<(Entity, &AnimState, &Transform, &NpcHumanoidVisual), With<Npc>>,
    children_q: Query<&Children>,
    mut anim_q: Query<(
        Entity,
        &mut AnimationPlayer,
        &mut AnimationTransitions,
        Option<&AnimSlot>,
    )>,
    anims: Res<MeshtintAnimations>,
    ctx: Res<AnimContext>,
    mut tracker: Local<MotionTracker>,
    mut commands: Commands,
) {
    if !anims.is_ready() {
        return;
    }
    let mut alive: std::collections::HashSet<Entity> = std::collections::HashSet::new();
    for (entity, state, tf, visual) in &npc_q {
        alive.insert(entity);
        let motion = tracker.sample(entity, tf.translation, tf.rotation);
        let choice = anim_state_clip_choice(
            *state,
            "",
            false,
            motion,
            ctx.attack_clip.get(&entity).copied(),
            ctx.hit_clip.get(&entity).copied(),
        );
        let Some(desired_node) = anims.node_for(choice.key) else {
            continue;
        };
        let desired_transient = is_transient_anim(*state);
        retarget_subtree_in_place(
            visual.child,
            desired_node,
            desired_transient,
            choice.speed,
            &children_q,
            &mut anim_q,
            &mut commands,
        );
    }
    tracker.retain(|e| alive.contains(&e));
}

/// Weapon-unaware sibling of [`drive_own_player_animation`]. Walks
/// every remote player's Quaternius subtree and drives its
/// `AnimationPlayer`s from the replicated `AnimState`. Remote entities
/// don't carry a cast school resource on the client, so magic casts
/// fall back to physical idle poses; weapon-keyed clip selection stays
/// own-player-only for now.
fn drive_remote_player_animation(
    remote_q: Query<(Entity, &AnimState, &Transform, &RemoteVisual), With<RemotePlayer>>,
    children_q: Query<&Children>,
    mut anim_q: Query<(
        Entity,
        &mut AnimationPlayer,
        &mut AnimationTransitions,
        Option<&AnimSlot>,
    )>,
    anims: Res<MeshtintAnimations>,
    ctx: Res<AnimContext>,
    mut tracker: Local<MotionTracker>,
    mut commands: Commands,
) {
    if !anims.is_ready() {
        return;
    }
    let mut alive: std::collections::HashSet<Entity> = std::collections::HashSet::new();
    for (entity, state, tf, visual) in &remote_q {
        alive.insert(entity);
        let Some(root) = visual.child else { continue };
        let motion = tracker.sample(entity, tf.translation, tf.rotation);
        let choice = anim_state_clip_choice(
            *state,
            "",
            false,
            motion,
            ctx.attack_clip.get(&entity).copied(),
            ctx.hit_clip.get(&entity).copied(),
        );
        let Some(desired_node) = anims.node_for(choice.key) else {
            continue;
        };
        let desired_transient = is_transient_anim(*state);
        retarget_subtree_in_place(
            root,
            desired_node,
            desired_transient,
            choice.speed,
            &children_q,
            &mut anim_q,
            &mut commands,
        );
    }
    tracker.retain(|e| alive.contains(&e));
}

/// Walk a scene subtree, retargeting every `AnimationPlayer` to
/// `desired_node` at `desired_speed` (with in-place mutable borrows —
/// used by the remote-player driver which iterates over multiple
/// roots).
///
/// Three transition cases:
/// - **No slot / different node** — `transitions.play` crossfades to
///   the new clip and stamps the speed. Transient clips don't repeat.
/// - **Same node, mid-transient** — refuse to interrupt until
///   `is_finished()`, then crossfade to the desired loop.
/// - **Same node, different speed** — adjust the active animation's
///   speed in-place. No crossfade — flipping forward → reversed
///   playback is a direction change on the same clip, so the body
///   continues from its current frame and just unwinds. Cheap +
///   visually clean.
fn retarget_subtree_in_place(
    root: Entity,
    desired_node: AnimationNodeIndex,
    desired_transient: bool,
    desired_speed: f32,
    children_q: &Query<&Children>,
    anim_q: &mut Query<(
        Entity,
        &mut AnimationPlayer,
        &mut AnimationTransitions,
        Option<&AnimSlot>,
    )>,
    commands: &mut Commands,
) {
    let desired_centi = quantize_speed(desired_speed);
    let mut stack = vec![root];
    while let Some(e) = stack.pop() {
        if let Ok((ent, mut player, mut transitions, slot)) = anim_q.get_mut(e) {
            let action = match slot {
                None => SlotAction::Play,
                Some(s) if s.node == desired_node => {
                    if s.speed_centi != desired_centi {
                        SlotAction::AdjustSpeed
                    } else {
                        SlotAction::Hold
                    }
                }
                Some(s) if s.transient => {
                    if player
                        .animation(s.node)
                        .map(|a| a.is_finished())
                        .unwrap_or(true)
                    {
                        SlotAction::Play
                    } else {
                        SlotAction::Hold
                    }
                }
                Some(_) => SlotAction::Play,
            };
            match action {
                SlotAction::Play => {
                    let anim = transitions.play(&mut player, desired_node, ANIM_CROSSFADE);
                    anim.set_speed(desired_speed);
                    if !desired_transient {
                        anim.repeat();
                    }
                    commands.entity(ent).insert(AnimSlot {
                        node: desired_node,
                        transient: desired_transient,
                        speed_centi: desired_centi,
                    });
                }
                SlotAction::AdjustSpeed => {
                    if let Some(active) = player.animation_mut(desired_node) {
                        active.set_speed(desired_speed);
                    }
                    commands.entity(ent).insert(AnimSlot {
                        node: desired_node,
                        transient: desired_transient,
                        speed_centi: desired_centi,
                    });
                }
                SlotAction::Hold => {}
            }
        }
        if let Ok(kids) = children_q.get(e) {
            for &c in kids {
                stack.push(c);
            }
        }
    }
}

#[derive(Clone, Copy)]
enum SlotAction {
    /// Crossfade-play the desired clip from the start.
    Play,
    /// Same clip already active — just nudge `set_speed` so a
    /// forward → reversed flip (back-pedal) doesn't restart the cycle.
    AdjustSpeed,
    /// Either the active clip already matches, or a transient one-shot
    /// hasn't finished yet. Leave the animation player alone.
    Hold,
}

/// Convenience wrapper for the own-player single-root case — takes
/// ownership of the query + commands.
fn retarget_subtree(
    root: Entity,
    desired_node: AnimationNodeIndex,
    desired_transient: bool,
    desired_speed: f32,
    children_q: &Query<&Children>,
    mut anim_q: Query<(
        Entity,
        &mut AnimationPlayer,
        &mut AnimationTransitions,
        Option<&AnimSlot>,
    )>,
    mut commands: Commands,
) {
    retarget_subtree_in_place(
        root,
        desired_node,
        desired_transient,
        desired_speed,
        children_q,
        &mut anim_q,
        &mut commands,
    );
}
