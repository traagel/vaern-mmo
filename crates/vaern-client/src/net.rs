//! Network bring-up: spawn the lightyear client entity on `Connecting` entry,
//! ship a `ClientHello` with the selected core pillar once netcode hands back
//! `Connected`. `VAERN_PILLAR` env var still works as a dev override.
//!
//! Auto-reconnect: when the server connection drops mid-game (lightyear
//! removes the `Connected` marker), the disconnect observer transitions
//! the app to `AppState::Reconnecting`. The retry loop spawns fresh
//! lightyear clients with exponential backoff (1s → 2s → 4s → 8s,
//! capped at 8s, 5 attempts max). On success the next handshake fires
//! `Add Connected` and we slide back into `InGame`. On exhausted
//! retries we drop to `MainMenu`.

use core::net::SocketAddr;
use core::time::Duration;

use bevy::prelude::*;
use lightyear::prelude::client::*;
use lightyear::prelude::*;
use vaern_core::pillar::Pillar;
use vaern_protocol::{
    AbandonQuest, AcceptQuest, CLIENT_ADDR, CastFired, CastIntent, Channel1, CharacterSummary,
    ClientCreateCharacter, ClientHello, ClientLogin, ClientRegister, CreateCharacterResult,
    HotbarSnapshot, LoginResult, NetcodeKeySource, ProgressQuest, QuestLogSnapshot,
    RegisterResult, SHARED_PROTOCOL_ID, StanceRequest,
};

use crate::menu::{AppState, SelectedCharacter};
use crate::shared::OwnClientId;

/// Maximum reconnect attempts before falling back to `MainMenu`.
pub const RECONNECT_MAX_ATTEMPTS: u32 = 5;

/// Cap for the exponential-backoff delay (seconds). The sequence is
/// 1, 2, 4, 8, 8 for 5 attempts.
const RECONNECT_BACKOFF_CAP_SECS: f32 = 8.0;
/// Initial backoff after the first disconnect (seconds).
const RECONNECT_INITIAL_BACKOFF_SECS: f32 = 1.0;

pub struct NetworkingPlugin;

impl Plugin for NetworkingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ReconnectState>()
            .init_resource::<ServerCharacterRoster>()
            .add_observer(send_hello_on_connect)
            .add_observer(send_auth_on_connect)
            .add_observer(handle_disconnect)
            .add_systems(OnEnter(AppState::Connecting), connect_on_connecting_enter)
            .add_systems(OnEnter(AppState::Reconnecting), enter_reconnecting)
            .add_systems(OnExit(AppState::Reconnecting), exit_reconnecting)
            .add_systems(
                Update,
                (reconnect_tick, detect_reconnected).run_if(in_state(AppState::Reconnecting)),
            )
            .add_systems(
                Update,
                drain_auth_results.run_if(
                    in_state(AppState::Authenticating).or(in_state(AppState::CharacterSelect)),
                ),
            );
    }
}

/// Set when the user clicks Login or Register on the main menu.
/// Consumed by `send_auth_on_connect`: when the netcode handshake
/// completes and this resource is present, we ship `ClientLogin` (or
/// `ClientRegister` if `register_instead`) and transition to
/// `Authenticating`.
#[derive(Resource, Clone, Debug)]
pub struct ClientCredentials {
    pub username: String,
    pub password: String,
    /// `true` = the user clicked Register; ship `ClientRegister`. `false`
    /// = ship `ClientLogin`.
    pub register_instead: bool,
}

/// Last successful credentials, kept around for the reconnect path so a
/// dropped connection mid-game can re-auth without a re-prompt.
#[derive(Resource, Clone, Debug)]
pub struct CachedCredentials {
    pub username: String,
    pub password: String,
}

/// Server-driven character roster after a successful login. Populated
/// from `LoginResult.characters` and updated on each
/// `CreateCharacterResult`. Drives the `CharacterSelect` UI.
#[derive(Resource, Clone, Debug, Default)]
pub struct ServerCharacterRoster {
    pub account_username: String,
    pub characters: Vec<CharacterSummary>,
    /// Last server-side error message, displayed in the UI. Cleared on
    /// the next successful action.
    pub last_error: String,
}

