use std::path::PathBuf;
use std::time::Duration;

use bevy::app::{AppExit, ScheduleRunnerPlugin};
use bevy::prelude::*;
use vaern_combat::{
    AbilityCooldown, AbilitySpec, Caster, CastEvent, CombatPlugin, DeathEvent, Health,
    ResourcePool, Schools, Target,
};

/// Fixed-tick rate for the deterministic combat sim used for PPO training.
/// 60 Hz matches the canonical client fixed update; revisit if the PPO loop
/// benefits from a coarser budget.
const TICK_HZ: f64 = 60.0;

#[derive(Resource)]
struct Dummy(Entity);

fn main() {
    let tick = Duration::from_secs_f64(1.0 / TICK_HZ);
    let schools = load_schools_from_disk().expect("load schools");

    App::new()
        .add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(tick)))
        .add_plugins(CombatPlugin)
        .insert_resource(schools)
        .add_systems(Startup, spawn_fight)
        .add_systems(Update, (log_casts, exit_on_dummy_death))
        .run();
}

fn load_schools_from_disk() -> Result<Schools, vaern_data::LoadError> {
    let root = project_root().join("src/generated/schools");
    let loaded = vaern_data::load_schools(root)?;
    Ok(Schools(vaern_data::into_index(loaded)))
}

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn spawn_fight(mut commands: Commands, schools: Res<Schools>) {
    let dummy = commands.spawn(Health::full(100.0)).id();
    commands.insert_resource(Dummy(dummy));

    let attacker = commands
        .spawn((
            Health::full(100.0),
            ResourcePool::full(50.0, 10.0),
            Target(dummy),
        ))
        .id();

    commands.spawn((
        AbilitySpec {
            damage: 10.0,
            cooldown_secs: 0.5,
            cast_secs: 0.0,
            resource_cost: 15.0,
            school: "fire".into(),
            threat_multiplier: 1.0,
            ..AbilitySpec::default()
        },
        AbilityCooldown::ready(),
        Caster(attacker),
    ));

    let fire = schools.get("fire").expect("fire school loaded");
    println!(
        "sim: fire ability — pillar={:?} morality={:?} damage_type={:?}",
        fire.pillar, fire.morality, fire.damage_type
    );
    println!("sim: attacker (50 pool, 10/s regen) vs dummy (100 HP)");
}

fn log_casts(mut casts: MessageReader<CastEvent>, schools: Res<Schools>) {
    for ev in casts.read() {
        let dmg_type = schools
            .get(&ev.school)
            .and_then(|s| s.damage_type.as_deref())
            .unwrap_or("?");
        println!(
            "sim: cast {} → dummy ({} dmg, {})",
            ev.school, ev.damage, dmg_type
        );
    }
}

fn exit_on_dummy_death(
    dummy: Res<Dummy>,
    mut deaths: MessageReader<DeathEvent>,
    mut exit: MessageWriter<AppExit>,
) {
    for ev in deaths.read() {
        if ev.entity == dummy.0 {
            println!("sim: dummy died — exiting");
            exit.write(AppExit::Success);
        }
    }
}
