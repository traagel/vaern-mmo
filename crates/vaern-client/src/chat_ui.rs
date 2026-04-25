//! Chat UI — bottom-left history overlay + Enter-to-focus input bar.
//!
//! Data flow:
//!   Server `ChatMessage` → `ChatHistory` ring buffer (cap 50 lines).
//!   Enter → focuses input; typing doesn't leak to WASD because
//!     `ChatInputFocused` resource goes true and `buffer_wasd_input`
//!     early-returns on it.
//!   Enter (while focused) → parses prefix, ships `ChatSend`, clears.
//!   Esc (while focused) → cancels, clears focus.
//!
//! Prefix parsing (on send):
//!   `<no prefix>`     → Say
//!   `/s <msg>`        → Say
//!   `/say <msg>`      → Say
//!   `/z <msg>`        → Zone
//!   `/zone <msg>`     → Zone
//!   `/w <name> <msg>` → Whisper (also `/whisper`, `/tell`)
//!
//! Unknown slash commands are treated as Say content so a typo doesn't
//! silently swallow your message.

use std::collections::VecDeque;

use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use lightyear::prelude::*;
use lightyear::prelude::client::Client;

use vaern_protocol::{
    Channel1, ChatChannel, ChatMessage, ChatSend, PartyInviteRequest, PartyLeaveRequest,
    PartyKickRequest,
};

use crate::menu::{AppState, SelectedCharacter};
use crate::party_ui::{parse_party_command, PartyCommand};

pub const HISTORY_MAX: usize = 50;

/// Local Bevy message fired by `ingest_chat_messages` for every Say
/// or Zone chat line. `nameplates` subscribes to this to spawn the
/// speech-bubble UI node above the speaker's head. Whisper and Party
/// don't fire bubbles — they're private channels.
#[derive(bevy::prelude::Message, Clone, Debug)]
pub struct ChatBubbleEvent {
    /// Display name of the speaker — resolved to an entity at bubble
    /// spawn time by matching against `DisplayName` components.
    pub from: String,
    pub text: String,
}

pub struct ChatUiPlugin;

impl Plugin for ChatUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChatHistory>()
            .init_resource::<ChatInput>()
            .init_resource::<ChatInputFocused>()
            .add_message::<ChatBubbleEvent>()
            .add_systems(
                Update,
                ingest_chat_messages.run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                EguiPrimaryContextPass,
                chat_ui.run_if(in_state(AppState::InGame)),
            )
            .add_systems(OnExit(AppState::InGame), reset_state);
    }
}

/// Rolling chat history — latest line at the back. Cap at
/// `HISTORY_MAX`; older lines drop silently.
#[derive(Resource, Default, Debug)]
pub struct ChatHistory {
    pub lines: VecDeque<ChatMessage>,
}

impl ChatHistory {
    fn push(&mut self, msg: ChatMessage) {
        self.lines.push_back(msg);
        while self.lines.len() > HISTORY_MAX {
            self.lines.pop_front();
        }
    }
}

/// Input-buffer state. `open == true` means the input bar is shown
/// and has keyboard focus (or is trying to grab it this frame).
#[derive(Resource, Default, Debug)]
pub struct ChatInput {
    pub open: bool,
    pub buffer: String,
    /// Set once when we open the input so egui focuses it on the
    /// next paint pass. Cleared after focus lands.
    pub want_focus: bool,
}

/// Mirror of `ChatInput.open` exposed as its own resource so systems
/// outside `chat_ui` (WASD movement, hotbar casts, Tab targeting) can
/// gate on it without importing the whole UI module.
///
/// The truthy case is: player opened the chat input bar and is typing.
/// While this is true, movement + ability input should be suppressed
/// so hitting "W" doesn't walk the character across the map.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct ChatInputFocused(pub bool);

