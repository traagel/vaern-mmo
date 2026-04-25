//! Quest-giver interaction + quest log UI.
//!
//!   F — if a QuestGiver or QuestPoi is within INTERACT_RANGE, open a
//!       dialogue. Giver dialogue lists chains the player can accept /
//!       progress. POI dialogue is a single-step "investigate" turn-in.
//!   L — toggle the quest log panel
//!   Esc — close whichever panel is open

use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use vaern_combat::{DisplayName, NpcKind, QuestGiverHub, QuestPoi};

use crate::inventory_ui::OwnInventory;
use crate::menu::AppState;
use crate::quests::{send_abandon, send_accept, send_progress, PlayerQuestLog, ZoneChains};
use crate::shared::Player;

pub const INTERACT_RANGE: f32 = 5.0;

pub struct InteractPlugin;

impl Plugin for InteractPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NearbyQuestGiver>()
            .init_resource::<NearbyQuestPoi>()
            .init_resource::<DialogueState>()
            .init_resource::<QuestLogPanelOpen>()
            .add_systems(
                Update,
                (
                    detect_nearby_giver,
                    detect_nearby_poi,
                    open_dialogue_on_f,
                    toggle_quest_log_on_l,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                EguiPrimaryContextPass,
                (dialogue_ui, quest_log_ui).run_if(in_state(AppState::InGame)),
            )
            .add_systems(OnExit(AppState::InGame), reset_state);
    }
}

// ─── resources ──────────────────────────────────────────────────────────────

#[derive(Resource, Default, Debug)]
pub struct NearbyQuestGiver {
    pub entity: Option<Entity>,
    pub name: String,
    /// Hub metadata of the nearby giver. None if the NPC has no QuestGiverHub.
    pub hub: Option<NearbyHub>,
}

#[derive(Debug, Clone, Default)]
pub struct NearbyHub {
    pub hub_id: String,
    pub hub_role: String,
    pub zone_id: String,
    pub chain_id: Option<String>,
    pub step_index: Option<u32>,
}

/// Quest point-of-interest currently within `INTERACT_RANGE`. Populated by
/// `detect_nearby_poi`; cleared when the player walks away. Distinct from
/// `NearbyQuestGiver` so the interaction prompt can show a different
/// "[F] Investigate X" affordance.
#[derive(Resource, Default, Debug)]
pub struct NearbyQuestPoi {
    pub entity: Option<Entity>,
    pub name: String,
    pub chain_id: String,
    pub step_index: u32,
}

#[derive(Resource, Default, Debug)]
struct DialogueState {
    target: Option<Entity>,
    target_name: String,
    /// Copy of the giver's hub metadata at the moment dialogue opened.
    hub: Option<NearbyHub>,
    /// Set when the dialogue was opened by F-pressing a `QuestPoi`. Drives
    /// the investigate-step branch of `dialogue_ui`.
    poi: Option<PoiBinding>,
}

#[derive(Debug, Clone)]
struct PoiBinding {
    chain_id: String,
    step_index: u32,
}

#[derive(Resource, Default, Debug)]
struct QuestLogPanelOpen(bool);

// ─── systems ────────────────────────────────────────────────────────────────

fn detect_nearby_giver(
    player: Query<&Transform, With<Player>>,
    givers: Query<(
        Entity,
        &Transform,
        &NpcKind,
        Option<&DisplayName>,
        Option<&QuestGiverHub>,
    )>,
    mut nearby: ResMut<NearbyQuestGiver>,
) {
    let Ok(player_tf) = player.single() else {
        nearby.entity = None;
        nearby.hub = None;
        return;
    };
    let range_sq = INTERACT_RANGE * INTERACT_RANGE;
    let best = givers
        .iter()
        .filter(|(_, _, kind, _, _)| matches!(*kind, NpcKind::QuestGiver))
        .map(|(e, tf, _, name, hub)| {
            (
                e,
                tf.translation.distance_squared(player_tf.translation),
                name,
                hub,
            )
        })
        .filter(|(_, d_sq, _, _)| *d_sq <= range_sq)
        .min_by(|a, b| a.1.total_cmp(&b.1));
    match best {
        Some((e, _, name, hub)) => {
            nearby.entity = Some(e);
            nearby.name = name.map(|n| n.0.clone()).unwrap_or_else(|| "NPC".into());
            nearby.hub = hub.map(|h| NearbyHub {
                hub_id: h.hub_id.clone(),
                hub_role: h.hub_role.clone(),
                zone_id: h.zone_id.clone(),
                chain_id: h.chain_id.clone(),
                step_index: h.step_index,
            });
        }
        None => {
            nearby.entity = None;
            nearby.name.clear();
            nearby.hub = None;
        }
    }
}

