//! Player movement: apply buffered `Inputs` to the player's `Transform`
//! deterministically, tick-scaled so the client's prediction matches.

use bevy::prelude::*;
use lightyear::input::native::prelude::ActionState;
use vaern_combat::StatusEffects;
use vaern_core::terrain;
use vaern_protocol::{Inputs, MOVE_PER_TICK, PlayerTag, input_to_direction};
use vaern_voxel::chunk::ChunkStore;
use vaern_voxel::query::ground_y;

/// How far above the player's current Y to start the voxel descent
/// probe. Generous so any reasonable edit (craters, towers, cliffs)
/// still finds the top of the world.
const GROUND_PROBE_TOP: f32 = 64.0;
/// How far down to descend before giving up and falling back to the
/// analytical heightmap. Covers terrain amplitude + a deep carved pit.
const GROUND_PROBE_MAX_DESCENT: f32 = 96.0;

/// Resolve the authoritative ground Y at world (x, z). Queries the
/// voxel store first — catches any edits the server has applied —
/// and falls back to the analytical heightmap for chunks that haven't
/// been seeded yet (player just teleported into a zone without cached
/// chunks, or similar).
#[inline]
fn resolve_ground_y(store: &ChunkStore, x: f32, z: f32, current_y: f32) -> f32 {
    let top = current_y + GROUND_PROBE_TOP;
    ground_y(store, x, z, top, GROUND_PROBE_MAX_DESCENT)
        .unwrap_or_else(|| terrain::height(x, z))
}

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
    store: Res<ChunkStore>,
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
        // Snap Y to the authoritative voxel ground, falling back to
        // the analytical heightmap for chunks not yet seeded on the
        // server. Pre-edit the two agree by construction (same
        // `HeightfieldGenerator`); post-edit the voxel query reflects
        // craters + carvings the server has applied.
        tf.translation.y = resolve_ground_y(
            &store,
            tf.translation.x,
            tf.translation.z,
            tf.translation.y,
        );
    }
}
