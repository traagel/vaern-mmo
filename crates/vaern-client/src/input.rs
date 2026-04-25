//! Keyboard input: WASD into lightyear's ActionState (shipped to server and
//! also used for local-predicted movement), auto-target nearest NPC,
//! 1..=6 hotkeys → `CastIntent` messages.

use bevy::prelude::*;
use lightyear::input::native::prelude::{ActionState, InputMarker};
use lightyear::prelude::client::input::InputSystems;
use lightyear::prelude::*;
use vaern_combat::{Casting, NpcKind, Target};
use vaern_core::terrain;
use vaern_protocol::{
    CastIntent, Channel1, Inputs, MOVE_PER_TICK, StanceRequest, WasdInput, input_to_direction,
};
use vaern_voxel::chunk::ChunkStore;
use vaern_voxel::query::ground_y;

use crate::hotbar_ui::CastAttempted;
use crate::menu::AppState;
use crate::scene::CameraController;
use crate::shared::{Npc, Player};

pub struct ClientInputPlugin;

impl Plugin for ClientInputPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedPreUpdate,
            buffer_wasd_input
                .in_set(InputSystems::WriteClientInputs)
                .run_if(in_state(AppState::InGame)),
        )
        .add_systems(
            FixedUpdate,
            predicted_player_movement.run_if(in_state(AppState::InGame)),
        )
        .add_systems(
            Update,
            (
                // Target-acquire snap must run AFTER mouse-look so that any
                // mouse delta in the same frame is applied first, then the
                // snap overrides yaw to face the new target. Otherwise the
                // snap can be wiped by a late-frame mouse delta.
                update_player_target.after(crate::scene::CameraSet::Input),
                handle_ability_input,
                handle_stance_input,
            )
                .run_if(in_state(AppState::InGame)),
        );
    }
}

/// Read WASD into the InputMarker entity's ActionState. Lightyear ships this
/// up to the server every tick and replays it during rollback.
fn buffer_wasd_input(
    keys: Res<ButtonInput<KeyCode>>,
    controller: Res<CameraController>,
    chat_focused: Res<crate::chat_ui::ChatInputFocused>,
    mut q: Query<&mut ActionState<Inputs>, With<InputMarker<Inputs>>>,
) {
    let Ok(mut state) = q.single_mut() else {
        return;
    };
    // Quantize yaw to milliradians so the input stays `Eq`-able (lightyear
    // rollback needs bitwise input equality) while still having ~0.057°
    // resolution — finer than any camera motion a player can perceive.
    let camera_yaw_mrad = (controller.yaw * 1000.0).round() as i32;
    // While the chat input has focus, suppress W/A/S/D so typing a
    // message doesn't walk the character. Camera yaw still streams
    // (mouse-look keeps working in the background — same model most
    // MMO clients use).
    let typing = chat_focused.0;
    let wasd = WasdInput {
        up: !typing && keys.pressed(KeyCode::KeyW),
        down: !typing && keys.pressed(KeyCode::KeyS),
        left: !typing && keys.pressed(KeyCode::KeyA),
        right: !typing && keys.pressed(KeyCode::KeyD),
        camera_yaw_mrad,
    };
    state.0 = Inputs::Move(wasd);
}

/// Mirror of the server's `apply_player_movement`, but only on the local
/// predicted entity. Identical math → minimal mispredict — including the
/// voxel-first Y-snap so predicted steps honor carved craters + server
/// edits instead of floating over them until the next correction.
fn predicted_player_movement(
    mut q: Query<(&mut Transform, &ActionState<Inputs>), With<Predicted>>,
    store: Res<ChunkStore>,
) {
    for (mut tf, state) in &mut q {
        let Inputs::Move(d) = &state.0;
        // Mouse-look: rotate to camera yaw every tick, independent of movement.
        let yaw = d.camera_yaw_mrad as f32 * 0.001;
        tf.rotation = Quat::from_rotation_y(yaw);

        let dir = input_to_direction(&state.0);
        if dir != Vec3::ZERO {
            tf.translation += dir * MOVE_PER_TICK;
        }
        // Voxel-first Y-snap — matches `vaern-server::movement` so the
        // predicted player stays on the authoritative surface (voxel
        // edits visible locally). Falls back to analytical heightmap
        // for chunks not yet streamed into the local store.
        let top = tf.translation.y + 64.0;
        tf.translation.y = ground_y(&store, tf.translation.x, tf.translation.z, top, 96.0)
            .unwrap_or_else(|| terrain::height(tf.translation.x, tf.translation.z));
    }
}

/// Peak idle turn speed (rad/s) while locked but not attacking. 2.5 rad/s
/// ≈ 143°/s — fast enough to not feel sluggish, slow enough to not be
/// twitchy. Actual speed ramps up via `TURN_ACCEL` and brakes before
/// reaching the target (kinematic brake plan).
const IDLE_TURN_RATE: f32 = 0.3;