fn open_dialogue_on_f(
    keys: Res<ButtonInput<KeyCode>>,
    nearby: Res<NearbyQuestGiver>,
    nearby_poi: Res<NearbyQuestPoi>,
    mut dialogue: ResMut<DialogueState>,
) {
    if !keys.just_pressed(KeyCode::KeyF) {
        return;
    }
    // Givers take precedence — they may have multi-chain dialogue trees
    // a POI can't host.
    if let Some(e) = nearby.entity {
        dialogue.target = Some(e);
        dialogue.target_name = nearby.name.clone();
        dialogue.hub = nearby.hub.clone();
        dialogue.poi = None;
        return;
    }
    if let Some(e) = nearby_poi.entity {
        dialogue.target = Some(e);
        dialogue.target_name = nearby_poi.name.clone();
        dialogue.hub = None;
        dialogue.poi = Some(PoiBinding {
            chain_id: nearby_poi.chain_id.clone(),
            step_index: nearby_poi.step_index,
        });
    }
}

fn detect_nearby_poi(
    player: Query<&Transform, With<Player>>,
    pois: Query<(Entity, &Transform, &QuestPoi)>,
    mut nearby: ResMut<NearbyQuestPoi>,
) {
    let Ok(player_tf) = player.single() else {
        nearby.entity = None;
        return;
    };
    let range_sq = INTERACT_RANGE * INTERACT_RANGE;
    let best = pois
        .iter()
        .map(|(e, tf, p)| {
            (
                e,
                tf.translation.distance_squared(player_tf.translation),
                p,
            )
        })
        .filter(|(_, d_sq, _)| *d_sq <= range_sq)
        .min_by(|a, b| a.1.total_cmp(&b.1));
    match best {
        Some((e, _, poi)) => {
            nearby.entity = Some(e);
            nearby.name = poi.name.clone();
            nearby.chain_id = poi.chain_id.clone();
            nearby.step_index = poi.step_index;
        }
        None => {
            nearby.entity = None;
            nearby.name.clear();
            nearby.chain_id.clear();
        }
    }
}

fn toggle_quest_log_on_l(
    keys: Res<ButtonInput<KeyCode>>,
    mut panel: ResMut<QuestLogPanelOpen>,
) {
    if keys.just_pressed(KeyCode::KeyL) {
        panel.0 = !panel.0;
    }
}