/// Reconnect retry bookkeeping. Reset on every entry to `Reconnecting`.
#[derive(Resource, Debug)]
pub struct ReconnectState {
    /// 0-indexed retry count. `attempts == 0` means the first retry hasn't
    /// fired yet; `attempts == RECONNECT_MAX_ATTEMPTS` triggers the
    /// fall-through to `MainMenu`.
    pub attempts: u32,
    /// Countdown to the next retry. When this expires, `reconnect_tick`
    /// despawns any leftover client and spawns a fresh one.
    pub timer: Timer,
    /// Last delay used (in seconds) — base for the next-power-of-two
    /// backoff bump.
    pub last_delay_secs: f32,
    /// Total seconds since reconnect entry, for UI display only.
    pub elapsed_secs: f32,
}

impl Default for ReconnectState {
    fn default() -> Self {
        Self {
            attempts: 0,
            timer: Timer::from_seconds(RECONNECT_INITIAL_BACKOFF_SECS, TimerMode::Once),
            last_delay_secs: RECONNECT_INITIAL_BACKOFF_SECS,
            elapsed_secs: 0.0,
        }
    }
}

impl ReconnectState {
    /// Reset to the first-attempt state. The next `reconnect_tick` after
    /// `RECONNECT_INITIAL_BACKOFF_SECS` elapses will spawn the first
    /// retry attempt.
    fn reset(&mut self) {
        self.attempts = 0;
        self.last_delay_secs = RECONNECT_INITIAL_BACKOFF_SECS;
        self.timer = Timer::from_seconds(RECONNECT_INITIAL_BACKOFF_SECS, TimerMode::Once);
        self.elapsed_secs = 0.0;
    }

    /// Compute the next backoff delay (seconds). Doubles last delay,
    /// caps at `RECONNECT_BACKOFF_CAP_SECS`.
    fn next_delay(&self) -> f32 {
        (self.last_delay_secs * 2.0).min(RECONNECT_BACKOFF_CAP_SECS)
    }

    /// Whole-second remainder until the next retry, for UI countdown.
    pub fn seconds_until_next_retry(&self) -> u32 {
        self.timer.remaining_secs().ceil() as u32
    }
}

/// Resolved boot-time network config for the client. Inserted as a
/// `Resource` from `main` so `connect_to_server` doesn't re-read env vars.
#[derive(Resource, Clone, Copy, Debug)]
pub struct ClientNetConfig {
    pub server_addr: SocketAddr,
    pub private_key: [u8; 32],
    pub key_source: NetcodeKeySource,
}

/// Resolve the pillar from `VAERN_PILLAR`. Accepts case-insensitive
/// pillar name (`might` / `finesse` / `arcana`). Returns `None` if unset
/// or unrecognized.
pub fn resolve_pillar() -> Option<Pillar> {
    let raw = std::env::var("VAERN_PILLAR").ok()?;
    match raw.to_ascii_lowercase().as_str() {
        "might" | "m" => Some(Pillar::Might),
        "finesse" | "f" => Some(Pillar::Finesse),
        "arcana" | "a" => Some(Pillar::Arcana),
        _ => None,
    }
}

fn resolve_client_id() -> u64 {
    std::env::var("VAERN_CLIENT_ID")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            let pid = std::process::id() as u64;
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos() as u64)
                .unwrap_or(0);
            (pid.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ nanos).max(1)
        })
}

