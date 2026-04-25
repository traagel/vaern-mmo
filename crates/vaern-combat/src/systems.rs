use bevy::prelude::*;

use crate::components::{
    AbilityCooldown, AbilityPriority, AbilityShape, AbilitySpec, Caster, CastRequest, Casting,
    CorpseOnDeath, Health, ManualCast, ProjectileVisual, Respawnable, ResourcePool, Stamina,
    Target,
};
use crate::effects::{EffectSpec, StatusEffects};

/// Emitted when an entity's HP crosses zero. Downstream systems (logging,
/// loot, sim exit) subscribe.
#[derive(Message, Debug, Clone, Copy)]
pub struct DeathEvent {
    pub entity: Entity,
}

/// Emitted when an ability resolves and deals damage (instant or
/// end-of-cast). Useful for logging, VFX triggers, and debug harnesses.
#[derive(Message, Debug, Clone)]
pub struct CastEvent {
    pub caster: Entity,
    pub ability: Entity,
    pub target: Entity,
    pub school: String,
    pub damage: f32,
    pub threat_multiplier: f32,
}

/// Advance each resource pool's regen, capped at its max.
pub fn regen_resources(time: Res<Time>, mut pools: Query<&mut ResourcePool>) {
    let dt = time.delta_secs();
    for mut pool in &mut pools {
        pool.current = (pool.current + pool.regen_per_sec * dt).min(pool.max);
    }
}

/// Tick every ability's cooldown toward zero.
pub fn tick_cooldowns(time: Res<Time>, mut abilities: Query<&mut AbilityCooldown>) {
    let dt = time.delta_secs();
    for mut cd in &mut abilities {
        cd.remaining_secs = (cd.remaining_secs - dt).max(0.0);
    }
}

/// Progress active casts. When a cast completes, apply its damage to the
/// cached target (or to every entity within `aoe_radius` of the origin for
/// AoE shapes), emit one CastEvent per damaged target, and remove the
/// Casting component.
pub fn progress_casts(
    time: Res<Time>,
    mut casts: Query<(Entity, &mut Casting)>,
    positions: Query<(Entity, &Transform), With<Health>>,
    mut healths: Query<&mut Health>,
    transforms: Query<&Transform>,
    stats: Query<&vaern_stats::CombinedStats>,
    mut stance_state: Query<(Option<&mut StatusEffects>, Option<&mut Stamina>)>,
    mut cast_out: MessageWriter<CastEvent>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();
    for (caster, mut casting) in &mut casts {
        casting.remaining_secs -= dt;
        if casting.remaining_secs > 0.0 {
            continue;
        }

        let caster_pos = transforms.get(caster).map(|t| t.translation).ok();
        let target_pos = transforms.get(casting.target).map(|t| t.translation).ok();

        let hit_spec = HitSpec {
            caster,
            caster_pos,
            primary_target: casting.target,
            target_pos,
            shape: casting.shape,
            range: casting.range,
            aoe_radius: casting.aoe_radius,
            cone_half_angle_deg: casting.cone_half_angle_deg,
            line_width: casting.line_width,
            aim: casting.aim,
        };

        // Projectile resolution: Casting is the channel time; when the channel
        // ends, spawn the projectile entity and let `tick_projectiles` handle
        // collision. No immediate damage here.
        if casting.shape == AbilityShape::Projectile {
            if let (Some(origin), Some(aim)) = (caster_pos, nonzero_aim(casting.aim)) {
                spawn_projectile(
                    &mut commands,
                    origin,
                    aim,
                    casting.projectile_speed,
                    casting.range,
                    casting.damage,
                    &casting.school,
                    casting.threat_multiplier,
                    casting.projectile_radius,
                    caster,
                    casting.ability,
                    casting.applies_effect.clone(),
                );
            }
        } else {
            let caster_stats = stats.get(caster).ok().copied();
            let caster_pos = caster_pos.unwrap_or(Vec3::ZERO);
            // Snapshot the caster's StatMods bonus once — stance_state is
            // borrowed mutably per-target in the loop, so we can't keep
            // a live reference. Buff duration is frame-granular, so the
            // snapshot is fine.
            let caster_bonus = stance_state
                .get(caster)
                .ok()
                .and_then(|(fx, _)| fx)
                .map(|fx| crate::damage::status_damage_bonus(Some(fx)))
                .unwrap_or(0.0);
            for target in resolve_hit_list(&hit_spec, &positions) {
                let target_stats = stats.get(target).ok().copied();
                let target_tf = transforms.get(target).copied().unwrap_or_default();
                let target_resist_bonus = stance_state
                    .get(target)
                    .ok()
                    .and_then(|(fx, _)| fx)
                    .map(|fx| {
                        crate::damage::status_resist_bonus_for_school(Some(fx), &casting.school)
                    })
                    .unwrap_or(0.0);
                let raw = crate::damage::compute_damage(
                    casting.damage,
                    &casting.school,
                    caster_stats.as_ref(),
                    caster_bonus,
                    target_stats.as_ref(),
                    target_resist_bonus,
                    &mut rand::rng(),
                );
                let final_damage = resolve_hit(
                    &mut stance_state,
                    target,
                    caster,
                    caster_pos,
                    &target_tf,
                    raw.final_damage,
                    casting.applies_effect.as_ref(),
                    &casting.school,
                    &mut commands,
                );
                if let Ok(mut hp) = healths.get_mut(target) {
                    hp.current = (hp.current - final_damage).max(0.0);
                }
                cast_out.write(CastEvent {
                    caster,
                    ability: casting.ability,
                    target,
                    school: casting.school.clone(),
                    damage: final_damage,
                    threat_multiplier: casting.threat_multiplier,
                });
            }
        }

        if let Ok(mut ec) = commands.get_entity(caster) {
            ec.remove::<Casting>();
        }
    }
}