fn ingest_chat_messages(
    mut rx: Query<&mut MessageReceiver<ChatMessage>, With<Client>>,
    mut history: ResMut<ChatHistory>,
    mut bubbles: MessageWriter<ChatBubbleEvent>,
) {
    let Ok(mut receiver) = rx.single_mut() else { return };
    for msg in receiver.receive() {
        // Fire a head-bubble only for public proximity / zone chat —
        // private channels (Whisper, Party) stay off the 3D world.
        // System messages are also excluded (server-authored banners).
        if matches!(msg.channel, ChatChannel::Say | ChatChannel::Zone)
            && !msg.from.is_empty()
            && !msg.text.is_empty()
        {
            bubbles.write(ChatBubbleEvent {
                from: msg.from.clone(),
                text: msg.text.clone(),
            });
        }
        history.push(msg);
    }
}

#[allow(clippy::too_many_arguments)]
fn chat_ui(
    mut contexts: EguiContexts,
    history: Res<ChatHistory>,
    mut input: ResMut<ChatInput>,
    mut focused: ResMut<ChatInputFocused>,
    keys: Res<ButtonInput<KeyCode>>,
    selected: Option<Res<SelectedCharacter>>,
    mut tx: Query<&mut MessageSender<ChatSend>, With<Client>>,
    mut invite_tx: Query<&mut MessageSender<PartyInviteRequest>, With<Client>>,
    mut leave_tx: Query<&mut MessageSender<PartyLeaveRequest>, With<Client>>,
    mut kick_tx: Query<&mut MessageSender<PartyKickRequest>, With<Client>>,
) {
    let Ok(ctx) = contexts.ctx_mut() else { return };

    // Enter opens the input when it's closed (and no other egui widget
    // has focus). Don't double-trigger on the same Enter that sends.
    if !input.open && keys.just_pressed(KeyCode::Enter) && !ctx.wants_keyboard_input() {
        input.open = true;
        input.buffer.clear();
        input.want_focus = true;
    }

    let own_name = selected
        .as_ref()
        .map(|s| s.name.clone())
        .filter(|n| !n.is_empty())
        .unwrap_or_default();

    egui::Window::new("chat")
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .movable(false)
        .anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(16.0, -16.0))
        .frame(
            egui::Frame::default()
                .fill(egui::Color32::from_rgba_unmultiplied(15, 18, 22, 200))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(70)))
                .inner_margin(egui::Margin::symmetric(8, 6))
                .corner_radius(4.0),
        )
        .show(ctx, |ui| {
            ui.set_width(440.0);

            // History — render last N lines top-to-bottom.
            egui::ScrollArea::vertical()
                .max_height(160.0)
                .stick_to_bottom(true)
                .id_salt("chat_scroll")
                .show(ui, |ui| {
                    for msg in &history.lines {
                        render_line(ui, msg, &own_name);
                    }
                    if history.lines.is_empty() {
                        ui.label(
                            egui::RichText::new("(no messages — press Enter to chat)")
                                .italics()
                                .color(egui::Color32::from_gray(130))
                                .size(11.0),
                        );
                    }
                });

            // Input bar — only visible while open.
            if input.open {
                ui.separator();
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut input.buffer)
                        .desired_width(f32::INFINITY)
                        .hint_text("say / /z zone / /w <name> message …"),
                );
                if input.want_focus {
                    resp.request_focus();
                    input.want_focus = false;
                }
                focused.0 = resp.has_focus();

                if resp.lost_focus() && keys.just_pressed(KeyCode::Enter) {
                    // First try party commands (/invite, /leave, /kick).
                    // If it matches, send the party request and skip
                    // the chat path entirely.
                    let text = input.buffer.trim().to_string();
                    if !text.is_empty() {
                        match parse_party_command(&text) {
                            Some(PartyCommand::Invite { target }) => {
                                if let Ok(mut sender) = invite_tx.single_mut() {
                                    let _ = sender
                                        .send::<Channel1>(PartyInviteRequest { target_name: target });
                                }
                            }
                            Some(PartyCommand::Leave) => {
                                if let Ok(mut sender) = leave_tx.single_mut() {
                                    let _ = sender.send::<Channel1>(PartyLeaveRequest);
                                }
                            }
                            Some(PartyCommand::Kick { target }) => {
                                if let Ok(mut sender) = kick_tx.single_mut() {
                                    let _ = sender
                                        .send::<Channel1>(PartyKickRequest { target_name: target });
                                }
                            }
                            None => {
                                if let Some(send) = parse_send(&text) {
                                    if let Ok(mut sender) = tx.single_mut() {
                                        let _ = sender.send::<Channel1>(send);
                                    }
                                }
                            }
                        }
                    }
                    input.buffer.clear();
                    input.open = false;
                    focused.0 = false;
                } else if keys.just_pressed(KeyCode::Escape) {
                    input.buffer.clear();
                    input.open = false;
                    focused.0 = false;
                }
            } else {
                focused.0 = false;
            }
        });
}