fn dialogue_ui(
    mut contexts: EguiContexts,
    mut dialogue: ResMut<DialogueState>,
    keys: Res<ButtonInput<KeyCode>>,
    nearby: Res<NearbyQuestGiver>,
    nearby_poi: Res<NearbyQuestPoi>,
    chains: Res<ZoneChains>,
    log: Res<PlayerQuestLog>,
    inventory: Res<OwnInventory>,
    mut commands: Commands,
) {
    let Ok(ctx) = contexts.ctx_mut() else { return };

    if dialogue.target.is_some() && keys.just_pressed(KeyCode::Escape) {
        dialogue.target = None;
        dialogue.target_name.clear();
        dialogue.poi = None;
    }

    // "[F] Talk to X" / "[F] Investigate Y" prompt whenever a giver or POI
    // is in range and the dialogue is closed.
    if dialogue.target.is_none() && (nearby.entity.is_some() || nearby_poi.entity.is_some()) {
        let (prompt_label, name) = if let Some(_) = nearby.entity {
            ("Talk to", nearby.name.as_str())
        } else {
            ("Investigate", nearby_poi.name.as_str())
        };
        egui::Area::new(egui::Id::new("interact_prompt"))
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -140.0))
            .show(ctx, |ui| {
                egui::Frame::default()
                    .fill(egui::Color32::from_rgba_unmultiplied(20, 20, 30, 220))
                    .stroke(egui::Stroke::new(
                        1.0,
                        egui::Color32::from_rgb(80, 180, 210),
                    ))
                    .inner_margin(egui::Margin::symmetric(14, 8))
                    .corner_radius(4.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("[F]")
                                    .strong()
                                    .color(egui::Color32::from_rgb(255, 220, 120))
                                    .size(14.0),
                            );
                            ui.label(
                                egui::RichText::new(format!("{prompt_label} {name}"))
                                    .color(egui::Color32::from_gray(220))
                                    .size(13.0),
                            );
                        });
                    });
            });
    }

    if dialogue.target.is_none() {
        return;
    }

    // POI dialogue is a single-step "investigate" turn-in. Different
    // shape from the giver dialogue — no chain-tree, just authored
    // completion_text + a button.
    if let Some(poi) = dialogue.poi.clone() {
        render_poi_dialogue(ctx, &mut dialogue, &poi, &chains, &log, &mut commands);
        return;
    }
    let name = dialogue.target_name.clone();
    let hub = dialogue.hub.clone();

    // Each named quest-giver is bound to a specific (chain_id, step_index).
    // - step_index == 0: main giver → Accept button (if not yet accepted).
    // - step_index > 0:  mid-chain contact → Progress button ONLY when the
    //                    player's current_step matches this NPC's step.
    let bound_chain_id = hub.as_ref().and_then(|h| h.chain_id.clone());
    let bound_step = hub.as_ref().and_then(|h| h.step_index);
    let bound_chain = bound_chain_id
        .as_ref()
        .and_then(|id| chains.find(id));

    // Hand-curated dialogue line from the chain's npcs registry. Matches the
    // giver by display_name since the client doesn't hold the server's npc_id.
    let custom_dialogue = bound_chain.and_then(|c| {
        c.npcs
            .iter()
            .find(|n| n.display_name == name)
            .and_then(|n| n.dialogue.clone())
    });
    let custom_title = bound_chain.and_then(|c| {
        c.npcs
            .iter()
            .find(|n| n.display_name == name)
            .and_then(|n| n.title.clone())
    });

    let mut close = false;
    egui::Window::new(format!("— {} —", name))
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .resizable(false)
        .collapsible(false)
        .default_width(540.0)
        .show(ctx, |ui| {
            ui.add_space(6.0);
            if let Some(h) = &hub {
                let subhead = match &custom_title {
                    Some(t) => format!(
                        "{}  ·  {} of {}",
                        t,
                        prettify(&h.hub_role),
                        prettify(&h.zone_id),
                    ),
                    None => format!(
                        "{}  ·  {} of {}",
                        prettify(&h.hub_id),
                        prettify(&h.hub_role),
                        prettify(&h.zone_id),
                    ),
                };
                ui.label(
                    egui::RichText::new(subhead)
                        .small()
                        .color(egui::Color32::from_gray(150)),
                );
                ui.add_space(4.0);
            }
            let greeting = custom_dialogue.clone().unwrap_or_else(|| {
                format!(
                    "“Well met, traveler. I'm {}. The march has much need of able hands.”",
                    name
                )
            });
            ui.label(
                egui::RichText::new(greeting)
                    .italics()
                    .color(egui::Color32::from_gray(210)),
            );
            ui.add_space(12.0);
            ui.separator();

            match (bound_chain, bound_step) {
                // Bound to a specific chain step.
                (Some(chain), Some(step_idx)) => {
                    ui.label(
                        egui::RichText::new(&chain.title)
                            .strong()
                            .color(egui::Color32::from_rgb(255, 210, 120))
                            .size(15.0),
                    );
                    ui.label(
                        egui::RichText::new(&chain.premise)
                            .italics()
                            .color(egui::Color32::from_gray(200)),
                    );
                    ui.add_space(6.0);

                    let entry = log.get(&chain.id);
                    match (step_idx, entry) {
                        // Main giver (step 0), not yet accepted.
                        (0, None) => {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "A {}-step task awaits.",
                                        chain.total_steps
                                    ))
                                    .color(egui::Color32::from_gray(210)),
                                );
                                if ui.button("Accept").clicked() {
                                    send_accept(&mut commands, chain.id.clone());
                                }
                            });
                        }
                        // Main giver, already completed.
                        (0, Some(e)) if e.completed => {
                            ui.label(
                                egui::RichText::new("✓ Completed — thank you, friend.")
                                    .strong()
                                    .color(egui::Color32::from_rgb(120, 220, 150)),
                            );
                        }
                        // Main giver, currently in progress. If the
                        // current step's objective.npc points back at
                        // this giver (e.g. step 5 deliver to Warden
                        // Telyn), show the turn-in button. Otherwise
                        // show the next-step breadcrumb.
                        (0, Some(e)) => {
                            let current_step = chain.step(e.current_step);
                            let reports_to_main = current_step
                                .and_then(|s| s.objective.npc.as_deref())
                                .and_then(|npc_id| chain.npc(npc_id))
                                .map(|n| n.display_name == name)
                                .unwrap_or(false);
                            ui.label(format!(
                                "In progress · step {}/{}",
                                (e.current_step + 1).min(e.total_steps),
                                e.total_steps,
                            ));
                            if reports_to_main {
                                if let Some(step) = current_step {
                                    ui.add_space(4.0);
                                    let reply = step
                                        .completion_text
                                        .clone()
                                        .unwrap_or_else(|| {
                                            "“You're back. Tell me what you found.”".into()
                                        });
                                    ui.label(
                                        egui::RichText::new(reply)
                                            .italics()
                                            .color(egui::Color32::from_gray(220)),
                                    );
                                    ui.add_space(8.0);

                                    let button_label = step
                                        .completion_button
                                        .clone()
                                        .unwrap_or_else(|| match step.objective.kind.as_str() {
                                            "deliver" => "Hand it over".into(),
                                            _ => "Continue".into(),
                                        });
                                    let (have_item, missing_label) =
                                        deliver_inventory_check(&step.objective, &inventory);
                                    let mut response = ui
                                        .add_enabled(have_item, egui::Button::new(button_label));
                                    if let Some(missing) = missing_label {
                                        response = response.on_disabled_hover_text(missing);
                                    }
                                    if response.clicked() && have_item {
                                        send_progress(&mut commands, chain.id.clone());
                                    }
                                }
                            } else if let Some(step) = current_step {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "Next: {} → {}",
                                        step.name, step.objective.target_hint
                                    ))
                                    .italics()
                                    .color(egui::Color32::from_gray(210)),
                                );
                            }
                        }
                        // Mid-chain contact, quest not accepted yet.
                        (_, None) => {
                            ui.label(
                                egui::RichText::new(
                                    "“Whoever sent you must speak the oath first — see the capital.”",
                                )
                                .italics()
                                .color(egui::Color32::from_gray(170)),
                            );
                        }
                        // Mid-chain contact, quest done.
                        (_, Some(e)) if e.completed => {
                            ui.label(
                                egui::RichText::new("“We're done here, thanks to you.”")
                                    .italics()
                                    .color(egui::Color32::from_gray(170)),
                            );
                        }
                        // Mid-chain contact, correct current step. The
                        // player has read the dialogue, completed the
                        // objective, and is reporting in. Show the
                        // authored completion_text (NPC's reply) and a
                        // contextual button that fires `ProgressQuest`.
                        // Deliver steps gate the button on inventory.
                        (step, Some(e)) if e.current_step == step => {
                            if let Some(step) = chain.step(e.current_step) {
                                ui.label(format!(
                                    "Step {} of {}: {}",
                                    e.current_step + 1,
                                    e.total_steps,
                                    step.name
                                ));
                                ui.add_space(4.0);
                                let reply = step
                                    .completion_text
                                    .clone()
                                    .unwrap_or_else(|| {
                                        "“You're back. Tell me what you found.”".into()
                                    });
                                ui.label(
                                    egui::RichText::new(reply)
                                        .italics()
                                        .color(egui::Color32::from_gray(220)),
                                );
                                ui.add_space(8.0);

                                let button_label = step
                                    .completion_button
                                    .clone()
                                    .unwrap_or_else(|| match step.objective.kind.as_str() {
                                        "deliver" => "Hand it over".into(),
                                        _ => "Continue".into(),
                                    });

                                let (have_item, missing_label) =
                                    deliver_inventory_check(&step.objective, &inventory);
                                let enabled = have_item;
                                let mut response =
                                    ui.add_enabled(enabled, egui::Button::new(button_label));
                                if let Some(missing) = missing_label {
                                    response = response.on_disabled_hover_text(missing);
                                }
                                if response.clicked() && enabled {
                                    send_progress(&mut commands, chain.id.clone());
                                }
                            }
                        }
                        // Mid-chain contact, player is on a different step.
                        (step, Some(e)) if step < e.current_step => {
                            ui.label(
                                egui::RichText::new("“You've moved past me. Go on.”")
                                    .italics()
                                    .color(egui::Color32::from_gray(170)),
                            );
                        }
                        (_, Some(_)) => {
                            ui.label(
                                egui::RichText::new("“Come back when you've done more.”")
                                    .italics()
                                    .color(egui::Color32::from_gray(170)),
                            );
                        }
                    }
                }
                // NPC has no chain binding at all.
                _ => {
                    ui.label(
                        egui::RichText::new("“Well met. I've nothing for you right now.”")
                            .italics()
                            .color(egui::Color32::from_gray(170)),
                    );
                }
            }

            ui.add_space(8.0);
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("[Esc] close · [L] quest log")
                        .small()
                        .color(egui::Color32::from_gray(130)),
                );
                if ui.button("Close").clicked() {
                    close = true;
                }
            });
        });

    if close {
        dialogue.target = None;
        dialogue.target_name.clear();
    }
}