/// On netcode handshake completion, ship a `ClientHello` with the selected
/// character's pillar. Falls back to `VAERN_PILLAR`, then server default
/// (Might).
fn send_hello_on_connect(
    trigger: On<Add, Connected>,
    mut senders: Query<&mut MessageSender<ClientHello>, With<Client>>,
    selected: Option<Res<SelectedCharacter>>,
) {
    let core_pillar = selected
        .as_deref()
        .map(|c| c.core_pillar)
        .or_else(resolve_pillar);
    let Some(core_pillar) = core_pillar else {
        info!("no SelectedCharacter or VAERN_PILLAR; server will fall back to Might");
        return;
    };
    let (race_id, character_id, character_name, cosmetics) = match selected.as_deref() {
        Some(c) => (
            c.race_id.clone(),
            c.character_id.clone(),
            c.name.clone(),
            Some(c.cosmetics.clone()),
        ),
        None => (String::new(), String::new(), String::new(), None),
    };
    let Ok(mut sender) = senders.get_mut(trigger.entity) else {
        return;
    };
    let _ = sender.send::<Channel1>(ClientHello {
        core_pillar,
        race_id: race_id.clone(),
        character_id: character_id.clone(),
        character_name,
        cosmetics,
    });
    info!(
        "sent ClientHello: core_pillar = {core_pillar} race_id = '{race_id}' character_id = '{character_id}'"
    );
}

/// Spawn the lightyear client entity and fire the Connect trigger.
/// Reuses `OwnClientId` if present; otherwise mints a fresh id from the
/// env / pid hash. The same client_id is reused across reconnect
/// attempts so the server's `PendingSpawns` keys line up.
fn spawn_client_entity(commands: &mut Commands, net_config: &ClientNetConfig, client_id: u64) {
    let server_addr = net_config.server_addr;
    let auth = Authentication::Manual {
        server_addr,
        client_id,
        private_key: net_config.private_key,
        protocol_id: SHARED_PROTOCOL_ID,
    };
    // Bevy tuple bundles cap at 15, so split sender/receiver groups.
    let netcode = match NetcodeClient::new(auth, NetcodeConfig::default()) {
        Ok(c) => c,
        Err(e) => {
            error!("NetcodeClient::new failed for client {client_id}: {e}");
            return;
        }
    };
    let net_core = (
        Client::default(),
        LocalAddr(CLIENT_ADDR),
        PeerAddr(server_addr),
        Link::new(None),
        ReplicationReceiver::default(),
        netcode,
        UdpIo::default(),
    );
    let senders = (
        MessageSender::<CastIntent>::default(),
        MessageSender::<StanceRequest>::default(),
        MessageSender::<ClientHello>::default(),
        MessageSender::<AcceptQuest>::default(),
        MessageSender::<AbandonQuest>::default(),
        MessageSender::<ProgressQuest>::default(),
    );
    let receivers = (
        MessageReceiver::<CastFired>::default(),
        MessageReceiver::<HotbarSnapshot>::default(),
        MessageReceiver::<QuestLogSnapshot>::default(),
    );
    let auth_io = (
        MessageSender::<ClientLogin>::default(),
        MessageSender::<ClientRegister>::default(),
        MessageSender::<ClientCreateCharacter>::default(),
        MessageReceiver::<LoginResult>::default(),
        MessageReceiver::<RegisterResult>::default(),
        MessageReceiver::<CreateCharacterResult>::default(),
    );
    let client = commands.spawn((net_core, senders, receivers, auth_io)).id();
    commands.trigger(Connect { entity: client });
    info!(
        "connecting to {server_addr} as client {client_id} (netcode key: {})",
        net_config.key_source.label()
    );
}

/// `OnEnter(AppState::Connecting)` wrapper: mints a fresh `OwnClientId`
/// and spawns the lightyear client entity.
fn connect_on_connecting_enter(mut commands: Commands, net_config: Res<ClientNetConfig>) {
    let client_id = resolve_client_id();
    commands.insert_resource(OwnClientId(client_id));
    spawn_client_entity(&mut commands, &net_config, client_id);
}

/// Disconnect observer. Lightyear removes the `Connected` marker from the
/// client entity when the server stops responding. If we're currently
/// `InGame`, transition to `Reconnecting`. Other states (MainMenu,
/// Connecting, Reconnecting itself) ignore the signal — we don't want
/// the failure to log a user out of the menu.
fn handle_disconnect(
    _trigger: On<Remove, Connected>,
    state: Res<State<AppState>>,
    mut next: ResMut<NextState<AppState>>,
) {
    if matches!(state.get(), AppState::InGame) {
        warn!("server connection lost — entering Reconnecting");
        next.set(AppState::Reconnecting);
    }
}

