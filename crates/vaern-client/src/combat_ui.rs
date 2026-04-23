//! Bevy-native HUD bits that the unit frame doesn't own: cast bar (bottom
//! center) + target frame (top center). HP + resource are rendered by
//! `unit_frame` from `PlayerStateSnapshot` data — this module no longer
//! touches own-player combat state.

use bevy::prelude::*;
use vaern_combat::{DisplayName, Health, NpcKind, Target};

use crate::hotbar_ui::{CastAttempted, Hotbar};
use crate::menu::AppState;
use crate::shared::school_color;
use crate::unit_frame::OwnPlayerState;

pub struct CombatUiPlugin;

impl Plugin for CombatUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SwingFlash>()
            .add_systems(OnEnter(AppState::InGame), setup_ui)
            .add_systems(
                Update,
                (
                    start_swing_flashes,
                    tick_swing_flash,
                    update_cast_bar,
                    update_target_frame,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

/// Client-only "swing flash": a brief 0.35s mini cast bar shown when the
/// player fires an instant ability so melee keypresses get the same visual
/// rhythm as spells. Server cast state (from `OwnPlayerState`) always wins
/// if both are active at once.
#[derive(Resource, Debug, Default, Clone)]
struct SwingFlash {
    remaining: f32,
    total: f32,
    name: String,
    school: String,
}

const SWING_FLASH_SECS: f32 = 0.35;

fn start_swing_flashes(
    mut attempts: MessageReader<CastAttempted>,
    hotbar: Res<Hotbar>,
    mut swing: ResMut<SwingFlash>,
) {
    for attempt in attempts.read() {
        let Some(slot) = hotbar.slots.get(attempt.slot_idx as usize) else { continue };
        // Only instant abilities get the swing flash; channeled casts already
        // show the real cast bar from `OwnPlayerState`.
        if slot.info.cast_secs > 0.0 {
            continue;
        }
        swing.remaining = SWING_FLASH_SECS;
        swing.total = SWING_FLASH_SECS;
        swing.name = slot.info.name.clone();
        swing.school = slot.info.school.clone();
    }
}

fn tick_swing_flash(time: Res<Time>, mut swing: ResMut<SwingFlash>) {
    if swing.remaining > 0.0 {
        swing.remaining = (swing.remaining - time.delta_secs()).max(0.0);
    }
}

// ─── markers ───────────────────────────────────────────────────────────────

#[derive(Component)]
struct CastBar;
#[derive(Component)]
struct CastBarFill;
#[derive(Component)]
struct CastBarText;
#[derive(Component)]
struct TargetFrame;
#[derive(Component)]
struct TargetHpFill;
#[derive(Component)]
struct TargetHpText;

// ─── layout ────────────────────────────────────────────────────────────────

fn setup_ui(mut commands: Commands) {
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|root| {
            // Top-center: target frame (hidden initially)
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(24.0),
                    left: Val::Percent(50.0),
                    margin: UiRect {
                        left: Val::Px(-160.0),
                        ..default()
                    },
                    width: Val::Px(320.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(4.0),
                    ..default()
                },
                Visibility::Hidden,
                TargetFrame,
            ))
            .with_children(|col| {
                bar(col, Color::srgb(0.75, 0.25, 0.25), TargetHpFill, TargetHpText);
            });

            // Bottom-center: cast bar (hidden unless casting). Currently a
            // stub — own-player `Casting` used to come from the Replicated
            // copy, but that pattern no longer works (see unit_frame.rs).
            // A future `PlayerStateSnapshot` field can drive this when it
            // matters again.
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    bottom: Val::Px(140.0),
                    left: Val::Percent(50.0),
                    margin: UiRect {
                        left: Val::Px(-160.0),
                        ..default()
                    },
                    width: Val::Px(320.0),
                    height: Val::Px(20.0),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BorderColor::all(Color::srgb(0.8, 0.8, 0.8)),
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
                Visibility::Hidden,
                CastBar,
            ))
            .with_children(|bar_frame| {
                bar_frame.spawn((
                    Node {
                        width: Val::Percent(0.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.95, 0.80, 0.30)),
                    CastBarFill,
                ));
                bar_frame.spawn((
                    Text::new("casting..."),
                    TextFont {
                        font_size: 14.0,
                        ..default()
                    },
                    TextColor(Color::WHITE),
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(8.0),
                        top: Val::Px(2.0),
                        ..default()
                    },
                    CastBarText,
                ));
            });
        });
}

