//! Slice 6 — shared loot-roll modal.
//!
//! When the server broadcasts `LootRollOpen` (boss kill with party
//! members in radius), every eligible client pops a centered egui
//! modal listing the boss drops with `[Need]` `[Greed]` `[Pass]`
//! buttons. Per-item votes ship as `LootRollVote`; the server settles
//! and broadcasts `LootRollResult` to every voter (winners + losers
//! + observers). Resolved items show the winner name + roll value
//! inline for ~3 seconds before fading.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPrimaryContextPass};
use lightyear::prelude::client::Client;
use lightyear::prelude::*;

use vaern_protocol::{
    Channel1, LootId, LootRollItem, LootRollOpen, LootRollResult, LootRollVote, RollVote,
};

use crate::inventory_ui::ClientContent;
use crate::item_icons::rarity_color;
use crate::menu::AppState;

/// Default seconds to keep a settled item visible before pruning
/// from the modal. Matches the plan's "result ~3s" toast budget.
const SETTLED_LINGER_SECS: f32 = 3.0;

/// One per `LootRollOpen` broadcast. Holds per-item local UI state —
/// the player's vote choice (if any), settled outcome (if delivered),
/// and a countdown toward expiry.
#[derive(Debug, Clone)]
pub struct RollSession {
    pub loot_id: LootId,
    pub items: Vec<RollSessionItem>,
    pub eligible_names: Vec<String>,
    pub remaining_secs: f32,
}

#[derive(Debug, Clone)]
pub struct RollSessionItem {
    pub item_index: u32,
    pub instance: vaern_items::ItemInstance,
    pub count: u32,
    /// Local vote — `Some(_)` once the player has clicked Need/Greed/
    /// Pass. Buttons disable after the first click (matches the
    /// "first vote sticks" server semantics).
    pub local_vote: Option<RollVote>,
    /// Server-confirmed settlement. While `None`, buttons are live.
    pub outcome: Option<RollItemOutcome>,
    /// Linger countdown after settlement so the result is readable
    /// before the row prunes.
    pub linger_secs: f32,
}

#[derive(Debug, Clone)]
pub struct RollItemOutcome {
    pub winner: String,
    pub vote_kind: RollVote,
    pub roll_value: u8,
}

#[derive(Resource, Default, Debug)]
pub struct ActiveRolls {
    pub sessions: Vec<RollSession>,
}

pub struct RollWindowPlugin;

impl Plugin for RollWindowPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActiveRolls>()
            .add_systems(
                Update,
                (
                    ingest_loot_roll_open,
                    ingest_loot_roll_result,
                    tick_roll_sessions,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                EguiPrimaryContextPass,
                draw_roll_modal.run_if(in_state(AppState::InGame)),
            );
    }
}

fn ingest_loot_roll_open(
    mut rx: Query<&mut MessageReceiver<LootRollOpen>, With<Client>>,
    mut active: ResMut<ActiveRolls>,
) {
    for mut receiver in &mut rx {
        for msg in receiver.receive() {
            // Replace any prior session for the same loot_id (defensive
            // — server should never re-open, but stale state from a
            // dropped reconnect would otherwise duplicate).
            active.sessions.retain(|s| s.loot_id != msg.loot_id);
            active.sessions.push(RollSession {
                loot_id: msg.loot_id,
                items: msg
                    .items
                    .into_iter()
                    .map(
                        |LootRollItem {
                             item_index,
                             instance,
                             count,
                         }| RollSessionItem {
                            item_index,
                            instance,
                            count,
                            local_vote: None,
                            outcome: None,
                            linger_secs: 0.0,
                        },
                    )
                    .collect(),
                eligible_names: msg.eligible,
                remaining_secs: msg.expires_in_secs as f32,
            });
        }
    }
}

fn ingest_loot_roll_result(
    mut rx: Query<&mut MessageReceiver<LootRollResult>, With<Client>>,
    mut active: ResMut<ActiveRolls>,
) {
    for mut receiver in &mut rx {
        for msg in receiver.receive() {
            let Some(session) = active
                .sessions
                .iter_mut()
                .find(|s| s.loot_id == msg.loot_id)
            else {
                continue;
            };
            let Some(item) = session
                .items
                .iter_mut()
                .find(|i| i.item_index == msg.item_index)
            else {
                continue;
            };
            item.outcome = Some(RollItemOutcome {
                winner: msg.winner,
                vote_kind: msg.vote_kind,
                roll_value: msg.roll_value,
            });
            item.linger_secs = SETTLED_LINGER_SECS;
        }
    }
}

fn tick_roll_sessions(time: Res<Time>, mut active: ResMut<ActiveRolls>) {
    let dt = time.delta_secs();
    for session in &mut active.sessions {
        if session.remaining_secs > 0.0 {
            session.remaining_secs = (session.remaining_secs - dt).max(0.0);
        }
        for item in &mut session.items {
            if item.outcome.is_some() && item.linger_secs > 0.0 {
                item.linger_secs = (item.linger_secs - dt).max(0.0);
            }
        }
        // Drop fully-settled-and-lingered items so the modal eventually
        // empties out.
        session
            .items
            .retain(|i| i.outcome.is_none() || i.linger_secs > 0.0);
    }
    // Drop sessions with zero items left.
    active.sessions.retain(|s| !s.items.is_empty());
}

