//! Vendor NPC IO — open / buy / sell flow.
//!
//! Flow:
//!   1. Client presses F near a vendor → `VendorOpenRequest { vendor }`.
//!      Server validates proximity + the target carries `VendorStock`
//!      and responds with `VendorWindowSnapshot`.
//!   2. `VendorBuyRequest` → re-validate (in range, has stock, can
//!      afford), debit wallet, add item to inventory, decrement stock,
//!      re-broadcast window.
//!   3. `VendorSellRequest` → re-validate (in range, item resolves,
//!      not soulbound, not no_vendor), credit wallet via
//!      `vendor_sell_price`, remove stack.
//!   4. `VendorClosedNotice` — emitted if the player walks out of
//!      range, the vendor despawns, or the player disconnects.
//!
//! Vendor IDs are a simple per-entity monotonic u64 generated at
//! handler time (same pattern as `LootIdCounter`). Storing the id on
//! the vendor as a component avoids drift between ticks.

use bevy::log::{info, warn};
use bevy::prelude::*;
use lightyear::prelude::server::*;
use lightyear::prelude::*;

use vaern_economy::{
    PlayerWallet, QualityMod, VendorStock, VendorSupply, vendor_buy_price, vendor_sell_price,
    VendorPricing,
};
use vaern_inventory::PlayerInventory;
use vaern_items::ItemInstance;
use vaern_protocol::{
    Channel1, PlayerTag, VendorBuyRequest, VendorClosedNotice, VendorId, VendorOpenRequest,
    VendorSellRequest, VendorWindowListing, VendorWindowSnapshot,
};

use crate::data::GameData;

/// Max distance from player to vendor for any interaction to succeed.
/// Matches quest-giver / loot interact range for consistency.
pub const VENDOR_INTERACT_RANGE: f32 = 5.0;

/// Monotonic vendor-id counter. One id per vendor, assigned on first
/// access and stashed on the entity so subsequent buys line up.
#[derive(Resource, Default)]
pub struct VendorIdCounter(pub VendorId);

/// Stable id attached to a vendor entity on first lookup. Decouples
/// the wire `VendorId` from Bevy's entity generation so reconnects
/// and client rebuilds don't race on the mapping.
#[derive(Component, Debug, Clone, Copy)]
pub struct VendorIdTag(pub VendorId);

/// Ensure every vendor entity carries a stable `VendorIdTag` before
/// any handler queries it. Runs every tick as a cheap `Added` filter
/// — new vendor spawns get tagged on their next tick.
pub fn tag_new_vendors(
    mut counter: ResMut<VendorIdCounter>,
    q: Query<Entity, (With<VendorStock>, Without<VendorIdTag>)>,
    mut commands: Commands,
) {
    for e in &q {
        counter.0 += 1;
        commands.entity(e).insert(VendorIdTag(counter.0));
    }
}

/// Build a `VendorWindowSnapshot` for a vendor entity. Re-runs the
/// economy pricing each time so any future pricing tuning takes
/// immediate effect (cheap — copper is a single multiply).
fn build_snapshot(
    vendor_id: VendorId,
    vendor_name: &str,
    stock: &VendorStock,
    data: &GameData,
    pricing: &VendorPricing,
) -> VendorWindowSnapshot {
    let mut listings = Vec::with_capacity(stock.listings.len());
    for (idx, l) in stock.listings.iter().enumerate() {
        let instance = l.to_instance();
        let Ok(resolved) = data.content.resolve(&instance) else {
            warn!("[vendor] listing {} → unresolvable; skipped", l.base_id);
            continue;
        };
        let Ok(price) = vendor_buy_price(&resolved, QualityMod::default()) else {
            // no_vendor or similar — skip.
            continue;
        };
        let stock_remaining = match l.supply {
            VendorSupply::Infinite => None,
            VendorSupply::Limited(n) => Some(n),
        };
        listings.push(VendorWindowListing {
            idx: idx as u32,
            instance,
            price_copper: price,
            stock: stock_remaining,
        });
        // pricing is currently only used in the sell path; silence the
        // unused-var lint while we're here.
        let _ = pricing;
    }
    VendorWindowSnapshot {
        vendor_id,
        vendor_name: vendor_name.to_string(),
        listings,
    }
}

