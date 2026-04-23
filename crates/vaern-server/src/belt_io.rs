//! Consumable belt (4-slot hotkey-bound potion strip).
//!
//! The belt component lives on the player (`ConsumableBelt`). Slots
//! store `ItemInstance` templates, not inventory indices — so bindings
//! survive stack rearrangement. At activation time the server searches
//! the player's inventory for a matching stack and applies the same
//! `ConsumeEffect` dispatch that `consume_io` uses.
//!
//! Three request handlers:
//!
//! * `handle_bind_belt_slot` — validate the instance is present + is a
//!   Consumable, then store.
//! * `handle_clear_belt_slot` — wipe a slot.
//! * `handle_consume_belt` — find the first inventory stack matching
//!   the bound template, apply the effect, decrement one charge. The
//!   binding persists even if the inventory is empty.
//!
//! Belt state ships to the client via `ConsumableBeltSnapshot`, gated on
//! `Changed<ConsumableBelt>`.

use bevy::prelude::*;
use lightyear::prelude::server::*;
use lightyear::prelude::*;

use vaern_combat::{Health, ResourcePool, Stamina, StatusEffects};
use vaern_inventory::{ConsumableBelt, PlayerInventory, BELT_SLOTS};
use vaern_items::ItemKind;
use vaern_protocol::{
    BindBeltSlotRequest, Channel1, ClearBeltSlotRequest, ConsumableBeltSnapshot,
    ConsumeBeltRequest, PlayerTag,
};

use crate::consume_io::apply_consume_effect;
use crate::data::GameData;

/// Drain `BindBeltSlotRequest` messages. Refuses the bind if
///   * slot_idx out of range,
///   * the item isn't present in the player's inventory,
///   * the item isn't a Consumable.
pub fn handle_bind_belt_slot(
    data: Res<GameData>,
    mut links: Query<(&RemoteId, &mut MessageReceiver<BindBeltSlotRequest>), With<ClientOf>>,
    mut players: Query<(&PlayerTag, &PlayerInventory, &mut ConsumableBelt)>,
) {
    for (remote, mut rx) in &mut links {
        let PeerId::Netcode(client_id) = remote.0 else { continue };
        for req in rx.receive() {
            let slot_idx = req.slot_idx as usize;
            if slot_idx >= BELT_SLOTS {
                println!("[belt] {client_id} bind slot {slot_idx} out of range");
                continue;
            }
            let Some((_, inv, mut belt)) = players
                .iter_mut()
                .find(|(tag, _, _)| tag.client_id == client_id)
            else {
                continue;
            };
            // Must own at least one matching stack.
            if inv.find_matching(&req.instance).is_none() {
                println!(
                    "[belt] {client_id} bind rejected: {} not in inventory",
                    req.instance.base_id
                );
                continue;
            }
            // Must be a Consumable — binding a sword to the potion belt
            // would be a bug, not a feature.
            let Ok(resolved) = data.content.resolve(&req.instance) else {
                println!("[belt] {client_id} bind resolve failed");
                continue;
            };
            if !matches!(resolved.kind, ItemKind::Consumable { .. }) {
                println!(
                    "[belt] {client_id} bind rejected: {} is not a consumable",
                    resolved.display_name
                );
                continue;
            }
            belt.bind(slot_idx, req.instance.clone());
        }
    }
}

/// Drain `ClearBeltSlotRequest` messages — just wipe the slot.
pub fn handle_clear_belt_slot(
    mut links: Query<(&RemoteId, &mut MessageReceiver<ClearBeltSlotRequest>), With<ClientOf>>,
    mut players: Query<(&PlayerTag, &mut ConsumableBelt)>,
) {
    for (remote, mut rx) in &mut links {
        let PeerId::Netcode(client_id) = remote.0 else { continue };
        for req in rx.receive() {
            let Some((_, mut belt)) = players
                .iter_mut()
                .find(|(tag, _)| tag.client_id == client_id)
            else {
                continue;
            };
            belt.clear(req.slot_idx as usize);
        }
    }
}

/// Drain `ConsumeBeltRequest` messages. For each request:
///   1. Look up the bound template (no-op if unbound).
///   2. Find the first matching inventory stack (no-op if none).
///   3. Dispatch on the `ConsumeEffect` via `apply_consume_effect`.
///   4. Decrement one charge.
pub fn handle_consume_belt(
    data: Res<GameData>,
    mut links: Query<(&RemoteId, &mut MessageReceiver<ConsumeBeltRequest>), With<ClientOf>>,
    mut players: Query<(
        Entity,
        &PlayerTag,
        &mut PlayerInventory,
        &ConsumableBelt,
        &mut Health,
        &mut ResourcePool,
        &mut Stamina,
        Option<&mut StatusEffects>,
    )>,
    mut commands: Commands,
) {
    for (remote, mut rx) in &mut links {
        let PeerId::Netcode(client_id) = remote.0 else { continue };
        for req in rx.receive() {
            let Some((
                player_e,
                _,
                mut inv,
                belt,
                mut hp,
                mut pool,
                mut stamina,
                mut effects,
            )) = players.iter_mut().find(|(_, tag, ..)| tag.client_id == client_id)
            else {
                continue;
            };
            let slot_idx = req.slot_idx as usize;
            let Some(template) = belt.get(slot_idx).cloned() else {
                // Unbound slot — silent no-op. Hotkey spam shouldn't spam logs.
                continue;
            };
            if !inv.find_matching(&template).is_some() {
                println!(
                    "[belt] {client_id} fire slot {slot_idx}: no {} in inventory",
                    template.base_id
                );
                continue;
            }
            // Resolve for the effect only. Bindings that later become
            // invalid (content reload) degrade to a resolve-failure log.
            let resolved = match data.content.resolve(&template) {
                Ok(r) => r,
                Err(e) => {
                    println!("[belt] {client_id} fire resolve failed: {e}");
                    continue;
                }
            };
            let ItemKind::Consumable { effect, .. } = &resolved.kind else {
                // A bind validation check means this shouldn't happen, but
                // degrade gracefully if it does.
                continue;
            };
            apply_consume_effect(
                effect,
                player_e,
                &mut hp,
                &mut pool,
                &mut stamina,
                &mut effects,
                &mut commands,
            );
            inv.consume_matching(&template);
        }
    }
}

/// Ship a belt snapshot to the owning client whenever the belt changes.
/// Mirrors the inventory snapshot pattern — idle ticks send nothing.
pub fn broadcast_belt(
    players: Query<(&ControlledBy, &ConsumableBelt), (With<PlayerTag>, Changed<ConsumableBelt>)>,
    mut senders: Query<&mut MessageSender<ConsumableBeltSnapshot>, With<ClientOf>>,
) {
    for (cb, belt) in &players {
        if let Ok(mut sender) = senders.get_mut(cb.owner) {
            let snapshot = ConsumableBeltSnapshot {
                slots: belt.slots.iter().cloned().collect(),
            };
            let _ = sender.send::<Channel1>(snapshot);
        }
    }
}
