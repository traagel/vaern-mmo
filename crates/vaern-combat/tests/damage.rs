//! Proves the combat plane: abilities live on their own entities, linked to
//! a caster via Caster(Entity); damage resolves on cooldown; resource cost
//! gates firing; priority selects among multiple abilities per caster.

mod common;

use std::time::Duration;

use bevy::ecs::entity::Entity;
use bevy::ecs::world::World;
use vaern_combat::{
    AbilityCooldown, AbilityPriority, AbilitySpec, Caster, Casting, Health, ResourcePool, Target,
};
use vaern_core::DAMAGE_TYPE_COUNT;
use vaern_stats::CombinedStats;

fn hasted_stats(haste_pct: f32) -> CombinedStats {
    CombinedStats {
        hp_max: 100,
        mana_max: 50,
        melee_mult: 1.0,
        spell_mult: 1.0,
        total_crit_pct: 0.0,
        total_dodge_pct: 0.0,
        total_haste_pct: haste_pct,
        total_parry_pct: 0.0,
        carry_kg: 50.0,
        armor: 0,
        fortune_pct: 0.0,
        mp5: 0.0,
        weapon_min_dmg: 0.0,
        weapon_max_dmg: 0.0,
        block_chance_pct: 0.0,
        block_value: 0,
        resist_total: [0.0; DAMAGE_TYPE_COUNT],
        luck: 0,
        leech_pct: 0.0,
        move_speed_pct: 0.0,
        avoidance_pct: 0.0,
    }
}

fn spawn_attacker(world: &mut World, target: Entity) -> Entity {
    world.spawn((Health::full(100.0), Target(target))).id()
}

fn spawn_ability(world: &mut World, caster: Entity, spec: AbilitySpec, priority: Option<u8>) {
    let mut entity = world.spawn((spec, AbilityCooldown::ready(), Caster(caster)));
    if let Some(p) = priority {
        entity.insert(AbilityPriority(p));
    }
}

#[test]
fn attacker_kills_dummy() {
    let mut app = common::headless_app(Duration::from_millis(500));
    let dummy = app.world_mut().spawn(Health::full(30.0)).id();
    let attacker = spawn_attacker(app.world_mut(), dummy);
    spawn_ability(
        app.world_mut(),
        attacker,
        AbilitySpec {
            damage: 10.0,
            cooldown_secs: 0.5,
            cast_secs: 0.0,
            resource_cost: 0.0,
            school: "blade".into(),
            threat_multiplier: 1.0,
            ..AbilitySpec::default()
        },
        None,
    );

    for _ in 0..5 {
        app.update();
    }

    assert!(
        app.world().get_entity(dummy).is_err(),
        "dummy should have been despawned after reaching 0 HP"
    );
}

/// Attacker has resource for only 2 hits; must wait for regen before the third.
#[test]
fn resource_gate_delays_kill() {
    let mut app = common::headless_app(Duration::from_millis(500));
    let dummy = app.world_mut().spawn(Health::full(30.0)).id();
    let attacker = spawn_attacker(app.world_mut(), dummy);
    app.world_mut().entity_mut(attacker).insert(ResourcePool {
        current: 20.0,
        max: 20.0,
        regen_per_sec: 2.0,
    });
    spawn_ability(
        app.world_mut(),
        attacker,
        AbilitySpec {
            damage: 10.0,
            cooldown_secs: 0.5,
            cast_secs: 0.0,
            resource_cost: 10.0,
            school: "blade".into(),
            threat_multiplier: 1.0,
            ..AbilitySpec::default()
        },
        None,
    );

    // After 3 ticks, only 2 casts have fired (pool exhausted after cast 2).
    for _ in 0..3 {
        app.update();
    }
    let hp_after_3 = app
        .world()
        .get::<Health>(dummy)
        .expect("dummy alive")
        .current;
    assert_eq!(hp_after_3, 10.0, "only 2 casts should have landed");

    // Keep running until dummy dies.
    for _ in 0..30 {
        app.update();
    }
    assert!(
        app.world().get_entity(dummy).is_err(),
        "dummy should eventually die once resource regenerates"
    );
}

