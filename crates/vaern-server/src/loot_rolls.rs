//! Slice 6 — shared Need-Before-Greed-Pass loot rolls.
//!
//! Boss-tier kills (`NpcKind::Named`) with 2+ party members within
//! `PARTY_SHARE_RADIUS=40u` of the kill site spawn a `LootRollContainer`
//! instead of the single-owner `LootContainer`. Every eligible client
//! gets a `LootRollOpen` modal listing the boss drops; per-item votes
//! resolve via `decide_roll_winner`. Solo kills bypass the roll layer
//! entirely (`loot_io::spawn_loot_container_on_mob_death` keeps the
//! single-owner branch unchanged).
//!
//! Open Need (user decision): the resolver does not pillar-gate Need
//! votes — any eligible member can cast Need on any item. Pillar tags
//! on `ItemReward` entries inside the boss-drop YAML are advisory only.

use std::collections::HashMap;

use bevy::log::info;
use bevy::prelude::*;
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use rand::Rng;

use vaern_combat::components::DisplayName;
use vaern_inventory::{InventorySlot, PlayerInventory};
use vaern_protocol::{
    Channel1, LootId, LootRollItem, LootRollOpen, LootRollResult, LootRollVote, PlayerTag,
    RollVote,
};

use crate::data::GameData;
use crate::loot_io::LootRng;

/// Default per-roll deadline. Server settles unvoted items at this
/// point regardless of how many ballots have come in.
pub const ROLL_EXPIRES_SECS: f32 = 60.0;

/// Hard upper bound on container age — even after rolls settle, the
/// container despawns this long after spawn (matches the single-owner
/// `LOOT_DESPAWN_SECS` budget).
pub const ROLL_CONTAINER_DESPAWN_SECS: f32 = 300.0;

/// Server-only roll container. Spawned in lieu of `LootContainer` when
/// a boss kill has eligible party members. NOT replicated; clients
/// receive a `LootRollOpen` push at spawn and `LootRollResult`
/// per-item as votes settle.
#[derive(Component, Debug)]
pub struct LootRollContainer {
    pub loot_id: LootId,
    pub eligible: Vec<u64>,
    pub items: Vec<RollItemState>,
    pub age_secs: f32,
    pub expires_at_secs: f32,
}

#[derive(Debug, Clone)]
pub struct RollItemState {
    pub item: InventorySlot,
    pub votes: HashMap<u64, RollVote>,
    pub settled: bool,
}

impl RollItemState {
    pub fn new(item: InventorySlot) -> Self {
        Self {
            item,
            votes: HashMap::new(),
            settled: false,
        }
    }

    /// True iff every eligible voter has cast a ballot on this item.
    pub fn all_voted(&self, eligible: &[u64]) -> bool {
        eligible.iter().all(|c| self.votes.contains_key(c))
    }
}

/// Outcome of a single item's roll.
///
/// `roll_value` is `255` when the winning vote-kind tier had a single
/// uncontested vote (no d100 needed); `0` when there is no winner
/// (all-Pass or expired with zero votes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RollOutcome {
    pub winner: Option<u64>,
    pub vote_kind: RollVote,
    pub roll_value: u8,
}

impl RollOutcome {
    pub fn no_winner() -> Self {
        Self {
            winner: None,
            vote_kind: RollVote::Pass,
            roll_value: 0,
        }
    }
}

/// Pure helper — picks the Need-Before-Greed-Pass winner from a vote map.
///
/// Resolution order:
/// 1. Any `Need` votes → highest d100 wins. Single Need = uncontested (`roll_value = 255`).
/// 2. No Needs, any `Greed` → highest d100 wins. Single Greed = uncontested.
/// 3. All `Pass` (or empty) → no winner.
///
/// `rng` is only consulted when there's a contested tier (≥2 voters at
/// the winning tier). Open Need: caller does not pre-filter votes by
/// pillar — that's a downstream user decision.
pub fn decide_roll_winner<R: Rng>(votes: &HashMap<u64, RollVote>, rng: &mut R) -> RollOutcome {
    let needers: Vec<u64> = votes
        .iter()
        .filter_map(|(c, v)| (*v == RollVote::Need).then_some(*c))
        .collect();
    if !needers.is_empty() {
        return resolve_tier(&needers, RollVote::Need, rng);
    }
    let greeders: Vec<u64> = votes
        .iter()
        .filter_map(|(c, v)| (*v == RollVote::Greed).then_some(*c))
        .collect();
    if !greeders.is_empty() {
        return resolve_tier(&greeders, RollVote::Greed, rng);
    }
    RollOutcome::no_winner()
}