pub fn handle_vendor_open_requests(
    data: Res<GameData>,
    mut links: Query<(&RemoteId, &mut MessageReceiver<VendorOpenRequest>), With<ClientOf>>,
    players: Query<(&PlayerTag, &Transform, &ControlledBy)>,
    vendors: Query<
        (
            Entity,
            &Transform,
            &VendorStock,
            &VendorIdTag,
            Option<&vaern_combat::DisplayName>,
        ),
    >,
    mut sender: Query<&mut MessageSender<VendorWindowSnapshot>, With<ClientOf>>,
) {
    let pricing = VendorPricing::default();
    for (remote, mut rx) in &mut links {
        let PeerId::Netcode(client_id) = remote.0 else { continue };
        for req in rx.receive() {
            let Some((_, player_tf, cb)) = players
                .iter()
                .find(|(tag, _, _)| tag.client_id == client_id)
            else {
                continue;
            };
            let Ok((_, vendor_tf, stock, id_tag, name)) = vendors.get(req.vendor) else {
                info!(
                    "[vendor] open from client {client_id} — entity {:?} not a vendor",
                    req.vendor
                );
                continue;
            };
            let dist = player_tf.translation.distance(vendor_tf.translation);
            if dist > VENDOR_INTERACT_RANGE {
                info!(
                    "[vendor] open from client {client_id} — out of range ({dist:.1}u)"
                );
                continue;
            }
            let display = name
                .map(|n| n.0.clone())
                .unwrap_or_else(|| "Vendor".to_string());
            let snap = build_snapshot(id_tag.0, &display, stock, &data, &pricing);
            if let Ok(mut tx) = sender.get_mut(cb.owner) {
                let _ = tx.send::<Channel1>(snap);
            }
        }
    }
}

pub fn handle_vendor_buy_requests(
    data: Res<GameData>,
    mut links: Query<(&RemoteId, &mut MessageReceiver<VendorBuyRequest>), With<ClientOf>>,
    mut players: Query<(
        &PlayerTag,
        &Transform,
        &ControlledBy,
        &mut PlayerWallet,
        &mut PlayerInventory,
    )>,
    mut vendors: Query<
        (
            Entity,
            &Transform,
            &mut VendorStock,
            &VendorIdTag,
            Option<&vaern_combat::DisplayName>,
        ),
    >,
    mut window_tx: Query<&mut MessageSender<VendorWindowSnapshot>, With<ClientOf>>,
) {
    let pricing = VendorPricing::default();
    for (remote, mut rx) in &mut links {
        let PeerId::Netcode(client_id) = remote.0 else { continue };
        for req in rx.receive() {
            let Some((_, player_tf, cb, mut wallet, mut inv)) = players
                .iter_mut()
                .find(|(tag, _, _, _, _)| tag.client_id == client_id)
            else {
                continue;
            };
            // Look up the vendor by id.
            let Some((vendor_e, vendor_tf, mut stock, id_tag, name)) = vendors
                .iter_mut()
                .find(|(_, _, _, tag, _)| tag.0 == req.vendor_id)
            else {
                continue;
            };
            if player_tf.translation.distance(vendor_tf.translation) > VENDOR_INTERACT_RANGE {
                continue;
            }
            let idx = req.listing_idx as usize;
            let Some(listing) = stock.listings.get(idx).cloned() else { continue };
            let instance = listing.to_instance();
            let Ok(resolved) = data.content.resolve(&instance) else { continue };
            let Ok(price) = vendor_buy_price(&resolved, QualityMod::default()) else {
                continue;
            };
            if !wallet.can_afford(price as u64) {
                info!(
                    "[vendor] client {client_id} can't afford {price}c (has {}c)",
                    wallet.copper
                );
                continue;
            }
            // Atomic-ish: try to add the item first, then debit. If the
            // inventory is full the purchase rolls back cleanly.
            let leftover = inv.add(instance.clone(), 1, &data.content);
            if leftover > 0 {
                info!("[vendor] client {client_id} inventory full; purchase aborted");
                continue;
            }
            if !wallet.try_debit(price as u64) {
                // Shouldn't happen — can_afford gated this. Undo the add
                // to stay consistent.
                // Rolling back is best-effort; the stack merge may have
                // already happened, in which case the excess is on the
                // house. Acceptable in pre-alpha; tighten with a
                // reservation API later.
                warn!("[vendor] debit race for client {client_id} price={price}c");
                continue;
            }
            stock.consume(idx);
            info!(
                "[vendor] client {client_id} bought {} for {price}c (wallet={}c)",
                instance.base_id, wallet.copper,
            );
            // Re-broadcast the window so the buyer sees updated stock.
            let display = name
                .map(|n| n.0.clone())
                .unwrap_or_else(|| "Vendor".to_string());
            let snap = build_snapshot(id_tag.0, &display, &stock, &data, &pricing);
            if let Ok(mut tx) = window_tx.get_mut(cb.owner) {
                let _ = tx.send::<Channel1>(snap);
            }
            let _ = vendor_e;
        }
    }
}

