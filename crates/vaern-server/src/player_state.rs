//! Stream the owning player's combat + progression state over a server→client
//! message. Bypasses lightyear replication for own-player data because in this
//! setup the owning client only receives a Predicted copy of its player —
//! combat state seeded at spawn, never updated via replication alone.
//!
//! Fires whenever `Health`, `ResourcePool`, or `Experience` changes on a
//! player entity. Change detection does the throttling for us; idle ticks
//! send nothing.

use bevy::prelude::*;
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use vaern_character::{Experience, XpCurve};
use vaern_combat::{AbilitySpec, Caster, Casting, Health, ResourcePool, Stamina, StatusEffects};
use vaern_protocol::{Channel1, PlayerStateSnapshot, PlayerTag};
use vaern_stats::{PillarCaps, PillarScores, PillarXp, xp_to_next_point};

/// Ship a `PlayerStateSnapshot` to each player every tick. Not gated on
/// change detection because:
///   - cast bars need live progress every frame while `Casting` exists
///   - the "cast just ended" frame must still emit a snapshot with
///     `is_casting = false` — change-detection misses `RemovedComponents`
///   - at single-digit player counts the bandwidth is trivial
pub fn broadcast_player_state(
    curve: Res<XpCurve>,
    players: Query<
        (
            Entity,
            &ControlledBy,
            &Health,
            &ResourcePool,
            Option<&Stamina>,
            &Experience,
            &PillarScores,
            &PillarCaps,
            &PillarXp,
            Option<&Casting>,
            Option<&StatusEffects>,
        ),
        With<PlayerTag>,
    >,
    ability_specs: Query<&AbilitySpec>,
    ability_casters: Query<&Caster>,
    mut senders: Query<&mut MessageSender<PlayerStateSnapshot>, With<ClientOf>>,
) {
    for (player_e, cb, hp, pool, stamina, xp, pillars, caps, pillar_xp, casting, effects) in
        &players
    {
        let Ok(mut sender) = senders.get_mut(cb.owner) else { continue };

        // Resolve ability display info from the `Casting` snapshot. `school`
        // is already on Casting; `ability name` needs an extra hop through
        // the ability entity's spec.
        let (is_casting, cast_total, cast_remaining, cast_school, cast_name) = match casting {
            Some(c) => {
                // Verify the ability still belongs to this caster (defensive;
                // `cleanup_orphan_abilities` should keep this consistent).
                let _ = ability_casters;
                let name = ability_specs
                    .get(c.ability)
                    .map(|s| s.school.clone()) // fallback: use school as name
                    .unwrap_or_else(|_| String::new());
                let _ = (player_e, name);
                (
                    true,
                    c.total_secs,
                    c.remaining_secs.max(0.0),
                    c.school.clone(),
                    // We don't carry the pretty display name onto Casting
                    // right now. School is enough to color the bar; the
                    // hotbar already shows the name under the pressed slot.
                    c.school.clone(),
                )
            }
            None => (false, 0.0, 0.0, String::new(), String::new()),
        };

        let snap = PlayerStateSnapshot {
            hp_current: hp.current,
            hp_max: hp.max,
            pool_current: pool.current,
            pool_max: pool.max,
            xp_current: xp.current,
            xp_level: xp.level,
            xp_to_next: curve.to_next(xp.level),
            is_casting,
            cast_total,
            cast_remaining,
            cast_school,
            cast_ability_name: cast_name,
            might: pillars.might,
            finesse: pillars.finesse,
            arcana: pillars.arcana,
            might_cap: caps.might,
            finesse_cap: caps.finesse,
            arcana_cap: caps.arcana,
            might_xp: pillar_xp.might,
            finesse_xp: pillar_xp.finesse,
            arcana_xp: pillar_xp.arcana,
            might_xp_to_next: xp_to_next_point(pillars.might),
            finesse_xp_to_next: xp_to_next_point(pillars.finesse),
            arcana_xp_to_next: xp_to_next_point(pillars.arcana),
            stamina_current: stamina.map(|s| s.current).unwrap_or(0.0),
            stamina_max: stamina.map(|s| s.max).unwrap_or(0.0),
            is_blocking: effects.is_some_and(|e| e.has("blocking")),
            is_parrying: effects.is_some_and(|e| e.has("parrying")),
        };
        let _ = sender.send::<Channel1>(snap);
    }
}
