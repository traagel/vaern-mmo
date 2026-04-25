//! XP awards — granted on mob kill (observer) and on quest step / chain
//! completion (called from `quests.rs`).
//!
//! Mob-kill path uses an observer on `On<Remove, MobSourceId>` rather than
//! `MessageReader<DeathEvent>` + system ordering: the observer fires
//! synchronously during the mob's despawn, so `ThreatTable` and `NpcKind`
//! are still readable without worrying about whether `apply_deaths` has
//! already run. Mirrors the pattern in `quests::apply_kill_objectives`.

use std::collections::HashSet;

use bevy::log::info;
use bevy::prelude::*;
use vaern_character::{Experience, XpCurve};
use vaern_combat::{CastEvent, Health, NpcKind};
use vaern_protocol::PlayerTag;
use vaern_stats::{
    PillarCaps, PillarScores, PillarXp, XP_PER_ABILITY_CAST, award_pillar_xp, derive_primaries,
};

use crate::data::GameData;
use crate::npc::{MobLevel, MobSourceId, Npc, ThreatTable};

/// Base XP per kind before level-delta scaling. Common sweepers
/// contribute little, named kills pay out meaningfully.
fn xp_base_for_kind(kind: NpcKind) -> u32 {
    match kind {
        NpcKind::Combat => 50,
        NpcKind::Elite => 150,
        NpcKind::Named => 400,
        NpcKind::QuestGiver | NpcKind::Vendor | NpcKind::QuestPoi => 0,
    }
}

/// Scale a kill reward by `(mob_level - killer_level)`. Same level → 1.0×.
/// Each level the mob is *above* the killer earns more (capped 1.5×).
/// Each level *below* shrinks: -3 → 0.5×, -5 → 0.1×, -7+ → 0.0× (grey).
/// Returns the scaled reward; never panics on edge cases.
pub fn level_xp_multiplier(mob_level: u32, killer_level: u32) -> f32 {
    let delta = mob_level as i32 - killer_level as i32;
    match delta {
        d if d >= 5 => 1.5,
        4 => 1.35,
        3 => 1.20,
        2 => 1.10,
        1 => 1.05,
        0 => 1.0,
        -1 => 0.85,
        -2 => 0.7,
        -3 => 0.5,
        -4 => 0.3,
        -5 => 0.1,
        _ => 0.0, // grey-cap at delta <= -6
    }
}

/// Final scaled XP reward. Picks 0 for non-combat NPCs.
fn xp_reward_scaled(kind: NpcKind, mob_level: u32, killer_level: u32) -> u32 {
    let base = xp_base_for_kind(kind);
    if base == 0 || mob_level == 0 {
        return base;
    }
    let scaled = base as f32 * level_xp_multiplier(mob_level, killer_level);
    scaled.round() as u32
}

/// Add `amount` XP to a player's `Experience`, rolling level-ups against the
/// curve. Logs the delta. Returns the number of levels gained so callers
/// can hand out per-level rewards (pillar points, etc.).
pub fn grant_xp(
    player: Entity,
    xp: &mut Experience,
    curve: &XpCurve,
    amount: u32,
    source: &str,
) -> u32 {
    if amount == 0 {
        return 0;
    }
    let before = *xp;
    xp.current = xp.current.saturating_add(amount);
    let mut leveled = 0u32;
    loop {
        let needed = curve.to_next(xp.level);
        if xp.current < needed || needed == 0 {
            break;
        }
        xp.current -= needed;
        xp.level += 1;
        leveled += 1;
        if leveled >= 20 {
            // Safety net against a pathological curve returning 0.
            break;
        }
    }
    if leveled > 0 {
        info!(
            "[xp] player {player:?} +{amount} xp ({source}) → L{} ({}xp); was L{} ({}xp)",
            xp.level, xp.current, before.level, before.current
        );
    } else {
        info!(
            "[xp] player {player:?} +{amount} xp ({source}) → L{} ({}/{}xp)",
            xp.level,
            xp.current,
            curve.to_next(xp.level)
        );
    }
    leveled
}

/// `grant_xp` plus the per-level pillar-point bonus. Use this at any
/// callsite that wants the level-up to read as a felt-power moment
/// (kills, quest rewards). Returns levels gained.
pub fn grant_xp_with_levelup_bonus(
    player: Entity,
    xp: &mut Experience,
    scores: &mut PillarScores,
    caps: &PillarCaps,
    curve: &XpCurve,
    amount: u32,
    source: &str,
) -> u32 {
    let levels = grant_xp(player, xp, curve, amount, source);
    if levels > 0 {
        award_levelup_pillar_points(levels, scores, caps);
    }
    levels
}