/// Parameters that `resolve_hit_list` needs to decide which entities are
/// affected by an ability's resolution. Bundled to keep the function signature
/// tractable.
struct HitSpec {
    caster: Entity,
    caster_pos: Option<Vec3>,
    primary_target: Entity,
    target_pos: Option<Vec3>,
    shape: AbilityShape,
    range: f32,
    aoe_radius: f32,
    cone_half_angle_deg: f32,
    line_width: f32,
    /// Horizontal unit vector from caster to target at cast start. Zero
    /// means "no aim" — Cone/Line fizzle.
    aim: Vec3,
}

/// Enumerate which entities an ability's resolution should damage.
/// Projectile is handled separately (spawns an entity, resolves via
/// `tick_projectiles`).
fn resolve_hit_list(
    spec: &HitSpec,
    positions: &Query<(Entity, &Transform), With<Health>>,
) -> Vec<Entity> {
    match spec.shape {
        AbilityShape::Target => vec![spec.primary_target],
        AbilityShape::AoeOnTarget | AbilityShape::AoeOnSelf => {
            let origin = match spec.shape {
                AbilityShape::AoeOnSelf => spec.caster_pos,
                _ => spec.target_pos,
            };
            let Some(origin) = origin else { return vec![] };
            let r2 = spec.aoe_radius * spec.aoe_radius;
            positions
                .iter()
                .filter(|(e, _)| *e != spec.caster)
                .filter(|(_, tf)| tf.translation.distance_squared(origin) <= r2)
                .map(|(e, _)| e)
                .collect()
        }
        AbilityShape::Cone => {
            let Some(origin) = spec.caster_pos else { return vec![] };
            let aim = match nonzero_aim(spec.aim) {
                Some(a) => a,
                None => return vec![],
            };
            let r2 = spec.range * spec.range;
            let cos_half = spec.cone_half_angle_deg.to_radians().cos();
            positions
                .iter()
                .filter(|(e, _)| *e != spec.caster)
                .filter_map(|(e, tf)| {
                    let mut to = tf.translation - origin;
                    to.y = 0.0;
                    let d2 = to.length_squared();
                    if d2 > r2 || d2 < 1e-4 {
                        return None;
                    }
                    let dir = to / d2.sqrt();
                    if dir.dot(aim) >= cos_half { Some(e) } else { None }
                })
                .collect()
        }
        AbilityShape::Line => {
            let Some(origin) = spec.caster_pos else { return vec![] };
            let aim = match nonzero_aim(spec.aim) {
                Some(a) => a,
                None => return vec![],
            };
            let half_w = (spec.line_width * 0.5).max(0.1);
            positions
                .iter()
                .filter(|(e, _)| *e != spec.caster)
                .filter_map(|(e, tf)| {
                    let mut to = tf.translation - origin;
                    to.y = 0.0;
                    // Project onto aim; perpendicular distance is the
                    // remaining component's length.
                    let along = to.dot(aim);
                    if along <= 0.0 || along > spec.range {
                        return None;
                    }
                    let perp = to - aim * along;
                    if perp.length() <= half_w { Some(e) } else { None }
                })
                .collect()
        }
        AbilityShape::Projectile => Vec::new(),
    }
}

