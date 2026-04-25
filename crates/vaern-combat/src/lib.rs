use std::collections::HashMap;

use bevy::prelude::*;
use vaern_core::School;

pub mod anim;
pub mod components;
pub mod effects;
pub mod systems;

pub use anim::{AnimOverride, AnimState};
pub use components::{
    AbilityCooldown, AbilityPriority, AbilityShape, AbilitySpec, CastRequest, Caster, Casting,
    CorpseOnDeath, DisplayName, FactionTag, Health, ManualCast, NpcKind, Position3Pillar,
    ProjectileVisual, QuestGiverHub, Respawnable, ResourcePool, Stamina, Target,
};
pub use effects::{EffectKind, StanceKind, StatusEffect, StatusEffects};
pub use systems::{CastEvent, DeathEvent, Projectile};

pub mod damage;
pub use damage::{DamageResult, apply_stances, compute_damage};

/// Resource holding the loaded school definitions, keyed by school id.
/// Populated by bootstrap code (sim/server), read by systems that need to
/// resolve damage type, morality, or faction-gating for an ability.
#[derive(Resource, Debug, Default)]
pub struct Schools(pub HashMap<String, School>);

impl Schools {
    pub fn get(&self, id: &str) -> Option<&School> {
        self.0.get(id)
    }
}

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<DeathEvent>()
            .add_message::<CastEvent>()
            .add_systems(
                Update,
                (
                    systems::regen_resources,
                    effects::regen_stamina,
                    systems::tick_cooldowns,
                    anim::tick_anim_override,
                    // Status effects tick BEFORE cast resolution so DoT
                    // damage from a previous frame lands before the
                    // current frame's casts race it.
                    effects::tick_status_effects,
                    systems::progress_casts,
                    systems::select_and_fire,
                    // mark_attack_and_hit reads CastEvents written by
                    // progress_casts + select_and_fire THIS tick, so it
                    // must run after both to capture the flash on the
                    // same frame the damage resolves.
                    anim::mark_attack_and_hit,
                    // `detect_deaths` fires `DeathEvent` for UI / VFX
                    // consumers. It's safe to run on both server and
                    // client — it's read-only, no despawn side effect.
                    // `apply_deaths` is NOT included here because it
                    // despawns the entity (for non-Respawnable) and
                    // teleports/resets Respawnable ones; both are
                    // authoritative actions. Client-side that would
                    // despawn a replicated NPC early and leave in-
                    // flight server updates dangling at the "entity
                    // does not exist" logs. Server registers
                    // `apply_deaths` separately in its own schedule;
                    // clients just wait for lightyear to propagate
                    // the real despawn.
                    systems::detect_deaths,
                    systems::cleanup_orphan_abilities,
                )
                    .chain(),
            )
            // Projectiles move tick-scaled in FixedUpdate so their travel
            // feels deterministic regardless of frame rate.
            //
            // Animation state derivation also lives here — reading
            // Transform deltas against the 60Hz movement cadence
            // produces stable speed classification. Runs after
            // projectile ticks so that chain doesn't matter (anim
            // derivation only reads state).
            .add_systems(
                FixedUpdate,
                (systems::tick_projectiles, anim::derive_anim_state),
            );
    }
}
