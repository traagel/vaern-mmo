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
#[derive(Component, Debug, Clone, Copy)]
struct AnimSlot {
    node: AnimationNodeIndex,
    transient: bool,
}

// --- plugin -----------------------------------------------------------------

pub struct AnimationPlugin;

impl Plugin for AnimationPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<CastFiredLocal>().add_systems(
            Update,
            (
                drive_remote_player_animation,
                drive_npc_humanoid_animation,
                // Relay must run before any CastFiredLocal reader so
                // the Bevy message is populated this tick.
                relay_cast_fired,
                (
                    drive_own_transient_anim_from_cast_fired,
                    override_own_player_anim_state,
                    drive_own_player_animation,
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

/// Map the current `AnimState` onto a UAL clip key registered in the
/// shared `MeshtintAnimations` graph. All keys are prefixed `UAL_` by
/// [`vaern_assets::MeshtintAnimationCatalog::scan`].
///
/// Casting is split by school so a 0.4s melee windup (RMB heavy attack,
/// school = "blade") holds a sword-ready pose instead of playing a
/// spellcaster's floaty loop. Idle is weapon-aware too — when holding
/// a blade the base standing pose is `Sword_Idle` rather than the
/// unarmed `Idle_Loop`.
fn anim_state_clip_key(state: AnimState, cast_school: &str, armed: bool) -> &'static str {
    match state {
        AnimState::Idle => {
            if armed {
                "UAL_Sword_Idle"
            } else {
                "UAL_Idle_Loop"
            }
        }
        AnimState::Walking => "UAL_Walk_Loop",
        AnimState::Running => "UAL_Jog_Fwd_Loop",
        AnimState::Casting => {
            if is_physical_school(cast_school) {
                // Physical melee/ranged windup — hold a weapon-ready
                // pose. The actual strike plays on cast completion via
                // AnimState::Attacking.
                "UAL_Sword_Idle"
            } else {
                "UAL_Spell_Simple_Idle_Loop"
            }
        }
        AnimState::Blocking => "UAL_Sword_Block",
        AnimState::Attacking => "UAL_Sword_Attack",
        AnimState::Hit => "UAL_Hit_Chest",
        AnimState::Dead => "UAL_Death01",
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
fn drive_own_player_animation(
    player_q: Query<(&AnimState, &PlayerVisual), With<Player>>,
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
    commands: Commands,
) {
    if !anims.is_ready() {
        return;
    }
    let Ok((state, visual)) = player_q.single() else {
        return;
    };
    let Some(root) = visual.child else {
        return;
    };
    let armed = content
        .as_deref()
        .map(|c| mainhand_is_physical(&equipped, c))
        .unwrap_or(false);
    let desired_key = anim_state_clip_key(*state, &own_state.snap.cast_school, armed);
    let Some(desired_node) = anims.node_for(desired_key) else {
        return;
    };
    retarget_subtree(
        root,
        desired_node,
        is_transient_anim(*state),
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
    npc_q: Query<(&AnimState, &NpcHumanoidVisual), With<Npc>>,
    children_q: Query<&Children>,
    mut anim_q: Query<(
        Entity,
        &mut AnimationPlayer,
        &mut AnimationTransitions,
        Option<&AnimSlot>,
    )>,
    anims: Res<MeshtintAnimations>,
    mut commands: Commands,
) {
    if !anims.is_ready() {
        return;
    }
    for (state, visual) in &npc_q {
        let desired_key = anim_state_clip_key(*state, "", false);
        let Some(desired_node) = anims.node_for(desired_key) else {
            continue;
        };
        let desired_transient = is_transient_anim(*state);
        retarget_subtree_in_place(
            visual.child,
            desired_node,
            desired_transient,
            &children_q,
            &mut anim_q,
            &mut commands,
        );
    }
}

/// Weapon-unaware sibling of [`drive_own_player_animation`]. Walks
/// every remote player's Quaternius subtree and drives its
/// `AnimationPlayer`s from the replicated `AnimState`. Remote entities
/// don't carry a cast school resource on the client, so magic casts
/// fall back to physical idle poses; weapon-keyed clip selection stays
/// own-player-only for now.
fn drive_remote_player_animation(
    remote_q: Query<(&AnimState, &RemoteVisual), With<RemotePlayer>>,
    children_q: Query<&Children>,
    mut anim_q: Query<(
        Entity,
        &mut AnimationPlayer,
        &mut AnimationTransitions,
        Option<&AnimSlot>,
    )>,
    anims: Res<MeshtintAnimations>,
    mut commands: Commands,
) {
    if !anims.is_ready() {
        return;
    }
    for (state, visual) in &remote_q {
        let Some(root) = visual.child else { continue };
        let desired_key = anim_state_clip_key(*state, "", false);
        let Some(desired_node) = anims.node_for(desired_key) else {
            continue;
        };
        let desired_transient = is_transient_anim(*state);
        retarget_subtree_in_place(
            root,
            desired_node,
            desired_transient,
            &children_q,
            &mut anim_q,
            &mut commands,
        );
    }
}

/// Walk a scene subtree, retargeting every `AnimationPlayer` to
/// `desired_node` (with in-place mutable borrows — used by the
/// remote-player driver which iterates over multiple roots).
fn retarget_subtree_in_place(
    root: Entity,
    desired_node: AnimationNodeIndex,
    desired_transient: bool,
    children_q: &Query<&Children>,
    anim_q: &mut Query<(
        Entity,
        &mut AnimationPlayer,
        &mut AnimationTransitions,
        Option<&AnimSlot>,
    )>,
    commands: &mut Commands,
) {
    let mut stack = vec![root];
    while let Some(e) = stack.pop() {
        if let Ok((ent, mut player, mut transitions, slot)) = anim_q.get_mut(e) {
            let switch = match slot {
                None => true,
                Some(s) if s.node == desired_node => false,
                Some(s) if s.transient => player
                    .animation(s.node)
                    .map(|a| a.is_finished())
                    .unwrap_or(true),
                Some(_) => true,
            };
            if switch {
                let anim = transitions.play(&mut player, desired_node, ANIM_CROSSFADE);
                if !desired_transient {
                    anim.repeat();
                }
                commands.entity(ent).insert(AnimSlot {
                    node: desired_node,
                    transient: desired_transient,
                });
            }
        }
        if let Ok(kids) = children_q.get(e) {
            for &c in kids {
                stack.push(c);
            }
        }
    }
}

/// Convenience wrapper for the own-player single-root case — takes
/// ownership of the query + commands.
fn retarget_subtree(
    root: Entity,
    desired_node: AnimationNodeIndex,
    desired_transient: bool,
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
        children_q,
        &mut anim_q,
        &mut commands,
    );
}