/// Grant `levels` extra pillar points to the player's *primary* pillar (the
/// one with the highest cap — i.e. the pillar they committed to at char
/// create). Rewards levelling beyond the natural pillar-XP-from-casting
/// flow so a level-up reads as a real felt-power moment.
pub(crate) fn award_levelup_pillar_points(
    levels: u32,
    scores: &mut PillarScores,
    caps: &PillarCaps,
) {
    use vaern_core::Pillar;
    if levels == 0 {
        return;
    }
    // Pick the highest-cap pillar; tie-break Might > Finesse > Arcana.
    let order = [Pillar::Might, Pillar::Finesse, Pillar::Arcana];
    let primary = order
        .iter()
        .copied()
        .max_by_key(|p| caps.get(*p))
        .unwrap_or(Pillar::Might);
    let cap = caps.get(primary) as u32;
    let cur = scores.get(primary) as u32;
    let granted = (cap.saturating_sub(cur)).min(levels);
    if granted == 0 {
        return;
    }
    let new_score = (cur + granted) as u16;
    scores.set(primary, new_score);
    info!(
        "[xp] level-up bonus: +{} {:?} pt → {}/{}",
        granted, primary, new_score, cap
    );
}

/// Observer on `Remove<MobSourceId>` — fires during the mob's despawn with
/// all its components still readable. Credits the top-threat player (if any)
/// with XP matching the mob's `NpcKind`. Mirrors the reliable pattern used
/// by `quests::apply_kill_objectives`.
pub fn award_xp_on_mob_death(
    trigger: On<Remove, MobSourceId>,
    mobs: Query<(
        Option<&NpcKind>,
        Option<&ThreatTable>,
        Option<&Npc>,
        Option<&MobLevel>,
    )>,
    mut players: Query<(
        &PlayerTag,
        &Transform,
        &mut Experience,
        &mut PillarScores,
        &PillarCaps,
    )>,
    curve: Res<XpCurve>,
    party_table: Res<crate::party_io::PartyTable>,
) {
    let entity = trigger.entity;
    let Ok((kind_opt, threat_opt, npc_opt, level_opt)) = mobs.get(entity) else {
        info!("[xp:mob-death] entity {entity:?} not found in mobs query");
        return;
    };
    let Some(kind) = kind_opt else {
        info!(
            "[xp:mob-death] {entity:?} missing NpcKind (has_threat={} has_npc={})",
            threat_opt.is_some(),
            npc_opt.is_some()
        );
        return;
    };
    let Some(threat) = threat_opt else {
        info!("[xp:mob-death] {entity:?} missing ThreatTable");
        return;
    };
    let mob_level = level_opt.map(|m| m.0).unwrap_or(0);

    // Top threat wins the credit. If nobody has built threat (e.g. mob died
    // to a leash reset or an AoE from a despawned caster), skip.
    let top = threat
        .0
        .iter()
        .filter(|(_, t)| **t > 0.0)
        .max_by(|a, b| a.1.total_cmp(b.1));
    let Some((top_entity, _)) = top else {
        info!(
            "[xp:mob-death] {entity:?} died with empty threat table ({} entries total)",
            threat.0.len()
        );
        return;
    };
    let killer = *top_entity;

    let Ok((killer_tag, killer_tf, mut xp, mut scores, caps)) = players.get_mut(killer) else {
        info!("[xp:mob-death] killer {killer:?} not a player (threat map had {} entries)", threat.0.len());
        return;
    };
    let killer_client = killer_tag.client_id;
    let killer_pos = killer_tf.translation;
    let killer_level = xp.level;
    let reward = xp_reward_scaled(*kind, mob_level, killer_level);
    if reward == 0 {
        info!(
            "[xp:mob-death] {entity:?} killed at L{} by L{} player → grey, no xp",
            mob_level, killer_level
        );
        return;
    }
    let levels = grant_xp(killer, &mut xp, &curve, reward, "kill");
    if levels > 0 {
        award_levelup_pillar_points(levels, &mut scores, caps);
    }
    drop(xp);
    drop(scores);

    // Shared XP — every party member within PARTY_SHARE_RADIUS of the
    // killer (XZ plane, including killer) gets a fractional share. Share
    // size scales down with group size but total payout rises (small-
    // group XP bonus).
    let Some(party) = party_table.party_of(killer_client) else { return };
    let share_radius = crate::party_io::PARTY_SHARE_RADIUS;
    // Build the list of sharers (members in range). This second pass
    // iterates players again but we just released the mutable borrow.
    let mut sharer_entities: Vec<Entity> = Vec::new();
    for (tag, tf, _, _, _) in players.iter() {
        if !party.members.contains(&tag.client_id) {
            continue;
        }
        let d = (tf.translation - killer_pos).length();
        if d <= share_radius {
            sharer_entities.push(if tag.client_id == killer_client {
                killer
            } else {
                // Resolve the entity via the same iter — no Entity in
                // this tuple, so we fall back to a linear rescan.
                // Cheap at pre-alpha player counts.
                Entity::PLACEHOLDER
            });
        }
    }
    let n_sharers = sharer_entities.len();
    if n_sharers <= 1 {
        return;
    }
    let per = {
        let mult = match n_sharers {
            0 | 1 => 1.00,
            2 => 0.70,
            3 => 0.55,
            4 => 0.45,
            _ => 0.38,
        };
        ((reward as f32) * mult).round() as u32
    };
    // Second pass: grant per-share XP to each sharer except the killer
    // (who already got the full reward). Sharers also get pillar-point
    // bonus on level-up.
    for (tag, tf, mut xp, mut scores, caps) in players.iter_mut() {
        if tag.client_id == killer_client {
            continue;
        }
        if !party.members.contains(&tag.client_id) {
            continue;
        }
        if (tf.translation - killer_pos).length() > share_radius {
            continue;
        }
        let levels = grant_xp(Entity::PLACEHOLDER, &mut xp, &curve, per, "party-share");
        if levels > 0 {
            award_levelup_pillar_points(levels, &mut scores, caps);
        }
    }
    info!(
        "[party] shared {} XP to {} other member(s) (killer={killer_client})",
        per,
        n_sharers.saturating_sub(1)
    );
}