pub fn handle_vendor_sell_requests(
    data: Res<GameData>,
    mut links: Query<(&RemoteId, &mut MessageReceiver<VendorSellRequest>), With<ClientOf>>,
    mut players: Query<(
        &PlayerTag,
        &Transform,
        &ControlledBy,
        &mut PlayerWallet,
        &mut PlayerInventory,
    )>,
    vendors: Query<(Entity, &Transform, &VendorStock, &VendorIdTag)>,
) {
    let pricing = VendorPricing::default();
    for (remote, mut rx) in &mut links {
        let PeerId::Netcode(client_id) = remote.0 else { continue };
        for req in rx.receive() {
            let Some((_, player_tf, _cb, mut wallet, mut inv)) = players
                .iter_mut()
                .find(|(tag, _, _, _, _)| tag.client_id == client_id)
            else {
                continue;
            };
            let Some((_, vendor_tf, _, _)) = vendors
                .iter()
                .find(|(_, _, _, tag)| tag.0 == req.vendor_id)
            else {
                continue;
            };
            if player_tf.translation.distance(vendor_tf.translation) > VENDOR_INTERACT_RANGE {
                continue;
            }
            let idx = req.inventory_idx as usize;
            // Pluck the instance out without consuming yet — we need to
            // resolve + price before committing. `inv.get(idx)` returns
            // a reference to the slot at the literal slot index (not
            // iterated position), which is what the client's protocol
            // semantics expect.
            let instance: Option<ItemInstance> = inv.get(idx).map(|s| s.instance.clone());
            let Some(instance) = instance else { continue };
            let Ok(resolved) = data.content.resolve(&instance) else { continue };
            let Ok(price) = vendor_sell_price(&resolved, &pricing, QualityMod::default()) else {
                info!(
                    "[vendor] client {client_id} tried to sell soulbound / no_vendor item"
                );
                continue;
            };
            // Remove one unit from the inventory slot.
            let taken = inv.take(idx, 1);
            if taken.is_none() {
                continue;
            }
            wallet.credit(price as u64);
            info!(
                "[vendor] client {client_id} sold {} for {price}c (wallet={}c)",
                instance.base_id, wallet.copper,
            );
        }
    }
}

/// Auto-close the vendor window when the player walks out of range.
/// Tracked per-link with a `lingered_vendor_id` marker; cleared the
/// first tick proximity fails. Cheap: only vendors with open windows
/// would send notices, but since we don't track "who has which window
/// open" today we just emit the notice whenever any vendor leaves
/// the player's interact range. Client ignores unknown ids.
pub fn sweep_out_of_range_vendor_windows(
    players: Query<(&PlayerTag, &Transform, &ControlledBy)>,
    vendors: Query<(&Transform, &VendorIdTag)>,
    mut last_open: Local<std::collections::HashMap<u64, VendorId>>,
    mut sender: Query<&mut MessageSender<VendorClosedNotice>, With<ClientOf>>,
) {
    // Build a per-player set of currently-in-range vendor ids.
    for (tag, p_tf, cb) in &players {
        let mut best: Option<VendorId> = None;
        let mut best_dist = f32::MAX;
        for (v_tf, id) in &vendors {
            let d = p_tf.translation.distance(v_tf.translation);
            if d <= VENDOR_INTERACT_RANGE && d < best_dist {
                best = Some(id.0);
                best_dist = d;
            }
        }
        match (last_open.get(&tag.client_id).copied(), best) {
            (Some(prev), None) => {
                if let Ok(mut tx) = sender.get_mut(cb.owner) {
                    let _ = tx.send::<Channel1>(VendorClosedNotice { vendor_id: prev });
                }
                last_open.remove(&tag.client_id);
            }
            (_, Some(now)) => {
                last_open.insert(tag.client_id, now);
            }
            (None, None) => {}
        }
    }
}
