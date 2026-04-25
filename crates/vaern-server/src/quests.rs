//! Quest state on the server. `QuestLog` is a per-player component (not
//! replicated); changes are pushed to clients via `QuestLogSnapshot`
//! messages on the tick they're made. Kill objectives auto-advance via
//! an observer that fires when a mob's `MobSourceId` is removed
//! (= mob despawned = dead).

use std::collections::HashMap;

use bevy::prelude::*;
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use vaern_character::{Experience, XpCurve};
use vaern_core::Pillar;
use vaern_data::ItemReward;
use vaern_economy::PlayerWallet;
use vaern_inventory::PlayerInventory;
use vaern_items::{ContentRegistry, ItemInstance};
use vaern_protocol::{
    AbandonQuest, AcceptQuest, PlayerTag, ProgressQuest, QuestLogEntry, QuestLogSnapshot,
};
use vaern_stats::{PillarCaps, PillarScores};

use crate::data::GameData;
use crate::npc::MobSourceId;
use crate::xp::grant_xp_with_levelup_bonus;

/// Hand out a list of `ItemReward` entries to one player. Entries with a
/// `pillar` filter that doesn't match the player's `core_pillar` are
/// skipped silently. Resolution failures (missing base / material /
/// quality, or invalid combination) log + skip; inventory-full overflow
/// logs but the rest of the list keeps trying.
fn grant_item_rewards(
    rewards: &[ItemReward],
    inventory: &mut PlayerInventory,
    registry: &ContentRegistry,
    player_pillar: Pillar,
    player_e: Entity,
    label: &str,
) {
    for r in rewards {
        if let Some(p) = r.pillar
            && p != player_pillar
        {
            continue;
        }
        let instance = match &r.material {
            Some(m) => ItemInstance::new(&r.base, m, &r.quality),
            None => ItemInstance::materialless(&r.base, &r.quality),
        };
        if let Err(e) = registry.resolve(&instance) {
            println!(
                "[quest:reward] {label}: skipping {} ({:?}/{}) for player {player_e:?}: {e}",
                r.base, r.material, r.quality
            );
            continue;
        }
        let leftover = inventory.add(instance, r.count, registry);
        if leftover > 0 {
            println!(
                "[quest:reward] {label}: inventory full, {leftover} of {} didn't fit (player {player_e:?})",
                r.base
            );
        } else {
            println!(
                "[quest:reward] {label}: granted {}× {} to player {player_e:?}",
                r.count, r.base
            );
        }
    }
}

/// Hard refuse a quest accept if its starting level is more than this many
/// above the player's current level. 3 = "you can pick up quests up to 3
/// over your head, but no further" — matches WoW's yellow/orange threshold.
const QUEST_LEVEL_GATE: u32 = 3;

