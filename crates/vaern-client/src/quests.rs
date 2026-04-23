//! Client-side quest plumbing:
//!   - load chain YAMLs for the active zone at startup (same data the server
//!     has, for display-only rendering)
//!   - drain `MessageReceiver<QuestLogSnapshot>` → `PlayerQuestLog` resource
//!   - helpers + senders for Accept / Progress / Abandon actions
//!
//! Dialogue + quest log UI consume `PlayerQuestLog` + `ZoneChains`.

use std::collections::HashMap;
use std::path::Path;

use bevy::prelude::*;
use lightyear::prelude::client::Client;
use lightyear::prelude::*;
use vaern_data::QuestChain;
use vaern_protocol::{
    AbandonQuest, AcceptQuest, Channel1, ProgressQuest, QuestLogEntry, QuestLogSnapshot,
};

use crate::menu::AppState;

pub struct QuestsPlugin;

impl Plugin for QuestsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ZoneChains::load_all())
            .init_resource::<PlayerQuestLog>()
            .add_systems(
                Update,
                ingest_quest_log_snapshot.run_if(in_state(AppState::InGame)),
            )
            .add_systems(OnExit(AppState::InGame), reset_log);
    }
}

// ─── resources ──────────────────────────────────────────────────────────────

/// All chains across all zones, with a zone-id → Vec<chain_id> index for
/// filtering in the dialogue UI.
#[derive(Resource, Debug, Default)]
pub struct ZoneChains {
    pub chains: Vec<QuestChain>,
    pub by_zone: HashMap<String, Vec<String>>,
}

impl ZoneChains {
    pub fn load_all() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src/generated/world");
        match vaern_data::load_all_chains(&root) {
            Ok(idx) => {
                let chains: Vec<_> = idx.chains.values().cloned().collect();
                Self {
                    chains,
                    by_zone: idx.by_zone,
                }
            }
            Err(e) => {
                error!("failed to load quest chains: {e:?}");
                Self::default()
            }
        }
    }

    pub fn find(&self, chain_id: &str) -> Option<&QuestChain> {
        self.chains.iter().find(|c| c.id == chain_id)
    }

    pub fn chains_in_zone(&self, zone_id: &str) -> impl Iterator<Item = &QuestChain> {
        self.by_zone
            .get(zone_id)
            .into_iter()
            .flat_map(move |ids| {
                ids.iter().filter_map(move |id| self.find(id))
            })
    }
}

/// Server-authoritative view of the owning player's quest log. Populated by
/// draining `QuestLogSnapshot` messages on every change.
#[derive(Resource, Default, Debug)]
pub struct PlayerQuestLog {
    pub entries: HashMap<String, QuestLogEntry>,
}

impl PlayerQuestLog {
    pub fn get(&self, chain_id: &str) -> Option<&QuestLogEntry> {
        self.entries.get(chain_id)
    }

    pub fn active(&self) -> impl Iterator<Item = &QuestLogEntry> {
        self.entries.values().filter(|e| !e.completed)
    }

    pub fn is_completed(&self, chain_id: &str) -> bool {
        self.entries
            .get(chain_id)
            .is_some_and(|e| e.completed)
    }

    pub fn is_active(&self, chain_id: &str) -> bool {
        self.entries
            .get(chain_id)
            .is_some_and(|e| !e.completed)
    }
}

// ─── systems ────────────────────────────────────────────────────────────────

fn ingest_quest_log_snapshot(
    mut receivers: Query<&mut MessageReceiver<QuestLogSnapshot>, With<Client>>,
    mut log: ResMut<PlayerQuestLog>,
) {
    let Ok(mut rx) = receivers.single_mut() else { return };
    for snap in rx.receive() {
        log.entries = snap
            .entries
            .into_iter()
            .map(|e| (e.chain_id.clone(), e))
            .collect();
        info!(
            "[quest-log] snapshot: {} entries ({} active)",
            log.entries.len(),
            log.entries.values().filter(|e| !e.completed).count()
        );
    }
}

fn reset_log(mut log: ResMut<PlayerQuestLog>) {
    log.entries.clear();
}

// ─── send helpers (called from dialogue + log UI) ──────────────────────────

pub fn send_accept(commands: &mut Commands, chain_id: String) {
    // Lightyear message senders need a world query; we defer via an
    // exclusive system-scoped closure using commands.queue.
    commands.queue(move |world: &mut World| {
        let mut q = world.query_filtered::<&mut MessageSender<AcceptQuest>, With<Client>>();
        for mut sender in q.iter_mut(world) {
            let _ = sender.send::<Channel1>(AcceptQuest {
                chain_id: chain_id.clone(),
            });
        }
    });
}

pub fn send_abandon(commands: &mut Commands, chain_id: String) {
    commands.queue(move |world: &mut World| {
        let mut q = world.query_filtered::<&mut MessageSender<AbandonQuest>, With<Client>>();
        for mut sender in q.iter_mut(world) {
            let _ = sender.send::<Channel1>(AbandonQuest {
                chain_id: chain_id.clone(),
            });
        }
    });
}

pub fn send_progress(commands: &mut Commands, chain_id: String) {
    commands.queue(move |world: &mut World| {
        let mut q = world.query_filtered::<&mut MessageSender<ProgressQuest>, With<Client>>();
        for mut sender in q.iter_mut(world) {
            let _ = sender.send::<Channel1>(ProgressQuest {
                chain_id: chain_id.clone(),
            });
        }
    });
}