/// Resolve a single hit against `target`: apply stance-based damage
/// modification (Parry full-negate, Block reduction) and, on actual
/// damage land, attach the ability's rider effect (DoT / Slow) via
/// the target's `StatusEffects`. Returns final damage applied to HP.
///
/// `target_tf` must be the target's actual world transform — block
/// angle math reads its facing. Callers already query it from the
/// positions table.
///
/// Rider effects are gated on `final_damage > 0` so parried / missed
/// hits don't apply debuffs — matches the "successful parry negates
/// the whole hit" stance promise.
#[allow(clippy::too_many_arguments)]
fn resolve_hit(
    stance_state: &mut Query<(Option<&mut StatusEffects>, Option<&mut Stamina>)>,
    target: Entity,
    caster: Entity,
    caster_pos: Vec3,
    target_tf: &Transform,
    raw: f32,
    rider: Option<&EffectSpec>,
    rider_fallback_school: &str,
    commands: &mut Commands,
) -> f32 {
    let Ok((mut effects, mut stamina)) = stance_state.get_mut(target) else {
        return raw;
    };

    // Stances first. `as_deref_mut` reborrows the Mut wrappers without
    // consuming them — the rider branch below can still reach
    // StatusEffects mutably afterward.
    let final_damage = crate::damage::apply_stances(
        raw,
        caster_pos,
        target_tf,
        effects.as_deref_mut(),
        stamina.as_deref_mut(),
    );

    if final_damage > 0.0 {
        if let Some(spec) = rider {
            let effect = spec.build(caster, rider_fallback_school);
            match effects {
                Some(mut existing) => existing.apply(effect),
                None => {
                    let mut fresh = StatusEffects::default();
                    fresh.apply(effect);
                    commands.entity(target).insert(fresh);
                }
            }
        }
    }
    final_damage
}

/// XZ-plane unit vector if non-degenerate, else None.
fn nonzero_aim(v: Vec3) -> Option<Vec3> {
    let flat = Vec3::new(v.x, 0.0, v.z);
    if flat.length_squared() < 1e-4 {
        None
    } else {
        Some(flat.normalize())
    }
}