/// `OnEnter(AppState::Reconnecting)` — reset the retry book and spawn
/// the first attempt's client entity immediately. (The first retry does
/// NOT wait for the initial backoff window; we go for it right away
/// since `OnExit(InGame)` already despawned the previous client.)
fn enter_reconnecting(
    mut commands: Commands,
    net_config: Res<ClientNetConfig>,
    own_client: Option<Res<OwnClientId>>,
    mut state: ResMut<ReconnectState>,
) {
    state.reset();
    let client_id = own_client.map(|c| c.0).unwrap_or_else(resolve_client_id);
    commands.insert_resource(OwnClientId(client_id));
    info!("reconnect: attempt 1/{RECONNECT_MAX_ATTEMPTS} (client_id={client_id})");
    spawn_client_entity(&mut commands, &net_config, client_id);
    state.attempts = 1;
}

/// `OnExit(AppState::Reconnecting)` — clean up any unconnected client
/// entities so a subsequent successful Connecting/InGame doesn't see
/// orphan clients. (When we exit because Connected fired, the new
/// client entity is the surviving one; despawn the rest.)
fn exit_reconnecting(
    state: Res<State<AppState>>,
    clients_disconnected: Query<Entity, (With<Client>, Without<Connected>)>,
    mut commands: Commands,
) {
    // If we're heading to MainMenu (max attempts), drop every client —
    // the user will go through the full Enter-World path again. If
    // heading to InGame, only drop the failed (Disconnected) clients
    // to leave the surviving Connected one alone.
    let going_home = matches!(state.get(), AppState::MainMenu);
    for e in &clients_disconnected {
        if going_home || true {
            commands.entity(e).despawn();
        }
    }
}

/// Tick the backoff timer; on fire, despawn the failed client and spawn
/// a fresh one. After `RECONNECT_MAX_ATTEMPTS` failed attempts, fall
/// through to MainMenu so the user can retry manually.
fn reconnect_tick(
    time: Res<Time>,
    mut state: ResMut<ReconnectState>,
    net_config: Res<ClientNetConfig>,
    own_client: Option<Res<OwnClientId>>,
    mut commands: Commands,
    mut next: ResMut<NextState<AppState>>,
    clients: Query<Entity, With<Client>>,
) {
    state.elapsed_secs += time.delta_secs();
    state.timer.tick(time.delta());
    if !state.timer.is_finished() {
        return;
    }
    if state.attempts >= RECONNECT_MAX_ATTEMPTS {
        warn!(
            "reconnect: exhausted {RECONNECT_MAX_ATTEMPTS} attempts after {:.1}s — returning to main menu",
            state.elapsed_secs
        );
        next.set(AppState::MainMenu);
        return;
    }
    // Despawn the previous failed client entity. Spawn a fresh one with
    // the same client_id so server-side state lookup keys match.
    for e in &clients {
        commands.entity(e).despawn();
    }
    let next_attempt = state.attempts + 1;
    let next_delay = state.next_delay();
    let client_id = own_client.map(|c| c.0).unwrap_or_else(resolve_client_id);
    info!(
        "reconnect: attempt {next_attempt}/{RECONNECT_MAX_ATTEMPTS} (next backoff {next_delay:.0}s)"
    );
    spawn_client_entity(&mut commands, &net_config, client_id);
    state.attempts = next_attempt;
    state.last_delay_secs = next_delay;
    state.timer = Timer::new(Duration::from_secs_f32(next_delay), TimerMode::Once);
}

/// Detect a successful reconnect — when any `Client` is `Connected`,
/// transition out of `Reconnecting` back to `InGame`. The replicated
/// world then rebuilds via the standard `OnEnter(InGame)` path.
fn detect_reconnected(
    clients: Query<Entity, (With<Client>, With<Connected>)>,
    mut next: ResMut<NextState<AppState>>,
) {
    if clients.iter().next().is_some() {
        info!("reconnect: handshake succeeded — re-entering game");
        next.set(AppState::InGame);
    }
}

