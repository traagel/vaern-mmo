//! Spawn `QuestPoi` waypoint markers for `investigate` / `explore` quest
//! steps. Runs once at Startup, after `seed_npc_spawns`, off the loaded
//! `QuestIndex` + `LandmarkIndex` in `GameData`. POIs don't despawn or
//! respawn — they're stationary world fixtures.

use std::collections::HashSet;

use bevy::prelude::*;
use lightyear::prelude::*;
use vaern_combat::{AnimState, DisplayName, Health, NpcKind, QuestPoi};

use super::components::{NonCombat, Npc, NpcHome};
use crate::data::GameData;

pub fn seed_quest_pois(data: Res<GameData>, mut commands: Commands) {
    let mut spawned: HashSet<(String, String)> = HashSet::new();
    let mut count = 0usize;

    for zone_id in data.zone_offsets.keys() {
        let zone_origin = data.zone_origin(zone_id);
        for chain in data.quests.zone_chains(zone_id) {
            for step in &chain.steps {
                if !matches!(step.objective.kind.as_str(), "investigate" | "explore") {
                    continue;
                }
                let Some(loc_id) = step.objective.location.as_deref() else {
                    continue;
                };
                // De-dup on (chain_id, location_id) — multiple chains
                // can reference the same landmark and we want one marker
                // per (chain, location) so per-step QuestPoi metadata is
                // unambiguous.
                let key = (chain.id.clone(), loc_id.to_string());
                if !spawned.insert(key) {
                    continue;
                }
                let Some(landmark) = data.landmarks.get(zone_id, loc_id) else {
                    warn!(
                        "[quest:poi] chain {} step {} references unknown landmark {} in zone {}",
                        chain.id, step.step, loc_id, zone_id,
                    );
                    continue;
                };
                let pos = zone_origin
                    + Vec3::new(
                        landmark.offset_from_zone_origin.x,
                        0.0,
                        landmark.offset_from_zone_origin.z,
                    );
                let display_name = landmark.name.clone();
                let step_index = step.step.saturating_sub(1);
                // Health(9999) is what makes the client's
                // `render_replicated_npcs` query pick the entity up
                // (gated on `With<Health>`); POI is invulnerable in
                // practice because `NonCombat` blocks targeting in
                // input.rs and damage pipelines skip non-targetable
                // entities.
                commands.spawn((
                    Name::new(format!("QuestPoi:{}:{}", chain.id, loc_id)),
                    Transform::from_translation(pos),
                    Health::full(9999.0),
                    AnimState::default(),
                    Npc,
                    NpcHome(pos),
                    NonCombat,
                    NpcKind::QuestPoi,
                    DisplayName(display_name.clone()),
                    QuestPoi {
                        chain_id: chain.id.clone(),
                        step_index,
                        landmark_id: loc_id.to_string(),
                        name: display_name,
                    },
                    Replicate::to_clients(NetworkTarget::All),
                    InterpolationTarget::to_clients(NetworkTarget::All),
                ));
                count += 1;
            }
        }
    }

    println!("seeded {count} quest POIs");
}
