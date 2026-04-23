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
use crate::npc::{MobSourceId, Npc, ThreatTable};

/// Flat XP granted per mob kill, by rarity. Keyed off `NpcKind` since that's
/// what's carried on the entity — rarity strings are normalized into Kind
/// at spawn. Crude but scales with interaction effort: common sweepers
/// contribute little, named kills pay out meaningfully.
fn xp_reward(kind: NpcKind) -> u32 {
    match kind {
        NpcKind::Combat => 50,
        NpcKind::Elite => 150,
        NpcKind::Named => 400,
        NpcKind::QuestGiver => 0,
    }
}

/// Add `amount` XP to a player's `Experience`, rolling level-ups against the
/// curve. Logs the delta. Safe to call from any server-side context that
/// has a mutable reference to the player's Experience.
pub fn grant_xp(
    player: Entity,
    xp: &mut Experience,
    curve: &XpCurve,
    amount: u32,
    source: &str,
) {
    if amount == 0 {
        return;
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
}

/// Observer on `Remove<MobSourceId>` — fires during the mob's despawn with
/// all its components still readable. Credits the top-threat player (if any)
/// with XP matching the mob's `NpcKind`. Mirrors the reliable pattern used
/// by `quests::apply_kill_objectives`.
pub fn award_xp_on_mob_death(
    trigger: On<Remove, MobSourceId>,
    mobs: Query<(Option<&NpcKind>, Option<&ThreatTable>, Option<&Npc>)>,
    mut players: Query<(&PlayerTag, &mut Experience)>,
    curve: Res<XpCurve>,
) {
    let entity = trigger.entity;
    let Ok((kind_opt, threat_opt, npc_opt)) = mobs.get(entity) else {
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
    let reward = xp_reward(*kind);
    if reward == 0 {
        return;
    }

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

    let Ok((_, mut xp)) = players.get_mut(killer) else {
        info!("[xp:mob-death] killer {killer:?} not a player (threat map had {} entries)", threat.0.len());
        return;
    };
    grant_xp(killer, &mut xp, &curve, reward, "kill");
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