fn resolve_tier<R: Rng>(candidates: &[u64], kind: RollVote, rng: &mut R) -> RollOutcome {
    if candidates.len() == 1 {
        return RollOutcome {
            winner: Some(candidates[0]),
            vote_kind: kind,
            roll_value: 255,
        };
    }
    // Stable iteration order on the candidate list — sort by client_id
    // so the highest-roll lookup is deterministic against the rng draw
    // order. `rng.random_range(1..=100)` per candidate.
    let mut sorted = candidates.to_vec();
    sorted.sort_unstable();
    let mut best: (u64, u8) = (sorted[0], 0);
    for c in &sorted {
        let r: u8 = rng.random_range(1..=100);
        if r > best.1 {
            best = (*c, r);
        }
    }
    RollOutcome {
        winner: Some(best.0),
        vote_kind: kind,
        roll_value: best.1,
    }
}

/// Tick container ages; settle any item whose deadline has elapsed;
/// despawn containers whose items are all settled past the despawn
/// budget.
pub fn tick_roll_containers(
    time: Res<Time>,
    mut containers: Query<(Entity, &mut LootRollContainer)>,
    mut players: Query<(&PlayerTag, &DisplayName, &mut PlayerInventory, &ControlledBy)>,
    mut rng: ResMut<LootRng>,
    mut result_tx: Query<&mut MessageSender<LootRollResult>, With<ClientOf>>,
    data: Res<GameData>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();
    for (e, mut c) in &mut containers {
        c.age_secs += dt;

        // Settle expired items.
        if c.age_secs >= c.expires_at_secs {
            let mut to_settle: Vec<usize> = Vec::new();
            for (i, item) in c.items.iter().enumerate() {
                if !item.settled {
                    to_settle.push(i);
                }
            }
            for i in to_settle {
                settle_item(&mut c, i, &mut rng.0, &mut players, &mut result_tx, &data);
            }
        }

        // Hard cap: despawn everything past the despawn budget OR when
        // every item has settled.
        let all_settled = c.items.iter().all(|s| s.settled);
        let timed_out = c.age_secs >= ROLL_CONTAINER_DESPAWN_SECS;
        if all_settled || timed_out {
            commands.entity(e).despawn();
        }
    }
}

/// Drain `LootRollVote` messages → record into the matching item's
/// vote map → settle if every eligible voter has voted on it.
pub fn handle_loot_roll_votes(
    mut links: Query<(&RemoteId, &mut MessageReceiver<LootRollVote>), With<ClientOf>>,
    mut containers: Query<(Entity, &mut LootRollContainer)>,
    mut players: Query<(&PlayerTag, &DisplayName, &mut PlayerInventory, &ControlledBy)>,
    mut rng: ResMut<LootRng>,
    mut result_tx: Query<&mut MessageSender<LootRollResult>, With<ClientOf>>,
    data: Res<GameData>,
) {
    let mut votes: Vec<(u64, LootRollVote)> = Vec::new();
    for (remote, mut rx) in &mut links {
        let PeerId::Netcode(client_id) = remote.0 else {
            continue;
        };
        for v in rx.receive() {
            votes.push((client_id, v));
        }
    }

    for (client_id, vote) in votes {
        let Some((_, mut container)) = containers
            .iter_mut()
            .find(|(_, c)| c.loot_id == vote.loot_id)
        else {
            continue;
        };
        if !container.eligible.contains(&client_id) {
            continue;
        }
        let idx = vote.item_index as usize;
        let eligible_snapshot: Vec<u64> = container.eligible.clone();
        let Some(item) = container.items.get_mut(idx) else {
            continue;
        };
        if item.settled {
            continue;
        }
        // First vote per (client, item) sticks — explicit re-vote is
        // a no-op (matches classic Need-Before-Greed UX).
        if item.votes.contains_key(&client_id) {
            continue;
        }
        item.votes.insert(client_id, vote.vote);
        let triggered = item.all_voted(&eligible_snapshot);

        if triggered {
            settle_item(
                &mut container,
                idx,
                &mut rng.0,
                &mut players,
                &mut result_tx,
                &data,
            );
        }
    }
}

