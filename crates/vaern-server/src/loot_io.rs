//! Mob death → loot container spawn → client window flow.
//!
//! Flow:
//!   1. Mob dies → `spawn_loot_container_on_mob_death` observer rolls a
//!      drop against the NpcKind-derived table. If non-empty, a
//!      server-only entity carrying `LootContainer` spawns at the
//!      mob's death position, owned by the top-threat player.
//!   2. Every tick, `broadcast_pending_loots` sends each client the
//!      list of their open containers (position + item count).
//!   3. Client presses `G` near one → `LootOpenRequest`. Server
//!      responds with `LootWindowSnapshot`.
//!   4. `LootTakeRequest` / `LootTakeAllRequest` move items into the
//!      player's inventory. Fresh `LootWindowSnapshot` on change.
//!   5. When a container empties (or the despawn timer trips),
//!      `cleanup_loot_containers` despawns and sends
//!      `LootClosedNotice` so the client window auto-closes.

use bevy::log::info;
use bevy::prelude::*;
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use rand::{SeedableRng, rngs::StdRng};

use vaern_combat::NpcKind;
use vaern_combat::components::DisplayName;
use vaern_data::ItemReward;
use vaern_economy::PlayerWallet;
use vaern_inventory::{InventorySlot, PlayerInventory};
use vaern_items::{ContentRegistry, ItemInstance};
use vaern_loot::{DropTable, roll_drop};
use vaern_protocol::{
    Channel1, LootClosedNotice, LootContainerSummary, LootId, LootOpenRequest, LootRollOpen,
    LootTakeAllRequest, LootTakeRequest, LootWindowEntry, LootWindowSnapshot,
    PendingLootsSnapshot, PlayerTag,
};

use crate::data::GameData;
use crate::loot_rolls::{
    build_roll_open_payload, LootRollContainer, RollItemState, ROLL_EXPIRES_SECS,
};
use crate::npc::{MobSourceId, Npc, ThreatTable};
use crate::party_io::{PartyTable, PARTY_SHARE_RADIUS};

/// How long a loot container persists before auto-despawn. Generous at
/// 5 minutes so a player fighting through a zone can circle back for
/// missed drops. Tighten once party-loot-rules land.
const LOOT_DESPAWN_SECS: f32 = 300.0;

/// Max distance from player to container for `G` to open it. Matches
/// the current quest-giver F-range for consistency.
pub const LOOT_OPEN_RANGE: f32 = 5.0;

fn material_tier_for_mob_level(level: u32) -> u8 {
    let tier = 1 + (level / 12) as u8;
    tier.clamp(1, 6)
}

/// Monotonic loot-id counter. Resource so the observer can bump it
/// safely from multiple spawn sites.
#[derive(Resource, Default)]
pub struct LootIdCounter(pub LootId);

/// Global "something about the pending-loots set changed this tick" flag.
/// Flipped to true whenever a container spawns, despawns, or has its
/// contents mutated. `broadcast_pending_loots` early-returns when it's
/// false, which is most ticks — and that's the biggest win on the loot
/// side of the per-tick budget.
///
/// Coarse (one flag for all players, not per-owner), but correct: a
/// dirty tick broadcasts to every player, each of whom gets the same
/// owner-filtered snapshot they'd have gotten anyway. An extra empty
/// snapshot to the non-owner is cheap.
#[derive(Resource, Default)]
pub struct PendingLootsDirty(pub bool);

#[derive(Resource)]
pub struct LootRng(pub StdRng);

impl Default for LootRng {
    fn default() -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0xDEADBEEF);
        Self(StdRng::seed_from_u64(seed))
    }
}

/// Server-only component on a loot entity. NOT replicated — clients
/// see containers via `PendingLootsSnapshot` messages, not via
/// lightyear replication.
#[derive(Component, Debug)]
pub struct LootContainer {
    pub loot_id: LootId,
    pub owner: u64,
    pub position: Vec3,
    pub contents: Vec<InventorySlot>,
    pub age_secs: f32,
}