fn quest_log_ui(
    mut contexts: EguiContexts,
    mut panel: ResMut<QuestLogPanelOpen>,
    chains: Res<ZoneChains>,
    log: Res<PlayerQuestLog>,
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
) {
    let Ok(ctx) = contexts.ctx_mut() else { return };

    if panel.0 && keys.just_pressed(KeyCode::Escape) {
        panel.0 = false;
    }
    if !panel.0 {
        return;
    }

    let mut open = panel.0;
    egui::Window::new("Quest Log [L]")
        .default_pos(egui::pos2(40.0, 80.0))
        .default_width(460.0)
        .open(&mut open)
        .show(ctx, |ui| {
            if log.entries.is_empty() {
                ui.label(
                    egui::RichText::new(
                        "No active quests. Find a quest-giver (gold ! nameplate) and press [F].",
                    )
                    .italics()
                    .color(egui::Color32::from_gray(170)),
                );
                return;
            }

            // Separate active vs completed.
            let mut active: Vec<_> = log
                .entries
                .values()
                .filter(|e| !e.completed)
                .collect();
            let mut done: Vec<_> = log
                .entries
                .values()
                .filter(|e| e.completed)
                .collect();
            active.sort_by(|a, b| a.chain_id.cmp(&b.chain_id));
            done.sort_by(|a, b| a.chain_id.cmp(&b.chain_id));

            ui.label(
                egui::RichText::new(format!("Active ({})", active.len()))
                    .strong()
                    .color(egui::Color32::from_rgb(255, 210, 120)),
            );
            ui.add_space(4.0);
            for entry in active {
                let chain = chains.find(&entry.chain_id);
                ui.group(|ui| {
                    ui.label(
                        egui::RichText::new(
                            chain.map(|c| c.title.as_str()).unwrap_or(&entry.chain_id),
                        )
                        .strong()
                        .color(egui::Color32::WHITE),
                    );
                    ui.label(format!(
                        "Step {} of {}",
                        (entry.current_step + 1).min(entry.total_steps),
                        entry.total_steps,
                    ));

                    if let Some(chain) = chain {
                        // Show the current step's objective hint, with a
                        // kill counter when the step is multi-kill.
                        if let Some(step) = chain.step(entry.current_step) {
                            let counter_suffix = if entry.kill_count_required > 1 {
                                format!(
                                    " · {}/{}",
                                    entry.kill_count, entry.kill_count_required
                                )
                            } else {
                                String::new()
                            };
                            ui.label(
                                egui::RichText::new(format!(
                                    "→ {} · {}{counter_suffix}",
                                    step.name, step.objective.target_hint
                                ))
                                .italics()
                                .color(egui::Color32::from_gray(210)),
                            );
                        }
                    }

                    ui.horizontal(|ui| {
                        // Debug-only manual advance — useful for testing
                        // talk / deliver / investigate flows in dev. Not
                        // shipped in release builds (real auto-advance
                        // goes through F-press dialogue + ProgressQuest
                        // with proximity validation).
                        #[cfg(debug_assertions)]
                        if ui
                            .button("Progress (debug)")
                            .on_hover_text(
                                "Dev-only: skip the click-through dialogue. Release builds advance via F-press at the right NPC / waypoint.",
                            )
                            .clicked()
                        {
                            send_progress(&mut commands, entry.chain_id.clone());
                        }
                        if ui.button("Abandon").clicked() {
                            send_abandon(&mut commands, entry.chain_id.clone());
                        }
                    });
                });
                ui.add_space(2.0);
            }

            if !done.is_empty() {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(format!("Completed ({})", done.len()))
                        .strong()
                        .color(egui::Color32::from_rgb(120, 220, 150)),
                );
                for entry in done {
                    let chain = chains.find(&entry.chain_id);
                    ui.label(format!(
                        "  ✓ {}",
                        chain.map(|c| c.title.as_str()).unwrap_or(&entry.chain_id)
                    ));
                }
            }
        });
    panel.0 = open;
}

