//! NPC behavior loops: threat accounting, target selection, chase, leash,
//! and idle roam. No entity spawning happens here — all systems operate on
//! already-spawned NPCs from `super::spawn`.

use std::collections::HashSet;

use bevy::prelude::*;
use vaern_combat::{CastEvent, Health, StatusEffects, Target};
use vaern_core::terrain;
use vaern_protocol::PlayerTag;

use super::components::{AggroRange, LeashRange, Npc, NpcHome, NonCombat, RoamState, ThreatTable};
use super::{NPC_MELEE_RANGE, NPC_MOVE_SPEED, NPC_ROAM_RADIUS, NPC_ROAM_SPEED};

/// Snap every NPC's Y onto the shared terrain height field. Runs
/// after the chase/roam/leash systems each fixed tick so any X/Z
/// movement that frame is immediately followed by a Y correction.
/// Cheap — two sin + one cos per NPC per tick — and keeps the
/// server's authoritative positions aligned with the client's
/// displaced mesh (both sample `vaern_core::terrain::height`).
pub fn snap_npcs_to_terrain(mut npcs: Query<&mut Transform, With<Npc>>) {
    for mut tf in &mut npcs {
        tf.translation.y = terrain::height(tf.translation.x, tf.translation.z);
    }
}

/// Credit threat to the caster when damage resolves against an NPC. Threat
/// per cast = `damage * spec.threat_multiplier`; taunt-like abilities
/// (might/threat, arcana/protection) set that >1.0 in the kit builder so
/// tanks hold aggro without outdamaging DPS.
pub fn credit_threat_from_casts(
    mut events: MessageReader<CastEvent>,
    players: Query<(), With<PlayerTag>>,
    mut threat_tables: Query<&mut ThreatTable, With<Npc>>,
) {
    for ev in events.read() {
        if players.get(ev.caster).is_err() {
            continue;
        }
        if let Ok(mut table) = threat_tables.get_mut(ev.target) {
            let threat = (ev.damage.max(0.0)) * ev.threat_multiplier.max(0.0);
            table.add(ev.caster, threat);
        }
    }
}

/// Aggro: each NPC picks a target based on threat table first (highest threat
/// among alive players in aggro range), falling back to nearest player in
/// range when no one is on the threat list yet.
pub fn npc_select_targets(
    npcs: Query<
        (Entity, &Transform, &ThreatTable, &AggroRange, Option<&Target>),
        (With<Npc>, Without<NonCombat>),
    >,
    players: Query<(Entity, &Transform), With<PlayerTag>>,
    mut commands: Commands,
) {
    for (npc, npc_tf, threat, aggro, current) in &npcs {
        let aggro_sq = aggro.0 * aggro.0;
        let in_range: HashSet<Entity> = players
            .iter()
            .filter(|(_, tf)| tf.translation.distance_squared(npc_tf.translation) <= aggro_sq)
            .map(|(e, _)| e)
            .collect();

        let new_target = threat.top(&in_range).or_else(|| {
            players
                .iter()
                .filter(|(e, tf)| {
                    in_range.contains(e)
                        && tf.translation.distance_squared(npc_tf.translation) <= aggro_sq
                })
                .min_by(|a, b| {
                    a.1.translation
                        .distance_squared(npc_tf.translation)
                        .total_cmp(&b.1.translation.distance_squared(npc_tf.translation))
                })
                .map(|(e, _)| e)
        });

        // Only touch the component when the target VALUE actually changes;
        // unconditional insert would mark `Changed<Target>` every frame and
        // spam any observer that listens on target transitions.
        let current_value = current.map(|t| t.0);
        if current_value == new_target {
            continue;
        }
        let Ok(mut ec) = commands.get_entity(npc) else { continue };
        match new_target {
            Some(e) => {
                ec.insert(Target(e));
            }
            None => {
                ec.remove::<Target>();
            }
        }
    }
}