/// Observer on mob despawn. Rolls the NpcKind-derived drop table for
/// both item + coin. Coin credits the top-threat player's wallet
/// directly (no loot container dance — pre-alpha keeps coin flow
/// immediate and legible). Item drops still spawn a container.
///
/// Slice 6: when the dying mob has authored boss-drops in
/// `GameData::boss_drops` (keyed on `MobSourceId`), those guaranteed
/// `ItemReward`s are resolved through the content registry and appended
/// to the same container's contents alongside the random roll. Phase C
/// will gate distribution to a party via Need-Before-Greed-Pass; for now
/// every drop lands with the top-threat owner.
pub fn spawn_loot_container_on_mob_death(
    trigger: On<Remove, MobSourceId>,
    mobs: Query<(
        Option<&NpcKind>,
        Option<&ThreatTable>,
        Option<&Transform>,
        Option<&Npc>,
        Option<&MobSourceId>,
    )>,
    mut players: Query<(
        &PlayerTag,
        &Transform,
        &DisplayName,
        &mut PlayerWallet,
        &ControlledBy,
    )>,
    party_table: Res<PartyTable>,
    mut roll_open_tx: Query<&mut MessageSender<LootRollOpen>, With<ClientOf>>,
    data: Res<GameData>,
    mut rng: ResMut<LootRng>,
    mut counter: ResMut<LootIdCounter>,
    mut dirty: ResMut<PendingLootsDirty>,
    mut commands: Commands,
) {
    let entity = trigger.entity;
    let Ok((kind_opt, threat_opt, transform_opt, _, source_id_opt)) = mobs.get(entity) else {
        return;
    };
    let Some(kind) = kind_opt else { return };
    let Some(threat) = threat_opt else { return };
    let Some(transform) = transform_opt else { return };
    let Some(table) = DropTable::for_npc(*kind, material_tier_for_mob_level(1)) else {
        return;
    };

    // Top-threat player owns the loot — same rule as XP.
    let top = threat
        .0
        .iter()
        .filter(|(_, t)| **t > 0.0)
        .max_by(|a, b| a.1.total_cmp(b.1));
    let Some((top_entity, _)) = top else { return };
    let owner_entity = *top_entity;

    let kill_pos = transform.translation;
    let is_named = matches!(*kind, NpcKind::Named);

    // Roll the random table once (coins + optional item) and credit
    // coins to the killer's wallet inside a scoped mut borrow. The
    // optional rolled item gets folded into `final_contents` below.
    let owner_client;
    let rolled_item;
    {
        let Ok((owner_tag, _tf, _dname, mut owner_wallet, _cb)) =
            players.get_mut(owner_entity)
        else {
            return;
        };
        owner_client = owner_tag.client_id;
        let r = roll_drop(&table, &data.content, &mut rng.0);
        if r.copper > 0 {
            owner_wallet.credit(r.copper as u64);
            info!(
                "[loot] credited {c}c to client {owner_client} (wallet now {total}c)",
                c = r.copper,
                total = owner_wallet.copper,
            );
        }
        rolled_item = r.item;
    }

    // Build container contents: authored boss drops first (deterministic),
    // then the random table roll (if any).
    let mut final_contents: Vec<InventorySlot> = Vec::new();
    if let Some(source_id) = source_id_opt {
        if let Some(drops) = data.boss_drops.drops_for_mob(&source_id.0) {
            for slot in resolve_boss_drops(drops, &data.content) {
                final_contents.push(slot);
            }
            if !final_contents.is_empty() {
                info!(
                    "[loot] boss drops for {mob}: {n} item(s) added to container",
                    mob = source_id.0,
                    n = final_contents.len(),
                );
            }
        }
    }
    if let Some(instance) = rolled_item {
        final_contents.push(InventorySlot { instance, count: 1 });
    }
    if final_contents.is_empty() {
        return;
    }

    counter.0 += 1;
    let loot_id = counter.0;

    // Eligibility for shared rolls — boss-tier (Named) + party + radius.
    let mut eligible_clients: Vec<u64> = Vec::new();
    let mut eligible_names: Vec<String> = Vec::new();
    let mut eligible_links: Vec<(u64, Entity)> = Vec::new();
    if is_named {
        if let Some(party) = party_table.party_of(owner_client) {
            for (tag, tf, dname, _wallet, cb) in players.iter() {
                if !party.members.contains(&tag.client_id) {
                    continue;
                }
                if (tf.translation - kill_pos).length() <= PARTY_SHARE_RADIUS {
                    eligible_clients.push(tag.client_id);
                    eligible_names.push(dname.0.clone());
                    eligible_links.push((tag.client_id, cb.owner));
                }
            }
        }
    }

    // ≥2 means "killer + at least one partner in radius" → shared rolls.
    let use_shared_rolls = eligible_clients.len() >= 2;

    if use_shared_rolls {
        let items: Vec<RollItemState> = final_contents
            .into_iter()
            .map(RollItemState::new)
            .collect();
        let container = LootRollContainer {
            loot_id,
            eligible: eligible_clients.clone(),
            items,
            age_secs: 0.0,
            expires_at_secs: ROLL_EXPIRES_SECS,
        };
        let payload = build_roll_open_payload(&container, eligible_names);
        for (_, link) in &eligible_links {
            if let Ok(mut tx) = roll_open_tx.get_mut(*link) {
                let _ = tx.send::<Channel1>(payload.clone());
            }
        }
        commands.spawn((
            Name::new(format!("loot-roll-container-{loot_id}")),
            container,
        ));
        info!(
            "[loot:roll] spawned roll container #{loot_id} for {n} eligible voter(s) at {:?}",
            kill_pos,
            n = eligible_clients.len(),
        );
    } else {
        commands.spawn((
            Name::new(format!("loot-container-{loot_id}")),
            LootContainer {
                loot_id,
                owner: owner_client,
                position: kill_pos,
                contents: final_contents,
                age_secs: 0.0,
            },
        ));
        dirty.0 = true;
        info!(
            "[loot] spawned container #{loot_id} for client {owner_client} at {:?}",
            kill_pos
        );
    }
}