/// For each caster, pick an ability and fire it.
///
/// - Casters with `ManualCast` only fire when a `CastRequest` is attached
///   (input-driven or scripted). The request is consumed regardless of
///   whether the cast actually resolves.
/// - Casters without `ManualCast` auto-select the highest-priority
///   ready+affordable ability.
///
/// Instant abilities resolve immediately; non-instant ones attach a `Casting`
/// component. Casters already mid-cast are skipped.
/// Resource cost and cooldown are consumed at cast START.
pub fn select_and_fire(
    mut abilities: Query<(
        Entity,
        &AbilitySpec,
        &mut AbilityCooldown,
        &Caster,
        Option<&AbilityPriority>,
    )>,
    casters: Query<(
        Entity,
        &Target,
        Option<&ManualCast>,
        Option<&CastRequest>,
    )>,
    casting: Query<(), With<Casting>>,
    mut pools: Query<&mut ResourcePool>,
    mut healths: Query<&mut Health>,
    positions: Query<(Entity, &Transform), With<Health>>,
    transforms: Query<&Transform>,
    stats: Query<&vaern_stats::CombinedStats>,
    mut stance_state: Query<(Option<&mut StatusEffects>, Option<&mut Stamina>)>,
    mut cast_out: MessageWriter<CastEvent>,
    mut commands: Commands,
) {
    // Build per-caster selection: if manual, use CastRequest's ability;
    // otherwise auto-select highest-priority ready ability.
    struct Pick {
        caster: Entity,
        target: Entity,
        ability: Entity,
    }

    // Index caster abilities: map caster -> Vec<(priority, ability_e, is_ready)>.
    use std::collections::HashMap;
    let mut per_caster: HashMap<Entity, Vec<(u8, Entity, bool)>> = HashMap::new();
    for (ability_e, _spec, cd, caster, priority) in abilities.iter() {
        per_caster.entry(caster.0).or_default().push((
            priority.map(|p| p.0).unwrap_or(0),
            ability_e,
            cd.is_ready(),
        ));
    }

    let mut picks: Vec<Pick> = Vec::new();
    for (caster, target, manual, request) in casters.iter() {
        if casting.get(caster).is_ok() {
            // Mid-cast. Drop any stale CastRequest anyway.
            if request.is_some() {
                if let Ok(mut ec) = commands.get_entity(caster) {
                    ec.remove::<CastRequest>();
                }
            }
            continue;
        }

        let ability_e = if manual.is_some() {
            let Some(req) = request else { continue };
            // Consume the request whether or not it resolves.
            if let Ok(mut ec) = commands.get_entity(caster) {
                ec.remove::<CastRequest>();
            }
            // Only accept if the requested ability is owned by this caster and ready.
            let owned_ready = per_caster
                .get(&caster)
                .is_some_and(|v| v.iter().any(|(_, e, ready)| *e == req.0 && *ready));
            if !owned_ready {
                continue;
            }
            req.0
        } else {
            // Auto-select: highest-priority ready ability.
            let Some(list) = per_caster.get(&caster) else { continue };
            let Some(&(_, chosen, _)) = list
                .iter()
                .filter(|(_, _, ready)| *ready)
                .max_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.cmp(&a.1)))
            else {
                continue;
            };
            chosen
        };

        picks.push(Pick { caster, target: target.0, ability: ability_e });
    }

    for Pick { caster, target, ability } in picks {
        let Ok((_, spec, mut cd, _, _)) = abilities.get_mut(ability) else { continue };
        let spec = spec.clone();

        let caster_tf = transforms.get(caster).ok();
        let caster_pos = caster_tf.map(|t| t.translation);
        let target_pos = transforms.get(target).ok().map(|t| t.translation);

        // Range check at cast start. Skips cost + cooldown consumption so
        // out-of-range casts are free to retry. AoeOnSelf ignores range (the
        // caster IS the origin).
        if !matches!(spec.shape, AbilityShape::AoeOnSelf) {
            if let (Some(cp), Some(tp)) = (caster_pos, target_pos) {
                if cp.distance(tp) > spec.range {
                    continue;
                }
            }
        }

        // Aim vector: caster's facing direction in the XZ plane. Cone/Line/
        // Projectile fire along this vector — players miss if the target is
        // behind them, and must actually aim to land cones. Target /
        // AoeOnTarget still bias toward the locked target (they don't use
        // aim). Snapshotted onto Casting for channeled casts so mouse-aim
        // during windup doesn't retarget the swing.
        let aim = caster_tf
            .map(|tf| {
                let fwd = tf.rotation * Vec3::NEG_Z;
                let mut flat = Vec3::new(fwd.x, 0.0, fwd.z);
                if flat.length_squared() > 1e-4 {
                    flat = flat.normalize();
                } else {
                    flat = Vec3::ZERO;
                }
                flat
            })
            .unwrap_or(Vec3::ZERO);

        // Resource check / deduct at cast start.
        match pools.get_mut(caster) {
            Ok(mut pool) => {
                if !pool.can_afford(spec.resource_cost) {
                    continue;
                }
                pool.current -= spec.resource_cost;
            }
            Err(_) if spec.resource_cost > 0.0 => continue,
            Err(_) => {}
        }

        // Haste scales both cast time and cooldown uniformly, snapshotted
        // at cast start. No GCD, so haste is the only cast/cd accelerator.
        let haste_scale = stats
            .get(caster)
            .map(|s| vaern_stats::formula::cast_speed_scale(s.total_haste_pct))
            .unwrap_or(1.0);
        cd.remaining_secs = spec.cooldown_secs * haste_scale;

        if spec.cast_secs > 0.0 {
            let scaled_cast_secs = spec.cast_secs * haste_scale;
            if let Ok(mut ec) = commands.get_entity(caster) {
                ec.insert(Casting {
                    ability,
                    target,
                    remaining_secs: scaled_cast_secs,
                    total_secs: scaled_cast_secs,
                    damage: spec.damage,
                    school: spec.school,
                    threat_multiplier: spec.threat_multiplier,
                    shape: spec.shape,
                    range: spec.range,
                    aoe_radius: spec.aoe_radius,
                    cone_half_angle_deg: spec.cone_half_angle_deg,
                    line_width: spec.line_width,
                    projectile_speed: spec.projectile_speed,
                    projectile_radius: spec.projectile_radius,
                    aim,
                    applies_effect: spec.applies_effect,
                });
            }
        } else {
            // Instant resolution. Projectile: spawn a travelling entity,
            // damage resolves via `tick_projectiles`. Other shapes: fan out.
            if spec.shape == AbilityShape::Projectile {
                if let (Some(origin), Some(aim_dir)) = (caster_pos, nonzero_aim(aim)) {
                    spawn_projectile(
                        &mut commands,
                        origin,
                        aim_dir,
                        spec.projectile_speed,
                        spec.range,
                        spec.damage,
                        &spec.school,
                        spec.threat_multiplier,
                        spec.projectile_radius,
                        caster,
                        ability,
                        spec.applies_effect.clone(),
                    );
                }
                continue;
            }
            let hit_spec = HitSpec {
                caster,
                caster_pos,
                primary_target: target,
                target_pos,
                shape: spec.shape,
                range: spec.range,
                aoe_radius: spec.aoe_radius,
                cone_half_angle_deg: spec.cone_half_angle_deg,
                line_width: spec.line_width,
                aim,
            };
            let caster_stats = stats.get(caster).ok().copied();
            let caster_pos_resolved = caster_pos.unwrap_or(Vec3::ZERO);
            let caster_bonus = stance_state
                .get(caster)
                .ok()
                .and_then(|(fx, _)| fx)
                .map(|fx| crate::damage::status_damage_bonus(Some(fx)))
                .unwrap_or(0.0);
            for victim in resolve_hit_list(&hit_spec, &positions) {
                let target_stats = stats.get(victim).ok().copied();
                let target_tf = transforms.get(victim).copied().unwrap_or_default();
                let target_resist_bonus = stance_state
                    .get(victim)
                    .ok()
                    .and_then(|(fx, _)| fx)
                    .map(|fx| {
                        crate::damage::status_resist_bonus_for_school(Some(fx), &spec.school)
                    })
                    .unwrap_or(0.0);
                let raw = crate::damage::compute_damage(
                    spec.damage,
                    &spec.school,
                    caster_stats.as_ref(),
                    caster_bonus,
                    target_stats.as_ref(),
                    target_resist_bonus,
                    &mut rand::rng(),
                );
                let final_damage = resolve_hit(
                    &mut stance_state,
                    victim,
                    caster,
                    caster_pos_resolved,
                    &target_tf,
                    raw.final_damage,
                    spec.applies_effect.as_ref(),
                    &spec.school,
                    &mut commands,
                );
                if let Ok(mut hp) = healths.get_mut(victim) {
                    hp.current = (hp.current - final_damage).max(0.0);
                }
                cast_out.write(CastEvent {
                    caster,
                    ability,
                    target: victim,
                    school: spec.school.clone(),
                    damage: final_damage,
                    threat_multiplier: spec.threat_multiplier,
                });
            }
        }
    }
}