/// Award pillar XP to the caster on every ability resolution. Dedupes
/// by (caster, ability) within a frame so AoEs hitting 5 targets don't
/// multiply the XP 5×. CastEvent carries the school id; we look up the
/// owning pillar in `GameData.schools` and credit that pillar.
///
/// XP amount is `XP_PER_ABILITY_CAST` (6 by default), flat. Balance pass
/// can tune per-school or per-tier later by extending this match.
pub fn award_pillar_xp_on_cast(
    mut events: MessageReader<CastEvent>,
    data: Res<GameData>,
    mut casters: Query<(&mut PillarScores, &mut PillarXp, &PillarCaps), With<PlayerTag>>,
) {
    let mut seen: HashSet<(Entity, Entity)> = HashSet::new();
    for ev in events.read() {
        if !seen.insert((ev.caster, ev.ability)) {
            continue;
        }
        let Some(school) = data.schools.get(&ev.school) else { continue };
        let Ok((mut scores, mut xp, caps)) = casters.get_mut(ev.caster) else { continue };
        let gain = award_pillar_xp(&mut scores, &mut xp, caps, school.pillar, XP_PER_ABILITY_CAST);
        if gain.points_gained > 0 {
            info!(
                "[pillar-xp] {:?} +{} {:?} pt (now {}), school={}",
                ev.caster, gain.points_gained, school.pillar, gain.new_score, ev.school
            );
        }
    }
}

/// When `PillarScores` change (initial insert or a pillar-point gain),
/// recompute `Health.max` from the derived primaries. Preserves current
/// HP as a fraction of the old max so growth doesn't feel like a free
/// heal (and damage doesn't get negated by a pillar-up mid-fight).
pub fn sync_hp_max_to_pillars(
    mut players: Query<(&PillarScores, &mut Health), (With<PlayerTag>, Changed<PillarScores>)>,
) {
    for (pillars, mut hp) in &mut players {
        let new_max = derive_primaries(pillars).hp_max as f32;
        if (hp.max - new_max).abs() < 0.5 {
            continue;
        }
        let ratio = if hp.max > 0.0 {
            (hp.current / hp.max).clamp(0.0, 1.0)
        } else {
            1.0
        };
        hp.max = new_max;
        hp.current = (hp.max * ratio).min(hp.max);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_multiplier_at_parity_is_one() {
        assert_eq!(level_xp_multiplier(5, 5), 1.0);
        assert_eq!(level_xp_multiplier(1, 1), 1.0);
    }

    #[test]
    fn level_multiplier_grows_for_higher_mob() {
        assert!(level_xp_multiplier(7, 5) > level_xp_multiplier(5, 5));
        assert_eq!(level_xp_multiplier(10, 5), 1.5); // capped 1.5
        assert_eq!(level_xp_multiplier(15, 5), 1.5); // still capped
    }

    #[test]
    fn level_multiplier_shrinks_for_lower_mob() {
        assert!(level_xp_multiplier(3, 5) < 1.0);
        assert_eq!(level_xp_multiplier(2, 5), 0.5); // -3
        assert!(level_xp_multiplier(0, 5) < 0.2); // -5+ very small
        assert_eq!(level_xp_multiplier(1, 10), 0.0); // -9 → grey
    }

    #[test]
    fn xp_reward_scaled_returns_zero_for_grey_kills() {
        // L1 mob killed by L10 player should yield no XP.
        assert_eq!(xp_reward_scaled(NpcKind::Combat, 1, 10), 0);
    }

    #[test]
    fn xp_reward_scaled_at_parity_matches_base() {
        assert_eq!(xp_reward_scaled(NpcKind::Combat, 5, 5), 50);
        assert_eq!(xp_reward_scaled(NpcKind::Elite, 5, 5), 150);
        assert_eq!(xp_reward_scaled(NpcKind::Named, 5, 5), 400);
    }

    #[test]
    fn xp_reward_scaled_caps_red_mobs() {
        // L10 mob killed by L5 player: 50 * 1.5 = 75.
        assert_eq!(xp_reward_scaled(NpcKind::Combat, 10, 5), 75);
    }

    #[test]
    fn xp_reward_zero_for_non_combat() {
        assert_eq!(xp_reward_scaled(NpcKind::QuestGiver, 5, 5), 0);
        assert_eq!(xp_reward_scaled(NpcKind::Vendor, 5, 5), 0);
    }
}
