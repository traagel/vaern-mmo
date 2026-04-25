//! Corpse-run death penalty.
//!
//! On player death:
//!   1. Spawn a server-side `Corpse` entity at the death position.
//!   2. Set HP to `RESPAWN_HP_FRACTION` (25%) of max, not full.
//!   3. Teleport to `Respawnable.home`.
//!   4. Clear stale Casting / Target.
//!
//! Recovery:
//!   * Walk within `CORPSE_RECOVERY_RADIUS` of your corpse → full HP +
//!     corpse despawns.
//!   * Corpse expires after `CORPSE_LIFETIME_SECS` — at that point the
//!     player keeps the partial HP and must regen / drink potions.
//!
//! Visual marker on the client is a follow-up; the mechanic still
//! creates the felt walkback drama because players remember where they
//! died.
//!
//! Players carry `CorpseOnDeath` from spawn, which makes
//! `vaern_combat::apply_deaths` skip them so this module is the sole
//! handler.

use bevy::prelude::*;

use vaern_combat::{Casting, DeathEvent, Health, Respawnable, Target};
use vaern_protocol::PlayerTag;

/// HP after respawn, as a fraction of the player's max HP.
pub const RESPAWN_HP_FRACTION: f32 = 0.25;
/// Distance (XZ plane, meters) at which a player recovers their corpse.
pub const CORPSE_RECOVERY_RADIUS: f32 = 3.0;
/// Server-side corpse lifetime. After this many seconds, the corpse
/// despawns and the player permanently keeps their post-death HP.
pub const CORPSE_LIFETIME_SECS: f32 = 600.0; // 10 minutes

/// Server-only entity placed at a player's death site. Owner gets HP
/// restoration on proximity; expires after `CORPSE_LIFETIME_SECS`.
#[derive(Component, Debug, Clone, Copy)]
pub struct Corpse {
    pub owner: Entity,
    pub position: Vec3,
    /// Seconds remaining until despawn.
    pub remaining_secs: f32,
}

/// Reads `DeathEvent` for player entities and runs the corpse-run flow:
/// captures death position, spawns Corpse marker, applies HP penalty,
/// teleports to `Respawnable.home`, clears stale Target / Casting.
///
/// Players carry `CorpseOnDeath` (added at spawn) so the shared
/// `apply_deaths` skips them — this is the sole handler for player
/// death.
pub fn apply_player_corpse_run(
    mut events: MessageReader<DeathEvent>,
    mut players: Query<
        (Entity, &mut Health, &mut Transform, &Respawnable),
        With<PlayerTag>,
    >,
    existing_corpses: Query<&Corpse>,
    mut commands: Commands,
) {
    for ev in events.read() {
        let Ok((entity, mut hp, mut tf, respawn)) = players.get_mut(ev.entity) else {
            continue;
        };
        // Avoid double-spawn if somehow the player's still-active corpse
        // didn't despawn from the previous death.
        if existing_corpses.iter().any(|c| c.owner == entity) {
            continue;
        }

        let death_pos = tf.translation;
        commands.spawn(Corpse {
            owner: entity,
            position: death_pos,
            remaining_secs: CORPSE_LIFETIME_SECS,
        });

        hp.current = (hp.max * RESPAWN_HP_FRACTION).max(1.0);
        tf.translation = respawn.home;
        if let Ok(mut ec) = commands.get_entity(entity) {
            ec.remove::<Target>();
            ec.remove::<Casting>();
        }

        info!(
            "[respawn] {entity:?} died at ({:.1}, {:.1}, {:.1}); respawning at ({:.1}, {:.1}, {:.1}) with {}/{:.0} HP",
            death_pos.x,
            death_pos.y,
            death_pos.z,
            respawn.home.x,
            respawn.home.y,
            respawn.home.z,
            hp.current as i32,
            hp.max,
        );
    }
}

/// Each tick: corpse lifetimes tick down. Owners within
/// `CORPSE_RECOVERY_RADIUS` of their corpse get HP restored to max + the
/// corpse despawns. Expired corpses are cleaned up. Owners that have
/// despawned (logout) drop their corpses too.
pub fn tick_corpses(
    time: Res<Time>,
    mut corpses: Query<(Entity, &mut Corpse)>,
    mut players: Query<(Entity, &mut Health, &Transform), With<PlayerTag>>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();
    let mut to_recover: Vec<Entity> = Vec::new();
    let mut to_despawn: Vec<Entity> = Vec::new();

    for (corpse_e, mut corpse) in &mut corpses {
        corpse.remaining_secs -= dt;
        if corpse.remaining_secs <= 0.0 {
            to_despawn.push(corpse_e);
            info!("[respawn] corpse {corpse_e:?} expired");
            continue;
        }
        let Ok((_, _, owner_tf)) = players.get(corpse.owner) else {
            // Owner despawned (logout). Drop the corpse — they get a
            // fresh start on next login.
            to_despawn.push(corpse_e);
            continue;
        };
        let dxz = (owner_tf.translation - corpse.position).truncate().length();
        if dxz <= CORPSE_RECOVERY_RADIUS {
            to_recover.push(corpse.owner);
            to_despawn.push(corpse_e);
            info!(
                "[respawn] {:?} recovered corpse at ({:.1}, {:.1}, {:.1}) — full HP restored",
                corpse.owner, corpse.position.x, corpse.position.y, corpse.position.z
            );
        }
    }

    for owner in to_recover {
        if let Ok((_, mut hp, _)) = players.get_mut(owner) {
            hp.current = hp.max;
        }
    }
    for corpse_e in to_despawn {
        if let Ok(mut ec) = commands.get_entity(corpse_e) {
            ec.despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn respawn_hp_fraction_is_below_full() {
        assert!(RESPAWN_HP_FRACTION > 0.0);
        assert!(RESPAWN_HP_FRACTION < 1.0);
    }

    #[test]
    fn corpse_recovery_radius_is_practical() {
        assert!(CORPSE_RECOVERY_RADIUS >= 1.0);
        assert!(CORPSE_RECOVERY_RADIUS <= 10.0);
    }

    #[test]
    fn corpse_lifetime_matches_design() {
        // Plan calls for 10-minute walkback window.
        assert!((CORPSE_LIFETIME_SECS - 600.0).abs() < 1.0);
    }
}
