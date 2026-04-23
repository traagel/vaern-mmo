//! Resource node lifecycle — spawn, harvest, respawn.
//!
//! v1 scope: seed a small hardcoded set of nodes per starter zone
//! at startup. Proper per-zone YAML placement lives in
//! `src/generated/world/zones/<id>/nodes/*.yaml` (planned for a
//! content pass — this file's `default_nodes_for_zone` is where
//! those authored lists would land, keyed by zone id).
//!
//! Flow:
//!   1. `seed_resource_nodes` (Startup) — spawn a node entity per
//!      default-table entry, replicated to all clients.
//!   2. `handle_harvest_requests` — on client request, validate
//!      proximity + skill + Available state → grant material →
//!      flip to `Harvested { remaining_secs }`.
//!   3. `tick_node_respawn` (Update) — count down remaining_secs;
//!      flip back to Available when it hits 0. Skill-gain rolls
//!      happen here too (future).

use bevy::log::info;
use bevy::prelude::*;
use lightyear::prelude::server::*;
use lightyear::prelude::*;

use vaern_inventory::PlayerInventory;
use vaern_items::ItemInstance;
use vaern_professions::{NodeKind, NodeState, Profession, ProfessionSkills};
use vaern_protocol::{HarvestRequest, PlayerTag};

use crate::data::GameData;

/// Max distance between player and node for a harvest to succeed.
pub const HARVEST_RANGE: f32 = 3.5;

/// Per-starter-zone hardcoded node placements for v1. Each tuple is
/// (NodeKind, offset-from-zone-origin). A content pass replaces this
/// with per-zone YAML.
fn default_nodes_for_zone(_zone_id: &str) -> Vec<(NodeKind, Vec3)> {
    // Five assorted tier-1 nodes around each zone's 30-unit inner ring.
    // Mining + Herbalism + Logging covered so any player profession
    // has something nearby.
    vec![
        (NodeKind::CopperVein, Vec3::new(28.0, 0.0, 0.0)),
        (NodeKind::StanchweedPatch, Vec3::new(-22.0, 0.0, 10.0)),
        (NodeKind::PineTree, Vec3::new(12.0, 0.0, -28.0)),
        (NodeKind::SunleafPatch, Vec3::new(-8.0, 0.0, -24.0)),
        (NodeKind::OakTree, Vec3::new(18.0, 0.0, 22.0)),
    ]
}

/// Startup system — spawn nodes across every starter zone.
pub fn seed_resource_nodes(data: Res<GameData>, mut commands: Commands) {
    let mut total = 0;
    for (zone_id, origin) in &data.zone_offsets {
        for (kind, local) in default_nodes_for_zone(zone_id) {
            let world_pos = *origin + local;
            commands.spawn((
                Name::new(format!("node-{}-{}", zone_id, kind.display())),
                Transform::from_translation(world_pos),
                kind,
                NodeState::Available,
                Replicate::to_clients(NetworkTarget::All),
                InterpolationTarget::to_clients(NetworkTarget::All),
            ));
            total += 1;
        }
    }
    info!("[nodes] seeded {} resource nodes across {} zones", total, data.zone_offsets.len());
}

/// Count down harvested nodes' respawn timers; flip back to Available
/// when the timer hits zero. Change detection fires replication so
/// clients see the state restore.
pub fn tick_node_respawn(time: Res<Time>, mut nodes: Query<&mut NodeState>) {
    let dt = time.delta_secs();
    for mut state in &mut nodes {
        if let NodeState::Harvested { remaining_secs } = *state {
            let next = remaining_secs - dt;
            if next <= 0.0 {
                *state = NodeState::Available;
            } else {
                *state = NodeState::Harvested { remaining_secs: next };
            }
        }
    }
}

/// Drain HarvestRequest messages. For each: find the node entity,
/// validate proximity + Available state + profession skill, then
/// grant the yield to inventory and flip the node to Harvested.
pub fn handle_harvest_requests(
    data: Res<GameData>,
    mut links: Query<(&RemoteId, &mut MessageReceiver<HarvestRequest>), With<ClientOf>>,
    mut players: Query<
        (&PlayerTag, &Transform, &ProfessionSkills, &mut PlayerInventory),
    >,
    mut nodes: Query<(&NodeKind, &mut NodeState, &Transform)>,
) {
    for (remote, mut rx) in &mut links {
        let PeerId::Netcode(client_id) = remote.0 else { continue };
        for req in rx.receive() {
            let Some((_, player_tf, skills, mut inv)) = players
                .iter_mut()
                .find(|(tag, _, _, _)| tag.client_id == client_id)
            else {
                continue;
            };
            let Ok((kind, mut state, node_tf)) = nodes.get_mut(req.node) else {
                info!("[nodes] {client_id} harvest: unknown node {:?}", req.node);
                continue;
            };

            // Distance gate.
            let dist = player_tf.translation.distance(node_tf.translation);
            if dist > HARVEST_RANGE {
                info!("[nodes] {client_id} harvest out of range ({dist:.1} > {HARVEST_RANGE})");
                continue;
            }

            // State gate — only Available nodes yield.
            if !matches!(*state, NodeState::Available) {
                continue;
            }

            // Skill gate.
            let prof = kind.profession();
            let skill = skills.get(prof);
            if skill < kind.min_skill() {
                info!(
                    "[nodes] {client_id} lacks {} skill ({skill} < {})",
                    prof.display(),
                    kind.min_skill()
                );
                continue;
            }

            // Grant the yield. Materialless instance (regular quality).
            let instance = ItemInstance::materialless(kind.yield_item_id(), "regular");
            let leftover = inv.add(instance, 1, &data.content);
            if leftover > 0 {
                info!("[nodes] {client_id} inventory full — harvest from {} wasted", kind.display());
            } else {
                info!(
                    "[nodes] {client_id} harvested {} (skill {} → {})",
                    kind.display(),
                    prof.display(),
                    skill
                );
            }

            // Flip to Harvested. Replication pushes the state change to
            // every client so the marker dims everywhere.
            *state = NodeState::Harvested {
                remaining_secs: kind.respawn_secs(),
            };
        }
    }
}

/// Grant `ProfessionSkills` to new players with a small starter bump
/// in Mining + Herbalism. Tier-1 nodes are open anyway (skill 0), but
/// a seeded 1 gives the player something to see in a future profession
/// UI.
pub fn starter_profession_skills() -> ProfessionSkills {
    let mut s = ProfessionSkills::default();
    s.set(Profession::Mining, 1);
    s.set(Profession::Herbalism, 1);
    s.set(Profession::Skinning, 1);
    s.set(Profession::Logging, 1);
    s
}