fn reset_state(
    mut nearby: ResMut<NearbyQuestGiver>,
    mut nearby_poi: ResMut<NearbyQuestPoi>,
    mut dialogue: ResMut<DialogueState>,
    mut panel: ResMut<QuestLogPanelOpen>,
) {
    nearby.entity = None;
    nearby.name.clear();
    nearby.hub = None;
    nearby_poi.entity = None;
    nearby_poi.name.clear();
    nearby_poi.chain_id.clear();
    dialogue.target = None;
    dialogue.target_name.clear();
    dialogue.hub = None;
    dialogue.poi = None;
    panel.0 = false;
}

/// True iff the step is a `deliver` with `item_required` AND the player's
/// inventory has at least the required count. Returns the disabled-tooltip
/// label as the second element when the gate fails.
fn deliver_inventory_check(
    objective: &vaern_data::QuestObjective,
    inventory: &OwnInventory,
) -> (bool, Option<String>) {
    if objective.kind != "deliver" {
        return (true, None);
    }
    let Some(req) = &objective.item_required else {
        return (true, None);
    };
    let needed = req.count.max(1);
    let total: u32 = inventory
        .slots
        .iter()
        .filter_map(|s| s.as_ref())
        .filter(|s| {
            s.instance.base_id == req.base
                && s.instance.material_id == req.material
                && s.instance.quality_id == req.quality
        })
        .map(|s| s.count)
        .sum();
    if total >= needed {
        (true, None)
    } else {
        (
            false,
            Some(format!(
                "Need {needed}× {} ({}/{needed} in inventory).",
                req.base, total
            )),
        )
    }
}