fn render_line(ui: &mut egui::Ui, msg: &ChatMessage, own_name: &str) {
    let (prefix_text, prefix_color) = match msg.channel {
        ChatChannel::Say => (
            format!("{}: ", msg.from),
            egui::Color32::from_rgb(220, 220, 220),
        ),
        ChatChannel::Zone => (
            format!("[Zone] {}: ", msg.from),
            egui::Color32::from_rgb(140, 210, 170),
        ),
        ChatChannel::Party => (
            format!("[Party] {}: ", msg.from),
            egui::Color32::from_rgb(170, 190, 240),
        ),
        ChatChannel::Whisper => {
            let is_echo = !own_name.is_empty() && msg.from == own_name;
            if is_echo {
                (
                    format!("To {}: ", msg.to),
                    egui::Color32::from_rgb(210, 160, 210),
                )
            } else {
                (
                    format!("From {}: ", msg.from),
                    egui::Color32::from_rgb(230, 130, 230),
                )
            }
        }
        ChatChannel::System => (
            format!("[System] {}", msg.from).trim_end().to_string() + " ",
            egui::Color32::from_rgb(230, 220, 130),
        ),
    };
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label(
            egui::RichText::new(prefix_text)
                .strong()
                .color(prefix_color)
                .size(12.0),
        );
        let body_color = match msg.channel {
            ChatChannel::Whisper => egui::Color32::from_rgb(220, 200, 220),
            ChatChannel::System => egui::Color32::from_rgb(230, 220, 130),
            ChatChannel::Party => egui::Color32::from_rgb(205, 215, 240),
            _ => egui::Color32::from_gray(220),
        };
        ui.label(
            egui::RichText::new(&msg.text)
                .color(body_color)
                .size(12.0),
        );
    });
}

