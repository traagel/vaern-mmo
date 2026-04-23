//! Event + periodic-snapshot logging to help debug combat state, connection
//! transitions, and the "my player vanished" class of bug. All output goes
//! through bevy tracing.
//!
//! Log levels:
//!   - `info!` for player-facing + boundary events (connect, disconnect,
//!     player deaths, snapshot header with counts)
//!   - `debug!` for per-NPC chatter (mob casts, mob deaths, per-NPC pos/hp
//!     rows, target changes). Silent by default — enable with
//!     `RUST_LOG=vaern_server=debug` when you need the NPC firehose.

use bevy::log::{debug, info};
use bevy::prelude::*;
use lightyear::prelude::*;
use vaern_combat::{CastEvent, DeathEvent, Health, Target};
use vaern_protocol::PlayerTag;

use crate::npc::{Npc, NpcHome};

pub struct LoggingPlugin;

impl Plugin for LoggingPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SnapshotTimer(Timer::from_seconds(
            2.0,
            TimerMode::Repeating,
        )))
        .init_resource::<TickRateState>()
        .add_systems(
            Update,
            (
                log_cast_events,
                log_death_events,
                log_npc_target_changes,
                log_periodic_snapshot,
                log_tick_rate,
            ),
        )
        .add_observer(log_new_client)
        .add_observer(log_client_disconnect)
        .add_observer(log_ability_cast_start);
    }
}

// ─── tick rate / frame time ────────────────────────────────────────────────

/// Rolling window state for the tick-rate logger. Accumulates frames over
/// ~1 real second, then emits Hz + worst-case frame time so we can spot
/// Update stretching past the 16.6ms budget (which starves replication
/// and makes NPCs rubber-band client-side).
#[derive(Resource, Default)]
struct TickRateState {
    frames: u32,
    accumulated_secs: f32,
    max_frame_ms: f32,
}

fn log_tick_rate(time: Res<Time>, mut state: ResMut<TickRateState>) {
    state.frames += 1;
    let dt = time.delta_secs();
    state.accumulated_secs += dt;
    let frame_ms = dt * 1000.0;
    if frame_ms > state.max_frame_ms {
        state.max_frame_ms = frame_ms;
    }
    if state.accumulated_secs < 1.0 {
        return;
    }
    let hz = state.frames as f32 / state.accumulated_secs;
    info!(
        "[tick] {:.0} Hz  avg_frame={:.2}ms  max_frame={:.2}ms  frames={}",
        hz,
        (state.accumulated_secs / state.frames as f32) * 1000.0,
        state.max_frame_ms,
        state.frames,
    );
    state.frames = 0;
    state.accumulated_secs = 0.0;
    state.max_frame_ms = 0.0;
}

// ─── connect / disconnect ──────────────────────────────────────────────────

fn log_new_client(trigger: On<Add, LinkOf>) {
    info!("[connect] new client link entity={:?}", trigger.entity);
}

/// Observer on `Remove<Connected>`. Fires when a link entity loses the
/// `Connected` component — the moment lightyear marks the client as gone.
/// We can't easily query the link's associated player here (commands run
/// before the query resolves), but we log the link itself + time.
fn log_client_disconnect(trigger: On<Remove, Connected>) {
    info!(
        "[disconnect] client link={:?} lost Connected marker",
        trigger.entity
    );
}

// ─── casts / damage ────────────────────────────────────────────────────────

fn log_cast_events(
    mut events: MessageReader<CastEvent>,
    players: Query<&PlayerTag>,
    npcs: Query<&Npc>,
    healths: Query<&Health>,
    transforms: Query<&Transform>,
) {
    for ev in events.read() {
        let caster_is_player = players.get(ev.caster).is_ok();
        let caster_kind = label_entity(ev.caster, &players, &npcs);
        let target_kind = label_entity(ev.target, &players, &npcs);
        let target_hp = healths
            .get(ev.target)
            .map(|h| format!("{:.0}/{:.0}", h.current, h.max))
            .unwrap_or_else(|_| "?".into());
        let target_pos = transforms
            .get(ev.target)
            .map(|t| format!("({:.1}, {:.1})", t.translation.x, t.translation.z))
            .unwrap_or_else(|_| "(?, ?)".into());
        // Player casts at `info!` (rare, player-initiated). NPC auto-attacks
        // at `debug!` — hundreds of mobs cycling every ~1.5s drowns the log.
        if caster_is_player {
            info!(
                "[cast] {caster_kind} → {target_kind}  dmg={:.1}  school={}  target_hp={target_hp}  target_pos={target_pos}",
                ev.damage, ev.school,
            );
        } else {
            debug!(
                "[cast] {caster_kind} → {target_kind}  dmg={:.1}  school={}  target_hp={target_hp}  target_pos={target_pos}",
                ev.damage, ev.school,
            );
        }
    }
}

