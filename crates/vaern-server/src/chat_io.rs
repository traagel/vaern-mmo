//! Chat routing.
//!
//! Channels:
//!   * Say     — broadcast to every player within `SAY_RADIUS` of sender
//!   * Zone    — broadcast to every player whose AoI room matches sender's
//!   * Whisper — single recipient by display name (case-insensitive)
//!   * System  — server-authored; not driven by clients
//!
//! Server is authoritative on `from` — it reads the sender player's
//! `DisplayName` and stamps the outbound `ChatMessage`. Clients never
//! name themselves on the wire.
//!
//! Rate limit: `MAX_MESSAGES_PER_SECOND` per sender on a rolling
//! 1-second window. Extra messages drop silently (logged as debug).
//! Payload truncated to `MAX_TEXT_CHARS` UTF-8 chars.
//!
//! The sender of a whisper gets an echo of their own message so their
//! client can render "To X: ..." without a request/response roundtrip.

use std::collections::{HashMap, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};

use bevy::log::{debug, info};
use bevy::prelude::*;
use lightyear::prelude::server::*;
use lightyear::prelude::*;

use vaern_combat::DisplayName;
use vaern_protocol::{Channel1, ChatChannel, ChatMessage, ChatSend, PlayerTag};

use crate::aoi::ClientZone;
use crate::party_io::PartyTable;

/// Radius for proximity `/say`. Covers a hub cluster comfortably;
/// wider than combat auto-attack range but short enough that it
/// behaves like actual proximity chat.
pub const SAY_RADIUS: f32 = 20.0;
pub const MAX_TEXT_CHARS: usize = 256;
pub const MAX_MESSAGES_PER_SECOND: usize = 5;

/// Rolling 1-second timestamp buffer per sender for rate-limiting.
/// Trimmed on every incoming message so the map doesn't grow
/// unboundedly under high churn.
#[derive(Resource, Default)]
pub struct ChatRateLimiter {
    // sender client_id → recent message wall-clock timestamps (ms)
    windows: HashMap<u64, VecDeque<u64>>,
}

impl ChatRateLimiter {
    /// Returns `true` if the sender is within budget and the current
    /// message should be forwarded. Side-effect: inserts the
    /// timestamp on success.
    fn allow(&mut self, client_id: u64, now_ms: u64) -> bool {
        let window = self.windows.entry(client_id).or_default();
        let cutoff = now_ms.saturating_sub(1000);
        while window.front().is_some_and(|t| *t < cutoff) {
            window.pop_front();
        }
        if window.len() >= MAX_MESSAGES_PER_SECOND {
            return false;
        }
        window.push_back(now_ms);
        true
    }
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    let mut out = String::with_capacity(text.len().min(max_chars * 4));
    for (i, c) in text.chars().enumerate() {
        if i >= max_chars {
            break;
        }
        out.push(c);
    }
    out
}

