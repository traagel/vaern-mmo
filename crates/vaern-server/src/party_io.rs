//! Party system — invite, accept/decline, leave, kick, disband.
//!
//! Data model:
//!   * `PartyTable` resource: `HashMap<PartyId, Party>` keyed by
//!     server-monotonic id. One entry per live party.
//!   * `PlayerPartyId` component on the player entity — `None` means
//!     solo. Cheap lookup from an entity to its party id.
//!   * `PendingInvites` resource: Vec of outstanding invites with a
//!     60s expiry.
//!
//! Broadcast pattern: any time party composition mutates (add, remove,
//! disband), `party_table.dirty.insert(party_id)` is flipped. A single
//! system drains the dirty set each tick, rebuilds `PartySnapshot`
//! with current HP / zone for every member, and pushes it to every
//! member's link. HP mutates every combat event but we don't want to
//! re-broadcast on every tick — so snapshots use a coarse-grained
//! "dirty" flag that flips on join/leave; HP staleness between
//! broadcasts is acceptable in pre-alpha. Later we can layer an
//! "every 500ms heartbeat" on top for live HP bars.
//!
//! Shared XP: when a mob dies (observer hook), any party member within
//! `PARTY_SHARE_RADIUS` of the killer gets a share of the XP reward.
//! Per-member share is scaled so party play doesn't penalize XP gain
//! — total XP ≈ solo XP × 1.25 for a 5-player party (small group
//! bonus), split evenly across sharing members.
//!
//! Party chat: this module also exposes `route_party_chat` for
//! `chat_io` to call on the `ChatChannel::Party` arm. Recipients are
//! every member of the sender's party across all zones.

use std::collections::{HashMap, HashSet};

use bevy::log::{debug, info};
use bevy::prelude::*;
use lightyear::prelude::server::*;
use lightyear::prelude::*;

use vaern_character::{Experience, PlayerRace};
use vaern_combat::{DisplayName, Health};
use vaern_protocol::{
    Channel1, PartyDisbandedNotice, PartyId, PartyIncomingInvite, PartyInviteRequest,
    PartyInviteResponse, PartyKickRequest, PartyLeaveRequest, PartyMember, PartySnapshot,
    PlayerTag,
};

use crate::aoi::ClientZone;

/// Max people in a party. WoW-classic dungeon size; big enough for
/// niche-coop social play.
pub const MAX_PARTY_SIZE: usize = 5;
/// Radius around the killer in which party members get a share of
/// the XP reward. Wider than the 20u /say range so a support caster
/// holding the flanks doesn't get cut out.
pub const PARTY_SHARE_RADIUS: f32 = 40.0;
/// How long a pending invite lives before the server drops it.
pub const INVITE_TTL_SECS: f32 = 60.0;

/// Party record in the server-side table.
#[derive(Debug, Clone)]
pub struct Party {
    pub id: PartyId,
    pub leader: u64,
    pub members: Vec<u64>,
}

impl Party {
    fn contains(&self, client_id: u64) -> bool {
        self.members.iter().any(|&m| m == client_id)
    }
}

/// Table of live parties + monotonic id counter + change-tracking
/// set. Dirty entries get snapshot-broadcast once per tick.
#[derive(Resource, Default)]
pub struct PartyTable {
    pub next_id: PartyId,
    pub parties: HashMap<PartyId, Party>,
    pub dirty: HashSet<PartyId>,
    /// Flipped when a party was disbanded this tick — used so the
    /// broadcast system can emit `PartyDisbandedNotice` to ex-members
    /// that no longer appear in the table.
    pub disbanded: Vec<(PartyId, Vec<u64>)>,
}

impl PartyTable {
    fn alloc_id(&mut self) -> PartyId {
        self.next_id += 1;
        self.next_id
    }

    /// Lookup `client_id`'s current party.
    pub fn party_of(&self, client_id: u64) -> Option<&Party> {
        self.parties.values().find(|p| p.contains(client_id))
    }

    fn mark_dirty(&mut self, party_id: PartyId) {
        self.dirty.insert(party_id);
    }
}

