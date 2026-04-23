//! Bridge inventory + equipment across the netcode boundary.
//!
//! Server owns `PlayerInventory` and `Equipped` components. Clients
//! receive snapshots and send `EquipRequest` / `UnequipRequest`.
//! Snapshots ship only when either component actually changed (or was
//! just added on first spawn). Bevy's `Changed<T>` is inclusive of
//! `Added<T>`, so fresh players get their initial state via the same
//! path without a separate heartbeat.

use bevy::prelude::*;
use lightyear::prelude::server::*;
use lightyear::prelude::*;

use vaern_equipment::Equipped;
use vaern_inventory::{InventorySlot, PlayerInventory};
use vaern_protocol::{
    Channel1, EquipRequest, EquippedSlotEntry, EquippedSnapshot, InventorySlotEntry,
    InventorySnapshot, PlayerTag, UnequipRequest,
};

use crate::data::GameData;

/// Broadcast inventory + equipped snapshots whenever either component
/// changed on a player entity. `Changed<T>` covers `Added<T>`, so a
/// freshly spawned player's initial state still goes out on its first
/// frame. Idle ticks send nothing — the biggest single saving on the
/// per-tick server loop.
pub fn broadcast_inventory_and_equipped(
    players: Query<
        (&ControlledBy, &PlayerInventory, &Equipped),
        (
            With<PlayerTag>,
            Or<(Changed<PlayerInventory>, Changed<Equipped>)>,
        ),
    >,
    mut inv_senders: Query<&mut MessageSender<InventorySnapshot>, With<ClientOf>>,
    mut eq_senders: Query<&mut MessageSender<EquippedSnapshot>, With<ClientOf>>,
) {
    for (cb, inv, eq) in &players {
        // Inventory: convert Vec<Option<InventorySlot>> to the wire type.
        // Walk every slot index so empty slots still map 1:1 to client
        // click indices.
        let slots: Vec<Option<InventorySlotEntry>> = (0..inv.capacity())
            .map(|i| {
                inv.get(i).map(|s: &InventorySlot| InventorySlotEntry {
                    instance: s.instance.clone(),
                    count: s.count,
                })
            })
            .collect();
        if let Ok(mut sender) = inv_senders.get_mut(cb.owner) {
            let _ = sender.send::<Channel1>(InventorySnapshot {
                capacity: inv.capacity() as u32,
                slots,
            });
        }

        // Equipped: list of (slot, instance) entries. Empty slots omitted.
        let entries: Vec<EquippedSlotEntry> = eq
            .iter()
            .map(|(slot, instance)| EquippedSlotEntry {
                slot,
                instance: instance.clone(),
            })
            .collect();
        if let Ok(mut sender) = eq_senders.get_mut(cb.owner) {
            let _ = sender.send::<Channel1>(EquippedSnapshot { entries });
        }
    }
}

/// Drain EquipRequest messages and apply them. Round trip:
///   1. Take the instance at `inventory_idx`.
///   2. Equipped::equip — returns `previous` (slot already held something)
///      and `displaced_offhand` (two-hander kicked offhand out).
///   3. Push both previous + displaced back to inventory.
///   4. On equip failure, restore the taken instance to its original slot.
pub fn handle_equip_requests(
    data: Res<GameData>,
    mut links: Query<(&RemoteId, &mut MessageReceiver<EquipRequest>), With<ClientOf>>,
    mut players: Query<(&PlayerTag, &mut PlayerInventory, &mut Equipped)>,
) {
    for (remote, mut rx) in &mut links {
        let PeerId::Netcode(client_id) = remote.0 else { continue };
        for req in rx.receive() {
            let Some((_, mut inv, mut eq)) = players
                .iter_mut()
                .find(|(tag, _, _)| tag.client_id == client_id)
            else {
                continue;
            };

            let idx = req.inventory_idx as usize;
            // Take one unit from the slot. For non-stackables count=1;
            // for stackables we pull one and leave the rest. Equip is
            // always "the specific instance in that slot."
            let Some((instance, taken)) = inv.take(idx, 1) else {
                println!(
                    "[inv-io] {client_id} equip_request: slot {idx} empty, ignoring"
                );
                continue;
            };
            debug_assert_eq!(taken, 1);

            match eq.equip(req.slot, instance.clone(), &data.content) {
                Ok(result) => {
                    // Push displaced + previous back. Any that don't fit
                    // fall through as leftover logs — full inventory is
                    // rare at 30 slots but handle it.
                    if let Some(prev) = result.previous {
                        let lo = inv.add(prev, 1, &data.content);
                        if lo > 0 {
                            println!("[inv-io] {client_id} no room for previous item; dropped");
                        }
                    }
                    if let Some(off) = result.displaced_offhand {
                        let lo = inv.add(off, 1, &data.content);
                        if lo > 0 {
                            println!("[inv-io] {client_id} no room for displaced offhand; dropped");
                        }
                    }
                }
                Err(e) => {
                    // Restore — put the taken instance back where it was.
                    println!(
                        "[inv-io] {client_id} equip to {:?} rejected: {e}",
                        req.slot
                    );
                    let lo = inv.add(instance, 1, &data.content);
                    if lo > 0 {
                        println!("[inv-io] {client_id} couldn't restore rejected equip; dropped");
                    }
                }
            }
        }
    }
}

/// Drain UnequipRequest messages. Clears the slot and pushes the
/// removed instance back to inventory. No-op if the slot is empty
/// or inventory is full (in the full case the item is dropped —
/// caller gets a log line).
pub fn handle_unequip_requests(
    data: Res<GameData>,
    mut links: Query<(&RemoteId, &mut MessageReceiver<UnequipRequest>), With<ClientOf>>,
    mut players: Query<(&PlayerTag, &mut PlayerInventory, &mut Equipped)>,
) {
    for (remote, mut rx) in &mut links {
        let PeerId::Netcode(client_id) = remote.0 else { continue };
        for req in rx.receive() {
            let Some((_, mut inv, mut eq)) = players
                .iter_mut()
                .find(|(tag, _, _)| tag.client_id == client_id)
            else {
                continue;
            };
            let Some(instance) = eq.unequip(req.slot) else {
                continue;
            };
            let lo = inv.add(instance, 1, &data.content);
            if lo > 0 {
                println!("[inv-io] {client_id} couldn't stow unequipped item; dropped");
            }
        }
    }
}