/// Peak cast-triggered turn speed (rad/s). Roughly 687°/s — a 180° turn
/// takes ~0.26s at the velocity peak. Feels like a visible swoosh, not a
/// teleport.
const CAST_TURN_RATE: f32 = 12.0;

/// Angular acceleration (rad/s²). Determines how quickly the turn ramps
/// up and how abruptly it brakes near the target. 20 rad/s² gives a
/// smooth ease-in / ease-out without feeling mushy.
const TURN_ACCEL: f32 = 20.0;

/// Max horizontal distance (world units) from player at which Tab considers
/// an NPC targetable. Zone origins sit ~800u apart, so without this cap Tab
/// would reach across zones. 40u ≈ well outside ranged ability reach (30u)
/// — you can always acquire before you can fire.
const MAX_TARGET_RANGE: f32 = 40.0;

/// Half-angle (degrees) of the "in-front" cone used to rank Tab candidates.
/// NPCs within this cone of the camera's forward direction are preferred;
/// behind-back NPCs only appear if no front candidates exist.
const FRONT_CONE_HALF_ANGLE_DEG: f32 = 80.0;

/// Console / tab-target MMO feel. Tab cycles through nearby NPCs by
/// ascending distance; Escape clears the target. Nothing is auto-selected
/// — if the player never presses Tab they remain untargeted and swing in
/// whatever direction the camera faces.
///
/// While a target IS held, the player's yaw smooth-follows it (camera
/// and mesh rotation). On any cast (`CastAttempted`) the yaw snaps fully
/// toward the target so the swing lands even if the follow hasn't caught
/// up. Mouse yaw input is suppressed in `mouse_look_camera_input` while
/// a target exists (pitch still mouse-driven).
///
/// Stale Target components (NPC despawned) resolve to "no target" this
/// frame and Tab re-picks from whatever's live.
fn update_player_target(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    chat_focused: Res<crate::chat_ui::ChatInputFocused>,
    mut attempts: MessageReader<CastAttempted>,
    mut player: Query<(Entity, &mut Transform, Option<&Target>), With<Player>>,
    npcs: Query<(Entity, &Transform, Option<&NpcKind>), (With<Npc>, Without<Player>)>,
    mut controller: ResMut<CameraController>,
    mut commands: Commands,
    // Persistent angular velocity across frames — lets the motion controller
    // ease-in/out naturally. Reset to 0 whenever there's no target.
    mut angular_vel: Local<f32>,
) {
    let Ok((player_e, mut player_tf, current)) = player.single_mut() else {
        attempts.clear();
        *angular_vel = 0.0;
        return;
    };
    let player_pos = player_tf.translation;

    // ---- Target selection: Tab to cycle, Escape to clear ----
    // Suppressed while chat input is focused so typing a message with
    // Esc to cancel or Tab to switch fields doesn't swap combat targets.
    let typing = chat_focused.0;
    if !typing && keys.just_pressed(KeyCode::Escape) {
        if let Ok(mut ec) = commands.get_entity(player_e) {
            ec.remove::<Target>();
        }
    } else if !typing && keys.just_pressed(KeyCode::Tab) {
        // Camera forward (XZ): at yaw=0 the camera looks -Z. Matches the
        // target_yaw = atan2(-d.x, -d.z) convention used by the follow math
        // below — a target directly ahead has dot(forward) == 1.
        let forward = Vec3::new(-controller.yaw.sin(), 0.0, -controller.yaw.cos());
        let max_range_sq = MAX_TARGET_RANGE * MAX_TARGET_RANGE;
        let cos_cone = FRONT_CONE_HALF_ANGLE_DEG.to_radians().cos();

        // Build (entity, distance, is_in_front) for every live combat NPC
        // within range. Quest-givers are filtered out — they share the `Npc`
        // marker with enemies but shouldn't be combat targets.
        let mut candidates: Vec<(Entity, f32, bool)> = npcs
            .iter()
            .filter(|(_, _, kind)| {
                !matches!(kind, Some(NpcKind::QuestGiver) | Some(NpcKind::Vendor))
            })
            .filter_map(|(e, tf, _)| {
                let mut d = tf.translation - player_pos;
                d.y = 0.0;
                let d2 = d.length_squared();
                if d2 > max_range_sq || d2 < 1e-6 {
                    return None;
                }
                let dist = d2.sqrt();
                let in_front = (d / dist).dot(forward) >= cos_cone;
                Some((e, dist, in_front))
            })
            .collect();

        // If anything is in the front cone, restrict to those — otherwise
        // fall back to everything in range so Tab still works when you turn
        // your back on the pack mid-fight.
        if candidates.iter().any(|(_, _, in_front)| *in_front) {
            candidates.retain(|(_, _, in_front)| *in_front);
        }
        candidates.sort_by(|a, b| a.1.total_cmp(&b.1));

        if let Some(&(first, _, _)) = candidates.first() {
            let next = match current {
                Some(t) => candidates
                    .iter()
                    .position(|(e, _, _)| *e == t.0)
                    .map(|i| candidates[(i + 1) % candidates.len()].0)
                    .unwrap_or(first),
                None => first,
            };
            if let Ok(mut ec) = commands.get_entity(player_e) {
                ec.insert(Target(next));
            }
        }
    }

    // ---- Follow math: only runs while a live Target exists. Reads the
    // npcs query for the target's current position; if the target was
    // despawned, we clear the component and bail so the player stops
    // facing a ghost.
    let target_pos = current.and_then(|t| {
        npcs.iter()
            .find_map(|(e, tf, _)| (e == t.0).then_some(tf.translation))
    });

    let Some(target_pos) = target_pos else {
        // Stale target → remove so input + combat don't try to use it.
        if current.is_some() {
            if let Ok(mut ec) = commands.get_entity(player_e) {
                ec.remove::<Target>();
            }
        }
        attempts.clear();
        *angular_vel = 0.0;
        return;
    };

    let mut d = target_pos - player_pos;
    d.y = 0.0;
    if d.length_squared() < 1e-4 {
        attempts.clear();
        *angular_vel = 0.0;
        return;
    }
    let target_yaw = (-d.x).atan2(-d.z);

    // Shortest-arc delta from current yaw to target, wrapped into [-π, π]
    // so we always turn the short way.
    let mut diff = target_yaw - controller.yaw;
    while diff > std::f32::consts::PI {
        diff -= std::f32::consts::TAU;
    }
    while diff < -std::f32::consts::PI {
        diff += std::f32::consts::TAU;
    }

    let cast_this_frame = attempts.read().next().is_some();
    let dt = time.delta_secs().max(0.0);
    let abs_diff = diff.abs();

    // Brake-plan peak velocity: the speed at which, starting now and
    // decelerating at TURN_ACCEL, we'd stop exactly at the target
    // (v² = 2·a·d). Capping desired velocity by this value prevents
    // overshoot without needing overshoot-correction logic.
    let brake_peak = (2.0 * TURN_ACCEL * abs_diff).sqrt();

    if cast_this_frame {
        // Cast kicks the velocity to the fastest brake-plan value that
        // still lets us stop at target — capped by CAST_TURN_RATE so big
        // turns still take ~a quarter second rather than happening
        // instantly. After the kick, normal brake logic takes over in
        // subsequent frames.
        *angular_vel = diff.signum() * brake_peak.min(CAST_TURN_RATE);
    } else {
        // Idle follow: accelerate toward the brake-plan-capped idle speed.
        let desired = diff.signum() * brake_peak.min(IDLE_TURN_RATE);
        let dv = (desired - *angular_vel).clamp(-TURN_ACCEL * dt, TURN_ACCEL * dt);
        *angular_vel += dv;
    }

    // Apply velocity, clamped so we never step past the target (would
    // cause the next frame to swing the other way).
    let mut applied = *angular_vel * dt;
    if applied.abs() > abs_diff {
        applied = diff;
        *angular_vel = 0.0;
    }
    controller.yaw += applied;
    player_tf.rotation = Quat::from_rotation_y(controller.yaw);
}