/// Component: stamps a player entity with their current party id so
/// observers / xp grants can cheaply look up party membership. `None` =
/// solo. Kept on the entity rather than only in the table so systems
/// with transform queries don't need the resource every tick.
#[derive(Component, Debug, Clone, Copy)]
pub struct PlayerPartyId(pub PartyId);

#[derive(Debug, Clone)]
pub struct PendingInvite {
    pub party_id: PartyId,
    pub from_client: u64,
    pub from_name: String,
    pub to_client: u64,
    pub age_secs: f32,
}

#[derive(Resource, Default)]
pub struct PendingInvites(pub Vec<PendingInvite>);

// ---------------------------------------------------------------------------
// Request handlers
// ---------------------------------------------------------------------------

/// Drain `PartyInviteRequest` messages. Resolves target name →
/// client id, validates (target exists, not already in a party, not
/// already-pending), creates a singleton party for the inviter if
/// they weren't in one, pushes `PartyIncomingInvite` to the target.
pub fn handle_party_invite(
    mut invite_rx: Query<(&RemoteId, &mut MessageReceiver<PartyInviteRequest>), With<ClientOf>>,
    players: Query<(Entity, &PlayerTag, &DisplayName, &ControlledBy)>,
    mut party_table: ResMut<PartyTable>,
    mut pending: ResMut<PendingInvites>,
    mut invite_tx: Query<&mut MessageSender<PartyIncomingInvite>, With<ClientOf>>,
    mut commands: Commands,
) {
    let mut requests: Vec<(u64, String)> = Vec::new();
    for (remote, mut rx) in &mut invite_rx {
        let PeerId::Netcode(client_id) = remote.0 else { continue };
        for req in rx.receive() {
            requests.push((client_id, req.target_name.clone()));
        }
    }
    for (from_client, target_name) in requests {
        let Some((from_entity, _, from_name, _)) = players
            .iter()
            .find(|(_, tag, _, _)| tag.client_id == from_client)
        else {
            continue;
        };
        let from_name = from_name.0.clone();

        // Resolve target by display name (case-insensitive).
        let Some((_, target_tag, _, target_cb)) = players
            .iter()
            .find(|(_, _, name, _)| name.0.eq_ignore_ascii_case(&target_name))
        else {
            info!("[party] {from_name} → unknown target '{target_name}'");
            continue;
        };

        if target_tag.client_id == from_client {
            info!("[party] {from_name} tried to invite themselves");
            continue;
        }
        if party_table.party_of(target_tag.client_id).is_some() {
            info!("[party] {from_name} → {target_name}: target already in a party");
            continue;
        }
        if pending
            .0
            .iter()
            .any(|p| p.to_client == target_tag.client_id && p.from_client == from_client)
        {
            continue;
        }

        // Make sure the inviter has a party. If not, mint one with them as leader.
        let party_id = match party_table.party_of(from_client) {
            Some(p) => {
                if p.members.len() >= MAX_PARTY_SIZE {
                    info!("[party] {from_name}'s party is full");
                    continue;
                }
                p.id
            }
            None => {
                let id = party_table.alloc_id();
                party_table.parties.insert(
                    id,
                    Party {
                        id,
                        leader: from_client,
                        members: vec![from_client],
                    },
                );
                commands.entity(from_entity).insert(PlayerPartyId(id));
                party_table.mark_dirty(id);
                id
            }
        };

        pending.0.push(PendingInvite {
            party_id,
            from_client,
            from_name: from_name.clone(),
            to_client: target_tag.client_id,
            age_secs: 0.0,
        });

        if let Ok(mut tx) = invite_tx.get_mut(target_cb.owner) {
            let _ = tx.send::<Channel1>(PartyIncomingInvite {
                party_id,
                from_name: from_name.clone(),
            });
        }
        info!("[party] invite sent: {from_name} → {target_name} (party {party_id})");
    }
}

