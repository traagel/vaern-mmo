//! Periodic + boundary logging for the client. Complements
//! `crates/vaern-server/src/logging.rs`.
//!
//! On `OnEnter(InGame)` logs which character entered. Every 2s inside InGame
//! logs own position / HP / target + remote / npc counts. On every CastFired
//! prints incoming damage with SELF/npc/other classification. On
//! `OnExit(InGame)` logs last-known position + HP.

use bevy::prelude::*;
use lightyear::prelude::{Predicted, Replicated};
use vaern_combat::{Casting, Health, ResourcePool, Target};

use vaern_protocol::PlayerTag;

use crate::menu::{AppState, SelectedCharacter};
use crate::scene::CastFiredLocal;
use crate::shared::{Npc, OwnClientId, Player, RemotePlayer};

pub struct DiagnosticsPlugin;

impl Plugin for DiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(PlayerSnapshotTimer(Timer::from_seconds(
            2.0,
            TimerMode::Repeating,
        )))
        .add_systems(OnEnter(AppState::InGame), log_connected)
        .add_systems(OnExit(AppState::InGame), log_disconnecting)
        .add_systems(
            Update,
            (log_player_snapshot, log_incoming_casts).run_if(in_state(AppState::InGame)),
        );
    }
}

#[derive(Resource)]
struct PlayerSnapshotTimer(Timer);

fn log_player_snapshot(
    time: Res<Time>,
    mut timer: ResMut<PlayerSnapshotTimer>,
    // Predicted copy: source of truth for Transform (we simulate movement
    // locally) and client-local Target.
    predicted: Query<(&Transform, Option<&Target>), With<Player>>,
    // Replicated copy of OUR player: source of truth for server-authoritative
    // Health / ResourcePool / Casting (the Predicted copy's values never
    // update because the client has no local combat simulation).
    own_replicated: Query<
        (&PlayerTag, Option<&Health>, Option<&ResourcePool>, Option<&Casting>),
        (With<Replicated>, Without<Predicted>),
    >,
    own_id: Option<Res<OwnClientId>>,
    remote_players: Query<(), With<RemotePlayer>>,
    npcs: Query<(&Transform, &Health), With<Npc>>,
) {
    timer.0.tick(time.delta());
    if !timer.0.just_finished() {
        return;
    }
    let own_match = own_id
        .as_deref()
        .and_then(|o| own_replicated.iter().find(|(t, _, _, _)| t.client_id == o.0));
    match predicted.single() {
        Ok((tf, target)) => {
            let hp = own_match.and_then(|(_, h, _, _)| h);
            let pool = own_match.and_then(|(_, _, p, _)| p);
            let cast = own_match.and_then(|(_, _, _, c)| c);
            let hp_str = hp
                .map(|h| format!("{:.0}/{:.0}", h.current, h.max))
                .unwrap_or_else(|| "?".into());
            let pool_str = pool
                .map(|p| format!("{:.0}/{:.0}", p.current, p.max))
                .unwrap_or_else(|| "?".into());
            let target_str = target
                .map(|t| format!("{:?}", t.0))
                .unwrap_or_else(|| "—".into());
            let cast_str = cast
                .map(|c| format!("{} {:.1}s", c.school, c.remaining_secs))
                .unwrap_or_else(|| "—".into());
            info!(
                "[snapshot] pos=({:.1},{:.1},{:.1}) hp={hp_str} pool={pool_str} target={target_str} casting={cast_str} remote={} npcs={}",
                tf.translation.x,
                tf.translation.y,
                tf.translation.z,
                remote_players.iter().count(),
                npcs.iter().count(),
            );
            for (ntf, nhp) in &npcs {
                let d = ntf.translation.distance(tf.translation);
                // Per-NPC rows at debug — runs the world ~600 mobs × every 2s.
                // Run with `RUST_LOG=vaern_client=debug` to see them.
                debug!(
                    "  npc pos=({:.1},{:.1},{:.1}) hp={:.0}/{:.0} distance={:.1}",
                    ntf.translation.x, ntf.translation.y, ntf.translation.z,
                    nhp.current, nhp.max, d,
                );
            }
        }
        Err(_) => {
            info!(
                "[snapshot] no local player entity found (predicted copy absent) — remote={} npcs={}",
                remote_players.iter().count(),
                npcs.iter().count(),
            );
        }
    }
}

fn log_incoming_casts(
    mut reader: MessageReader<CastFiredLocal>,
    own: Query<Entity, With<Player>>,
    npcs: Query<Entity, With<Npc>>,
    healths: Query<&Health>,
) {
    let own_id = own.single().ok();
    for CastFiredLocal(ev) in reader.read() {
        let is_self = Some(ev.target) == own_id;
        let target_kind = if is_self {
            "SELF"
        } else if npcs.get(ev.target).is_ok() {
            "npc"
        } else {
            "other"
        };
        let hp = healths
            .get(ev.target)
            .map(|h| format!("{:.0}/{:.0}", h.current, h.max))
            .unwrap_or_else(|_| "?".into());
        // Self-hits at info (we want to notice getting punched); npc/other
        // at debug (mob-on-mob, player-on-mob — every combat tick).
        if is_self {
            info!(
                "[cast-fired] target={target_kind} ({:?}) school={} dmg={:.1} hp_after={hp}",
                ev.target, ev.school, ev.damage,
            );
        } else {
            debug!(
                "[cast-fired] target={target_kind} ({:?}) school={} dmg={:.1} hp_after={hp}",
                ev.target, ev.school, ev.damage,
            );
        }
    }
}

fn log_connected(
    selected: Option<Res<SelectedCharacter>>,
    own_id: Option<Res<OwnClientId>>,
) {
    let name = selected
        .as_deref()
        .map(|c| c.name.clone())
        .unwrap_or_else(|| "(unnamed)".to_string());
    let pillar = selected
        .as_deref()
        .map(|c| c.core_pillar.to_string())
        .unwrap_or_else(|| "?".to_string());
    let race = selected
        .as_deref()
        .map(|c| c.race_id.clone())
        .unwrap_or_default();
    let cid = own_id.as_deref().map(|o| o.0).unwrap_or(0);
    info!(
        "[connected] entered world: client_id={cid} character='{name}' race={race} pillar={pillar}"
    );
}

fn log_disconnecting(
    own: Query<(&Transform, Option<&Health>), With<Player>>,
    selected: Option<Res<SelectedCharacter>>,
) {
    let name = selected
        .as_deref()
        .map(|c| c.name.clone())
        .unwrap_or_else(|| "(unnamed)".to_string());
    match own.single() {
        Ok((tf, hp)) => {
            let hp_str = hp
                .map(|h| format!("{:.0}/{:.0}", h.current, h.max))
                .unwrap_or_else(|| "?".into());
            info!(
                "[disconnecting] '{name}' last pos=({:.1},{:.1},{:.1}) hp={hp_str}",
                tf.translation.x, tf.translation.y, tf.translation.z,
            );
        }
        Err(_) => info!("[disconnecting] '{name}' (no local player transform)"),
    }
}