/// Translate Q (block hold) and E (parry tap) into `StanceRequest`
/// messages. Block uses edge-detection — `SetBlock(true)` on press,
/// `SetBlock(false)` on release — so the server doesn't need to guess
/// whether the key is still held. Parry is a one-shot tap.
fn handle_stance_input(
    keys: Res<ButtonInput<KeyCode>>,
    chat_focused: Res<crate::chat_ui::ChatInputFocused>,
    mut sender: Query<&mut MessageSender<StanceRequest>, With<Client>>,
) {
    let Ok(mut sender) = sender.single_mut() else { return };
    // Don't trigger block / parry while the user is typing in chat.
    // Also cleanly release a held block if chat grabs focus mid-hold
    // (handled by just_released never firing — the server auto-drops
    // block on disconnect/timeout anyway).
    if chat_focused.0 {
        return;
    }
    if keys.just_pressed(KeyCode::KeyQ) {
        let _ = sender.send::<Channel1>(StanceRequest::SetBlock(true));
    }
    if keys.just_released(KeyCode::KeyQ) {
        let _ = sender.send::<Channel1>(StanceRequest::SetBlock(false));
    }
    if keys.just_pressed(KeyCode::KeyE) {
        let _ = sender.send::<Channel1>(StanceRequest::ParryTap);
    }
}

fn handle_ability_input(
    mut attempts: MessageReader<CastAttempted>,
    players: Query<(&Target, Option<&Casting>), With<Player>>,
    mut sender: Query<&mut MessageSender<CastIntent>, With<Client>>,
) {
    let Ok((target, casting)) = players.single() else {
        attempts.clear();
        return;
    };
    if casting.is_some() {
        attempts.clear();
        return;
    }
    let Ok(mut sender) = sender.single_mut() else {
        attempts.clear();
        return;
    };
    for attempt in attempts.read() {
        let _ = sender.send::<Channel1>(CastIntent {
            slot: attempt.slot_idx,
            target: target.0,
        });
    }
}
