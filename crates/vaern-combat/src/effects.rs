//! Timed status effects: damage-over-time, stances (Block / Parry), and
//! generic stat-mod buffs/debuffs.
//!
//! Modelled as a single `StatusEffects` component per target entity —
//! one Vec, not one ECS entity per effect — because effects are queried
//! on every hit (damage site) and per-tick (this module's system).
//! Cache locality wins over ECS purity here.
//!
//! Every effect has a lifetime (`remaining_secs`), and optionally a
//! recurring tick (`tick_interval` / `tick_remaining`) for DoTs. When
//! `remaining_secs` hits zero, or when a `Stance::Block`'s stamina
//! runs out, the effect is dropped from the Vec.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{Health, Stamina};
use crate::systems::CastEvent;

/// Short stable id for display + stacking rules. DoTs refreshing
/// themselves (e.g. "burning" + new burning hit) find the existing
/// effect by this id and refresh `remaining_secs`.
pub type EffectId = String;

/// All timed modifications that can live on a combat target.
#[derive(Debug, Clone)]
pub struct StatusEffect {
    pub id: EffectId,
    pub source: Entity,
    pub kind: EffectKind,
    /// Seconds left before the effect expires. `f32::INFINITY` for
    /// "lasts until manually removed" (e.g. an active Block stance
    /// held by the player).
    pub remaining_secs: f32,
    /// Seconds to next DoT tick (or other recurring action).
    /// Ignored when `tick_interval == 0`.
    pub tick_interval: f32,
    pub tick_remaining: f32,
}

impl StatusEffect {
    /// Build a standard DoT effect from damage-per-second + duration.
    pub fn dot(
        id: impl Into<EffectId>,
        source: Entity,
        school: impl Into<String>,
        dps: f32,
        tick_interval: f32,
        duration_secs: f32,
    ) -> Self {
        Self {
            id: id.into(),
            source,
            kind: EffectKind::Dot {
                damage_per_tick: dps * tick_interval,
                school: school.into(),
                threat_multiplier: 1.0,
            },
            remaining_secs: duration_secs,
            tick_interval,
            tick_remaining: tick_interval,
        }
    }

    /// Build an active Block stance. Drains stamina continuously; the
    /// tick system removes the effect when stamina runs out.
    pub fn block(
        source: Entity,
        drain_per_sec: f32,
        frontal_reduction: f32,
        flank_reduction: f32,
    ) -> Self {
        Self {
            id: "blocking".into(),
            source,
            kind: EffectKind::Stance(StanceKind::Block {
                drain_per_sec,
                frontal_reduction,
                flank_reduction,
            }),
            remaining_secs: f32::INFINITY,
            tick_interval: 0.0,
            tick_remaining: 0.0,
        }
    }

    /// Build a one-shot Parry window. Full-negates the first incoming
    /// hit landed within the window, then consumes itself.
    pub fn parry(source: Entity, window_secs: f32, stamina_cost: f32) -> Self {
        Self {
            id: "parrying".into(),
            source,
            kind: EffectKind::Stance(StanceKind::Parry { stamina_cost }),
            remaining_secs: window_secs,
            tick_interval: 0.0,
            tick_remaining: 0.0,
        }
    }

    /// Build a timed `StatMods` buff. Used by consumables (damage +
    /// resist potions) and later by rites / auras.
    ///
    /// * `damage_mult_add` — additive onto caster mult, e.g. 0.2 = +20%
    ///   outgoing damage.
    /// * `resist_adds` — per-`DamageType` resist while the buff is
    ///   active (e.g. `[20.0]` on the fire index for a fire-resist
    ///   potion). Pass `[0.0; DAMAGE_TYPE_COUNT]` for pure offense
    ///   buffs.
    pub fn stat_mods(
        id: impl Into<EffectId>,
        source: Entity,
        duration_secs: f32,
        damage_mult_add: f32,
        resist_adds: [f32; vaern_core::DAMAGE_TYPE_COUNT],
    ) -> Self {
        Self {
            id: id.into(),
            source,
            kind: EffectKind::StatMods {
                damage_mult_add,
                resist_adds,
            },
            remaining_secs: duration_secs,
            tick_interval: 0.0,
            tick_remaining: 0.0,
        }
    }
}

/// Declarative description of the status-effect an ability applies on
/// a successful hit. Lives on `AbilitySpec` / `Casting` / `Projectile`
/// so each damage site can pick it up and attach a fresh
/// `StatusEffect` to the target. Populated from flavored YAML via
/// `class_kits::apply_flavored_overrides`; absent means no rider.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EffectSpec {
    pub id: String,
    pub duration_secs: f32,
    pub kind: EffectKindSpec,
}

