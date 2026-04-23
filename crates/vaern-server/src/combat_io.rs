//! Bridge combat events across the netcode boundary: client cast intents in,
//! cast-fired notifications out. Actual damage/cast resolution lives in
//! `vaern-combat`; this module only translates messages ↔ ECS.

use bevy::prelude::*;
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use vaern_combat::{
    CastEvent, CastRequest, Projectile, Stamina, StatusEffect, StatusEffects, Target,
};
use vaern_protocol::{CastFired, CastIntent, PlayerTag, StanceRequest};

use crate::connect::ServerHotbar;

/// Block stance parameters. Drains 15 stamina/sec while held; with a
/// 100-pool that's 6.7s full-pool sustain. 60% damage reduction on
/// frontal hits, 25% on flanks, 0% from behind — hardcore-prep feel
/// where positioning matters more than pure timing.
const BLOCK_DRAIN_PER_SEC: f32 = 15.0;
const BLOCK_FRONTAL_REDUCTION: f32 = 0.60;
const BLOCK_FLANK_REDUCTION: f32 = 0.25;

/// Parry window parameters. 0.35s window (~21 ticks @60Hz) after tap;
/// consuming a parry costs 20 stamina. A blown tap is free — so you
/// can spam-test timing without bleeding stamina. A parried hit
/// negates fully.
const PARRY_WINDOW_SECS: f32 = 0.35;
const PARRY_STAMINA_COST: f32 = 20.0;

/// For each client link, drain CastIntent messages and translate into a
/// Target + CastRequest on that client's player entity. `select_and_fire`
/// picks it up during Update.
pub fn handle_cast_intents(
    mut links: Query<(&RemoteId, &mut MessageReceiver<CastIntent>), With<ClientOf>>,
    players: Query<(Entity, &PlayerTag, &ServerHotbar)>,
    mut commands: Commands,
) {
    for (remote, mut rx) in &mut links {
        let PeerId::Netcode(id) = remote.0 else { continue };
        for intent in rx.receive() {
            let Some((player_entity, _, hotbar)) =
                players.iter().find(|(_, tag, _)| tag.client_id == id)
            else {
                continue;
            };
            let Some(&ability) = hotbar.slots.get(intent.slot as usize) else { continue };
            if let Ok(mut ec) = commands.get_entity(player_entity) {
                ec.insert((Target(intent.target), CastRequest(ability)));
            }
        }
    }
}

/// For each damage resolution on the server, broadcast a CastFired message so
/// all clients can spawn the impact VFX. Fires only for instant casts and
/// end-of-channel resolutions — both go through CastEvent.
pub fn broadcast_cast_fired(
    mut events: MessageReader<CastEvent>,
    mut links: Query<&mut MessageSender<CastFired>, With<ClientOf>>,
) {
    for ev in events.read() {
        let msg = CastFired {
            caster: ev.caster,
            target: ev.target,
            school: ev.school.clone(),
            damage: ev.damage,
        };
        for mut sender in &mut links {
            let _ = sender.send::<vaern_protocol::Channel1>(msg.clone());
        }
    }
}

/// Drain `StanceRequest` messages. Block toggles attach/detach a
/// persistent `blocking` StatusEffect; Parry taps open a short negate
/// window. Players already mid-cast are allowed to request stances —
/// the damage pipeline applies the effect regardless.
///
/// Both stances live on the player's `StatusEffects` component via
/// ids `"blocking"` / `"parrying"`. The refresh-on-reapply semantics
/// of `StatusEffects::apply` mean a second ParryTap within the window
/// simply resets it — no stacking.
pub fn handle_stance_requests(
    mut links: Query<(&RemoteId, &mut MessageReceiver<StanceRequest>), With<ClientOf>>,
    mut players: Query<(Entity, &PlayerTag, Option<&mut StatusEffects>, Option<&Stamina>)>,
    mut commands: Commands,
) {
    for (remote, mut rx) in &mut links {
        let PeerId::Netcode(id) = remote.0 else { continue };
        for req in rx.receive() {
            let Some((player_e, _, mut effects, stamina)) = players
                .iter_mut()
                .find(|(_, tag, _, _)| tag.client_id == id)
            else {
                continue;
            };
            match req {
                StanceRequest::SetBlock(true) => {
                    // Refuse if stamina already empty — no phantom stance.
                    if stamina.map(|s| s.current > 0.0).unwrap_or(false) {
                        let effect = StatusEffect::block(
                            player_e,
                            BLOCK_DRAIN_PER_SEC,
                            BLOCK_FRONTAL_REDUCTION,
                            BLOCK_FLANK_REDUCTION,
                        );
                        apply_effect(&mut effects, &mut commands, player_e, effect);
                    }
                }
                StanceRequest::SetBlock(false) => {
                    if let Some(effects) = effects.as_deref_mut() {
                        effects.remove("blocking");
                    }
                }
                StanceRequest::ParryTap => {
                    // Free to tap and miss — no cost until a hit actually
                    // consumes it via `StatusEffects::consume_parry`.
                    let effect =
                        StatusEffect::parry(player_e, PARRY_WINDOW_SECS, PARRY_STAMINA_COST);
                    apply_effect(&mut effects, &mut commands, player_e, effect);
                }
            }
        }
    }
}

/// Attach an effect, inserting `StatusEffects` first if the target
/// doesn't have one yet. Factored out so Block + Parry paths share
/// the same insertion glue.
fn apply_effect(
    effects: &mut Option<Mut<StatusEffects>>,
    commands: &mut Commands,
    entity: Entity,
    effect: StatusEffect,
) {
    match effects.as_deref_mut() {
        Some(existing) => existing.apply(effect),
        None => {
            let mut fresh = StatusEffects::default();
            fresh.apply(effect);
            commands.entity(entity).insert(fresh);
        }
    }
}

/// `vaern-combat` spawns projectile entities without knowing about
/// replication (it's a leaf crate). Server-side system attaches Replicate +
/// Interpolation so every client sees the projectile fly.
pub fn attach_projectile_replication(
    fresh: Query<Entity, (Added<Projectile>, Without<Replicate>)>,
    mut commands: Commands,
) {
    for e in &fresh {
        commands.entity(e).insert((
            Name::new("projectile"),
            Replicate::to_clients(NetworkTarget::All),
            InterpolationTarget::to_clients(NetworkTarget::All),
        ));
    }
}