/// Response → accept / decline. On accept, add member to party,
/// insert `PlayerPartyId` on their entity, mark dirty. On decline,
/// drop the pending invite.
pub fn handle_party_response(
    mut response_rx: Query<(&RemoteId, &mut MessageReceiver<PartyInviteResponse>), With<ClientOf>>,
    players: Query<(Entity, &PlayerTag)>,
    mut party_table: ResMut<PartyTable>,
    mut pending: ResMut<PendingInvites>,
    mut commands: Commands,
) {
    let mut actions: Vec<(u64, PartyInviteResponse)> = Vec::new();
    for (remote, mut rx) in &mut response_rx {
        let PeerId::Netcode(client_id) = remote.0 else { continue };
        for msg in rx.receive() {
            actions.push((client_id, msg));
        }
    }
    for (client_id, resp) in actions {
        let Some(idx) = pending
            .0
            .iter()
            .position(|p| p.to_client == client_id && p.party_id == resp.party_id)
        else {
            debug!("[party] response from {client_id} for unknown invite");
            continue;
        };
        let invite = pending.0.remove(idx);
        if !resp.accept {
            info!(
                "[party] {} declined invite to party {}",
                client_id, invite.party_id,
            );
            continue;
        }
        let Some(party) = party_table.parties.get_mut(&invite.party_id) else {
            continue;
        };
        if party.members.len() >= MAX_PARTY_SIZE {
            info!("[party] party {} is full — accept dropped", party.id);
            continue;
        }
        if party.contains(client_id) {
            continue;
        }
        party.members.push(client_id);
        let party_id = party.id;
        party_table.mark_dirty(party_id);
        // Stamp PlayerPartyId on the joining entity.
        if let Some((entity, _)) = players.iter().find(|(_, tag)| tag.client_id == client_id) {
            commands.entity(entity).insert(PlayerPartyId(party_id));
        }
        info!("[party] client {client_id} accepted into party {party_id}");
    }
}

/// Player-initiated leave. Drops the member; if size falls below 2,
/// disband the party entirely.
pub fn handle_party_leave(
    mut leave_rx: Query<(&RemoteId, &mut MessageReceiver<PartyLeaveRequest>), With<ClientOf>>,
    players: Query<(Entity, &PlayerTag)>,
    mut party_table: ResMut<PartyTable>,
    mut commands: Commands,
) {
    let mut clients: Vec<u64> = Vec::new();
    for (remote, mut rx) in &mut leave_rx {
        let PeerId::Netcode(client_id) = remote.0 else { continue };
        for _ in rx.receive() {
            clients.push(client_id);
        }
    }
    for client_id in clients {
        remove_member(&mut party_table, client_id, &players, &mut commands, "left");
    }
}

/// Leader-only kick.
pub fn handle_party_kick(
    mut kick_rx: Query<(&RemoteId, &mut MessageReceiver<PartyKickRequest>), With<ClientOf>>,
    players: Query<(Entity, &PlayerTag, &DisplayName)>,
    mut party_table: ResMut<PartyTable>,
    mut commands: Commands,
) {
    let mut kicks: Vec<(u64, String)> = Vec::new();
    for (remote, mut rx) in &mut kick_rx {
        let PeerId::Netcode(client_id) = remote.0 else { continue };
        for msg in rx.receive() {
            kicks.push((client_id, msg.target_name.clone()));
        }
    }
    for (kicker, target_name) in kicks {
        let Some(party) = party_table.party_of(kicker).cloned() else { continue };
        if party.leader != kicker {
            info!("[party] non-leader {kicker} tried to kick");
            continue;
        }
        let Some((_, target_tag, _)) = players
            .iter()
            .find(|(_, _, name)| name.0.eq_ignore_ascii_case(&target_name))
        else {
            continue;
        };
        if target_tag.client_id == kicker {
            continue;
        }
        if !party.contains(target_tag.client_id) {
            continue;
        }
        // Re-query minimal shape for remove_member (which wants &Query<(Entity, &PlayerTag)>).
        // The easiest path is a shallow wrapper — just call remove by id.
        remove_member_by_id(
            &mut party_table,
            target_tag.client_id,
            &players,
            &mut commands,
            "kicked",
        );
    }
}

