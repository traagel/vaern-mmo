//! Party UI — member frame + incoming-invite popup + chat-command
//! routing for `/invite`, `/leave`, `/kick`, `/disband`, `/p`.
//!
//! Data flow:
//!   Server `PartySnapshot` → `OwnParty` resource (Some/None)
//!   Server `PartyIncomingInvite` → `PendingInvite` resource
//!   Server `PartyDisbandedNotice` → clears `OwnParty`
//!
//! Frame layout: top-left, right below the player unit frame. One
//! row per member with name, level, and HP bar. Leader gets a gold
//! `[L]` tag.
//!
//! Chat-command routing: `/invite <name>`, `/leave`, `/kick <name>`
//! are parsed in the chat input and ship `PartyInviteRequest` /
//! `PartyLeaveRequest` / `PartyKickRequest`. `/p <msg>` is already
//! handled by the chat parser — routes through `ChatChannel::Party`.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use lightyear::prelude::*;
use lightyear::prelude::client::Client;

use vaern_protocol::{
    Channel1, PartyDisbandedNotice, PartyIncomingInvite, PartyInviteResponse, PartyKickRequest,
    PartyLeaveRequest, PartyMember, PartySnapshot,
};

use crate::menu::AppState;

pub struct PartyUiPlugin;

impl Plugin for PartyUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OwnParty>()
            .init_resource::<PendingInvite>()
            .add_systems(
                Update,
                (
                    ingest_party_snapshot,
                    ingest_party_invite,
                    ingest_party_disband,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                EguiPrimaryContextPass,
                (party_frame_ui, invite_popup_ui).run_if(in_state(AppState::InGame)),
            )
            .add_systems(OnExit(AppState::InGame), reset);
    }
}

/// Latest full party state from the server, or `None` when solo.
#[derive(Resource, Default, Debug)]
pub struct OwnParty(pub Option<PartySnapshot>);

/// Incoming invite awaiting an Accept/Decline click.
#[derive(Resource, Default, Debug)]
pub struct PendingInvite {
    pub offer: Option<PartyIncomingInvite>,
}

fn ingest_party_snapshot(
    mut rx: Query<&mut MessageReceiver<PartySnapshot>, With<Client>>,
    mut party: ResMut<OwnParty>,
) {
    let Ok(mut receiver) = rx.single_mut() else { return };
    for snap in receiver.receive() {
        party.0 = Some(snap);
    }
}

fn ingest_party_invite(
    mut rx: Query<&mut MessageReceiver<PartyIncomingInvite>, With<Client>>,
    mut pending: ResMut<PendingInvite>,
) {
    let Ok(mut receiver) = rx.single_mut() else { return };
    for invite in receiver.receive() {
        pending.offer = Some(invite);
    }
}

fn ingest_party_disband(
    mut rx: Query<&mut MessageReceiver<PartyDisbandedNotice>, With<Client>>,
    mut party: ResMut<OwnParty>,
) {
    let Ok(mut receiver) = rx.single_mut() else { return };
    for notice in receiver.receive() {
        // Only clear if the disband notice matches our current party id
        // — prevents a stale notice (delivered after we already joined
        // a new party) from clobbering a fresh snapshot.
        if party
            .0
            .as_ref()
            .map(|p| p.party_id == notice.party_id)
            .unwrap_or(false)
        {
            party.0 = None;
        }
    }
}