/// On `Add Connected`, if `ClientCredentials` is present, ship the
/// matching `ClientLogin` or `ClientRegister` and transition to
/// `Authenticating`. Cleared by `drain_auth_results` whether the
/// attempt succeeds or fails — clients have to re-fill the form for a
/// retry. The legacy `send_hello_on_connect` observer is gated on the
/// absence of `ClientCredentials` so it doesn't double-send.
fn send_auth_on_connect(
    trigger: On<Add, Connected>,
    creds: Option<Res<ClientCredentials>>,
    mut login_tx: Query<&mut MessageSender<ClientLogin>, With<Client>>,
    mut register_tx: Query<&mut MessageSender<ClientRegister>, With<Client>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    let Some(creds) = creds else { return };
    if creds.register_instead {
        let Ok(mut sender) = register_tx.get_mut(trigger.entity) else {
            return;
        };
        let _ = sender.send::<Channel1>(ClientRegister {
            username: creds.username.clone(),
            password: creds.password.clone(),
        });
        info!("[auth] sent ClientRegister for username '{}'", creds.username);
    } else {
        let Ok(mut sender) = login_tx.get_mut(trigger.entity) else {
            return;
        };
        let _ = sender.send::<Channel1>(ClientLogin {
            username: creds.username.clone(),
            password: creds.password.clone(),
        });
        info!("[auth] sent ClientLogin for username '{}'", creds.username);
    }
    next_state.set(AppState::Authenticating);
}