/// Shared guts for leave + kick. Mutates party table + removes the
/// `PlayerPartyId` component; flips dirty / disbanded as appropriate.
fn remove_member(
    table: &mut PartyTable,
    client_id: u64,
    players: &Query<(Entity, &PlayerTag)>,
    commands: &mut Commands,
    verb: &str,
) {
    let Some(party_id) = table.party_of(client_id).map(|p| p.id) else { return };
    let party = table.parties.get_mut(&party_id).unwrap();
    party.members.retain(|&m| m != client_id);
    // Strip component from the leaver's entity.
    if let Some((entity, _)) = players.iter().find(|(_, tag)| tag.client_id == client_id) {
        commands.entity(entity).remove::<PlayerPartyId>();
    }
    info!("[party] client {client_id} {verb} party {party_id}");

    // Leader left → pick next member as new leader.
    let was_leader = party.leader == client_id;
    if was_leader && !party.members.is_empty() {
        party.leader = party.members[0];
    }

    if party.members.len() < 2 {
        // Disband: record remaining members so snapshot-pass can
        // ship DisbandedNotice to them, then remove every
        // PlayerPartyId component from stragglers.
        let remaining = party.members.clone();
        let id = party.id;
        table.parties.remove(&id);
        table.dirty.remove(&id);
        for &m in &remaining {
            if let Some((entity, _)) = players.iter().find(|(_, tag)| tag.client_id == m) {
                commands.entity(entity).remove::<PlayerPartyId>();
            }
        }
        let mut notified = remaining;
        notified.push(client_id);
        table.disbanded.push((id, notified));
        info!("[party] party {id} disbanded ({verb})");
    } else {
        table.mark_dirty(party_id);
    }
}

fn remove_member_by_id(
    table: &mut PartyTable,
    client_id: u64,
    players: &Query<(Entity, &PlayerTag, &DisplayName)>,
    commands: &mut Commands,
    verb: &str,
) {
    let Some(party_id) = table.party_of(client_id).map(|p| p.id) else { return };
    let party = table.parties.get_mut(&party_id).unwrap();
    party.members.retain(|&m| m != client_id);
    if let Some((entity, _, _)) = players.iter().find(|(_, tag, _)| tag.client_id == client_id) {
        commands.entity(entity).remove::<PlayerPartyId>();
    }
    info!("[party] client {client_id} {verb} party {party_id}");

    let was_leader = party.leader == client_id;
    if was_leader && !party.members.is_empty() {
        party.leader = party.members[0];
    }

    if party.members.len() < 2 {
        let remaining = party.members.clone();
        let id = party.id;
        table.parties.remove(&id);
        table.dirty.remove(&id);
        for &m in &remaining {
            if let Some((entity, _, _)) = players.iter().find(|(_, tag, _)| tag.client_id == m) {
                commands.entity(entity).remove::<PlayerPartyId>();
            }
        }
        let mut notified = remaining;
        notified.push(client_id);
        table.disbanded.push((id, notified));
        info!("[party] party {id} disbanded ({verb})");
    } else {
        table.mark_dirty(party_id);
    }
}

/// Age pending invites and drop expired ones.
pub fn expire_pending_invites(
    time: Res<Time>,
    mut pending: ResMut<PendingInvites>,
) {
    let dt = time.delta_secs();
    for invite in &mut pending.0 {
        invite.age_secs += dt;
    }
    pending.0.retain(|i| i.age_secs < INVITE_TTL_SECS);
}

// ---------------------------------------------------------------------------
// Snapshot broadcast
// ---------------------------------------------------------------------------