// ─── projectiles ───────────────────────────────────────────────────────────

/// Helper: spawn a projectile entity with server-side simulation state + a
/// replicated `Transform` + `ProjectileVisual` marker so clients can render
/// it as it flies. Caller supplies raw stats; clamping is applied here.
#[allow(clippy::too_many_arguments)]
fn spawn_projectile(
    commands: &mut Commands,
    origin: Vec3,
    dir: Vec3,
    speed: f32,
    max_range: f32,
    damage: f32,
    school: &str,
    threat_multiplier: f32,
    radius: f32,
    caster: Entity,
    ability: Entity,
    applies_effect: Option<EffectSpec>,
) {
    // Start the visual slightly above the ground so it reads against terrain.
    let visual_origin = origin + Vec3::new(0.0, 1.2, 0.0);
    commands.spawn((
        Projectile {
            origin: visual_origin,
            current: visual_origin,
            dir,
            speed: speed.max(0.1),
            traveled: 0.0,
            max_range,
            damage,
            school: school.to_string(),
            threat_multiplier,
            radius: radius.max(0.1),
            caster,
            ability,
            applies_effect,
        },
        Transform::from_translation(visual_origin),
        ProjectileVisual {
            school: school.to_string(),
        },
    ));
}

/// A live in-flight projectile. Owned by the server; one entity per shot.
/// Resolves via `tick_projectiles` — a naive linear sweep with swept-sphere
/// collision against Health-bearing entities.
#[derive(Component, Debug, Clone)]
pub struct Projectile {
    pub origin: Vec3,
    pub current: Vec3,
    /// Horizontal unit direction.
    pub dir: Vec3,
    pub speed: f32,
    pub traveled: f32,
    pub max_range: f32,
    pub damage: f32,
    pub school: String,
    pub threat_multiplier: f32,
    pub radius: f32,
    pub caster: Entity,
    pub ability: Entity,
    /// Status-effect rider attached to the first entity the projectile
    /// hits (if that hit actually lands — parry blocks it). Snapshotted
    /// from the owning ability at spawn time.
    pub applies_effect: Option<EffectSpec>,
}