fn draw_roll_modal(
    mut contexts: EguiContexts,
    mut active: ResMut<ActiveRolls>,
    content: Option<Res<ClientContent>>,
    mut tx: Query<&mut MessageSender<LootRollVote>, With<Client>>,
) {
    if active.sessions.is_empty() {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else { return };
    let Some(content) = content else { return };

    let mut votes_to_send: Vec<LootRollVote> = Vec::new();

    for session in active.sessions.iter_mut() {
        let title = format!("Loot Roll — Container #{}", session.loot_id);
        egui::Window::new(title)
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 60.0))
            .collapsible(false)
            .resizable(false)
            .default_size(egui::vec2(440.0, 260.0))
            .show(ctx, |ui| {
                let countdown = session.remaining_secs.ceil() as u32;
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("Voters: {}", session.eligible_names.join(", ")))
                            .size(11.0)
                            .color(egui::Color32::from_gray(180)),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(format!("⏱ {}s", countdown))
                                .color(if countdown <= 10 {
                                    egui::Color32::from_rgb(220, 80, 80)
                                } else {
                                    egui::Color32::from_gray(200)
                                }),
                        );
                    });
                });
                ui.separator();

                egui::ScrollArea::vertical()
                    .max_height(220.0)
                    .id_salt(("roll_scroll", session.loot_id))
                    .show(ui, |ui| {
                        for item in session.items.iter_mut() {
                            ui.horizontal(|ui| {
                                let resolved = content.0.resolve(&item.instance).ok();
                                let name = resolved
                                    .as_ref()
                                    .map(|r| r.display_name.clone())
                                    .unwrap_or_else(|| {
                                        format!("<unresolved {}>", item.instance.base_id)
                                    });
                                let color = resolved
                                    .as_ref()
                                    .map(|r| rarity_color(r.rarity))
                                    .unwrap_or(egui::Color32::LIGHT_RED);
                                ui.label(egui::RichText::new(name).color(color).size(13.0));
                                if item.count > 1 {
                                    ui.label(
                                        egui::RichText::new(format!("×{}", item.count))
                                            .size(10.0)
                                            .color(egui::Color32::from_gray(170)),
                                    );
                                }

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if let Some(outcome) = &item.outcome {
                                            // Settled — show result inline.
                                            let text = if outcome.winner.is_empty() {
                                                "no winner".to_string()
                                            } else if outcome.roll_value == 255 {
                                                format!(
                                                    "{} ({:?})",
                                                    outcome.winner, outcome.vote_kind
                                                )
                                            } else {
                                                format!(
                                                    "{} won — {:?} {}",
                                                    outcome.winner,
                                                    outcome.vote_kind,
                                                    outcome.roll_value
                                                )
                                            };
                                            ui.label(
                                                egui::RichText::new(text)
                                                    .color(egui::Color32::from_rgb(220, 200, 120))
                                                    .size(12.0),
                                            );
                                        } else if let Some(vote) = item.local_vote {
                                            ui.label(
                                                egui::RichText::new(format!(
                                                    "voted {:?}",
                                                    vote
                                                ))
                                                .color(egui::Color32::from_gray(170))
                                                .size(12.0),
                                            );
                                        } else {
                                            // Live — Need / Greed / Pass.
                                            if ui
                                                .button(
                                                    egui::RichText::new("Pass")
                                                        .color(egui::Color32::from_gray(180)),
                                                )
                                                .clicked()
                                            {
                                                item.local_vote = Some(RollVote::Pass);
                                                votes_to_send.push(LootRollVote {
                                                    loot_id: session.loot_id,
                                                    item_index: item.item_index,
                                                    vote: RollVote::Pass,
                                                });
                                            }
                                            if ui
                                                .button(
                                                    egui::RichText::new("Greed")
                                                        .color(egui::Color32::from_rgb(
                                                            230, 200, 100,
                                                        )),
                                                )
                                                .clicked()
                                            {
                                                item.local_vote = Some(RollVote::Greed);
                                                votes_to_send.push(LootRollVote {
                                                    loot_id: session.loot_id,
                                                    item_index: item.item_index,
                                                    vote: RollVote::Greed,
                                                });
                                            }
                                            if ui
                                                .button(
                                                    egui::RichText::new("Need")
                                                        .color(egui::Color32::from_rgb(
                                                            120, 220, 120,
                                                        )),
                                                )
                                                .clicked()
                                            {
                                                item.local_vote = Some(RollVote::Need);
                                                votes_to_send.push(LootRollVote {
                                                    loot_id: session.loot_id,
                                                    item_index: item.item_index,
                                                    vote: RollVote::Need,
                                                });
                                            }
                                        }
                                    },
                                );
                            });
                        }
                    });
            });
    }

    if !votes_to_send.is_empty() {
        if let Ok(mut sender) = tx.single_mut() {
            for vote in votes_to_send {
                let _ = sender.send::<Channel1>(vote);
            }
        }
    }
}