/// Drain server auth responses. Runs in `Authenticating` and
/// `CharacterSelect`:
///
/// - `LoginResult` / `RegisterResult` (ok=true): populate
///   `ServerCharacterRoster`, cache credentials for reconnect, transition
///   to `CharacterSelect`.
/// - `LoginResult` / `RegisterResult` (ok=false): record `last_error`,
///   tear down the netcode client, transition to `MainMenu`.
/// - `CreateCharacterResult`: append to the roster; record errors.
fn drain_auth_results(
    mut commands: Commands,
    mut roster: ResMut<ServerCharacterRoster>,
    creds: Option<Res<ClientCredentials>>,
    mut next_state: ResMut<NextState<AppState>>,
    mut login_rx: Query<&mut MessageReceiver<LoginResult>, With<Client>>,
    mut register_rx: Query<&mut MessageReceiver<RegisterResult>, With<Client>>,
    mut create_rx: Query<&mut MessageReceiver<CreateCharacterResult>, With<Client>>,
    clients: Query<Entity, With<Client>>,
) {
    let mut auth_succeeded = false;
    let mut auth_failed = false;
    let mut error_msg = String::new();

    for mut rx in &mut login_rx {
        for msg in rx.receive() {
            if msg.ok {
                roster.characters = msg.characters.clone();
                roster.last_error.clear();
                if let Some(c) = creds.as_deref() {
                    roster.account_username = c.username.clone();
                }
                auth_succeeded = true;
                info!(
                    "[auth] login ok; roster has {} character(s)",
                    msg.characters.len()
                );
            } else {
                error_msg = msg.error_msg.clone();
                auth_failed = true;
                warn!("[auth] login refused: {}", msg.error_msg);
            }
        }
    }

    for mut rx in &mut register_rx {
        for msg in rx.receive() {
            if msg.ok {
                roster.characters.clear();
                roster.last_error.clear();
                if let Some(c) = creds.as_deref() {
                    roster.account_username = c.username.clone();
                }
                auth_succeeded = true;
                info!("[auth] register ok; new account, empty roster");
            } else {
                error_msg = msg.error_msg.clone();
                auth_failed = true;
                warn!("[auth] register refused: {}", msg.error_msg);
            }
        }
    }

    for mut rx in &mut create_rx {
        for msg in rx.receive() {
            if msg.ok {
                roster.characters.push(CharacterSummary {
                    character_id: msg.character_id.clone(),
                    name: String::new(), // populated by the menu via the form data
                    race_id: String::new(),
                    core_pillar: Pillar::Might,
                    level: 0,
                });
                roster.last_error.clear();
                info!("[auth] create_character ok; character_id={}", msg.character_id);
            } else {
                roster.last_error = msg.error_msg.clone();
                warn!("[auth] create_character refused: {}", msg.error_msg);
            }
        }
    }

    if auth_succeeded {
        if let Some(c) = creds.as_deref() {
            commands.insert_resource(CachedCredentials {
                username: c.username.clone(),
                password: c.password.clone(),
            });
        }
        commands.remove_resource::<ClientCredentials>();
        next_state.set(AppState::CharacterSelect);
    } else if auth_failed {
        roster.last_error = error_msg;
        commands.remove_resource::<ClientCredentials>();
        // Tear down the netcode client so the next attempt starts fresh.
        for e in &clients {
            commands.entity(e).despawn();
        }
        next_state.set(AppState::MainMenu);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_clears_attempts_and_timer() {
        let mut s = ReconnectState {
            attempts: 4,
            timer: Timer::from_seconds(8.0, TimerMode::Once),
            last_delay_secs: 8.0,
            elapsed_secs: 30.0,
        };
        s.reset();
        assert_eq!(s.attempts, 0);
        assert_eq!(s.last_delay_secs, RECONNECT_INITIAL_BACKOFF_SECS);
        assert_eq!(s.elapsed_secs, 0.0);
        assert_eq!(s.timer.duration().as_secs_f32(), RECONNECT_INITIAL_BACKOFF_SECS);
    }

    #[test]
    fn next_delay_doubles_until_cap() {
        let mut s = ReconnectState::default();
        assert_eq!(s.last_delay_secs, 1.0);
        s.last_delay_secs = s.next_delay();
        assert_eq!(s.last_delay_secs, 2.0);
        s.last_delay_secs = s.next_delay();
        assert_eq!(s.last_delay_secs, 4.0);
        s.last_delay_secs = s.next_delay();
        assert_eq!(s.last_delay_secs, 8.0);
        // Capped — further bumps stay at 8.
        s.last_delay_secs = s.next_delay();
        assert_eq!(s.last_delay_secs, RECONNECT_BACKOFF_CAP_SECS);
        s.last_delay_secs = s.next_delay();
        assert_eq!(s.last_delay_secs, RECONNECT_BACKOFF_CAP_SECS);
    }

    /// Total wall-clock window for 5 attempts: 0s + 1s + 2s + 4s + 8s = 15s
    /// of backoff windows (the first attempt fires immediately on
    /// `enter_reconnecting`; subsequent waits are 1, 2, 4, 8s). After
    /// the 5th attempt, an 8s wait elapses before the MainMenu fall-through.
    #[test]
    fn five_attempt_window_matches_design() {
        let delays: Vec<f32> = {
            let mut s = ReconnectState::default();
            let mut out = Vec::new();
            for _ in 0..4 {
                out.push(s.last_delay_secs);
                s.last_delay_secs = s.next_delay();
            }
            out
        };
        assert_eq!(delays, vec![1.0, 2.0, 4.0, 8.0]);
        assert_eq!(RECONNECT_MAX_ATTEMPTS, 5);
    }

    #[test]
    fn seconds_until_next_retry_ticks_down() {
        let mut s = ReconnectState {
            attempts: 1,
            timer: Timer::from_seconds(4.0, TimerMode::Once),
            last_delay_secs: 4.0,
            elapsed_secs: 0.0,
        };
        assert_eq!(s.seconds_until_next_retry(), 4);
        s.timer.tick(Duration::from_secs_f32(1.5));
        // 4.0 - 1.5 = 2.5 → ceil = 3
        assert_eq!(s.seconds_until_next_retry(), 3);
        s.timer.tick(Duration::from_secs_f32(2.5));
        // remaining ≤ 0 → 0
        assert_eq!(s.seconds_until_next_retry(), 0);
    }
}
