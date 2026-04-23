//! Zone-wide NPC seeding + per-slot (re)spawn. `seed_npc_spawns` runs once at
//! Startup and fills the `NpcSpawns` table from the loaded world YAML.
//! `manage_npc_respawn` ticks continuously, spawning fresh entities for
//! empty slots once the respawn countdown elapses.

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use lightyear::prelude::*;
use vaern_combat::{
    AbilityCooldown, AbilityPriority, AbilitySpec, AnimState, Caster, DisplayName, Health, NpcKind,
    QuestGiverHub,
};

use super::components::{
    AggroRange, LeashRange, MobSourceId, NonCombat, Npc, NpcHome, NpcSpawnSlot, NpcSpawns,
    RoamState, ThreatTable,
};
use super::{NPC_RESPAWN_SECS, aggro_for_kind, leash_for_kind};
use crate::data::GameData;
use crate::util::prettify_npc_name;

/// Populate every starter zone (one per race) with its mobs + quest givers.
/// Each zone's entities are placed relative to `GameData::zone_origin(zone)`
/// so distinct zones don't occupy the same world-space. Mob HP resolves via
/// `bestiary::CreatureType::base_hp_at_level` * rarity multiplier.
pub fn seed_npc_spawns(
    data: Res<GameData>,
    mesh_map: Res<crate::npc_mesh::NpcMeshMap>,
    mut spawns: ResMut<NpcSpawns>,
) {
    let mut slots = Vec::new();
    let mut zone_count = 0usize;
    let mut mob_total = 0usize;
    let mut giver_total = 0usize;

    let starter_ids: Vec<String> = data.zone_offsets.keys().cloned().collect();
    for zone_id in &starter_ids {
        let zone_origin = data.zone_origin(zone_id);
        zone_count += 1;

        // ── Combat mobs: scatter around the zone's own origin in a ring ──
        //
        // Big-zone layout (dalewatch redesign): each zone spans ~1200u, so
        // mob scatter radii grew ~7× from the legacy 18–45u so mobs actually
        // spread across the zone rather than pileup at zone_origin.
        let zone_mobs: Vec<_> = data.world.mobs_in_zone(zone_id).collect();
        let mob_count = zone_mobs.len() as f32;
        for (i, mob) in zone_mobs.iter().enumerate() {
            let radius = match mob.rarity.as_str() {
                "named" => 320.0,
                "elite" | "rare" => 240.0,
                _ => 140.0 + (i as f32 % 5.0) * 25.0,
            };
            let angle = (i as f32 / mob_count.max(1.0)) * std::f32::consts::TAU;
            let local = Vec3::new(radius * angle.cos(), 0.0, radius * angle.sin());

            let ctype = data
                .bestiary
                .creature_type(&mob.creature_type)
                .expect("mob references unknown creature_type");
            let base_hp = ctype.base_hp_at_level(mob.level) as f32;
            let rarity_mult = match mob.rarity.as_str() {
                "elite" => 2.75,
                "rare" => 3.50,
                "named" => 5.00,
                _ => 1.00,
            };
            let max_hp = (base_hp * rarity_mult).max(20.0);
            let kind = match mob.rarity.as_str() {
                "named" => NpcKind::Named,
                "elite" | "rare" => NpcKind::Elite,
                _ => NpcKind::Combat,
            };
            let attack_damage = 4.0 + mob.level as f32 * 0.5;

            // Mob's armor_class is always present (required field in
            // the world YAML schema); creature_type's default_armor_class
            // is just the fallback authors can use.
            let armor_class = data
                .bestiary
                .armor_class(&mob.armor_class)
                .expect("mob references unknown armor_class");
            let combined_stats = Some(super::stats::npc_combined_stats(ctype, armor_class, kind));

            let visual_from_map = mesh_map.lookup(&mob.name);
            slots.push(NpcSpawnSlot {
                position: zone_origin + local,
                max_hp,
                attack_damage,
                display_name: mob.name.clone(),
                kind,
                non_combat: false,
                hub_info: None,
                chain_info: None,
                mob_slot_id: Some(mob.id.clone()),
                current: None,
                countdown: 0.0,
                combined_stats,
                visual_from_map,
            });
            mob_total += 1;
        }

        // ── Quest-giver NPCs ──
        //
        // Layout: place each hub at its own angle around the zone origin,
        // so NPCs belonging to different hubs live in distinct clusters.
        // An NPC's spawn position = zone_origin + hub_center + local_ring.
        let zone_hubs: Vec<_> = data.world.hubs_in_zone(zone_id).collect();
        let hub_count = zone_hubs.len() as f32;
        // Hub placement: prefer an explicit `offset_from_zone_origin` in the
        // hub YAML (big-zone layout — dalewatch redesign). Fall back to the
        // legacy tight radial placement (8u ring) when the offset is absent,
        // so un-redesigned zones still spawn coherently.
        let hub_centers: HashMap<&str, Vec3> = zone_hubs
            .iter()
            .enumerate()
            .map(|(i, hub)| {
                let pos = match &hub.offset_from_zone_origin {
                    Some(o) => Vec3::new(o.x, 0.0, o.z),
                    None => {
                        let angle =
                            (i as f32 / hub_count.max(1.0)) * std::f32::consts::TAU;
                        Vec3::new(8.0 * angle.cos(), 0.0, 8.0 * angle.sin())
                    }
                };
                (hub.id.as_str(), pos)
            })
            .collect();
        let capital_hub = zone_hubs.iter().find(|h| h.role == "capital").copied();

        let chains: Vec<_> = data.quests.zone_chains(zone_id).collect();
        let mut placed: HashSet<String> = HashSet::new();

        for chain in &chains {
            if !chain.npcs.is_empty() {
                // Hand-curated registry — authoritative source for names,
                // hubs, and dialogue. Step_index is the lowest step where
                // objective.npc matches this entry's id.
                for (i, npc) in chain.npcs.iter().enumerate() {
                    if !placed.insert(npc.id.clone()) {
                        continue;
                    }
                    let step_index = chain
                        .steps
                        .iter()
                        .find(|s| s.objective.npc.as_deref() == Some(&npc.id))
                        .map(|s| s.step.saturating_sub(1))
                        .unwrap_or(0);
                    let hub = npc
                        .hub_id
                        .as_deref()
                        .and_then(|id| zone_hubs.iter().find(|h| h.id == id).copied())
                        .or(capital_hub);
                    let Some(hub) = hub else { continue };
                    let hub_center = hub_centers
                        .get(hub.id.as_str())
                        .copied()
                        .unwrap_or(Vec3::ZERO);
                    let offset_angle =
                        (i as f32 / chain.npcs.len().max(1) as f32) * std::f32::consts::TAU;
                    let offset = Vec3::new(
                        2.5 * offset_angle.cos(),
                        0.0,
                        2.5 * offset_angle.sin(),
                    );
                    let visual_from_map = mesh_map
                        .lookup(&npc.display_name)
                        .or_else(|| mesh_map.quest_giver_visual(&npc.display_name));
                    slots.push(NpcSpawnSlot {
                        position: zone_origin + hub_center + offset,
                        max_hp: 9999.0,
                        attack_damage: 0.0,
                        display_name: npc.display_name.clone(),
                        kind: NpcKind::QuestGiver,
                        non_combat: true,
                        hub_info: Some((hub.id.clone(), hub.role.clone(), zone_id.clone())),
                        chain_info: Some((chain.id.clone(), step_index)),
                        mob_slot_id: None,
                        current: None,
                        countdown: 0.0,
                        combined_stats: None,
                        visual_from_map,
                    });
                    giver_total += 1;
                }
                continue;
            }

            // Fallback: no curated registry. Parse talk-step target_hints
            // at the capital hub, same as before the hand-edit pass.
            let Some(capital) = capital_hub else { continue };
            let hub_center = hub_centers
                .get(capital.id.as_str())
                .copied()
                .unwrap_or(Vec3::ZERO);
            let mut chain_npcs: Vec<(u32, String)> = chain
                .steps
                .iter()
                .filter(|s| s.objective.kind == "talk")
                .map(|s| (s.step.saturating_sub(1), s.objective.target_hint.clone()))
                .collect();
            let mut seen: HashSet<String> = HashSet::new();
            chain_npcs.retain(|(_, n)| seen.insert(n.clone()));

            for (i, (step_idx, name)) in chain_npcs.iter().enumerate() {
                let display = prettify_npc_name(name);
                if !placed.insert(display.clone()) {
                    continue;
                }
                let n = chain_npcs.len().max(1) as f32;
                let offset_angle = (i as f32 / n) * std::f32::consts::TAU;
                let offset =
                    Vec3::new(2.5 * offset_angle.cos(), 0.0, 2.5 * offset_angle.sin());
                let visual_from_map = mesh_map
                    .lookup(&display)
                    .or_else(|| mesh_map.quest_giver_visual(&display));
                slots.push(NpcSpawnSlot {
                    position: zone_origin + hub_center + offset,
                    max_hp: 9999.0,
                    attack_damage: 0.0,
                    display_name: display,
                    kind: NpcKind::QuestGiver,
                    non_combat: true,
                    hub_info: Some((
                        capital.id.clone(),
                        capital.role.clone(),
                        zone_id.clone(),
                    )),
                    chain_info: Some((chain.id.clone(), *step_idx)),
                    mob_slot_id: None,
                    current: None,
                    countdown: 0.0,
                    combined_stats: None,
                    visual_from_map,
                });
                giver_total += 1;
            }
        }
    }

    println!(
        "seeded {} NPC spawn slots across {} starter zones: {} mobs + {} quest givers",
        slots.len(),
        zone_count,
        mob_total,
        giver_total,
    );
    spawns.slots = slots;
}