/// Settle a single item: pick the winner (or none), credit their
/// inventory if applicable, broadcast `LootRollResult` to every
/// eligible voter, mark settled.
fn settle_item<R: Rng>(
    container: &mut LootRollContainer,
    item_idx: usize,
    rng: &mut R,
    players: &mut Query<(&PlayerTag, &DisplayName, &mut PlayerInventory, &ControlledBy)>,
    result_tx: &mut Query<&mut MessageSender<LootRollResult>, With<ClientOf>>,
    data: &GameData,
) {
    let Some(item) = container.items.get_mut(item_idx) else {
        return;
    };
    if item.settled {
        return;
    }
    let outcome = decide_roll_winner(&item.votes, rng);
    let mut winner_name = String::new();
    if let Some(client_id) = outcome.winner {
        for (tag, dname, mut inv, _cb) in players.iter_mut() {
            if tag.client_id == client_id {
                let leftover = inv.add(
                    item.item.instance.clone(),
                    item.item.count,
                    &data.content,
                );
                if leftover > 0 {
                    info!(
                        "[loot:roll] inventory full for {dname:?}, {leftover} of {} didn't fit",
                        item.item.instance.base_id,
                        dname = dname.0
                    );
                }
                winner_name = dname.0.clone();
                break;
            }
        }
    }
    item.settled = true;

    // Broadcast result to every eligible voter (winners + losers + observers).
    let result = LootRollResult {
        loot_id: container.loot_id,
        item_index: item_idx as u32,
        winner: winner_name.clone(),
        vote_kind: outcome.vote_kind,
        roll_value: outcome.roll_value,
    };
    for (tag, _, _, cb) in players.iter() {
        if !container.eligible.contains(&tag.client_id) {
            continue;
        }
        if let Ok(mut tx) = result_tx.get_mut(cb.owner) {
            let _ = tx.send::<Channel1>(result.clone());
        }
    }
    info!(
        "[loot:roll] container #{lid} item {idx} → winner {w:?} ({kind:?}, roll {roll})",
        lid = container.loot_id,
        idx = item_idx,
        w = winner_name,
        kind = outcome.vote_kind,
        roll = outcome.roll_value,
    );
}

/// Pure helper — enumerate every party member of `killer_client_id` whose
/// position is within `radius` of `kill_pos`. Includes the killer
/// themselves when they're in their own party. Mirrors the share logic
/// in `xp::award_xp_on_mob_death`. Equivalent to the inline pass in
/// `loot_io::spawn_loot_container_on_mob_death`; kept as a free function
/// so the eligibility math can be unit-tested without spinning up a
/// Bevy world.
#[allow(dead_code)]
pub fn eligible_for_roll(
    killer_client_id: u64,
    kill_pos: Vec3,
    radius: f32,
    party_table: &crate::party_io::PartyTable,
    party_member_positions: &[(u64, Vec3)],
) -> Vec<u64> {
    let Some(party) = party_table.party_of(killer_client_id) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (cid, pos) in party_member_positions {
        if !party.members.contains(cid) {
            continue;
        }
        if (*pos - kill_pos).length() <= radius {
            out.push(*cid);
        }
    }
    out
}