/// Serialization-friendly mirror of `EffectKind` with `#[serde(tag)]`
/// so YAML drivers stay readable. `EffectKind`'s runtime variants can
/// carry non-serde fields (Entity refs) — this type is pure data.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum EffectKindSpec {
    Dot {
        dps: f32,
        tick_interval: f32,
        #[serde(default)]
        school: Option<String>,
    },
    Slow {
        speed_mult: f32,
    },
}

impl EffectSpec {
    /// Build a concrete `StatusEffect` from this spec, using the
    /// caster entity as the source and `fallback_school` when the
    /// spec's DoT variant didn't specify one (common case — the effect
    /// just inherits the owning ability's school).
    pub fn build(&self, source: Entity, fallback_school: &str) -> StatusEffect {
        let kind = match &self.kind {
            EffectKindSpec::Dot {
                dps,
                tick_interval,
                school,
            } => {
                let school = school.as_deref().unwrap_or(fallback_school).to_string();
                EffectKind::Dot {
                    damage_per_tick: dps * tick_interval,
                    school,
                    threat_multiplier: 1.0,
                }
            }
            EffectKindSpec::Slow { speed_mult } => EffectKind::Slow {
                speed_mult: *speed_mult,
            },
        };
        let tick_interval = match &self.kind {
            EffectKindSpec::Dot { tick_interval, .. } => *tick_interval,
            _ => 0.0,
        };
        StatusEffect {
            id: self.id.clone(),
            source,
            kind,
            remaining_secs: self.duration_secs,
            tick_interval,
            tick_remaining: tick_interval,
        }
    }
}

#[derive(Debug, Clone)]
pub enum EffectKind {
    /// Damage over time. Tick interval + per-tick damage baked in at
    /// construction (see `StatusEffect::dot`).
    Dot {
        damage_per_tick: f32,
        school: String,
        threat_multiplier: f32,
    },
    /// Active stance — mutually exclusive by id (only one of
    /// `"blocking"` / `"parrying"` at a time, enforced at apply site).
    Stance(StanceKind),
    /// Movement-speed debuff. `speed_mult` multiplies the carrier's
    /// base movement step (0.5 = half speed). Strongest slow wins —
    /// stacking more chills doesn't compound.
    Slow { speed_mult: f32 },
    /// Generic stat mods — an additive to the outgoing-damage multiplier
    /// plus per-damage-type resist additions. `compute_damage` sums the
    /// offensive side via `status_damage_bonus` and the defensive side
    /// via `status_resist_bonus`. Consumables and later rites push these
    /// onto their target.
    StatMods {
        /// Additive to melee_mult/spell_mult (e.g. 0.2 = +20%). Can be
        /// negative for debuffs.
        damage_mult_add: f32,
        /// Per-`DamageType` resist added while active. Flat 12-element
        /// array mirroring `SecondaryStats.resists`. Folded into the
        /// target's `resist_total[dt]` before the resist mitigation
        /// curve. The 80% resist cap in `compute_damage` still applies.
        resist_adds: [f32; vaern_core::DAMAGE_TYPE_COUNT],
    },
}

/// Separated so damage.rs can pattern-match stance variants without
/// depending on the full `EffectKind` enum.
#[derive(Debug, Clone, Copy)]
pub enum StanceKind {
    Block {
        drain_per_sec: f32,
        /// Fraction of damage removed when the hit lands in the target's
        /// front cone. Range 0..=1.
        frontal_reduction: f32,
        /// Weaker reduction for side hits. Front / flank split reflects
        /// the stance-aware protection of a real shield: backstabs go
        /// through untouched.
        flank_reduction: f32,
    },
    Parry {
        /// Stamina cost debited at the moment the parry consumes itself
        /// (actually absorbing a hit). Free to tap and miss.
        stamina_cost: f32,
    },
}

/// Per-entity collection of active effects. Absence of the component
/// means "no active effects"; we still only apply `try_insert` when
/// an effect is actually pushed.
#[derive(Component, Debug, Default, Clone)]
pub struct StatusEffects(pub Vec<StatusEffect>);

impl StatusEffects {
    /// Apply an effect, refreshing an existing one of the same id
    /// instead of stacking. Standard MMO "refresh-on-reapply" semantics.
    pub fn apply(&mut self, effect: StatusEffect) {
        if let Some(existing) = self.0.iter_mut().find(|e| e.id == effect.id) {
            *existing = effect;
        } else {
            self.0.push(effect);
        }
    }