fn party_frame_ui(
    mut contexts: EguiContexts,
    party: Res<OwnParty>,
    mut tx: Query<
        &mut MessageSender<PartyLeaveRequest>,
        (With<Client>, Without<MessageSender<PartyKickRequest>>),
    >,
    mut kick_tx: Query<
        &mut MessageSender<PartyKickRequest>,
        (With<Client>, Without<MessageSender<PartyLeaveRequest>>),
    >,
) {
    let Some(snap) = party.0.as_ref() else { return };
    let Ok(ctx) = contexts.ctx_mut() else { return };

    let mut leave_clicked = false;
    let mut kick_target: Option<String> = None;

    egui::Window::new("party_frame")
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .movable(false)
        .anchor(egui::Align2::LEFT_TOP, egui::vec2(16.0, 140.0))
        .frame(
            egui::Frame::default()
                .fill(egui::Color32::from_rgba_unmultiplied(15, 18, 22, 230))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(120, 150, 200)))
                .inner_margin(egui::Margin::symmetric(8, 6))
                .corner_radius(4.0),
        )
        .show(ctx, |ui| {
            ui.set_width(220.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Party")
                        .strong()
                        .color(egui::Color32::from_rgb(170, 190, 240))
                        .size(13.0),
                );
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(format!("{}/5", snap.members.len()))
                        .small()
                        .color(egui::Color32::from_gray(150)),
                );
                if ui
                    .button(egui::RichText::new("Leave").size(10.0))
                    .clicked()
                {
                    leave_clicked = true;
                }
            });
            ui.separator();
            for member in &snap.members {
                render_member_row(ui, member, &mut kick_target);
            }
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new("/p <msg>  ·  /invite <name>  ·  /kick <name>")
                    .small()
                    .color(egui::Color32::from_gray(130)),
            );
        });

    if leave_clicked {
        if let Ok(mut sender) = tx.single_mut() {
            let _ = sender.send::<Channel1>(PartyLeaveRequest);
        }
    }
    if let Some(name) = kick_target {
        if let Ok(mut sender) = kick_tx.single_mut() {
            let _ = sender.send::<Channel1>(PartyKickRequest { target_name: name });
        }
    }
}

fn render_member_row(ui: &mut egui::Ui, member: &PartyMember, kick_target: &mut Option<String>) {
    ui.horizontal(|ui| {
        if member.is_leader {
            ui.label(
                egui::RichText::new("[L]")
                    .strong()
                    .color(egui::Color32::from_rgb(255, 220, 120))
                    .size(11.0),
            );
        } else {
            ui.label(egui::RichText::new("   ").size(11.0));
        }
        ui.label(
            egui::RichText::new(&member.display_name)
                .strong()
                .color(egui::Color32::from_gray(230))
                .size(12.0),
        );
        ui.label(
            egui::RichText::new(format!("L{}", member.level.max(1)))
                .size(10.0)
                .color(egui::Color32::from_rgb(230, 200, 100)),
        );
    });
    let hp_max = if member.hp_max > 0.0 { member.hp_max } else { 100.0 };
    let hp_pct = (member.hp_current / hp_max).clamp(0.0, 1.0);
    let hp_label = format!("HP {:.0} / {:.0}", member.hp_current, member.hp_max);
    hp_bar(ui, hp_pct, &hp_label);
    ui.add_space(1.0);
    // Context-ish: right-click the name row to mark for kick. Since
    // egui doesn't have free context-menus here without more
    // scaffolding, keep the /kick command as the primary path. Leave
    // the kick_target path open for future polish.
    let _ = kick_target;
}

fn hp_bar(ui: &mut egui::Ui, pct: f32, label: &str) {
    let desired = egui::vec2(200.0, 12.0);
    let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, egui::Color32::from_rgb(25, 25, 30));
    let mut fill_rect = rect;
    fill_rect.set_width(rect.width() * pct);
    painter.rect_filled(fill_rect, 2.0, egui::Color32::from_rgb(200, 50, 50));
    painter.rect_stroke(
        rect,
        2.0,
        egui::Stroke::new(1.0, egui::Color32::from_rgb(60, 60, 70)),
        egui::StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(10.0),
        egui::Color32::WHITE,
    );
}