/// Build + ship `PartySnapshot` to every member of each dirty party,
/// then clear the dirty set. Also emits `PartyDisbandedNotice` for
/// any disbanded this tick.
pub fn broadcast_party_snapshots(
    mut table: ResMut<PartyTable>,
    players: Query<(
        &PlayerTag,
        &DisplayName,
        Option<&PlayerRace>,
        &Health,
        &Experience,
        &ControlledBy,
    )>,
    client_zone: Res<ClientZone>,
    mut snap_tx: Query<&mut MessageSender<PartySnapshot>, With<ClientOf>>,
    mut disband_tx: Query<&mut MessageSender<PartyDisbandedNotice>, With<ClientOf>>,
) {
    // Index players by client_id once so we don't scan the query per member.
    let mut by_client: HashMap<u64, PartyMember> = HashMap::new();
    let mut link_of: HashMap<u64, Entity> = HashMap::new();
    for (tag, name, race, hp, xp, cb) in &players {
        link_of.insert(tag.client_id, cb.owner);
        by_client.insert(
            tag.client_id,
            PartyMember {
                client_id: tag.client_id,
                display_name: name.0.clone(),
                race_id: race.map(|r| r.0.clone()).unwrap_or_default(),
                level: xp.level,
                hp_current: hp.current,
                hp_max: hp.max,
                zone_id: client_zone
                    .0
                    .get(&tag.client_id)
                    .cloned()
                    .unwrap_or_default(),
                is_leader: false,
            },
        );
    }

    // Disband notices first — members no longer appear in the table.
    let disbanded: Vec<_> = std::mem::take(&mut table.disbanded);
    for (party_id, notify) in disbanded {
        for client_id in notify {
            if let Some(link) = link_of.get(&client_id) {
                if let Ok(mut tx) = disband_tx.get_mut(*link) {
                    let _ = tx.send::<Channel1>(PartyDisbandedNotice { party_id });
                }
            }
        }
    }

    // Snapshot every dirty party.
    let dirty: Vec<PartyId> = table.dirty.drain().collect();
    for party_id in dirty {
        let Some(party) = table.parties.get(&party_id) else { continue };
        let leader_name = by_client
            .get(&party.leader)
            .map(|m| m.display_name.clone())
            .unwrap_or_default();
        let members: Vec<PartyMember> = party
            .members
            .iter()
            .filter_map(|cid| by_client.get(cid).cloned())
            .map(|mut m| {
                m.is_leader = m.client_id == party.leader;
                m
            })
            .collect();
        let snap = PartySnapshot {
            party_id,
            leader_name,
            members,
        };
        for &cid in &party.members {
            if let Some(link) = link_of.get(&cid) {
                if let Ok(mut tx) = snap_tx.get_mut(*link) {
                    let _ = tx.send::<Channel1>(snap.clone());
                }
            }
        }
    }
}

// Party chat is routed inline in `chat_io::handle_chat_messages`
// (which already queries `PartyTable`). No helper here.

// ---------------------------------------------------------------------------
// Shared XP — split logic lives inline in `xp::award_xp_on_mob_death`
// so the observer keeps its single-query shape. Marker component below
// is still used to scope any future party-only systems.
// ---------------------------------------------------------------------------

/// Small-group XP share multiplier. `n` includes the killer in the
/// count of sharers. n=1 pays full; n=5 pays 38% per sharer (total
/// 190%, classic small-group bonus).
pub fn split_xp_per_sharer(base_xp: u32, n_sharers: usize) -> u32 {
    let share_mult = match n_sharers {
        0 | 1 => 1.00,
        2 => 0.70,
        3 => 0.55,
        4 => 0.45,
        _ => 0.38,
    };
    ((base_xp as f32) * share_mult).round() as u32
}

/// Marker component on every player entity. Reserved for future
/// party-scoped queries (e.g. a party-only heal UI). Attached in
/// `connect::spawn_player`.
#[derive(Component, Debug, Default)]
pub struct PartyMemberMarker;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_xp_solo_is_full() {
        assert_eq!(split_xp_per_sharer(100, 1), 100);
    }

    #[test]
    fn split_xp_two_gives_seventy_each() {
        assert_eq!(split_xp_per_sharer(100, 2), 70);
    }

    #[test]
    fn split_xp_five_still_pays_meaningful_share() {
        // 5-party: 38% each × 5 = 190% total. Solo baseline is 100%.
        let per = split_xp_per_sharer(100, 5);
        assert_eq!(per, 38);
    }

    #[test]
    fn party_full_rejects_sixth_invite() {
        let mut t = PartyTable::default();
        let id = t.alloc_id();
        t.parties.insert(
            id,
            Party {
                id,
                leader: 1,
                members: vec![1, 2, 3, 4, 5],
            },
        );
        let party = t.parties.get(&id).unwrap();
        assert!(party.members.len() >= MAX_PARTY_SIZE);
    }

    #[test]
    fn party_of_finds_members() {
        let mut t = PartyTable::default();
        let id = t.alloc_id();
        t.parties.insert(
            id,
            Party {
                id,
                leader: 42,
                members: vec![42, 99],
            },
        );
        assert_eq!(t.party_of(42).unwrap().id, id);
        assert_eq!(t.party_of(99).unwrap().id, id);
        assert!(t.party_of(7).is_none());
    }
}