    /// Remove any effect with this id. Returns `true` if one was found.
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.0.len();
        self.0.retain(|e| e.id != id);
        before != self.0.len()
    }

    pub fn has(&self, id: &str) -> bool {
        self.0.iter().any(|e| e.id == id)
    }

    /// Find the currently-active Block stance, if any. Returns the
    /// stance parameters and the effect's parent index (so callers
    /// can mutate stamina externally without re-walking the Vec).
    pub fn active_block(&self) -> Option<StanceKind> {
        self.0.iter().find_map(|e| match e.kind {
            EffectKind::Stance(s @ StanceKind::Block { .. }) => Some(s),
            _ => None,
        })
    }

    /// Find the currently-active Parry window, if any.
    pub fn active_parry(&self) -> Option<StanceKind> {
        self.0.iter().find_map(|e| match e.kind {
            EffectKind::Stance(s @ StanceKind::Parry { .. }) => Some(s),
            _ => None,
        })
    }

    /// Strongest active slow multiplier. Returns `1.0` when no Slow
    /// effect is active (identity multiplier — movement unchanged).
    /// Multiple slows don't stack — only the deepest (lowest mult)
    /// applies, to keep kite chains from reducing a target to 0 speed.
    pub fn move_speed_mult(&self) -> f32 {
        let mut mult = 1.0_f32;
        for eff in &self.0 {
            if let EffectKind::Slow { speed_mult } = eff.kind {
                mult = mult.min(speed_mult.clamp(0.0, 1.0));
            }
        }
        mult
    }

    /// Consume the active parry (called when a hit is negated by it).
    /// Returns the stamina cost the caller should debit.
    pub fn consume_parry(&mut self) -> Option<f32> {
        let idx = self.0.iter().position(|e| {
            matches!(e.kind, EffectKind::Stance(StanceKind::Parry { .. }))
        })?;
        let cost = match self.0[idx].kind {
            EffectKind::Stance(StanceKind::Parry { stamina_cost }) => stamina_cost,
            _ => 0.0,
        };
        self.0.swap_remove(idx);
        Some(cost)
    }
}

/// Advance every active effect's lifetime, fire DoT ticks, drain
/// stamina for active Block stances, and drop expired entries.
///
/// Runs in Update. DoT damage resolves through `Health` directly and
/// emits a `CastEvent` so threat credit + floating damage numbers
/// still work.
pub fn tick_status_effects(
    time: Res<Time>,
    mut targets: Query<(Entity, &mut StatusEffects, Option<&mut Stamina>)>,
    mut healths: Query<&mut Health>,
    mut cast_out: MessageWriter<CastEvent>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    for (target_entity, mut effects, stamina) in &mut targets {
        if effects.0.is_empty() {
            continue;
        }

        // Stance stamina drain (only Block today). Handled first so
        // that a stance breaking this frame still prevents its DoT
        // handling below from double-counting.
        if let Some(mut stamina) = stamina {
            let mut drained = 0.0;
            for eff in effects.0.iter() {
                if let EffectKind::Stance(StanceKind::Block { drain_per_sec, .. }) = eff.kind {
                    drained += drain_per_sec * dt;
                }
            }
            if drained > 0.0 {
                stamina.current = (stamina.current - drained).max(0.0);
                if stamina.current <= 0.0 {
                    // Block breaks when stamina empties.
                    effects.remove("blocking");
                }
            }
        }

        for eff in effects.0.iter_mut() {
            eff.remaining_secs -= dt;

            // DoT tick
            if let EffectKind::Dot {
                damage_per_tick,
                school,
                threat_multiplier,
            } = &eff.kind
            {
                if eff.tick_interval > 0.0 {
                    eff.tick_remaining -= dt;
                    while eff.tick_remaining <= 0.0 && eff.remaining_secs > -eff.tick_interval {
                        eff.tick_remaining += eff.tick_interval;
                        if let Ok(mut hp) = healths.get_mut(target_entity) {
                            hp.current = (hp.current - *damage_per_tick).max(0.0);
                        }
                        cast_out.write(CastEvent {
                            caster: eff.source,
                            ability: eff.source, // DoTs have no ability entity; reuse source
                            target: target_entity,
                            school: school.clone(),
                            damage: *damage_per_tick,
                            threat_multiplier: *threat_multiplier,
                        });
                    }
                }
            }
        }

        // Drop expired effects. `<= 0.0` handles both the finite-duration
        // case and the "stance broke, remaining_secs forced to 0" case.
        effects
            .0
            .retain(|eff| eff.remaining_secs > 0.0);

        // No active effects left? Detach the component entirely so the
        // tick-loop short-circuit above skips this entity next frame.
        if effects.0.is_empty() {
            commands.entity(target_entity).remove::<StatusEffects>();
        }
    }
}

/// Passive stamina regen. Runs every frame; clamps to max.
pub fn regen_stamina(time: Res<Time>, mut pools: Query<&mut Stamina>) {
    let dt = time.delta_secs();
    for mut s in &mut pools {
        s.current = (s.current + s.regen_per_sec * dt).min(s.max);
    }
}
