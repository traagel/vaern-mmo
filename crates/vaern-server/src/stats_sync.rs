//! Keep `CombinedStats` denormalized onto every combat-capable entity.
//!
//! Combat reads `CombinedStats` per hit to scale caster damage and
//! apply target mitigation. Folding material + quality + affix stats
//! every hit would be wasteful; instead this module recomputes the
//! fold only on state change and caches the result as a component.
//!
//! Triggers:
//!   * `Changed<Equipped>` — a new item equipped or removed.
//!   * `Changed<PillarScores>` — pillar-point gain from ability casts.
//!
//! NPCs currently have no Equipped / PillarScores, so their entities
//! never gain `CombinedStats` — combat falls through to raw-damage
//! math for them. Once NPC stat blocks land this system grows to
//! cover them too (or a parallel system does).

use bevy::prelude::*;

use vaern_equipment::Equipped;
use vaern_items::SecondaryStats;
use vaern_stats::{PillarScores, TertiaryStats, combine, derive_primaries};

use crate::data::GameData;

/// Recompute `CombinedStats` for every player whose `Equipped` or
/// `PillarScores` changed. Also runs on first insert (Bevy's Changed
/// filter fires on Added).
pub fn sync_combined_stats(
    data: Res<GameData>,
    changed: Query<
        (Entity, &PillarScores, &Equipped),
        Or<(Changed<Equipped>, Changed<PillarScores>)>,
    >,
    mut commands: Commands,
) {
    for (entity, pillars, equipped) in &changed {
        let derived = derive_primaries(pillars);
        let mut gear = SecondaryStats::default();
        for (_, instance) in equipped.iter() {
            if let Ok(resolved) = data.content.resolve(instance) {
                gear.add(&resolved.stats);
            }
        }
        let combined = combine(&derived, &gear, &TertiaryStats::default());
        commands.entity(entity).insert(combined);
    }
}