/// Resolve a list of `ItemReward`s into `InventorySlot`s for a boss-drop
/// container. Mirrors `quests.rs::grant_item_rewards` for the per-item
/// resolve + skip-on-error semantics, but without the per-pillar filter
/// — boss-drops are dropped as a pool and Phase C distributes via party
/// rolls (Open Need; pillar tags are advisory only).
pub(crate) fn resolve_boss_drops(
    drops: &[ItemReward],
    registry: &ContentRegistry,
) -> Vec<InventorySlot> {
    let mut out = Vec::with_capacity(drops.len());
    for r in drops {
        let instance = match &r.material {
            Some(m) => ItemInstance::new(&r.base, m, &r.quality),
            None => ItemInstance::materialless(&r.base, &r.quality),
        };
        if let Err(e) = registry.resolve(&instance) {
            info!(
                "[loot:boss] skipping {} ({:?}/{}): {e}",
                r.base, r.material, r.quality
            );
            continue;
        }
        out.push(InventorySlot {
            instance,
            count: r.count,
        });
    }
    out
}

/// Age every container; despawn those older than LOOT_DESPAWN_SECS or
/// empty. Emits LootClosedNotice to the owner so the client can close
/// its window.
pub fn cleanup_loot_containers(
    time: Res<Time>,
    mut containers: Query<(Entity, &mut LootContainer)>,
    players: Query<(Entity, &PlayerTag, &ControlledBy)>,
    mut sender: Query<&mut MessageSender<LootClosedNotice>, With<ClientOf>>,
    mut dirty: ResMut<PendingLootsDirty>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();
    for (e, mut c) in &mut containers {
        c.age_secs += dt;
        let empty = c.contents.is_empty();
        let timed_out = c.age_secs >= LOOT_DESPAWN_SECS;
        if !empty && !timed_out {
            continue;
        }
        // Find the owner's link to send the close notice.
        if let Some((_, _, cb)) = players.iter().find(|(_, tag, _)| tag.client_id == c.owner) {
            if let Ok(mut tx) = sender.get_mut(cb.owner) {
                let _ = tx.send::<Channel1>(LootClosedNotice { loot_id: c.loot_id });
            }
        }
        commands.entity(e).despawn();
        dirty.0 = true;
    }
}