fn spawn_one_npc(commands: &mut Commands, slot: &NpcSpawnSlot) -> Entity {
    let aggro = aggro_for_kind(slot.kind);
    let leash = leash_for_kind(slot.kind);
    let mut ec = commands.spawn((
        Name::new(slot.display_name.clone()),
        Transform::from_translation(slot.position),
        Health::full(slot.max_hp),
        Npc,
        NpcHome(slot.position),
        AggroRange(aggro),
        LeashRange(leash),
        ThreatTable::default(),
        DisplayName(slot.display_name.clone()),
        slot.kind,
        AnimState::default(),
        Replicate::to_clients(NetworkTarget::All),
        InterpolationTarget::to_clients(NetworkTarget::All),
    ));
    // Combat mobs carry CombinedStats so vaern-combat's damage pipeline
    // mitigates hits against their armor + resist profile. Quest-givers
    // have None and fall through to raw-damage math (they don't take
    // hits anyway — NonCombat marker blocks targeting).
    if let Some(stats) = slot.combined_stats {
        ec.insert(stats);
    }
    // Combat mobs roam idle; quest-givers stand still.
    if !slot.non_combat {
        ec.insert(RoamState {
            waypoint: slot.position,
            // Staggered initial wait so they don't all pick waypoints on the
            // same tick (would look synchronized).
            wait_secs: 0.5 + super::ai::pseudo_rand(slot.position) * 2.5,
        });
    }
    if slot.non_combat {
        ec.insert(NonCombat);
    }
    if let Some((hub_id, hub_role, zone_id)) = &slot.hub_info {
        let (chain_id, step_index) = match &slot.chain_info {
            Some((cid, idx)) => (Some(cid.clone()), Some(*idx)),
            None => (None, None),
        };
        ec.insert(QuestGiverHub {
            hub_id: hub_id.clone(),
            hub_role: hub_role.clone(),
            zone_id: zone_id.clone(),
            chain_id,
            step_index,
        });
    }
    if let Some(mob_id) = &slot.mob_slot_id {
        ec.insert(MobSourceId(mob_id.clone()));
    }
    // Mesh lookup is keyed on the mob's display-name (the `name:`
    // field in the zone YAML). See `assets/npc_mesh_map.yaml` — the
    // map is the authoritative "what does this NPC looks like" data
    // source. Beasts → `NpcMesh`; humanoids → `NpcAppearance`;
    // unmapped → cuboid fallback on the client.
    match &slot.visual_from_map {
        Some(crate::npc_mesh::NpcVisual::Beast(mesh)) => {
            ec.insert(mesh.clone());
        }
        Some(crate::npc_mesh::NpcVisual::Humanoid(appearance)) => {
            ec.insert(appearance.clone());
        }
        None => {}
    }
    let entity = ec.id();

    // Combat mobs get an auto-cast blade attack. Quest givers don't.
    if !slot.non_combat {
        commands.spawn((
            AbilitySpec {
                damage: slot.attack_damage,
                cooldown_secs: 1.5,
                cast_secs: 0.0,
                resource_cost: 0.0,
                school: "blade".into(),
                threat_multiplier: 1.0,
                range: 3.5, // melee auto-attack
                ..AbilitySpec::default()
            },
            AbilityCooldown::ready(),
            Caster(entity),
            AbilityPriority(5),
        ));
    }
    entity
}

/// Each slot either has a live NPC or is ticking down to a respawn. Handles
/// initial spawn (on startup countdown=0, current=None → immediate spawn).
pub fn manage_npc_respawn(
    time: Res<Time>,
    mut spawns: ResMut<NpcSpawns>,
    npcs: Query<Entity, With<Npc>>,
    mut commands: Commands,
) {
    let alive: HashSet<Entity> = npcs.iter().collect();
    let dt = time.delta_secs();
    for slot in &mut spawns.slots {
        if let Some(e) = slot.current {
            if !alive.contains(&e) {
                slot.current = None;
                slot.countdown = NPC_RESPAWN_SECS;
            }
        } else {
            slot.countdown -= dt;
            if slot.countdown <= 0.0 {
                let entity = spawn_one_npc(&mut commands, slot);
                info!(
                    "[npc:respawn] {:?} name={:?} pos=({:.1},{:.1},{:.1})",
                    entity,
                    slot.display_name,
                    slot.position.x, slot.position.y, slot.position.z,
                );
                slot.current = Some(entity);
            }
        }
    }
}