/// Per-player quest state, server-authoritative. Not replicated directly —
/// the owning client gets `QuestLogSnapshot` messages on change. Marked dirty
/// by any system that mutates it; `broadcast_quest_logs` reads and clears.
#[derive(Component, Debug, Default)]
pub struct QuestLog {
    /// chain_id → current_step (0-indexed; `completed` if step >= total).
    pub entries: HashMap<String, QuestLogProgress>,
    pub dirty: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct QuestLogProgress {
    pub current_step: u32,
    pub total_steps: u32,
    pub completed: bool,
}

/// Drain quest-related client messages and mutate the matching player's
/// `QuestLog`. Messages are routed link → player via `ControlledBy.owner`.
/// Each applied change flips `QuestLog.dirty` so `broadcast_quest_logs`
/// ships a fresh snapshot to the owning client on the same tick.
pub fn handle_quest_messages(
    data: Res<GameData>,
    curve: Res<XpCurve>,
    mut players: Query<(
        Entity,
        &ControlledBy,
        &PlayerTag,
        &mut QuestLog,
        &mut Experience,
        &mut PlayerWallet,
        &mut PillarScores,
        &PillarCaps,
        &mut PlayerInventory,
    )>,
    mut accept_rx: Query<(Entity, &mut MessageReceiver<AcceptQuest>), With<ClientOf>>,
    mut abandon_rx: Query<(Entity, &mut MessageReceiver<AbandonQuest>), With<ClientOf>>,
    mut progress_rx: Query<(Entity, &mut MessageReceiver<ProgressQuest>), With<ClientOf>>,
) {
    enum Action {
        Accept(String),
        Abandon(String),
        Progress(String),
    }
    let link_to_player: HashMap<Entity, Entity> = players
        .iter()
        .map(|(p, cb, _, _, _, _, _, _, _)| (cb.owner, p))
        .collect();

    let mut actions: Vec<(Entity, Action)> = Vec::new();
    for (link, mut rx) in &mut accept_rx {
        for msg in rx.receive() {
            actions.push((link, Action::Accept(msg.chain_id.clone())));
        }
    }
    for (link, mut rx) in &mut abandon_rx {
        for msg in rx.receive() {
            actions.push((link, Action::Abandon(msg.chain_id.clone())));
        }
    }
    for (link, mut rx) in &mut progress_rx {
        for msg in rx.receive() {
            actions.push((link, Action::Progress(msg.chain_id.clone())));
        }
    }
    for (link, action) in actions {
        let Some(&player_e) = link_to_player.get(&link) else { continue };
        let Ok((_, _, tag, mut log, mut xp, mut wallet, mut scores, caps, mut inventory)) =
            players.get_mut(player_e)
        else {
            continue;
        };
        let core_pillar = tag.core_pillar;
        match action {
            Action::Accept(chain_id) => {
                let Some(chain) = data.quests.chain(&chain_id) else {
                    println!("[quest] unknown chain '{chain_id}' (accept ignored)");
                    continue;
                };
                if log.entries.contains_key(&chain_id) {
                    continue; // already in log
                }
                // Level gate: refuse if the chain's starting step is more than
                // QUEST_LEVEL_GATE levels above the player's current level. Soft
                // failure: log + skip; client will simply not see the entry
                // appear in their quest log, same as an unknown-chain reject.
                let chain_level = chain.steps.first().map(|s| s.level).unwrap_or(1);
                if chain_level > xp.level.saturating_add(QUEST_LEVEL_GATE) {
                    println!(
                        "[quest] player {player_e:?} L{} refused '{}' (chain starts at L{}, gate=+{})",
                        xp.level, chain_id, chain_level, QUEST_LEVEL_GATE
                    );
                    continue;
                }
                // Convention: step 1 is "meet / talk to the giver". Accepting
                // the quest from that giver IS completing step 1, so skip
                // it. Non-talk step 1s (investigate / kill / etc) count
                // their normal content — don't auto-skip those.
                let initial_step = if chain
                    .steps
                    .first()
                    .map(|s| s.objective.kind == "talk")
                    .unwrap_or(false)
                {
                    1
                } else {
                    0
                };
                log.entries.insert(
                    chain_id.clone(),
                    QuestLogProgress {
                        current_step: initial_step,
                        total_steps: chain.total_steps,
                        completed: false,
                    },
                );
                log.dirty = true;
                println!(
                    "[quest] player {player_e:?} accepted '{}' ({}): starting at step {}/{}",
                    chain_id, chain.title, initial_step, chain.total_steps
                );
            }
            Action::Abandon(chain_id) => {
                if log.entries.remove(&chain_id).is_some() {
                    log.dirty = true;
                    println!("[quest] player {player_e:?} abandoned '{chain_id}'");
                }
            }
            Action::Progress(chain_id) => {
                let (completed_step_idx, current_step_after, total_steps, completed) = {
                    let Some(entry) = log.entries.get_mut(&chain_id) else { continue };
                    if entry.completed {
                        continue;
                    }
                    let completed_idx = entry.current_step;
                    entry.current_step += 1;
                    let chain_done = entry.current_step >= entry.total_steps;
                    if chain_done {
                        entry.current_step = entry.total_steps;
                        entry.completed = true;
                    }
                    (completed_idx, entry.current_step, entry.total_steps, chain_done)
                };
                log.dirty = true;

                let step = data
                    .quests
                    .chain(&chain_id)
                    .and_then(|c| c.step(completed_step_idx));
                let step_reward = step.map(|s| s.xp_reward).unwrap_or(0);
                let step_gold = step.map(|s| s.gold_reward_copper).unwrap_or(0);
                let step_items: Vec<ItemReward> =
                    step.map(|s| s.item_reward.clone()).unwrap_or_default();
                if step_reward > 0 {
                    grant_xp_with_levelup_bonus(
                        player_e, &mut xp, &mut scores, caps, &curve, step_reward, "quest-step",
                    );
                }
                if step_gold > 0 {
                    wallet.credit(step_gold as u64);
                    println!(
                        "[quest] player {player_e:?} +{step_gold}c (step reward, wallet={})",
                        wallet.copper
                    );
                }
                if !step_items.is_empty() {
                    grant_item_rewards(
                        &step_items,
                        &mut inventory,
                        &data.content,
                        core_pillar,
                        player_e,
                        "quest-step",
                    );
                }

                if completed {
                    let chain_ref = data.quests.chain(&chain_id);
                    let title = chain_ref.map(|c| c.title.as_str()).unwrap_or("?");
                    let final_xp = chain_ref.map(|c| c.final_reward.xp_bonus).unwrap_or(0);
                    let final_gold = chain_ref
                        .map(|c| c.final_reward.gold_bonus_copper)
                        .unwrap_or(0);
                    let final_items: Vec<ItemReward> = chain_ref
                        .map(|c| c.final_reward.item_reward.clone())
                        .unwrap_or_default();
                    if final_xp > 0 {
                        grant_xp_with_levelup_bonus(
                            player_e, &mut xp, &mut scores, caps, &curve, final_xp,
                            "quest-complete",
                        );
                    }
                    if final_gold > 0 {
                        wallet.credit(final_gold as u64);
                    }
                    if !final_items.is_empty() {
                        grant_item_rewards(
                            &final_items,
                            &mut inventory,
                            &data.content,
                            core_pillar,
                            player_e,
                            "quest-complete",
                        );
                    }
                    println!(
                        "[quest] player {player_e:?} COMPLETED '{chain_id}' ({title}) — final +{final_xp}xp +{final_gold}c"
                    );
                } else {
                    println!(
                        "[quest] player {player_e:?} progressed '{chain_id}' to step {current_step_after}/{total_steps}"
                    );
                }
            }
        }
    }
}

/// Observer: when a mob with `MobSourceId` is despawned (= died), scan every
/// player's quest log for a kill-objective at their current step that
/// targets this mob id, and advance the step if it matches. Fires BEFORE
/// the entity is fully removed, so the component's value is still readable.
pub fn apply_kill_objectives(
    trigger: On<Remove, MobSourceId>,
    sources: Query<&MobSourceId>,
    data: Res<GameData>,
    curve: Res<XpCurve>,
    mut players: Query<(
        Entity,
        &PlayerTag,
        &mut QuestLog,
        &mut Experience,
        &mut PlayerWallet,
        &mut PillarScores,
        &PillarCaps,
        &mut PlayerInventory,
    )>,
) {
    let Ok(src) = sources.get(trigger.entity) else { return };
    let dead_mob_id = src.0.clone();

    for (player_e, tag, mut log, mut xp, mut wallet, mut scores, caps, mut inventory) in
        &mut players
    {
        let core_pillar = tag.core_pillar;
        let chain_ids: Vec<String> = log.entries.keys().cloned().collect();
        for chain_id in chain_ids {
            let Some(chain) = data.quests.chain(&chain_id) else { continue };
            let entry = match log.entries.get(&chain_id) {
                Some(e) if !e.completed => *e,
                _ => continue,
            };
            let Some(step) = chain.step(entry.current_step) else { continue };
            if step.objective.kind != "kill" {
                continue;
            }
            let Some(target) = step.objective.mob_id.as_deref() else { continue };
            if target != dead_mob_id {
                continue;
            }

            // Match — advance the step. (Count > 1 isn't tracked per-kill
            // yet; most chains use count=1. When we need multi-kill steps,
            // accumulate per-step kill_count in QuestLogProgress.)
            let step_xp = step.xp_reward;
            let step_gold = step.gold_reward_copper;
            let step_items = step.item_reward.clone();
            let (current_after, total_after, chain_complete) = {
                let entry_mut = log.entries.get_mut(&chain_id).unwrap();
                entry_mut.current_step += 1;
                let done = entry_mut.current_step >= entry_mut.total_steps;
                if done {
                    entry_mut.current_step = entry_mut.total_steps;
                    entry_mut.completed = true;
                }
                (entry_mut.current_step, entry_mut.total_steps, done)
            };
            log.dirty = true;

            if step_xp > 0 {
                grant_xp_with_levelup_bonus(
                    player_e, &mut xp, &mut scores, caps, &curve, step_xp, "quest-kill-step",
                );
            }
            if step_gold > 0 {
                wallet.credit(step_gold as u64);
            }
            if !step_items.is_empty() {
                grant_item_rewards(
                    &step_items,
                    &mut inventory,
                    &data.content,
                    core_pillar,
                    player_e,
                    "quest-kill-step",
                );
            }
            if chain_complete {
                let final_xp = chain.final_reward.xp_bonus;
                let final_gold = chain.final_reward.gold_bonus_copper;
                let final_items = chain.final_reward.item_reward.clone();
                if final_xp > 0 {
                    grant_xp_with_levelup_bonus(
                        player_e, &mut xp, &mut scores, caps, &curve, final_xp, "quest-complete",
                    );
                }
                if final_gold > 0 {
                    wallet.credit(final_gold as u64);
                }
                if !final_items.is_empty() {
                    grant_item_rewards(
                        &final_items,
                        &mut inventory,
                        &data.content,
                        core_pillar,
                        player_e,
                        "quest-complete-kill",
                    );
                }
                println!(
                    "[quest:kill] '{chain_id}' COMPLETED via killing {dead_mob_id} (+{final_gold}c final)"
                );
            } else {
                println!(
                    "[quest:kill] '{chain_id}' advanced to step {current_after}/{total_after} via kill of {dead_mob_id}"
                );
            }
        }
    }
}

/// Ship a QuestLogSnapshot to each player whose log was dirtied this tick.
pub fn broadcast_quest_logs(
    mut players: Query<(&ControlledBy, &mut QuestLog), With<PlayerTag>>,
    mut senders: Query<&mut MessageSender<QuestLogSnapshot>, With<ClientOf>>,
) {
    for (cb, mut log) in &mut players {
        if !log.dirty {
            continue;
        }
        let Ok(mut sender) = senders.get_mut(cb.owner) else { continue };
        let entries: Vec<QuestLogEntry> = log
            .entries
            .iter()
            .map(|(id, p)| QuestLogEntry {
                chain_id: id.clone(),
                current_step: p.current_step,
                total_steps: p.total_steps,
                completed: p.completed,
            })
            .collect();
        let _ = sender.send::<vaern_protocol::Channel1>(QuestLogSnapshot { entries });
        log.dirty = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn workspace_data_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../src/generated")
            .canonicalize()
            .expect("workspace src/generated must exist")
    }

    fn registry() -> ContentRegistry {
        let mut reg = ContentRegistry::new();
        reg.load_tree(workspace_data_root().join("items"))
            .expect("load content registry");
        reg
    }

    fn chains() -> vaern_data::QuestIndex {
        vaern_data::load_all_chains(workspace_data_root().join("world"))
            .expect("load quest chains")
    }

    /// Every `ItemReward` (step + final_reward) in every chain must resolve
    /// against the live content registry. Catches typos in base / material /
    /// quality ids before they hit the server.
    #[test]
    fn every_quest_item_reward_resolves() {
        let reg = registry();
        let idx = chains();
        let mut checked = 0usize;
        for (chain_id, chain) in &idx.chains {
            for step in &chain.steps {
                for r in &step.item_reward {
                    let inst = match &r.material {
                        Some(m) => ItemInstance::new(&r.base, m, &r.quality),
                        None => ItemInstance::materialless(&r.base, &r.quality),
                    };
                    reg.resolve(&inst).unwrap_or_else(|e| {
                        panic!(
                            "chain '{chain_id}' step '{}' item_reward {:?} failed to resolve: {e}",
                            step.id, r,
                        )
                    });
                    checked += 1;
                }
            }
            for r in &chain.final_reward.item_reward {
                let inst = match &r.material {
                    Some(m) => ItemInstance::new(&r.base, m, &r.quality),
                    None => ItemInstance::materialless(&r.base, &r.quality),
                };
                reg.resolve(&inst).unwrap_or_else(|e| {
                    panic!(
                        "chain '{chain_id}' final_reward item_reward {:?} failed to resolve: {e}",
                        r,
                    )
                });
                checked += 1;
            }
        }
        // Sanity: at least the Dalewatch ladder should be authored. If
        // someone removes it without replacement, this guards against a
        // silent regression.
        assert!(
            checked >= 30,
            "expected at least 30 ItemReward entries across all chains, got {checked}"
        );
    }

    /// The Dalewatch first-ride chain specifically should ship its 5-tier
    /// ladder. If the chain id is renamed or rewards are stripped, fail
    /// loudly rather than silently dropping the felt-progression slice.
    #[test]
    fn dalewatch_first_ride_has_5_tier_ladder() {
        let idx = chains();
        let chain = idx
            .chain("chain_dalewatch_first_ride")
            .expect("chain_dalewatch_first_ride must exist");
        // Steps with rewards: 4, 6, 7, 8 + final_reward = 5 reward tiers.
        let steps_with_items: Vec<u32> = chain
            .steps
            .iter()
            .filter(|s| !s.item_reward.is_empty())
            .map(|s| s.step)
            .collect();
        assert_eq!(
            steps_with_items,
            vec![4, 6, 7, 8],
            "expected item_reward on steps 4, 6, 7, 8"
        );
        assert!(
            !chain.final_reward.item_reward.is_empty(),
            "final_reward must hand out the capstone outfit"
        );

        // Each step's per-pillar entries must cover all three pillars (no
        // pillar should silently miss out on a ladder tier).
        for step in chain
            .steps
            .iter()
            .filter(|s| !s.item_reward.is_empty())
        {
            let pillars: std::collections::HashSet<_> = step
                .item_reward
                .iter()
                .filter_map(|r| r.pillar)
                .collect();
            assert_eq!(
                pillars.len(),
                3,
                "step {} ({}) must reward all three pillars, got {:?}",
                step.step,
                step.id,
                pillars
            );
        }
    }
}