fn bar<F: Component, T: Component>(
    parent: &mut ChildSpawnerCommands<'_>,
    fill_color: Color,
    fill_marker: F,
    text_marker: T,
) {
    parent
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Px(22.0),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BorderColor::all(Color::srgb(0.3, 0.3, 0.3)),
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
        ))
        .with_children(|frame| {
            frame.spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(fill_color),
                fill_marker,
            ));
            frame.spawn((
                Text::new(""),
                TextFont {
                    font_size: 13.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(8.0),
                    top: Val::Px(3.0),
                    ..default()
                },
                text_marker,
            ));
        });
}

// ─── updates ───────────────────────────────────────────────────────────────

/// Reads the cast snapshot from `OwnPlayerState` (populated by
/// `PlayerStateSnapshot` messages). Shows progress bottom-center while
/// `is_casting` is true; hidden otherwise. Fill color tracks the school.
fn update_cast_bar(
    state: Res<OwnPlayerState>,
    mut bar_vis: Query<(&mut Visibility, &mut BackgroundColor), With<CastBar>>,
    mut fill: Query<(&mut Node, &mut BackgroundColor), (With<CastBarFill>, Without<CastBar>)>,
    mut text: Query<&mut Text, With<CastBarText>>,
) {
    let Ok((mut vis, _)) = bar_vis.single_mut() else { return };
    if !state.snap.is_casting || state.snap.cast_total <= 0.0 {
        *vis = Visibility::Hidden;
        return;
    }
    *vis = Visibility::Visible;
    let progress = 1.0 - (state.snap.cast_remaining / state.snap.cast_total).clamp(0.0, 1.0);
    if let Ok((mut node, mut bg)) = fill.single_mut() {
        node.width = Val::Percent(progress * 100.0);
        *bg = BackgroundColor(school_color(&state.snap.cast_school));
    }
    if let Ok(mut t) = text.single_mut() {
        let label = if state.snap.cast_ability_name.is_empty() {
            state.snap.cast_school.clone()
        } else {
            state.snap.cast_ability_name.clone()
        };
        **t = format!("{label} · {:.1}s", state.snap.cast_remaining);
    }
}

fn update_target_frame(
    // Target is client-local (added by `update_player_target` in input.rs,
    // not replicated), so it lives on the Predicted copy tagged `Player`.
    player_target: Query<&Target, With<crate::shared::Player>>,
    targets: Query<(&Health, Option<&DisplayName>, Option<&NpcKind>)>,
    mut frame_vis: Query<&mut Visibility, With<TargetFrame>>,
    mut fill: Query<&mut Node, With<TargetHpFill>>,
    mut text: Query<&mut Text, With<TargetHpText>>,
) {
    let Ok(mut vis) = frame_vis.single_mut() else { return };
    let info = player_target
        .single()
        .ok()
        .and_then(|t| targets.get(t.0).ok());
    match info {
        Some((hp, display, kind)) => {
            *vis = Visibility::Visible;
            let pct = (hp.current / hp.max).clamp(0.0, 1.0) * 100.0;
            if let Ok(mut node) = fill.single_mut() {
                node.width = Val::Percent(pct);
            }
            if let Ok(mut t) = text.single_mut() {
                let name = display.map(|d| d.0.as_str()).unwrap_or("Target");
                let prefix = match kind {
                    Some(NpcKind::Named) => "★ ",
                    Some(NpcKind::Elite) => "⬥ ",
                    _ => "",
                };
                **t = format!("{prefix}{name}  ·  {:.0} / {:.0}", hp.current, hp.max);
            }
        }
        None => *vis = Visibility::Hidden,
    }
}
