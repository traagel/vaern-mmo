//! Authoritative game server. Runs the combat simulation headlessly, listens
//! on UDP for clients via lightyear+netcode, and replicates entities to them.
//!
//! `main.rs` is bootstrap + schedule-registration only. Real logic lives in:
//!
//! - `data`       — GameData + YAML loaders, XP curve loader
//! - `connect`    — client handshake, deferred player spawn, hotbar snapshot
//! - `npc`        — NPC components, spawn table, AI (aggro/chase/leash/roam)
//! - `quests`     — QuestLog, message handling, kill-objective observer
//! - `xp`         — mob-death XP awards + level-up loop
//! - `combat_io`  — CastIntent in, CastFired out (netcode bridge)
//! - `movement`   — per-tick WASD application to player Transforms
//! - `util`       — small name-prettify helpers
//! - `class_kits` — per-class ability kit builder
//! - `logging`    — diagnostics

mod aoi;
mod belt_io;
mod chat_io;
mod class_kits;
mod combat_io;
mod connect;
mod consume_io;
mod data;
mod inventory_io;
mod logging;
mod loot_io;
mod movement;
mod npc;
mod npc_mesh;
mod party_io;
mod persistence;
mod player_state;
mod quests;
mod resource_nodes;
mod respawn;
mod starter_gear;
mod stats_sync;
mod util;
mod vendor_io;
mod voxel_world;
mod wallet_io;
mod xp;

use core::time::Duration;

use bevy::app::ScheduleRunnerPlugin;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use lightyear::prelude::RoomPlugin;
use vaern_combat::{CombatPlugin, systems as combat_systems};
use vaern_persistence::ServerCharacterStore;
use vaern_voxel::plugin::VoxelCorePlugin;
use vaern_protocol::{
    FIXED_TIMESTEP_HZ, SERVER_ADDR, SHARED_PRIVATE_KEY, SHARED_PROTOCOL_ID, SharedPlugin,
};

use persistence::CharacterStore;

