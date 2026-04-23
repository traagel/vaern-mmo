//! Shared test harness. Bevy's `Time<Virtual>` clamps delta to 250ms by
//! default; headless combat tests with larger fixed steps need it widened.

use std::time::Duration;

use bevy::app::App;
use bevy::time::{Time, TimePlugin, TimeUpdateStrategy, Virtual};

use vaern_combat::CombatPlugin;

pub fn headless_app(step: Duration) -> App {
    let mut app = App::new();
    app.add_plugins(TimePlugin)
        .add_plugins(CombatPlugin)
        .insert_resource(TimeUpdateStrategy::ManualDuration(step));

    app.world_mut()
        .resource_mut::<Time<Virtual>>()
        .set_max_delta(Duration::from_secs(3600));

    app
}