/// Build a `LootRollOpen` payload for a freshly spawned roll container.
pub fn build_roll_open_payload(container: &LootRollContainer, eligible_names: Vec<String>) -> LootRollOpen {
    let items = container
        .items
        .iter()
        .enumerate()
        .map(|(i, s)| LootRollItem {
            item_index: i as u32,
            instance: s.item.instance.clone(),
            count: s.item.count,
        })
        .collect();
    LootRollOpen {
        loot_id: container.loot_id,
        items,
        eligible: eligible_names,
        expires_in_secs: container.expires_at_secs.ceil() as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    fn votes(pairs: &[(u64, RollVote)]) -> HashMap<u64, RollVote> {
        pairs.iter().copied().collect()
    }

    #[test]
    fn no_winner_when_all_pass() {
        let mut rng = StdRng::seed_from_u64(1);
        let v = votes(&[(1, RollVote::Pass), (2, RollVote::Pass)]);
        assert_eq!(decide_roll_winner(&v, &mut rng), RollOutcome::no_winner());
    }

    #[test]
    fn no_winner_when_empty() {
        let mut rng = StdRng::seed_from_u64(1);
        let v = votes(&[]);
        assert_eq!(decide_roll_winner(&v, &mut rng), RollOutcome::no_winner());
    }

    #[test]
    fn single_need_auto_wins() {
        let mut rng = StdRng::seed_from_u64(1);
        let v = votes(&[(7, RollVote::Need), (8, RollVote::Greed), (9, RollVote::Pass)]);
        let outcome = decide_roll_winner(&v, &mut rng);
        assert_eq!(outcome.winner, Some(7));
        assert_eq!(outcome.vote_kind, RollVote::Need);
        assert_eq!(outcome.roll_value, 255);
    }

    #[test]
    fn single_greed_auto_wins_when_no_need() {
        let mut rng = StdRng::seed_from_u64(1);
        let v = votes(&[(8, RollVote::Greed), (9, RollVote::Pass)]);
        let outcome = decide_roll_winner(&v, &mut rng);
        assert_eq!(outcome.winner, Some(8));
        assert_eq!(outcome.vote_kind, RollVote::Greed);
        assert_eq!(outcome.roll_value, 255);
    }

    #[test]
    fn need_beats_greed() {
        let mut rng = StdRng::seed_from_u64(42);
        let v = votes(&[
            (1, RollVote::Greed),
            (2, RollVote::Greed),
            (3, RollVote::Need),
        ]);
        let outcome = decide_roll_winner(&v, &mut rng);
        assert_eq!(outcome.winner, Some(3));
        assert_eq!(outcome.vote_kind, RollVote::Need);
    }

    #[test]
    fn tied_need_resolves_via_d100() {
        let mut rng = StdRng::seed_from_u64(42);
        let v = votes(&[(1, RollVote::Need), (2, RollVote::Need)]);
        let outcome = decide_roll_winner(&v, &mut rng);
        assert!(matches!(outcome.winner, Some(1) | Some(2)));
        assert_eq!(outcome.vote_kind, RollVote::Need);
        assert!(outcome.roll_value >= 1 && outcome.roll_value <= 100);
    }

    #[test]
    fn tied_greed_resolves_via_d100_when_no_need() {
        let mut rng = StdRng::seed_from_u64(7);
        let v = votes(&[
            (10, RollVote::Greed),
            (20, RollVote::Greed),
            (30, RollVote::Pass),
        ]);
        let outcome = decide_roll_winner(&v, &mut rng);
        assert!(matches!(outcome.winner, Some(10) | Some(20)));
        assert_eq!(outcome.vote_kind, RollVote::Greed);
        assert!(outcome.roll_value >= 1 && outcome.roll_value <= 100);
    }

    fn slot_for_test() -> InventorySlot {
        InventorySlot {
            instance: vaern_items::ItemInstance::materialless("plate_breastplate", "regular"),
            count: 1,
        }
    }

    #[test]
    fn item_state_all_voted_tracks_eligible() {
        let mut s = RollItemState::new(slot_for_test());
        let eligible = vec![1u64, 2, 3];
        assert!(!s.all_voted(&eligible));
        s.votes.insert(1, RollVote::Need);
        s.votes.insert(2, RollVote::Greed);
        assert!(!s.all_voted(&eligible));
        s.votes.insert(3, RollVote::Pass);
        assert!(s.all_voted(&eligible));
    }

    fn party_table_with(party_id: u64, members: &[u64]) -> crate::party_io::PartyTable {
        use crate::party_io::{Party, PartyTable};
        let mut t = PartyTable::default();
        t.parties.insert(
            party_id,
            Party {
                id: party_id,
                leader: members[0],
                members: members.to_vec(),
            },
        );
        t
    }

    #[test]
    fn eligible_for_roll_includes_killer_and_in_radius_partners() {
        let table = party_table_with(1, &[10, 20, 30]);
        let kill_pos = Vec3::new(100.0, 0.0, 100.0);
        let positions = vec![
            (10, kill_pos),                                // killer (in radius)
            (20, kill_pos + Vec3::new(20.0, 0.0, 0.0)),    // partner 20u away
            (30, kill_pos + Vec3::new(50.0, 0.0, 0.0)),    // partner 50u away (out)
        ];
        let elig = eligible_for_roll(10, kill_pos, 40.0, &table, &positions);
        assert_eq!(elig.len(), 2);
        assert!(elig.contains(&10));
        assert!(elig.contains(&20));
        assert!(!elig.contains(&30));
    }

    #[test]
    fn eligible_for_roll_returns_empty_when_killer_not_in_party() {
        let table = party_table_with(1, &[20, 30]); // 10 not in party
        let kill_pos = Vec3::ZERO;
        let positions = vec![(10, kill_pos), (20, kill_pos), (30, kill_pos)];
        let elig = eligible_for_roll(10, kill_pos, 40.0, &table, &positions);
        assert!(elig.is_empty());
    }

    #[test]
    fn eligible_for_roll_excludes_non_party_players_in_radius() {
        let table = party_table_with(1, &[10, 20]);
        let kill_pos = Vec3::ZERO;
        let positions = vec![
            (10, kill_pos),
            (20, kill_pos),
            (99, kill_pos), // in radius but not in party
        ];
        let elig = eligible_for_roll(10, kill_pos, 40.0, &table, &positions);
        assert_eq!(elig.len(), 2);
        assert!(!elig.contains(&99));
    }
}