/// Observer firing when a `Casting` component is inserted (cast begins).
/// Logs the caster's decision BEFORE damage resolves.
fn log_ability_cast_start(
    trigger: On<Add, vaern_combat::Casting>,
    casts: Query<&vaern_combat::Casting>,
    players: Query<&PlayerTag>,
    npcs: Query<&Npc>,
) {
    let Ok(cast) = casts.get(trigger.entity) else { return };
    let caster_is_player = players.get(trigger.entity).is_ok();
    let caster_kind = label_entity(trigger.entity, &players, &npcs);
    let target_kind = label_entity(cast.target, &players, &npcs);
    if caster_is_player {
        info!(
            "[cast:start] {caster_kind} → {target_kind}  school={}  cast_time={:.1}s  dmg={:.1}",
            cast.school, cast.total_secs, cast.damage,
        );
    } else {
        debug!(
            "[cast:start] {caster_kind} → {target_kind}  school={}  cast_time={:.1}s  dmg={:.1}",
            cast.school, cast.total_secs, cast.damage,
        );
    }
}

// ─── deaths ────────────────────────────────────────────────────────────────

fn log_death_events(
    mut events: MessageReader<DeathEvent>,
    players: Query<&PlayerTag>,
    npcs: Query<&Npc>,
    transforms: Query<&Transform>,
) {
    for ev in events.read() {
        let is_player = players.get(ev.entity).is_ok();
        let kind = label_entity(ev.entity, &players, &npcs);
        let pos = transforms
            .get(ev.entity)
            .map(|t| format!("({:.1}, {:.1})", t.translation.x, t.translation.z))
            .unwrap_or_else(|_| "(?, ?)".into());
        // Mob deaths at debug — they fire constantly. Player deaths at info.
        if is_player {
            info!("[death] {kind} at {pos}");
        } else {
            debug!("[death] {kind} at {pos}");
        }
    }
}

// ─── NPC target-change observers ───────────────────────────────────────────

fn log_npc_target_changes(
    // Entities whose Target was just added or changed this frame.
    added_or_changed: Query<(Entity, &Target), (With<Npc>, Changed<Target>)>,
    // Entities that had Target removed this frame.
    mut removed: RemovedComponents<Target>,
    npcs: Query<Entity, With<Npc>>,
    players: Query<&PlayerTag>,
    npcs_q: Query<&Npc>,
) {
    for (npc, target) in &added_or_changed {
        let npc_label = format!("npc={:?}", npc);
        let target_label = label_entity(target.0, &players, &npcs_q);
        debug!("[npc:target] {npc_label} acquired {target_label}");
    }
    for entity in removed.read() {
        if npcs.get(entity).is_ok() {
            debug!("[npc:target] npc={:?} lost target", entity);
        }
    }
}

// ─── periodic snapshot ─────────────────────────────────────────────────────

#[derive(Resource)]
struct SnapshotTimer(Timer);

fn log_periodic_snapshot(
    time: Res<Time>,
    mut timer: ResMut<SnapshotTimer>,
    players: Query<(Entity, &PlayerTag, &Transform, &Health)>,
    npcs: Query<(Entity, &Transform, &Health, Option<&NpcHome>, Option<&Target>), With<Npc>>,
) {
    timer.0.tick(time.delta());
    if !timer.0.just_finished() {
        return;
    }
    let player_count = players.iter().count();
    let npc_count = npcs.iter().count();
    info!("[snapshot] {} players, {} npcs", player_count, npc_count);
    for (entity, tag, tf, hp) in &players {
        info!(
            "  player {:?} client={} pillar={}  pos=({:.1},{:.1},{:.1})  hp={:.0}/{:.0}",
            entity,
            tag.client_id,
            tag.core_pillar,
            tf.translation.x,
            tf.translation.y,
            tf.translation.z,
            hp.current,
            hp.max,
        );
    }
    for (entity, tf, hp, home, target) in &npcs {
        let home_str = home
            .map(|h| format!("({:.1},{:.1})", h.0.x, h.0.z))
            .unwrap_or_else(|| "—".into());
        let target_str = target
            .map(|t| format!("{:?}", t.0))
            .unwrap_or_else(|| "—".into());
        debug!(
            "  npc    {:?}                pos=({:.1},{:.1},{:.1})  hp={:.0}/{:.0}  home={home_str}  target={target_str}",
            entity,
            tf.translation.x,
            tf.translation.y,
            tf.translation.z,
            hp.current,
            hp.max,
        );
    }
}

// ─── helpers ───────────────────────────────────────────────────────────────

fn label_entity(entity: Entity, players: &Query<&PlayerTag>, npcs: &Query<&Npc>) -> String {
    if let Ok(tag) = players.get(entity) {
        format!("player({:?} client={} pillar={})", entity, tag.client_id, tag.core_pillar)
    } else if npcs.get(entity).is_ok() {
        format!("npc({:?})", entity)
    } else {
        format!("entity({:?})", entity)
    }
}
