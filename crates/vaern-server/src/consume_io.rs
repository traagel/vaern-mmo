//! Handle `ConsumeItemRequest` — the client-side "use this potion" flow.
//!
//! Drain requests, resolve the inventory slot through `ContentRegistry`,
//! and dispatch on the item's `ConsumeEffect`:
//!
//! * `HealHp` / `HealMana` / `HealStamina` → clamp-add to the matching
//!   pool's current, capped at max.
//! * `Buff` → push a timed `StatusEffect::StatMods` onto the player.
//! * `None` → ignored with a debug log (inventory stack stays intact).
//!
//! A successful consume decrements one unit from the stack and relies on
//! `Changed<PlayerInventory>` in `inventory_io::broadcast_inventory_and_equipped`
//! to re-ship the snapshot. Health / mana / stamina changes surface via
//! the per-tick `PlayerStateSnapshot`.

use bevy::prelude::*;
use lightyear::prelude::server::*;
use lightyear::prelude::*;

use vaern_combat::{Health, ResourcePool, Stamina, StatusEffect, StatusEffects};
use vaern_inventory::PlayerInventory;
use vaern_items::{ConsumeEffect, ItemKind};
use vaern_protocol::{ConsumeItemRequest, PlayerTag};

use crate::data::GameData;

pub fn handle_consume_requests(
    data: Res<GameData>,
    mut links: Query<(&RemoteId, &mut MessageReceiver<ConsumeItemRequest>), With<ClientOf>>,
    mut players: Query<
        (
            Entity,
            &PlayerTag,
            &mut PlayerInventory,
            &mut Health,
            &mut ResourcePool,
            &mut Stamina,
            Option<&mut StatusEffects>,
        ),
        With<PlayerTag>,
    >,
    mut commands: Commands,
) {
    for (remote, mut rx) in &mut links {
        let PeerId::Netcode(client_id) = remote.0 else { continue };
        for req in rx.receive() {
            let Some((player_e, _, mut inv, mut hp, mut pool, mut stamina, mut effects)) =
                players.iter_mut().find(|(_, tag, ..)| tag.client_id == client_id)
            else {
                continue;
            };

            let idx = req.inventory_idx as usize;
            let Some(slot) = inv.get(idx) else {
                println!("[consume] {client_id} slot {idx} empty; ignoring");
                continue;
            };

            // Resolve the instance to inspect its kind + effect. Unknown
            // bases / invalid pairings are logged and skipped — the stack
            // stays untouched (player doesn't lose the item).
            let resolved = match data.content.resolve(&slot.instance) {
                Ok(r) => r,
                Err(e) => {
                    println!("[consume] {client_id} resolve failed: {e}");
                    continue;
                }
            };
            let ItemKind::Consumable { effect, .. } = &resolved.kind else {
                println!(
                    "[consume] {client_id} item at slot {idx} ({}) is not a consumable",
                    resolved.display_name
                );
                continue;
            };
            if matches!(effect, ConsumeEffect::None) {
                // Flavor / quest consumable with no mechanical effect.
                // Explicit no-op keeps inventory intact.
                continue;
            }

            apply_consume_effect(
                effect,
                player_e,
                &mut hp,
                &mut pool,
                &mut stamina,
                &mut effects,
                &mut commands,
            );

            // Decrement one unit. `take` with count=1 pulls one charge
            // and removes the slot if that was the last one. Shouldn't
            // fail — we just read it above — but handle gracefully.
            if inv.take(idx, 1).is_none() {
                println!("[consume] {client_id} decrement failed on slot {idx}");
            }
        }
    }
}

/// Apply a `ConsumeEffect` to its target. Shared between inventory-slot
/// consumes (`handle_consume_requests`) and belt-hotkey consumes
/// (`belt_io::handle_consume_belt`). Caller is responsible for the
/// inventory decrement afterward. `None` effects are no-op but still
/// "successful" from the caller's perspective.
pub(crate) fn apply_consume_effect(
    effect: &ConsumeEffect,
    entity: Entity,
    hp: &mut Health,
    pool: &mut ResourcePool,
    stamina: &mut Stamina,
    effects: &mut Option<Mut<StatusEffects>>,
    commands: &mut Commands,
) {
    match effect {
        ConsumeEffect::None => {}
        ConsumeEffect::HealHp { amount } => {
            hp.current = (hp.current + amount).min(hp.max);
        }
        ConsumeEffect::HealMana { amount } => {
            pool.current = (pool.current + amount).min(pool.max);
        }
        ConsumeEffect::HealStamina { amount } => {
            stamina.current = (stamina.current + amount).min(stamina.max);
        }
        ConsumeEffect::Buff {
            id,
            duration_secs,
            damage_mult_add,
            resist_adds,
        } => {
            let fx = StatusEffect::stat_mods(
                id.clone(),
                entity,
                *duration_secs,
                *damage_mult_add,
                *resist_adds,
            );
            apply_effect(effects, commands, entity, fx);
        }
    }
}

/// Attach a `StatusEffect` to a player, inserting `StatusEffects` if the
/// component doesn't exist yet. Mirror of `combat_io::apply_effect`.
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
