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
use vaern_protocol::{
    AbandonQuest, AcceptQuest, PlayerTag, ProgressQuest, QuestLogEntry, QuestLogSnapshot,
};

use crate::data::GameData;
use crate::npc::MobSourceId;
use crate::xp::grant_xp;

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
    mut players: Query<
        (Entity, &ControlledBy, &mut QuestLog, &mut Experience),
        With<PlayerTag>,
    >,
    mut accept_rx: Query<(Entity, &mut MessageReceiver<AcceptQuest>), With<ClientOf>>,
    mut abandon_rx: Query<(Entity, &mut MessageReceiver<AbandonQuest>), With<ClientOf>>,
    mut progress_rx: Query<(Entity, &mut MessageReceiver<ProgressQuest>), With<ClientOf>>,
) {
    enum Action {
        Accept(String),
        Abandon(String),
        Progress(String),
    }
    let link_to_player: HashMap<Entity, Entity> =
        players.iter().map(|(p, cb, _, _)| (cb.owner, p)).collect();

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
        let Ok((_, _, mut log, mut xp)) = players.get_mut(player_e) else { continue };
        match action {
            Action::Accept(chain_id) => {
                let Some(chain) = data.quests.chain(&chain_id) else {
                    println!("[quest] unknown chain '{chain_id}' (accept ignored)");
                    continue;
                };
                if log.entries.contains_key(&chain_id) {
                    continue; // already in log
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

                let step_reward = data
                    .quests
                    .chain(&chain_id)
                    .and_then(|c| c.step(completed_step_idx))
                    .map(|s| s.xp_reward)
                    .unwrap_or(0);
                if step_reward > 0 {
                    grant_xp(player_e, &mut xp, &curve, step_reward, "quest-step");
                }

                if completed {
                    let (title, final_xp) = data
                        .quests
                        .chain(&chain_id)
                        .map(|c| (c.title.as_str(), c.final_reward.xp_bonus))
                        .unwrap_or(("?", 0));
                    if final_xp > 0 {
                        grant_xp(player_e, &mut xp, &curve, final_xp, "quest-complete");
                    }
                    println!(
                        "[quest] player {player_e:?} COMPLETED '{chain_id}' ({title}) — final +{final_xp}xp"
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
    mut players: Query<(Entity, &mut QuestLog, &mut Experience), With<PlayerTag>>,
) {
    let Ok(src) = sources.get(trigger.entity) else { return };
    let dead_mob_id = src.0.clone();

    for (player_e, mut log, mut xp) in &mut players {
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
                grant_xp(player_e, &mut xp, &curve, step_xp, "quest-kill-step");
            }
            if chain_complete {
                let final_xp = chain.final_reward.xp_bonus;
                if final_xp > 0 {
                    grant_xp(player_e, &mut xp, &curve, final_xp, "quest-complete");
                }
                println!("[quest:kill] '{chain_id}' COMPLETED via killing {dead_mob_id}");
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