/// Turn a typed line into a `ChatSend` (or `None` on a malformed
/// whisper with no body). Unknown `/foo` commands fall through to Say
/// so typos don't eat messages.
fn parse_send(line: &str) -> Option<ChatSend> {
    let trimmed = line.trim();
    if !trimmed.starts_with('/') {
        return Some(ChatSend {
            channel: ChatChannel::Say,
            text: trimmed.to_string(),
            whisper_target: None,
        });
    }
    // Split into command + rest.
    let rest = &trimmed[1..];
    let (cmd, body) = match rest.split_once(char::is_whitespace) {
        Some((c, b)) => (c.to_lowercase(), b.trim()),
        None => (rest.to_lowercase(), ""),
    };
    match cmd.as_str() {
        "s" | "say" => {
            if body.is_empty() {
                None
            } else {
                Some(ChatSend {
                    channel: ChatChannel::Say,
                    text: body.to_string(),
                    whisper_target: None,
                })
            }
        }
        "z" | "zone" => {
            if body.is_empty() {
                None
            } else {
                Some(ChatSend {
                    channel: ChatChannel::Zone,
                    text: body.to_string(),
                    whisper_target: None,
                })
            }
        }
        "p" | "party" => {
            if body.is_empty() {
                None
            } else {
                Some(ChatSend {
                    channel: ChatChannel::Party,
                    text: body.to_string(),
                    whisper_target: None,
                })
            }
        }
        "w" | "whisper" | "tell" | "msg" => {
            // `/w <name> <rest>` — split on the first whitespace.
            let (name, rest) = match body.split_once(char::is_whitespace) {
                Some((n, r)) => (n.trim(), r.trim()),
                None => (body, ""),
            };
            if name.is_empty() || rest.is_empty() {
                None
            } else {
                Some(ChatSend {
                    channel: ChatChannel::Whisper,
                    text: rest.to_string(),
                    whisper_target: Some(name.to_string()),
                })
            }
        }
        // Emotes — translate the command into a third-person body sent on
        // Say channel. The chat-bubble system shows "Brenn: waves." which
        // reads as an emote without needing a separate channel.
        "wave" => emote_send("waves."),
        "bow" => emote_send("bows."),
        "sit" => emote_send("sits down."),
        "cheer" => emote_send("cheers!"),
        "dance" => emote_send("starts dancing!"),
        "point" => emote_send("points."),
        // Unknown command — treat the whole line as Say content so
        // "/dodgethis" doesn't silently vanish.
        _ => Some(ChatSend {
            channel: ChatChannel::Say,
            text: trimmed.to_string(),
            whisper_target: None,
        }),
    }
}

fn emote_send(body: &str) -> Option<ChatSend> {
    Some(ChatSend {
        channel: ChatChannel::Say,
        text: format!("*{body}*"),
        whisper_target: None,
    })
}

fn reset_state(
    mut history: ResMut<ChatHistory>,
    mut input: ResMut<ChatInput>,
    mut focused: ResMut<ChatInputFocused>,
) {
    history.lines.clear();
    input.buffer.clear();
    input.open = false;
    input.want_focus = false;
    focused.0 = false;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_text_parses_as_say() {
        let s = parse_send("hello there").unwrap();
        assert_eq!(s.channel, ChatChannel::Say);
        assert_eq!(s.text, "hello there");
        assert!(s.whisper_target.is_none());
    }

    #[test]
    fn slash_z_parses_as_zone() {
        let s = parse_send("/z anyone near miller's?").unwrap();
        assert_eq!(s.channel, ChatChannel::Zone);
        assert_eq!(s.text, "anyone near miller's?");
    }

    #[test]
    fn slash_w_parses_whisper_with_target() {
        let s = parse_send("/w Merchant_Kell what's the spread on potions?").unwrap();
        assert_eq!(s.channel, ChatChannel::Whisper);
        assert_eq!(s.whisper_target.as_deref(), Some("Merchant_Kell"));
        assert_eq!(s.text, "what's the spread on potions?");
    }

    #[test]
    fn empty_whisper_body_rejects() {
        assert!(parse_send("/w Kell").is_none());
        assert!(parse_send("/w").is_none());
    }

    #[test]
    fn unknown_slash_falls_through_to_say() {
        let s = parse_send("/dodge this fire").unwrap();
        assert_eq!(s.channel, ChatChannel::Say);
        assert_eq!(s.text, "/dodge this fire");
    }

    #[test]
    fn emote_wave_translates_to_third_person_say() {
        let s = parse_send("/wave").unwrap();
        assert_eq!(s.channel, ChatChannel::Say);
        assert_eq!(s.text, "*waves.*");
    }

    #[test]
    fn all_emotes_parse() {
        for cmd in ["/bow", "/sit", "/cheer", "/dance", "/point"] {
            let s = parse_send(cmd).expect(cmd);
            assert_eq!(s.channel, ChatChannel::Say);
            assert!(
                s.text.starts_with('*') && s.text.ends_with('*'),
                "{cmd} → {:?}",
                s.text
            );
        }
    }
}