fn main() {
    let tick = Duration::from_secs_f64(1.0 / FIXED_TIMESTEP_HZ);
    App::new()
        .add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(tick)))
        // Route tracing output to stdout. Default filter keeps player-level
        // events (info!) visible; the NPC firehose lives at debug! — enable
        // with `RUST_LOG=vaern_server=debug`.
        .add_plugins(LogPlugin::default())
        .add_plugins(TransformPlugin)
        .add_plugins(ServerPlugins { tick_duration: tick })
        .add_plugins(SharedPlugin)
        .add_plugins(CombatPlugin)
        // Authoritative voxel world. `VoxelCorePlugin` provides the
        // `ChunkStore` + `DirtyChunks` resources; `VoxelServerPlugin`
        // (our own) streams chunks around each active player every
        // fixed tick so `vaern_voxel::query::ground_y` has data to
        // probe against from `movement` + `npc::ai`. Edit strokes land
        // on this store; delta replication is a follow-up slice.
        .add_plugins(VoxelCorePlugin)
        .add_plugins(voxel_world::VoxelServerPlugin)
        .add_plugins(RoomPlugin)
        .add_plugins(logging::LoggingPlugin)
        .insert_resource(npc::NpcSpawns::default())
        .insert_resource(connect::PendingSpawns::default())
        .insert_resource(data::load_game_data())
        .insert_resource(data::load_xp_curve())
        .insert_resource(load_npc_mesh_map())
        .init_resource::<loot_io::LootRng>()
        .init_resource::<loot_io::LootIdCounter>()
        .init_resource::<loot_io::PendingLootsDirty>()
        .init_resource::<vendor_io::VendorIdCounter>()
        .init_resource::<chat_io::ChatRateLimiter>()
        .init_resource::<party_io::PartyTable>()
        .init_resource::<party_io::PendingInvites>()
        .init_resource::<aoi::ZoneRooms>()
        .init_resource::<aoi::ClientZone>()
        .insert_resource(CharacterStore(
            ServerCharacterStore::open_default()
                .expect("open character store at ~/.config/vaern/server/characters"),
        ))
        .init_resource::<persistence::CharactersDirty>()
        .init_resource::<persistence::SaveTimer>()
        .add_systems(
            Startup,
            (
                start_server,
                // Zone rooms must exist before NPCs or nodes spawn so
                // `assign_added_entities_to_rooms` can find their room.
                aoi::init_zone_rooms,
                npc::seed_npc_spawns,
                resource_nodes::seed_resource_nodes,
            )
                .chain(),
        )
        .add_systems(
            FixedUpdate,
            (
                movement::apply_player_movement,
                npc::npc_chase_target,
                npc::npc_roam,
                npc::npc_leash_home,
                // Run LAST in FixedUpdate so any X/Z movement above is
                // followed by a single Y snap to the shared terrain.
                npc::snap_npcs_to_terrain
                    .after(npc::npc_chase_target)
                    .after(npc::npc_roam)
                    .after(npc::npc_leash_home),
            ),
        )
        // Bevy caps system tuples at 20 — split into two add_systems calls.
        .add_systems(
            Update,
            (
                // NPC respawn → AoI room assignment must run same-tick so
                // a freshly-respawned mob can't receive replication updates
                // before it's bound to its zone room. Without this chain,
                // lightyear sees a new entity with `Replicate::to_clients(All)`
                // but no `NetworkVisibility` yet and starts sending updates
                // to every client — including the one whose client world
                // has no spawn message for that entity yet, which is the
                // "update for entity that doesn't exist / cannot find
                // entity" log we were seeing on NPC respawn.
                (npc::manage_npc_respawn, aoi::assign_added_entities_to_rooms).chain(),
                // Threat crediting must run AFTER casts resolve (so CastEvents
                // are in the buffer) but BEFORE detect_deaths → apply_deaths
                // despawn any one-shot-killed mob. Otherwise the mob's
                // ThreatTable is empty by the time the kill-XP observer fires
                // and credit falls through the cracks.
                npc::credit_threat_from_casts
                    .after(combat_systems::select_and_fire)
                    .before(combat_systems::detect_deaths),
                // Authoritative despawn/respawn. Lives on the server only
                // (not in the shared CombatPlugin) so clients don't double-
                // despawn replicated NPCs ahead of lightyear's canonical
                // despawn packet. Must run after detect_deaths so the
                // DeathEvent buffer is populated this tick.
                //
                // Player entities carry `CorpseOnDeath`, which makes
                // `apply_deaths` skip them — `respawn::apply_player_corpse_run`
                // is the sole handler for player deaths. Both read the same
                // DeathEvent stream (per-system cursors).
                (
                    combat_systems::apply_deaths.after(combat_systems::detect_deaths),
                    respawn::apply_player_corpse_run.after(combat_systems::detect_deaths),
                    respawn::tick_corpses.after(respawn::apply_player_corpse_run),
                ),
                npc::npc_select_targets,
                (connect::process_pending_spawns, connect::send_pending_hotbars).chain(),
                combat_io::handle_cast_intents,
                combat_io::handle_stance_requests,
                combat_io::broadcast_cast_fired,
                combat_io::attach_projectile_replication,
                (quests::handle_quest_messages, quests::broadcast_quest_logs).chain(),
                player_state::broadcast_player_state,
                inventory_io::handle_equip_requests,
                inventory_io::handle_unequip_requests,
                inventory_io::broadcast_inventory_and_equipped,
                wallet_io::broadcast_wallet_on_change,
                consume_io::handle_consume_requests,
                belt_io::handle_bind_belt_slot,
                belt_io::handle_clear_belt_slot,
                belt_io::handle_consume_belt,
                belt_io::broadcast_belt,
            ),
        )
        .add_systems(
            Update,
            (
                loot_io::handle_loot_open_requests,
                loot_io::handle_loot_take_requests,
                loot_io::handle_loot_take_all_requests,
                loot_io::broadcast_pending_loots,
                loot_io::cleanup_loot_containers,
                resource_nodes::handle_harvest_requests,
                resource_nodes::tick_node_respawn,
                // Pillar XP must read CastEvents before they're consumed by
                // other listeners; keep it in Update so the buffer still has
                // entries. sync_hp_max runs after so HP reflects any pillar
                // point that just landed this frame.
                (xp::award_pillar_xp_on_cast, xp::sync_hp_max_to_pillars).chain(),
                // CombinedStats denormalization — runs after HP sync so
                // PillarScores changes are visible for derivation. Combat
                // reads CombinedStats downstream, so keep this before any
                // damage-resolution systems in the Update schedule
                // (CombatPlugin's own chain orders itself).
                stats_sync::sync_combined_stats,
                // Area-of-interest: migrate each player's link sender
                // between rooms as they cross zones. NPC/node room
                // assignment is chained with their spawn sites (see
                // the manage_npc_respawn + seed_resource_nodes chains).
                aoi::sync_player_zone_subscriptions,
                // Persistence: collect dirty flags + flush on the 5s
                // wall-clock timer. Both run every tick; flush only
                // does work when the timer fires.
                persistence::mark_dirty_on_change,
                persistence::flush_dirty_characters,
                // Fold gear outfit into PlayerAppearance so remote
                // clients can render the player's current equipment.
                persistence::sync_player_appearance_from_gear,
                // Same, but for weapon loadout — server authoritative
                // mainhand/offhand prop ids pushed via PlayerWeapons.
                persistence::sync_player_weapons_from_gear,
                // Vendor NPC IO. Tag-new-vendors runs first so every
                // vendor carries a stable `VendorIdTag` before open /
                // buy / sell handlers query by id.
                (
                    vendor_io::tag_new_vendors,
                    vendor_io::handle_vendor_open_requests,
                    vendor_io::handle_vendor_buy_requests,
                    vendor_io::handle_vendor_sell_requests,
                    vendor_io::sweep_out_of_range_vendor_windows,
                )
                    .chain(),
                // Chat routing. Runs after per-tick zone sync so zone
                // chats land in the right room on the same tick the
                // player crossed a boundary.
                chat_io::handle_chat_messages.after(aoi::sync_player_zone_subscriptions),
            ),
        )
        .add_systems(
            Update,
            (
                // Party system — invite/response/leave/kick drained and
                // routed to the party table. Snapshot broadcast last so
                // it sees every mutation this tick.
                (
                    party_io::handle_party_invite,
                    party_io::handle_party_response,
                    party_io::handle_party_leave,
                    party_io::handle_party_kick,
                    party_io::expire_pending_invites,
                    party_io::broadcast_party_snapshots,
                )
                    .chain(),
            ),
        )
        .add_observer(connect::handle_new_client)
        .add_observer(connect::handle_connected)
        .add_observer(quests::apply_kill_objectives)
        .add_observer(xp::award_xp_on_mob_death)
        .add_observer(loot_io::spawn_loot_container_on_mob_death)
        .add_observer(aoi::handle_client_disconnect)
        .add_observer(persistence::save_on_disconnect)
        .run();
}

/// Load `assets/npc_mesh_map.yaml` at startup. Missing or malformed
/// file degrades gracefully to an empty map (every NPC renders as
/// cuboid) with a warn log — bad YAML shouldn't block the server.
fn load_npc_mesh_map() -> npc_mesh::NpcMeshMap {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/npc_mesh_map.yaml");
    match npc_mesh::NpcMeshMap::load_yaml(&path) {
        Ok(m) => m,
        Err(e) => {
            bevy::log::warn!(
                "failed to load {:?} ({e}); every NPC will render as a cuboid",
                path.display()
            );
            npc_mesh::NpcMeshMap::default()
        }
    }
}

fn start_server(mut commands: Commands) {
    let server = commands
        .spawn((
            NetcodeServer::new(NetcodeConfig {
                protocol_id: SHARED_PROTOCOL_ID,
                private_key: SHARED_PRIVATE_KEY,
                ..default()
            }),
            LocalAddr(SERVER_ADDR),
            ServerUdpIo::default(),
        ))
        .id();
    commands.trigger(Start { entity: server });
    println!("vaern-server listening on {}", SERVER_ADDR);
}