/// Push each owner a `PendingLootsSnapshot` listing their open
/// containers, but only when the pending-loot set changed this tick
/// (spawn, despawn, or contents mutation). The dirty flag is cleared
/// at the end of the run, so idle ticks cost one resource read.
pub fn broadcast_pending_loots(
    containers: Query<&LootContainer>,
    players: Query<(&PlayerTag, &ControlledBy)>,
    mut sender: Query<&mut MessageSender<PendingLootsSnapshot>, With<ClientOf>>,
    mut dirty: ResMut<PendingLootsDirty>,
) {
    if !dirty.0 {
        return;
    }
    for (tag, cb) in &players {
        let summaries: Vec<LootContainerSummary> = containers
            .iter()
            .filter(|c| c.owner == tag.client_id)
            .map(|c| LootContainerSummary {
                loot_id: c.loot_id,
                pos_x: c.position.x,
                pos_y: c.position.y,
                pos_z: c.position.z,
                item_count: c.contents.len() as u32,
            })
            .collect();
        if let Ok(mut tx) = sender.get_mut(cb.owner) {
            let _ = tx.send::<Channel1>(PendingLootsSnapshot {
                containers: summaries,
            });
        }
    }
    dirty.0 = false;
}

/// Find a container by loot_id owned by a specific client_id.
fn find_container<'a, I>(
    containers: I,
    loot_id: LootId,
    client_id: u64,
) -> Option<Entity>
where
    I: IntoIterator<Item = (Entity, &'a LootContainer)>,
{
    containers
        .into_iter()
        .find(|(_, c)| c.loot_id == loot_id && c.owner == client_id)
        .map(|(e, _)| e)
}

fn send_window_snapshot(
    loot_id: LootId,
    contents: &[InventorySlot],
    link_entity: Entity,
    sender: &mut Query<&mut MessageSender<LootWindowSnapshot>, With<ClientOf>>,
) {
    let slots: Vec<LootWindowEntry> = contents
        .iter()
        .map(|s| LootWindowEntry {
            instance: s.instance.clone(),
            count: s.count,
        })
        .collect();
    if let Ok(mut tx) = sender.get_mut(link_entity) {
        let _ = tx.send::<Channel1>(LootWindowSnapshot { loot_id, slots });
    }
}

pub fn handle_loot_open_requests(
    mut links: Query<(&RemoteId, &mut MessageReceiver<LootOpenRequest>), With<ClientOf>>,
    players: Query<(&PlayerTag, &Transform, &ControlledBy)>,
    containers: Query<(Entity, &LootContainer)>,
    mut sender: Query<&mut MessageSender<LootWindowSnapshot>, With<ClientOf>>,
) {
    for (remote, mut rx) in &mut links {
        let PeerId::Netcode(client_id) = remote.0 else { continue };
        for req in rx.receive() {
            let Some((_, player_tf, cb)) = players
                .iter()
                .find(|(tag, _, _)| tag.client_id == client_id)
            else {
                continue;
            };
            let Some(entity) = find_container(containers.iter(), req.loot_id, client_id) else {
                info!("[loot] open: {client_id} no container #{}", req.loot_id);
                continue;
            };
            let Ok((_, container)) = containers.get(entity) else { continue };
            let dist = player_tf.translation.distance(container.position);
            if dist > LOOT_OPEN_RANGE {
                info!(
                    "[loot] open: {client_id} container #{} out of range ({dist:.1})",
                    req.loot_id
                );
                continue;
            }
            send_window_snapshot(container.loot_id, &container.contents, cb.owner, &mut sender);
        }
    }
}

pub fn handle_loot_take_requests(
    data: Res<GameData>,
    mut links: Query<(&RemoteId, &mut MessageReceiver<LootTakeRequest>), With<ClientOf>>,
    mut players: Query<(&PlayerTag, &Transform, &ControlledBy, &mut PlayerInventory)>,
    mut containers: Query<(Entity, &mut LootContainer)>,
    mut sender: Query<&mut MessageSender<LootWindowSnapshot>, With<ClientOf>>,
    mut dirty: ResMut<PendingLootsDirty>,
) {
    for (remote, mut rx) in &mut links {
        let PeerId::Netcode(client_id) = remote.0 else { continue };
        for req in rx.receive() {
            let Some((_, player_tf, cb, mut inv)) = players
                .iter_mut()
                .find(|(tag, _, _, _)| tag.client_id == client_id)
            else {
                continue;
            };

            // Locate + re-query for mutable access.
            let container_entity = containers
                .iter()
                .find(|(_, c)| c.loot_id == req.loot_id && c.owner == client_id)
                .map(|(e, _)| e);
            let Some(ce) = container_entity else { continue };
            let Ok((_, mut container)) = containers.get_mut(ce) else { continue };

            if player_tf.translation.distance(container.position) > LOOT_OPEN_RANGE {
                continue;
            }
            let idx = req.slot_idx as usize;
            if idx >= container.contents.len() {
                continue;
            }
            let slot = container.contents[idx].clone();
            let leftover = inv.add(slot.instance, slot.count, &data.content);
            if leftover >= slot.count {
                // Didn't fit at all — leave the container untouched.
                continue;
            }
            // Partial success: remove what went through.
            let taken = slot.count - leftover;
            if taken >= container.contents[idx].count {
                container.contents.remove(idx);
            } else {
                container.contents[idx].count -= taken;
            }
            dirty.0 = true;
            send_window_snapshot(container.loot_id, &container.contents, cb.owner, &mut sender);
        }
    }
}

pub fn handle_loot_take_all_requests(
    data: Res<GameData>,
    mut links: Query<(&RemoteId, &mut MessageReceiver<LootTakeAllRequest>), With<ClientOf>>,
    mut players: Query<(&PlayerTag, &Transform, &ControlledBy, &mut PlayerInventory)>,
    mut containers: Query<(Entity, &mut LootContainer)>,
    mut sender: Query<&mut MessageSender<LootWindowSnapshot>, With<ClientOf>>,
    mut dirty: ResMut<PendingLootsDirty>,
) {
    for (remote, mut rx) in &mut links {
        let PeerId::Netcode(client_id) = remote.0 else { continue };
        for req in rx.receive() {
            let Some((_, player_tf, cb, mut inv)) = players
                .iter_mut()
                .find(|(tag, _, _, _)| tag.client_id == client_id)
            else {
                continue;
            };
            let container_entity = containers
                .iter()
                .find(|(_, c)| c.loot_id == req.loot_id && c.owner == client_id)
                .map(|(e, _)| e);
            let Some(ce) = container_entity else { continue };
            let Ok((_, mut container)) = containers.get_mut(ce) else { continue };

            if player_tf.translation.distance(container.position) > LOOT_OPEN_RANGE {
                continue;
            }

            // Walk contents; attempt to add each. Any that don't fit
            // stay in the container so the player can come back.
            let mut remaining: Vec<InventorySlot> = Vec::new();
            for slot in container.contents.drain(..) {
                let leftover = inv.add(slot.instance.clone(), slot.count, &data.content);
                if leftover > 0 {
                    remaining.push(InventorySlot {
                        instance: slot.instance,
                        count: leftover,
                    });
                }
            }
            container.contents = remaining;
            dirty.0 = true;
            send_window_snapshot(container.loot_id, &container.contents, cb.owner, &mut sender);
        }
    }
}