/// Two abilities on one caster; the higher-priority one wins when both ready.
#[test]
fn priority_selects_highest() {
    let mut app = common::headless_app(Duration::from_secs(1));
    let dummy = app.world_mut().spawn(Health::full(1000.0)).id();
    let attacker = spawn_attacker(app.world_mut(), dummy);

    spawn_ability(
        app.world_mut(),
        attacker,
        AbilitySpec {
            damage: 1.0,
            cooldown_secs: 1.0,
            cast_secs: 0.0,
            resource_cost: 0.0,
            school: "blade".into(),
            threat_multiplier: 1.0,
            ..AbilitySpec::default()
        },
        Some(1),
    );
    spawn_ability(
        app.world_mut(),
        attacker,
        AbilitySpec {
            damage: 10.0,
            cooldown_secs: 1.0,
            cast_secs: 0.0,
            resource_cost: 0.0,
            school: "fire".into(),
            threat_multiplier: 1.0,
            ..AbilitySpec::default()
        },
        Some(9),
    );

    // Two updates: high-prio fires each tick (1s step matches 1s cooldown).
    // Low-prio should never fire.
    for _ in 0..2 {
        app.update();
    }
    let hp = app.world().get::<Health>(dummy).expect("alive").current;
    assert_eq!(hp, 1000.0 - 2.0 * 10.0, "high-priority should fire both ticks");
}

/// Two abilities, identical cooldowns; only one cast lands per caster per tick.
#[test]
fn one_cast_per_tick_per_caster() {
    let mut app = common::headless_app(Duration::from_secs(1));
    let dummy = app.world_mut().spawn(Health::full(1000.0)).id();
    let attacker = spawn_attacker(app.world_mut(), dummy);

    spawn_ability(
        app.world_mut(),
        attacker,
        AbilitySpec {
            damage: 5.0,
            cooldown_secs: 1.0,
            cast_secs: 0.0,
            resource_cost: 0.0,
            school: "blade".into(),
            threat_multiplier: 1.0,
            ..AbilitySpec::default()
        },
        Some(1),
    );
    spawn_ability(
        app.world_mut(),
        attacker,
        AbilitySpec {
            damage: 7.0,
            cooldown_secs: 1.0,
            cast_secs: 0.0,
            resource_cost: 0.0,
            school: "fire".into(),
            threat_multiplier: 1.0,
            ..AbilitySpec::default()
        },
        Some(9),
    );

    for _ in 0..4 {
        app.update();
    }
    // Only high-priority (7 dmg) should have fired across the 4 ticks; low-priority
    // remains always-ready but is never picked because it loses every selection.
    let hp = app.world().get::<Health>(dummy).expect("alive").current;
    assert_eq!(hp, 1000.0 - 4.0 * 7.0);
}

/// Cast-time ability: damage should land only after cast_secs elapses.
#[test]
fn cast_time_delays_damage() {
    let mut app = common::headless_app(Duration::from_millis(500));
    let dummy = app.world_mut().spawn(Health::full(100.0)).id();
    let attacker = spawn_attacker(app.world_mut(), dummy);
    spawn_ability(
        app.world_mut(),
        attacker,
        AbilitySpec {
            damage: 25.0,
            cooldown_secs: 5.0,
            cast_secs: 1.0,
            resource_cost: 0.0,
            school: "fire".into(),
            threat_multiplier: 1.0,
            ..AbilitySpec::default()
        },
        None,
    );

    // Tick 1 (dt=0.5s): cast starts (Casting inserted via Commands).
    // Inserted component won't be visible this frame, but remaining=1.0 set.
    app.update();
    assert_eq!(
        app.world().get::<Health>(dummy).expect("alive").current,
        100.0,
        "cast just started, no damage yet"
    );

    // Tick 2 (dt=0.5s): progress_casts sees Casting, ticks to 0.5; still casting.
    app.update();
    assert_eq!(
        app.world().get::<Health>(dummy).expect("alive").current,
        100.0,
        "cast still in progress"
    );
    assert!(
        app.world().get::<Casting>(attacker).is_some(),
        "Casting component present mid-cast"
    );

    // Tick 3 (dt=0.5s): remaining hits 0, damage resolves, Casting removed.
    app.update();
    assert_eq!(
        app.world().get::<Health>(dummy).expect("alive").current,
        75.0,
        "cast completed — 25 damage landed"
    );
    assert!(
        app.world().get::<Casting>(attacker).is_none(),
        "Casting component removed after resolve"
    );
}