/// Advance each `Projectile`, resolve first-hit against any Health-bearing
/// entity (excluding the caster), despawn on hit or on reaching max range.
/// Runs in FixedUpdate for deterministic travel.
pub fn tick_projectiles(
    time: Res<Time>,
    mut projectiles: Query<(Entity, &mut Projectile, &mut Transform)>,
    targets: Query<(Entity, &Transform), (With<Health>, Without<Projectile>)>,
    mut healths: Query<&mut Health>,
    stats: Query<&vaern_stats::CombinedStats>,
    mut stance_state: Query<(Option<&mut StatusEffects>, Option<&mut Stamina>)>,
    mut cast_out: MessageWriter<CastEvent>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();
    for (pe, mut proj, mut tf) in &mut projectiles {
        let step = (proj.speed * dt).max(0.0);
        let next = proj.current + proj.dir * step;

        // Swept-sphere collision: find nearest entity whose perpendicular
        // distance to the segment current→next is ≤ radius AND whose
        // projection onto the segment is inside [0, step]. Horizontal only.
        let seg_len = step.max(1e-4);
        let seg = proj.dir;
        let mut best: Option<(f32, Entity)> = None;
        let r2 = proj.radius * proj.radius;
        for (e, tf) in &targets {
            if e == proj.caster {
                continue;
            }
            let mut to = tf.translation - proj.current;
            to.y = 0.0;
            let along = to.dot(seg).clamp(0.0, seg_len);
            let closest = proj.current + seg * along;
            let mut d = tf.translation - closest;
            d.y = 0.0;
            if d.length_squared() <= r2 {
                let score = along;
                if best.map_or(true, |(s, _)| score < s) {
                    best = Some((score, e));
                }
            }
        }

        if let Some((_, victim)) = best {
            let caster_stats = stats.get(proj.caster).ok().copied();
            let caster_bonus = stance_state
                .get(proj.caster)
                .ok()
                .and_then(|(fx, _)| fx)
                .map(|fx| crate::damage::status_damage_bonus(Some(fx)))
                .unwrap_or(0.0);
            let target_stats = stats.get(victim).ok().copied();
            let target_tf = targets
                .get(victim)
                .map(|(_, tf)| *tf)
                .unwrap_or_default();
            let target_resist_bonus = stance_state
                .get(victim)
                .ok()
                .and_then(|(fx, _)| fx)
                .map(|fx| {
                    crate::damage::status_resist_bonus_for_school(Some(fx), &proj.school)
                })
                .unwrap_or(0.0);
            let raw = crate::damage::compute_damage(
                proj.damage,
                &proj.school,
                caster_stats.as_ref(),
                caster_bonus,
                target_stats.as_ref(),
                target_resist_bonus,
                &mut rand::rng(),
            );
            let final_damage = resolve_hit(
                &mut stance_state,
                victim,
                proj.caster,
                proj.current,
                &target_tf,
                raw.final_damage,
                proj.applies_effect.as_ref(),
                &proj.school,
                &mut commands,
            );
            if let Ok(mut hp) = healths.get_mut(victim) {
                hp.current = (hp.current - final_damage).max(0.0);
            }
            cast_out.write(CastEvent {
                caster: proj.caster,
                ability: proj.ability,
                target: victim,
                school: proj.school.clone(),
                damage: final_damage,
                threat_multiplier: proj.threat_multiplier,
            });
            if let Ok(mut ec) = commands.get_entity(pe) {
                ec.despawn();
            }
            continue;
        }

        proj.traveled += step;
        proj.current = next;
        tf.translation = next;
        if proj.traveled >= proj.max_range {
            if let Ok(mut ec) = commands.get_entity(pe) {
                ec.despawn();
            }
        }
    }
}