/// POI dialogue. Always renders against the player's current chain step
/// because a POI is fixed to a specific (chain_id, step_index).
fn render_poi_dialogue(
    ctx: &egui::Context,
    dialogue: &mut DialogueState,
    poi: &PoiBinding,
    chains: &ZoneChains,
    log: &PlayerQuestLog,
    commands: &mut Commands,
) {
    let chain = chains.find(&poi.chain_id);
    let entry = log.get(&poi.chain_id);
    let mut close = false;
    egui::Window::new(format!("— {} —", dialogue.target_name))
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .resizable(false)
        .collapsible(false)
        .default_width(540.0)
        .show(ctx, |ui| {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("Investigate")
                    .small()
                    .color(egui::Color32::from_gray(150)),
            );
            ui.add_space(8.0);
            match (chain, entry) {
                (Some(chain), Some(e)) if !e.completed && e.current_step == poi.step_index => {
                    if let Some(step) = chain.step(e.current_step) {
                        ui.label(
                            egui::RichText::new(&chain.title)
                                .strong()
                                .color(egui::Color32::from_rgb(255, 210, 120))
                                .size(15.0),
                        );
                        ui.label(format!(
                            "Step {} of {}: {}",
                            e.current_step + 1,
                            e.total_steps,
                            step.name
                        ));
                        ui.add_space(4.0);
                        let body = step
                            .completion_text
                            .clone()
                            .unwrap_or_else(|| step.objective.target_hint.clone());
                        ui.label(
                            egui::RichText::new(body)
                                .italics()
                                .color(egui::Color32::from_gray(220)),
                        );
                        ui.add_space(8.0);
                        let button_label = step
                            .completion_button
                            .clone()
                            .unwrap_or_else(|| "Continue".into());
                        if ui.button(button_label).clicked() {
                            send_progress(commands, chain.id.clone());
                        }
                    }
                }
                (Some(_), Some(e)) if e.completed || e.current_step > poi.step_index => {
                    ui.label(
                        egui::RichText::new(
                            "The trail is cold. There's nothing more to find here.",
                        )
                        .italics()
                        .color(egui::Color32::from_gray(170)),
                    );
                }
                _ => {
                    ui.label(
                        egui::RichText::new(
                            "Nothing here is meaningful to you yet — your story hasn't reached this place.",
                        )
                        .italics()
                        .color(egui::Color32::from_gray(170)),
                    );
                }
            }
            ui.add_space(8.0);
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("[Esc] close")
                        .small()
                        .color(egui::Color32::from_gray(130)),
                );
                if ui.button("Close").clicked() {
                    close = true;
                }
            });
        });
    if close {
        dialogue.target = None;
        dialogue.target_name.clear();
        dialogue.poi = None;
    }
}

fn prettify(s: &str) -> String {
    s.split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(c) => c.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