/// Instant ability (cast_secs=0) should not insert a Casting component.
#[test]
fn instant_cast_no_casting_component() {
    let mut app = common::headless_app(Duration::from_millis(500));
    let dummy = app.world_mut().spawn(Health::full(100.0)).id();
    let attacker = spawn_attacker(app.world_mut(), dummy);
    spawn_ability(
        app.world_mut(),
        attacker,
        AbilitySpec {
            damage: 5.0,
            cooldown_secs: 10.0,
            cast_secs: 0.0,
            resource_cost: 0.0,
            school: "blade".into(),
            threat_multiplier: 1.0,
            ..AbilitySpec::default()
        },
        None,
    );

    app.update();
    assert_eq!(
        app.world().get::<Health>(dummy).expect("alive").current,
        95.0,
        "instant cast applies damage same tick"
    );
    assert!(
        app.world().get::<Casting>(attacker).is_none(),
        "instant casts do not attach Casting"
    );
}

/// 100% haste halves cast time: a 1.0s cast that normally takes 3 ticks
/// @500ms resolves in 2 ticks when the caster carries +100% haste. Pairs
/// with `cast_time_delays_damage` above — identical setup minus the
/// `CombinedStats` component.
#[test]
fn haste_shrinks_cast_time() {
    let mut app = common::headless_app(Duration::from_millis(500));
    let dummy = app.world_mut().spawn(Health::full(100.0)).id();
    let attacker = spawn_attacker(app.world_mut(), dummy);
    app.world_mut()
        .entity_mut(attacker)
        .insert(hasted_stats(100.0));
    spawn_ability(
        app.world_mut(),
        attacker,
        AbilitySpec {
            damage: 25.0,
            cooldown_secs: 5.0,
            cast_secs: 1.0,
            resource_cost: 0.0,
            school: "fire".into(),
            threat_multiplier: 1.0,
            ..AbilitySpec::default()
        },
        None,
    );

    // Tick 1: cast starts with scaled remaining = 0.5s.
    app.update();
    assert_eq!(
        app.world().get::<Health>(dummy).expect("alive").current,
        100.0,
        "cast just started, no damage yet"
    );

    // Tick 2: progress_casts ticks by 0.5s → hits 0, resolves.
    app.update();
    assert_eq!(
        app.world().get::<Health>(dummy).expect("alive").current,
        75.0,
        "hasted cast resolved one tick earlier than base"
    );
    assert!(
        app.world().get::<Casting>(attacker).is_none(),
        "Casting removed after resolve"
    );
}

/// 100% haste halves cooldown: a 1.0s-cd instant ability that normally
/// fires every 3 ticks @500ms fires every 2 ticks when hasted.
#[test]
fn haste_shrinks_cooldown() {
    let mut app = common::headless_app(Duration::from_millis(500));
    let dummy = app.world_mut().spawn(Health::full(1000.0)).id();
    let attacker = spawn_attacker(app.world_mut(), dummy);
    app.world_mut()
        .entity_mut(attacker)
        .insert(hasted_stats(100.0));
    spawn_ability(
        app.world_mut(),
        attacker,
        AbilitySpec {
            damage: 10.0,
            cooldown_secs: 1.0,
            cast_secs: 0.0,
            resource_cost: 0.0,
            school: "blade".into(),
            threat_multiplier: 1.0,
            ..AbilitySpec::default()
        },
        None,
    );

    // 4 ticks × 500ms = 2.0s. Base 1.0s cd ⇒ 2 casts (t=0, t=1.0).
    // Hasted to 0.5s cd ⇒ 4 casts (t=0, 0.5, 1.0, 1.5).
    for _ in 0..4 {
        app.update();
    }
    let hp = app.world().get::<Health>(dummy).expect("alive").current;
    assert_eq!(hp, 1000.0 - 4.0 * 10.0, "hasted caster fired 4× in 2.0s");
}