/// Move each targeting NPC toward its target in the XZ plane, stopping
/// at melee range. Runs in FixedUpdate for deterministic, tick-scaled
/// motion. Also yaws the NPC to face the target every tick — even
/// inside melee range, so a wolf biting you keeps its snout pointed at
/// you instead of locking to its last walk-in heading.
pub fn npc_chase_target(
    time: Res<Time>,
    mut npcs: Query<
        (&mut Transform, &Target, Option<&StatusEffects>),
        (With<Npc>, Without<PlayerTag>),
    >,
    players: Query<&Transform, (With<PlayerTag>, Without<Npc>)>,
) {
    let dt = time.delta_secs();
    for (mut tf, target, effects) in &mut npcs {
        let Ok(target_tf) = players.get(target.0) else { continue };
        let mut to_target = target_tf.translation - tf.translation;
        to_target.y = 0.0;
        let dist = to_target.length();
        if dist <= f32::EPSILON {
            continue;
        }
        // Face the target. Use look_to + Dir3 so zero-length vectors
        // can't crash us; the dist check above already rules it out.
        if let Ok(dir) = Dir3::new(to_target / dist) {
            tf.look_to(dir, Dir3::Y);
        }
        if dist <= NPC_MELEE_RANGE {
            continue;
        }
        let slow = effects.map_or(1.0, |e| e.move_speed_mult());
        let step = NPC_MOVE_SPEED * dt * slow;
        let move_dist = step.min(dist - NPC_MELEE_RANGE);
        tf.translation += (to_target / dist) * move_dist;
    }
}

/// If an NPC wanders (or is kited) past its leash range, drop aggro, warp
/// home, and refill HP.
pub fn npc_leash_home(
    mut npcs: Query<
        (
            &mut Transform,
            &NpcHome,
            &LeashRange,
            &mut Health,
            &mut ThreatTable,
        ),
        With<Npc>,
    >,
) {
    for (mut tf, home, leash, mut hp, mut threat) in &mut npcs {
        let leash_sq = leash.0 * leash.0;
        if tf.translation.distance_squared(home.0) > leash_sq {
            tf.translation = home.0;
            hp.current = hp.max;
            threat.clear();
        }
    }
}

/// Idle wander: while an NPC has no Target, walk between random points
/// within NPC_ROAM_RADIUS of its home. `npc_chase_target` takes priority via
/// system ordering — when a Target appears, the chase overrides the roam.
pub fn npc_roam(
    time: Res<Time>,
    mut npcs: Query<
        (&mut Transform, &NpcHome, &mut RoamState, Option<&StatusEffects>),
        (With<Npc>, Without<Target>, Without<NonCombat>),
    >,
) {
    let dt = time.delta_secs();
    let now = time.elapsed_secs();
    for (mut tf, home, mut roam, effects) in &mut npcs {
        let slow = effects.map_or(1.0, |e| e.move_speed_mult());
        let step = NPC_ROAM_SPEED * dt * slow;
        // Waiting at a waypoint?
        if roam.wait_secs > 0.0 {
            roam.wait_secs = (roam.wait_secs - dt).max(0.0);
            continue;
        }

        let mut to_wp = roam.waypoint - tf.translation;
        to_wp.y = 0.0;
        let dist = to_wp.length();

        // Arrived: pick a new waypoint within roam radius of home and idle.
        if dist < 0.3 {
            let seed = home.0.x * 7.31 + home.0.z * 13.07 + now;
            let angle = pseudo_rand_angle(seed);
            let radius = NPC_ROAM_RADIUS * pseudo_rand(Vec3::new(seed, seed * 0.3, seed * 0.7));
            roam.waypoint = home.0 + Vec3::new(angle.cos() * radius, 0.0, angle.sin() * radius);
            roam.wait_secs = 1.5 + 2.0 * pseudo_rand(Vec3::new(seed * 0.11, now, home.0.z));
            continue;
        }

        // Face the waypoint so the mesh leads its walk with its nose
        // instead of sliding sideways.
        if let Ok(dir) = Dir3::new(to_wp / dist) {
            tf.look_to(dir, Dir3::Y);
        }
        // Walk toward waypoint, but keep inside roam radius of home.
        let next = tf.translation + (to_wp / dist) * step;
        if (next - home.0).length_squared() > (NPC_ROAM_RADIUS * 1.5).powi(2) {
            // Overshooting zone — reset to a fresh waypoint next tick.
            roam.wait_secs = 0.1;
            continue;
        }
        tf.translation = next;
    }
}

/// Cheap deterministic 0..1 pseudo-random from a Vec3 seed. Hash the bits and
/// fold them into [0, 1). Good enough for scattering roam waypoints so we
/// don't need a `rand` dep.
pub(super) fn pseudo_rand(seed: Vec3) -> f32 {
    let mut h = (seed.x * 12.9898 + seed.y * 78.233 + seed.z * 37.719).sin();
    h = (h * 43758.5453).fract().abs();
    h
}

fn pseudo_rand_angle(seed: f32) -> f32 {
    let v = (seed.sin() * 43758.5453).fract();
    v.abs() * std::f32::consts::TAU
}
