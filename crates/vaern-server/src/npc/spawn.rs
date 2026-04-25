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
use super::{aggro_for_kind, leash_for_kind, respawn_secs_for_kind};
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
    let mut vendor_total = 0usize;

    let starter_ids: Vec<String> = data.zone_offsets.keys().cloned().collect();
    for zone_id in &starter_ids {
        let zone_origin = data.zone_origin(zone_id);
        zone_count += 1;

        // Pre-compute hub centers so the level-banded mob anchor can use
        // them. Same logic as the quest-giver block below.
        let zone_hubs: Vec<_> = data.world.hubs_in_zone(zone_id).collect();
        let hub_count_f = zone_hubs.len() as f32;
        let hub_centers: HashMap<&str, Vec3> = zone_hubs
            .iter()
            .enumerate()
            .map(|(i, hub)| {
                let pos = match &hub.offset_from_zone_origin {
                    Some(o) => Vec3::new(o.x, 0.0, o.z),
                    None => {
                        let angle =
                            (i as f32 / hub_count_f.max(1.0)) * std::f32::consts::TAU;
                        Vec3::new(8.0 * angle.cos(), 0.0, 8.0 * angle.sin())
                    }
                };
                (hub.id.as_str(), pos)
            })
            .collect();
        let capital_hub = zone_hubs.iter().find(|h| h.role == "capital").copied();

        // ── Combat mobs: scatter around level-appropriate anchor hubs ──
        //
        // Dalewatch tiers mobs into four bands by level so a player at the
        // keep doesn't see L8 named bosses in their face. Other zones still
        // ring around zone_origin (legacy procedural fallback).
        //
        // Big-zone layout (dalewatch redesign): each zone spans ~1200u, so
        // scatter radii are 70–110u around the anchor — wide enough that
        // mobs spread but tight enough that the level band reads.
        let zone_mobs: Vec<_> = data.world.mobs_in_zone(zone_id).collect();
        let mob_count = zone_mobs.len() as f32;
        for (i, mob) in zone_mobs.iter().enumerate() {
            let anchor = mob_anchor_for_level(zone_id, mob.level, &hub_centers);
            let radius = match mob.rarity.as_str() {
                "named" => 110.0,
                "elite" | "rare" => 90.0,
                _ => 70.0 + (i as f32 % 5.0) * 18.0,
            };
            let angle = (i as f32 / mob_count.max(1.0)) * std::f32::consts::TAU;
            let local = anchor + Vec3::new(radius * angle.cos(), 0.0, radius * angle.sin());

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
                level: mob.level,
                current: None,
                countdown: 0.0,
                combined_stats,
                visual_from_map,
                vendor_stock: None,
            });
            mob_total += 1;
        }

        // ── Quest-giver NPCs ──
        //
        // Layout: place each hub at its own angle around the zone origin,
        // so NPCs belonging to different hubs live in distinct clusters.
        // An NPC's spawn position = zone_origin + hub_center + local_ring.
        // (zone_hubs / hub_centers / capital_hub were computed above so the
        // mob-level-band anchor could share them.)

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
                        level: 0,
                        current: None,
                        countdown: 0.0,
                        combined_stats: None,
                        visual_from_map,
                vendor_stock: None,
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
                    level: 0,
                    current: None,
                    countdown: 0.0,
                    combined_stats: None,
                    visual_from_map,
                vendor_stock: None,
                });
                giver_total += 1;
            }
        }

        // ── Side-quest givers ──
        //
        // Each `world/zones/<zone>/quests/side/<hub>.yaml` may declare an
        // authored `giver:` NPC. Spawn one per declared giver at the
        // matching hub, on a small offset that doesn't collide with the
        // chain-NPC ring or the vendor ring.
        for bundle in data.side_quests.hubs_in_zone(zone_id) {
            let Some(giver) = bundle.giver.as_ref() else {
                continue;
            };
            if !placed.insert(giver.id.clone()) {
                continue;
            }
            let hub_id = giver.hub_id.as_deref().unwrap_or(&bundle.hub);
            let Some(hub) = zone_hubs.iter().find(|h| h.id == hub_id).copied() else {
                warn!(
                    "[side-quests] giver {} references unknown hub {} in zone {} — skipped",
                    giver.id, hub_id, zone_id,
                );
                continue;
            };
            let hub_center = hub_centers.get(hub.id.as_str()).copied().unwrap_or(Vec3::ZERO);
            // Place 4u NW of hub center (angle = 3π/4) so side-quest
            // givers cluster predictably away from chain NPCs (2.5u) and
            // vendors (5u NE-ish).
            let angle = 3.0 * std::f32::consts::FRAC_PI_4;
            let offset = Vec3::new(4.0 * angle.cos(), 0.0, 4.0 * angle.sin());
            let visual_from_map = mesh_map
                .lookup(&giver.display_name)
                .or_else(|| mesh_map.quest_giver_visual(&giver.display_name));
            slots.push(NpcSpawnSlot {
                position: zone_origin + hub_center + offset,
                max_hp: 9999.0,
                attack_damage: 0.0,
                display_name: giver.display_name.clone(),
                kind: NpcKind::QuestGiver,
                non_combat: true,
                hub_info: Some((hub.id.clone(), hub.role.clone(), zone_id.clone())),
                // No chain context — side-quest givers aren't tied to a
                // single chain step. Quest pickup goes through hub_info.
                chain_info: None,
                mob_slot_id: None,
                level: 0,
                current: None,
                countdown: 0.0,
                combined_stats: None,
                visual_from_map,
                vendor_stock: None,
            });
            giver_total += 1;
        }

        // ── Vendor NPCs ──
        //
        // Vendors are authored in `src/generated/vendors.yaml` and
        // placed at a named hub in the zone. Pre-alpha: infinite
        // stock, no restock cycle. Placed on a wider ring than
        // quest-givers so they don't overlap; hub_id must match a
        // real hub in this zone.
        let vendors_here: Vec<_> = data
            .vendors
            .iter()
            .filter(|v| v.zone_id == *zone_id)
            .collect();
        for (i, vdef) in vendors_here.iter().enumerate() {
            let Some(hub_center) = hub_centers.get(vdef.hub_id.as_str()).copied() else {
                warn!(
                    "[vendors] {} references unknown hub {} in zone {} — skipped",
                    vdef.id, vdef.hub_id, vdef.zone_id,
                );
                continue;
            };
            let n = vendors_here.len().max(1) as f32;
            let angle = (i as f32 / n) * std::f32::consts::TAU + std::f32::consts::FRAC_PI_4;
            // 5u ring — outside the 2.5u quest-giver ring, but still
            // inside the hub's "near the square" cluster.
            let offset = Vec3::new(5.0 * angle.cos(), 0.0, 5.0 * angle.sin());
            let listings: Vec<_> = vdef
                .listings
                .iter()
                .map(|l| l.to_stock_listing())
                .collect();
            // Prefer the authored archetype hint if set; otherwise fall
            // through to the hashed quest-giver-style visual so every
            // vendor has a distinct look without hand-mapping.
            let visual_from_map = vdef
                .archetype
                .as_deref()
                .map(|arch| {
                    crate::npc_mesh::NpcVisual::Humanoid(
                        vaern_protocol::NpcAppearance::new(arch, 1.0),
                    )
                })
                .or_else(|| mesh_map.quest_giver_visual(&vdef.display_name));
            slots.push(NpcSpawnSlot {
                position: zone_origin + hub_center + offset,
                max_hp: 9999.0,
                attack_damage: 0.0,
                display_name: vdef.display_name.clone(),
                kind: NpcKind::Vendor,
                non_combat: true,
                hub_info: Some((vdef.hub_id.clone(), "vendor".into(), zone_id.clone())),
                chain_info: None,
                mob_slot_id: None,
                level: 0,
                current: None,
                countdown: 0.0,
                combined_stats: None,
                visual_from_map,
                vendor_stock: Some(vaern_economy::VendorStock::new(listings)),
            });
            vendor_total += 1;
        }
    }

    println!(
        "seeded {} NPC spawn slots across {} starter zones: {} mobs + {} quest givers + {} vendors",
        slots.len(),
        zone_count,
        mob_total,
        giver_total,
        vendor_total,
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
        ec.insert(super::components::MobLevel(slot.level));
    }
    if let Some(stock) = &slot.vendor_stock {
        ec.insert(stock.clone());
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
                slot.countdown = respawn_secs_for_kind(slot.kind);
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

/// Pick a zone-local anchor (relative to `zone_origin`) for a mob based
/// on its level. Dalewatch tiers level → hub band; other zones return
/// `Vec3::ZERO` and fall back to the legacy "ring around zone origin"
/// placement.
fn mob_anchor_for_level(
    zone_id: &str,
    level: u32,
    hub_centers: &HashMap<&str, Vec3>,
) -> Vec3 {
    if zone_id == "dalewatch_marches" {
        // Pick the most level-appropriate hub. Falls through if a hub
        // isn't loaded (degenerate test or pre-redesign zone).
        let preferred: &[&str] = match level {
            0..=2 => &["dalewatch_keep"],
            3..=4 => &["harriers_rest", "kingsroad_waypost"],
            5..=6 => &["miller_crossing", "ford_of_ashmere"],
            // L7+ — Drifter's Lair anchor: east of Ford of Ashmere,
            // outside any authored hub. Drives final-boss placement.
            _ => return Vec3::new(470.0, 0.0, 80.0),
        };
        for hub_id in preferred {
            if let Some(c) = hub_centers.get(hub_id) {
                return *c;
            }
        }
    }
    Vec3::ZERO
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dalewatch_hub_centers() -> HashMap<&'static str, Vec3> {
        let mut h = HashMap::new();
        h.insert("dalewatch_keep", Vec3::new(0.0, 0.0, 0.0));
        h.insert("harriers_rest", Vec3::new(60.0, 0.0, -100.0));
        h.insert("kingsroad_waypost", Vec3::new(-110.0, 0.0, -30.0));
        h.insert("miller_crossing", Vec3::new(-220.0, 0.0, 20.0));
        h.insert("ford_of_ashmere", Vec3::new(270.0, 0.0, 40.0));
        h
    }

    #[test]
    fn level_band_picks_keep_for_low_levels() {
        let h = dalewatch_hub_centers();
        assert_eq!(mob_anchor_for_level("dalewatch_marches", 1, &h), Vec3::ZERO);
        assert_eq!(mob_anchor_for_level("dalewatch_marches", 2, &h), Vec3::ZERO);
    }

    #[test]
    fn level_band_picks_mid_hubs_for_mid_levels() {
        let h = dalewatch_hub_centers();
        for lvl in [3, 4] {
            let a = mob_anchor_for_level("dalewatch_marches", lvl, &h);
            assert!(
                a == Vec3::new(60.0, 0.0, -100.0)
                    || a == Vec3::new(-110.0, 0.0, -30.0),
                "L{lvl} should anchor at harriers/kingsroad, got {a:?}"
            );
        }
        for lvl in [5, 6] {
            let a = mob_anchor_for_level("dalewatch_marches", lvl, &h);
            assert!(
                a == Vec3::new(-220.0, 0.0, 20.0) || a == Vec3::new(270.0, 0.0, 40.0),
                "L{lvl} should anchor at miller/ford, got {a:?}"
            );
        }
    }

    #[test]
    fn level_band_picks_drifters_lair_for_capstone() {
        let h = dalewatch_hub_centers();
        let a = mob_anchor_for_level("dalewatch_marches", 8, &h);
        assert_eq!(a, Vec3::new(470.0, 0.0, 80.0));
        let a = mob_anchor_for_level("dalewatch_marches", 10, &h);
        assert_eq!(a, Vec3::new(470.0, 0.0, 80.0));
    }

    #[test]
    fn other_zones_fall_back_to_zero() {
        let h = dalewatch_hub_centers();
        assert_eq!(mob_anchor_for_level("ashen_holt", 5, &h), Vec3::ZERO);
        assert_eq!(mob_anchor_for_level("scrap_marsh", 1, &h), Vec3::ZERO);
    }
}