fn invite_popup_ui(
    mut contexts: EguiContexts,
    mut pending: ResMut<PendingInvite>,
    mut tx: Query<&mut MessageSender<PartyInviteResponse>, With<Client>>,
) {
    let Some(offer) = pending.offer.clone() else { return };
    let Ok(ctx) = contexts.ctx_mut() else { return };
    let mut response: Option<bool> = None;

    egui::Window::new("party_invite")
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 60.0))
        .frame(
            egui::Frame::default()
                .fill(egui::Color32::from_rgba_unmultiplied(20, 25, 35, 240))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(170, 190, 240)))
                .inner_margin(egui::Margin::symmetric(14, 10))
                .corner_radius(6.0),
        )
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new("Party Invite")
                        .strong()
                        .color(egui::Color32::from_rgb(170, 190, 240))
                        .size(14.0),
                );
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(format!(
                        "{} has invited you to their party.",
                        offer.from_name
                    ))
                    .color(egui::Color32::from_gray(220))
                    .size(12.0),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui
                        .button(egui::RichText::new("Accept").strong())
                        .clicked()
                    {
                        response = Some(true);
                    }
                    ui.add_space(8.0);
                    if ui.button("Decline").clicked() {
                        response = Some(false);
                    }
                });
            });
        });

    if let Some(accept) = response {
        if let Ok(mut sender) = tx.single_mut() {
            let _ = sender.send::<Channel1>(PartyInviteResponse {
                party_id: offer.party_id,
                accept,
            });
        }
        pending.offer = None;
    }
}

// ---------------------------------------------------------------------------
// Chat-command handler — invoked from chat_ui's parse path for
// `/invite`, `/leave`, `/kick`. Returns `true` if the command was
// a party command and shouldn't be routed to chat.
// ---------------------------------------------------------------------------

/// Parsed party command from the chat input. Returns `None` if the
/// line isn't a party command.
pub fn parse_party_command(line: &str) -> Option<PartyCommand> {
    let trimmed = line.trim();
    if !trimmed.starts_with('/') {
        return None;
    }
    let rest = &trimmed[1..];
    let (cmd, body) = match rest.split_once(char::is_whitespace) {
        Some((c, b)) => (c.to_lowercase(), b.trim()),
        None => (rest.to_lowercase(), ""),
    };
    match cmd.as_str() {
        "invite" | "inv" => {
            if body.is_empty() {
                None
            } else {
                Some(PartyCommand::Invite {
                    target: body.to_string(),
                })
            }
        }
        "leave" | "disband" => Some(PartyCommand::Leave),
        "kick" => {
            if body.is_empty() {
                None
            } else {
                Some(PartyCommand::Kick {
                    target: body.to_string(),
                })
            }
        }
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PartyCommand {
    Invite { target: String },
    Leave,
    Kick { target: String },
}

fn reset(mut party: ResMut<OwnParty>, mut pending: ResMut<PendingInvite>) {
    party.0 = None;
    pending.offer = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slash_invite_parses() {
        let cmd = parse_party_command("/invite Kell").unwrap();
        assert_eq!(cmd, PartyCommand::Invite { target: "Kell".into() });
    }

    #[test]
    fn slash_inv_abbreviation_parses() {
        let cmd = parse_party_command("/inv Brenn").unwrap();
        assert_eq!(cmd, PartyCommand::Invite { target: "Brenn".into() });
    }

    #[test]
    fn slash_leave_parses() {
        assert_eq!(parse_party_command("/leave").unwrap(), PartyCommand::Leave);
        assert_eq!(parse_party_command("/disband").unwrap(), PartyCommand::Leave);
    }

    #[test]
    fn slash_kick_parses() {
        let cmd = parse_party_command("/kick Seyla").unwrap();
        assert_eq!(cmd, PartyCommand::Kick { target: "Seyla".into() });
    }

    #[test]
    fn non_party_slash_returns_none() {
        assert!(parse_party_command("/z anyone near?").is_none());
        assert!(parse_party_command("hello").is_none());
    }

    #[test]
    fn empty_invite_rejects() {
        assert!(parse_party_command("/invite").is_none());
    }
}