/// Drain every link's `ChatSend` queue, resolve the sender's
/// `DisplayName`, apply channel routing, and push outbound
/// `ChatMessage`s to recipient links.
pub fn handle_chat_messages(
    mut links: Query<(&RemoteId, &mut MessageReceiver<ChatSend>), With<ClientOf>>,
    players: Query<(&PlayerTag, &Transform, &ControlledBy, &DisplayName)>,
    client_zone: Res<ClientZone>,
    party_table: Res<PartyTable>,
    mut rate: ResMut<ChatRateLimiter>,
    mut sender: Query<&mut MessageSender<ChatMessage>, With<ClientOf>>,
) {
    // Materialize the list of drained messages first so we don't hold
    // a mutable link query while iterating recipients.
    let mut to_dispatch: Vec<(u64, ChatSend)> = Vec::new();
    let now_ms = now_unix_ms();
    for (remote, mut rx) in &mut links {
        let PeerId::Netcode(client_id) = remote.0 else { continue };
        for msg in rx.receive() {
            if !rate.allow(client_id, now_ms) {
                debug!("[chat] client {client_id} rate-limited");
                continue;
            }
            if msg.text.trim().is_empty() {
                continue;
            }
            to_dispatch.push((client_id, msg));
        }
    }

    // Build a lookup from client_id → (DisplayName, Transform, link Entity)
    // once. Every send needs these; O(players) per tick regardless of
    // message count is the right shape at pre-alpha scale.
    let mut by_client: HashMap<
        u64,
        (String, Vec3, Entity, String /* zone */),
    > = HashMap::new();
    for (tag, tf, cb, name) in &players {
        let zone = client_zone
            .0
            .get(&tag.client_id)
            .cloned()
            .unwrap_or_default();
        by_client.insert(
            tag.client_id,
            (name.0.clone(), tf.translation, cb.owner, zone),
        );
    }

    for (client_id, send) in to_dispatch {
        let Some((from_name, from_pos, _sender_link, from_zone)) =
            by_client.get(&client_id).cloned()
        else {
            continue;
        };
        let text = truncate_chars(send.text.trim(), MAX_TEXT_CHARS);
        if text.is_empty() {
            continue;
        }

        let ts = now_ms / 1000;
        let recipients: Vec<Entity> = match send.channel {
            ChatChannel::Say => by_client
                .values()
                .filter(|(_, pos, _, _)| pos.distance(from_pos) <= SAY_RADIUS)
                .map(|(_, _, link, _)| *link)
                .collect(),
            ChatChannel::Zone => by_client
                .values()
                .filter(|(_, _, _, zone)| zone == &from_zone)
                .map(|(_, _, link, _)| *link)
                .collect(),
            ChatChannel::Whisper => {
                let Some(target_name) = send
                    .whisper_target
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                else {
                    continue;
                };
                // Case-insensitive match on display name. Also whisper
                // back to the sender so they see an echo.
                let mut recs: Vec<Entity> = by_client
                    .values()
                    .filter(|(name, _, _, _)| name.eq_ignore_ascii_case(target_name))
                    .map(|(_, _, link, _)| *link)
                    .collect();
                if let Some((_, _, sender_link, _)) = by_client.get(&client_id) {
                    if !recs.contains(sender_link) {
                        recs.push(*sender_link);
                    }
                }
                // Preserve the exact target capitalization the sender typed,
                // so the "To X:" echo reads with their spelling.
                let to_field = target_name.to_string();
                send_to_recipients(
                    &mut sender,
                    &recs,
                    ChatMessage {
                        channel: ChatChannel::Whisper,
                        from: from_name.clone(),
                        to: to_field,
                        text: text.clone(),
                        timestamp_unix: ts,
                    },
                );
                info!(
                    "[chat] whisper {from_name} → {target_name} ({} recipients)",
                    recs.len()
                );
                continue;
            }
            ChatChannel::Party => {
                // Look up the sender's party; ship the message to every
                // member's link across zones. Dropped silently if the
                // sender is solo.
                let Some(party) = party_table.party_of(client_id) else {
                    debug!("[chat] {from_name} tried /p while solo — dropped");
                    continue;
                };
                let recs: Vec<Entity> = party
                    .members
                    .iter()
                    .filter_map(|cid| by_client.get(cid).map(|(_, _, link, _)| *link))
                    .collect();
                info!(
                    "[chat] party {from_name}: {} ({} recipients)",
                    text,
                    recs.len()
                );
                send_to_recipients(
                    &mut sender,
                    &recs,
                    ChatMessage {
                        channel: ChatChannel::Party,
                        from: from_name.clone(),
                        to: String::new(),
                        text: text.clone(),
                        timestamp_unix: ts,
                    },
                );
                continue;
            }
            ChatChannel::System => {
                // Server reserves System for broadcast-to-all; clients
                // can't originate. Drop silently.
                debug!("[chat] client {client_id} tried System channel — dropped");
                continue;
            }
        };
        let channel_tag = match send.channel {
            ChatChannel::Say => "say",
            ChatChannel::Zone => "zone",
            _ => "?",
        };
        info!(
            "[chat] {channel_tag} {from_name}: {} ({} recipients)",
            text,
            recipients.len()
        );
        send_to_recipients(
            &mut sender,
            &recipients,
            ChatMessage {
                channel: send.channel,
                from: from_name,
                to: String::new(),
                text,
                timestamp_unix: ts,
            },
        );
    }
}

fn send_to_recipients(
    sender: &mut Query<&mut MessageSender<ChatMessage>, With<ClientOf>>,
    recipients: &[Entity],
    msg: ChatMessage,
) {
    for link in recipients {
        if let Ok(mut tx) = sender.get_mut(*link) {
            let _ = tx.send::<Channel1>(msg.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limit_allows_up_to_cap() {
        let mut rl = ChatRateLimiter::default();
        let now = 1_000_000u64;
        for _ in 0..MAX_MESSAGES_PER_SECOND {
            assert!(rl.allow(42, now));
        }
        // Cap+1 within the same second is rejected.
        assert!(!rl.allow(42, now));
    }

    #[test]
    fn rate_limit_window_rolls_after_one_second() {
        let mut rl = ChatRateLimiter::default();
        let t0 = 1_000_000u64;
        for _ in 0..MAX_MESSAGES_PER_SECOND {
            assert!(rl.allow(7, t0));
        }
        // 1.1s later the window has rolled; room for more.
        assert!(rl.allow(7, t0 + 1_100));
    }

    #[test]
    fn truncate_handles_multibyte() {
        let s = "héllo wörld";
        let t = truncate_chars(s, 5);
        assert_eq!(t.chars().count(), 5);
        assert_eq!(t, "héllo");
    }

    #[test]
    fn truncate_shorter_than_max_is_noop() {
        let t = truncate_chars("hi", 256);
        assert_eq!(t, "hi");
    }
}