/// Emit DeathEvent for any entity that crossed zero HP this tick. Despawn is
/// deferred to apply_deaths so listeners can still read the entity.
pub fn detect_deaths(
    q: Query<(Entity, &Health), Changed<Health>>,
    mut out: MessageWriter<DeathEvent>,
) {
    for (entity, hp) in &q {
        if hp.is_dead() {
            out.write(DeathEvent { entity });
        }
    }
}

/// On DeathEvent: if the entity has `Respawnable`, reset HP / resource /
/// position and clear its Target + Casting. Otherwise despawn. Keeps player
/// entities alive across deaths so the client's Predicted copy — and every
/// UI that reads its Health — stays coherent.
pub fn apply_deaths(
    mut events: MessageReader<DeathEvent>,
    mut respawnables: Query<
        (
            &Respawnable,
            &mut Health,
            &mut Transform,
            Option<&mut ResourcePool>,
            Option<&CorpseOnDeath>,
        ),
        (),
    >,
    mut commands: Commands,
) {
    for ev in events.read() {
        if let Ok((respawn, mut hp, mut tf, pool, corpse_on_death)) =
            respawnables.get_mut(ev.entity)
        {
            // CorpseOnDeath entities (players) are handled by a server-side
            // corpse-run system that needs the death position before
            // teleport. Skip the default reset here.
            if corpse_on_death.is_some() {
                continue;
            }
            hp.current = hp.max;
            tf.translation = respawn.home;
            if let Some(mut p) = pool {
                p.current = p.max;
            }
            if let Ok(mut ec) = commands.get_entity(ev.entity) {
                ec.remove::<Target>();
                ec.remove::<Casting>();
            }
            continue;
        }
        if let Ok(mut ec) = commands.get_entity(ev.entity) {
            ec.despawn();
        }
    }
}

/// Despawn ability entities whose caster no longer exists.
pub fn cleanup_orphan_abilities(
    abilities: Query<(Entity, &Caster)>,
    existing: Query<()>,
    mut commands: Commands,
) {
    for (ability_e, caster) in &abilities {
        if existing.get(caster.0).is_err() {
            if let Ok(mut ec) = commands.get_entity(ability_e) {
                ec.despawn();
            }
        }
    }
}
