//! Player movement: apply buffered `Inputs` to the player's `Transform`
//! deterministically, tick-scaled so the client's prediction matches.

use bevy::prelude::*;
use lightyear::input::native::prelude::ActionState;
use vaern_combat::StatusEffects;
use vaern_core::terrain;
use vaern_protocol::{Inputs, MOVE_PER_TICK, PlayerTag, input_to_direction};

/// Apply buffered inputs to each player's Transform. Deterministic tick-scaled
/// step so the client's prediction matches exactly.
///
/// Third-person mouse-look: the player always faces where the camera points
/// (rotation mirrors `WasdInput.camera_yaw_mrad` every tick). Movement keys
/// are camera-relative — W = forward toward the camera look direction,
/// A/D = strafe. Separating yaw from move-direction lets you run sideways
/// while still facing forward, which is the expected ARPG feel.
///
/// Active Slow effects scale the per-tick step by their strongest
/// multiplier (e.g. `chilled` at 0.6 → 60% movement speed). See
/// `StatusEffects::move_speed_mult`.
pub fn apply_player_movement(
    mut players: Query<
        (&ActionState<Inputs>, &mut Transform, Option<&StatusEffects>),
        With<PlayerTag>,
    >,
) {
    for (state, mut tf, effects) in &mut players {
        let Inputs::Move(d) = &state.0;
        let yaw = d.camera_yaw_mrad as f32 * 0.001;
        tf.rotation = Quat::from_rotation_y(yaw);

        let dir = input_to_direction(&state.0);
        if dir != Vec3::ZERO {
            let slow = effects.map_or(1.0, |e| e.move_speed_mult());
            tf.translation += dir * (MOVE_PER_TICK * slow);
        }
        // Snap to terrain height every tick — cheap (two sin/cos),
        // keeps Y server-authoritative and matches the client mesh.
        tf.translation.y = terrain::height(tf.translation.x, tf.translation.z);
    }
}
