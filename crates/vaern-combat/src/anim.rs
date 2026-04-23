//! High-level animation state tracking.
//!
//! Every combat-capable entity (players, NPCs) carries an `AnimState`
//! that's derived each tick from its low-level state: Transform deltas
//! feed speed classification (Idle / Walking / Running), `Casting`
//! presence promotes to Casting, an active Block `StatusEffect`
//! promotes to Blocking, and `Health::is_dead` wins over everything.
//!
//! The component is replicated so clients can drive per-entity
//! animation state machines off a single authoritative value. Until
//! real character skeletons land, the client-side UI just tags the
//! nameplate with the current state.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::components::{Casting, Health};
use crate::effects::StatusEffects;

/// Coarse motion / combat state. Kept intentionally small so every
/// animation rig can map it to a concrete clip trivially; richer
/// states (e.g. "windup", "recovery") will layer on top via a
/// per-ability system when skeletons land.
///
/// `Attacking` and `Hit` are *transient* — they're written once by
/// `mark_attack_and_hit` on a `CastEvent` and paired with an
/// `AnimOverride` so `derive_anim_state` won't clobber them for the
/// flash duration. Long-lived states (Casting, Blocking) continue to
/// be derived every tick.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnimState {
    Idle,
    Walking,
    Running,
    Casting,
    Blocking,
    Attacking,
    Hit,
    Dead,
}

impl Default for AnimState {
    fn default() -> Self {
        Self::Idle
    }
}

impl AnimState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Walking => "walking",
            Self::Running => "running",
            Self::Casting => "casting",
            Self::Blocking => "blocking",
            Self::Attacking => "attacking",
            Self::Hit => "hit",
            Self::Dead => "dead",
        }
    }
}

/// Timer component that freezes `AnimState` for its duration.
/// Attached alongside a transient state (Attacking / Hit) so
/// `derive_anim_state` can skip the entity for a frame or two,
/// giving the flash time to be visible.
///
/// Replicated (with prediction) alongside `AnimState` — without it, the
/// client-side `derive_anim_state` would immediately clobber an
/// incoming `Attacking`/`Hit` replication back to Idle because its
/// `Without<AnimOverride>` filter would always match.
#[derive(Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AnimOverride {
    pub remaining_secs: f32,
}

/// Flash duration for Attacking + Hit states. Short enough to not
/// hide real state transitions (e.g. dying right after being hit),
/// long enough to read at 60fps.
pub const ANIM_FLASH_SECS: f32 = 0.25;

/// Decrement every `AnimOverride`; remove the component when it
/// expires so `derive_anim_state` takes over on the next frame.
pub fn tick_anim_override(
    time: Res<Time>,
    mut q: Query<(Entity, &mut AnimOverride)>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();
    for (entity, mut ov) in &mut q {
        ov.remaining_secs -= dt;
        if ov.remaining_secs <= 0.0 {
            commands.entity(entity).remove::<AnimOverride>();
        }
    }
}

/// Read `CastEvent`s: flash the caster to `Attacking`, flash the
/// target to `Hit` (only when damage actually lands — parries /
/// whiffs don't make the target flinch). Both paths attach an
/// `AnimOverride` to freeze the flash for its duration.
pub fn mark_attack_and_hit(
    mut events: MessageReader<crate::systems::CastEvent>,
    mut states: Query<&mut AnimState>,
    mut commands: Commands,
) {
    for ev in events.read() {
        if let Ok(mut state) = states.get_mut(ev.caster) {
            *state = AnimState::Attacking;
            commands
                .entity(ev.caster)
                .try_insert(AnimOverride { remaining_secs: ANIM_FLASH_SECS });
        }
        if ev.damage > 0.0 && ev.target != ev.caster {
            if let Ok(mut state) = states.get_mut(ev.target) {
                *state = AnimState::Hit;
                commands
                    .entity(ev.target)
                    .try_insert(AnimOverride { remaining_secs: ANIM_FLASH_SECS });
            }
        }
    }
}

/// Speed threshold above which we classify motion as "walking" (world
/// units / second). Anything below is Idle — catches numerical jitter
/// on stationary entities.
const WALK_THRESHOLD: f32 = 0.5;

/// Speed threshold above which we classify motion as "running". NPC
/// roam = 2.2 u/s lands in Walking; chase = 4.5 u/s and player = 6.0
/// u/s land in Running.
const RUN_THRESHOLD: f32 = 3.0;

/// Recompute `AnimState` for every animatable entity. Runs in
/// `FixedUpdate` so Transform deltas read consistently against the
/// 60Hz movement cadence — running in `Update` (variable dt) makes
/// speed jitter badly between ticks where movement happened and
/// ticks where it didn't.
pub fn derive_anim_state(
    time: Res<Time>,
    mut q: Query<
        (
            Entity,
            &Transform,
            &mut AnimState,
            Option<&Casting>,
            Option<&Health>,
            Option<&StatusEffects>,
        ),
        Without<AnimOverride>,
    >,
    mut last_pos: Local<HashMap<Entity, Vec3>>,
) {
    let dt = time.delta_secs().max(1e-4);

    // Drop stale entries so despawned entities don't leak. Only keep
    // keys present in this tick's query.
    let mut keep: std::collections::HashSet<Entity> = std::collections::HashSet::new();

    for (entity, tf, mut state, casting, health, effects) in &mut q {
        keep.insert(entity);

        let prev = last_pos.insert(entity, tf.translation).unwrap_or(tf.translation);
        let delta = tf.translation - prev;
        // Project to XZ — vertical motion (bounces, ground snap) shouldn't
        // bleed into motion-state classification.
        let speed = Vec2::new(delta.x, delta.z).length() / dt;

        let new_state = if health.map_or(false, |h| h.is_dead()) {
            AnimState::Dead
        } else if effects.map_or(false, |e| e.has("blocking")) {
            AnimState::Blocking
        } else if casting.is_some() {
            AnimState::Casting
        } else if speed >= RUN_THRESHOLD {
            AnimState::Running
        } else if speed >= WALK_THRESHOLD {
            AnimState::Walking
        } else {
            AnimState::Idle
        };

        if *state != new_state {
            *state = new_state;
        }
    }

    last_pos.retain(|e, _| keep.contains(e));
}
