//! Network bring-up: spawn the lightyear client entity on `Connecting` entry,
//! ship a `ClientHello` with the selected core pillar once netcode hands back
//! `Connected`. `VAERN_PILLAR` env var still works as a dev override.

use core::net::SocketAddr;

use bevy::prelude::*;
use lightyear::prelude::client::*;
use lightyear::prelude::*;
use vaern_core::pillar::Pillar;
use vaern_protocol::{
    AbandonQuest, AcceptQuest, CLIENT_ADDR, CastFired, CastIntent, Channel1, ClientHello,
    HotbarSnapshot, NetcodeKeySource, ProgressQuest, QuestLogSnapshot, SHARED_PROTOCOL_ID,
    StanceRequest,
};

use crate::menu::{AppState, SelectedCharacter};
use crate::shared::OwnClientId;

pub struct NetworkingPlugin;

impl Plugin for NetworkingPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(send_hello_on_connect)
            .add_systems(OnEnter(AppState::Connecting), connect_to_server);
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
fn connect_to_server(mut commands: Commands, net_config: Res<ClientNetConfig>) -> Result {
    let client_id = resolve_client_id();
    commands.insert_resource(OwnClientId(client_id));

    let server_addr = net_config.server_addr;
    let auth = Authentication::Manual {
        server_addr,
        client_id,
        private_key: net_config.private_key,
        protocol_id: SHARED_PROTOCOL_ID,
    };
    // Bevy tuple bundles cap at 15, so split sender/receiver groups.
    let net_core = (
        Client::default(),
        LocalAddr(CLIENT_ADDR),
        PeerAddr(server_addr),
        Link::new(None),
        ReplicationReceiver::default(),
        NetcodeClient::new(auth, NetcodeConfig::default())?,
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
    let client = commands.spawn((net_core, senders, receivers)).id();
    commands.trigger(Connect { entity: client });
    info!(
        "connecting to {server_addr} as client {client_id} (netcode key: {})",
        net_config.key_source.label()
    );
    Ok(())
}
