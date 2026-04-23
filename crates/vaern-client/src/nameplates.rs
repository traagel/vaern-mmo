//! World-space nameplates + rising damage numbers for every living entity.
//! Positioned each frame by projecting world pos → screen via the main camera.

use std::collections::HashMap;

use bevy::prelude::*;
use vaern_combat::{AnimState, DeathEvent, DisplayName, Health, NpcKind};
use vaern_protocol::PlayerTag;

use crate::menu::AppState;
use crate::menu::pillar_display;
use crate::scene::CastFiredLocal;
use crate::shared::MainCamera;

pub struct NameplatesPlugin;

impl Plugin for NameplatesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NameplateIndex>()
            .add_systems(OnExit(AppState::InGame), clear_nameplates)
            .add_systems(
                Update,
                (
                    spawn_nameplates,
                    update_nameplates,
                    tick_damage_numbers,
                    log_deaths,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

// ─── types ─────────────────────────────────────────────────────────────────

/// UI node that tracks a world entity. `hp_fill` is the inner bar whose width
/// we set from the target's HP %. `state_tag` is the state-badge text node
/// whose string we rewrite from the target's `AnimState` each frame.
#[derive(Component, Debug, Clone, Copy)]
struct Nameplate {
    target: Entity,
    hp_fill: Entity,
    state_tag: Entity,
}

/// Marker on the inner HP-bar node so `update_nameplates` can borrow it
/// mutably alongside the outer plate's Node.
#[derive(Component)]
struct HpFillTag;

/// Marker on the state-badge text node. Lets `update_nameplates` locate
/// and rewrite it without holding another query parameter.
#[derive(Component)]
struct StateTagMark;

/// `world_entity → nameplate_ui_entity`, so plates despawn when their
/// target goes away without leaking UI nodes.
#[derive(Resource, Default)]
pub struct NameplateIndex(pub HashMap<Entity, Entity>);

#[derive(Component, Debug, Clone, Copy)]
struct DamageNumber {
    target: Entity,
    remaining: f32,
    total: f32,
}

// ─── systems ───────────────────────────────────────────────────────────────

fn spawn_nameplates(
    with_health: Query<
        (
            Entity,
            Option<&PlayerTag>,
            Option<&DisplayName>,
            Option<&NpcKind>,
        ),
        With<Health>,
    >,
    mut index: ResMut<NameplateIndex>,
    mut commands: Commands,
) {
    for (entity, tag, display, kind) in &with_health {
        if index.0.contains_key(&entity) {
            continue;
        }
        let label = if let Some(t) = tag {
            pillar_display(t.core_pillar).to_string()
        } else if let Some(d) = display {
            d.0.clone()
        } else {
            "NPC".to_string()
        };
        let label_color = match kind {
            Some(NpcKind::QuestGiver) => Color::srgb(1.0, 0.85, 0.35), // gold
            Some(NpcKind::Named) => Color::srgb(1.0, 0.40, 0.85),      // pink
            Some(NpcKind::Elite) => Color::srgb(0.85, 0.55, 1.0),      // violet
            _ => Color::srgb(0.95, 0.95, 0.95),                        // default
        };

        let hp_fill = commands
            .spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.85, 0.25, 0.25)),
                HpFillTag,
            ))
            .id();
        let bar_frame = commands
            .spawn((
                Node {
                    width: Val::Px(100.0),
                    height: Val::Px(6.0),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BorderColor::all(Color::srgb(0.25, 0.25, 0.25)),
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
            ))
            .id();
        commands.entity(bar_frame).add_child(hp_fill);

        let label_node = commands
            .spawn((
                Text::new(label),
                TextFont {
                    font_size: 12.0,
                    ..default()
                },
                TextColor(label_color),
            ))
            .id();

        // State badge — faded grey "[idle]" / "[running]" / etc. that
        // `update_nameplates` rewrites from the target's AnimState
        // each frame. Placeholder "[…]" until the first replication
        // tick arrives.
        let state_tag = commands
            .spawn((
                Text::new("[…]"),
                TextFont {
                    font_size: 10.0,
                    ..default()
                },
                TextColor(Color::srgb(0.70, 0.70, 0.70)),
                StateTagMark,
            ))
            .id();

        // "!" quest-giver indicator above the name label. Spawned for every
        // nameplate but added to the plate only for QuestGiver kind.
        let quest_marker = if matches!(kind, Some(NpcKind::QuestGiver)) {
            Some(
                commands
                    .spawn((
                        Text::new("!"),
                        TextFont {
                            font_size: 22.0,
                            ..default()
                        },
                        TextColor(Color::srgb(1.0, 0.85, 0.15)),
                    ))
                    .id(),
            )
        } else {
            None
        };

        let plate = commands
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(110.0),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(2.0),
                    ..default()
                },
                Pickable::IGNORE,
                Visibility::Hidden,
                Nameplate { target: entity, hp_fill, state_tag },
            ))
            .id();
        if let Some(m) = quest_marker {
            commands
                .entity(plate)
                .add_children(&[m, label_node, bar_frame, state_tag]);
        } else {
            commands
                .entity(plate)
                .add_children(&[label_node, bar_frame, state_tag]);
        }

        index.0.insert(entity, plate);
    }

    // Despawn plates whose target is gone.
    let alive: std::collections::HashSet<Entity> =
        with_health.iter().map(|(e, _, _, _)| e).collect();
    index.0.retain(|target, plate| {
        if alive.contains(target) {
            true
        } else {
            if let Ok(mut ec) = commands.get_entity(*plate) {
                ec.despawn();
            }
            false
        }
    });
}

fn update_nameplates(
    cams: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    targets: Query<(&GlobalTransform, &Health, Option<&AnimState>)>,
    mut plates: Query<(&Nameplate, &mut Node, &mut Visibility), Without<HpFillTag>>,
    mut fills: Query<&mut Node, With<HpFillTag>>,
    mut state_tags: Query<&mut Text, With<StateTagMark>>,
) {
    let Ok((cam, cam_tf)) = cams.single() else {
        return;
    };
    for (plate, mut node, mut vis) in &mut plates {
        let Ok((target_tf, hp, state)) = targets.get(plate.target) else {
            *vis = Visibility::Hidden;
            continue;
        };
        let head = target_tf.translation() + Vec3::Y * 2.1;
        match cam.world_to_viewport(cam_tf, head) {
            Ok(screen) => {
                *vis = Visibility::Visible;
                node.left = Val::Px(screen.x - 55.0);
                node.top = Val::Px(screen.y - 28.0);
                if let Ok(mut fill) = fills.get_mut(plate.hp_fill) {
                    let pct = (hp.current / hp.max).clamp(0.0, 1.0) * 100.0;
                    fill.width = Val::Percent(pct);
                }
                if let Ok(mut text) = state_tags.get_mut(plate.state_tag) {
                    let label = state.map(|s| s.label()).unwrap_or("…");
                    let new = format!("[{label}]");
                    if text.0 != new {
                        text.0 = new;
                    }
                }
            }
            Err(_) => *vis = Visibility::Hidden,
        }
    }
}

/// Spawn new damage-number UI nodes on every CastFired, then tick existing
/// ones: rise, fade, expire.
fn tick_damage_numbers(
    time: Res<Time>,
    mut reader: MessageReader<CastFiredLocal>,
    targets: Query<&GlobalTransform>,
    cams: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut numbers: Query<(
        Entity,
        &mut DamageNumber,
        &mut Node,
        &mut Visibility,
        &mut TextColor,
    )>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();

    {
        for CastFiredLocal(ev) in reader.read() {
            if !targets.contains(ev.target) {
                continue;
            }
            commands.spawn((
                Text::new(format!("{:.0}", ev.damage)),
                TextFont {
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.92, 0.55)),
                Node {
                    position_type: PositionType::Absolute,
                    ..default()
                },
                Visibility::Hidden,
                Pickable::IGNORE,
                DamageNumber {
                    target: ev.target,
                    remaining: 0.85,
                    total: 0.85,
                },
            ));
        }
    }

    let Ok((cam, cam_tf)) = cams.single() else {
        return;
    };
    for (entity, mut num, mut node, mut vis, mut color) in &mut numbers {
        num.remaining -= dt;
        if num.remaining <= 0.0 {
            if let Ok(mut ec) = commands.get_entity(entity) {
                ec.despawn();
            }
            continue;
        }
        let Ok(target_tf) = targets.get(num.target) else {
            *vis = Visibility::Hidden;
            continue;
        };
        let progress = 1.0 - (num.remaining / num.total).clamp(0.0, 1.0);
        let world = target_tf.translation() + Vec3::Y * (2.3 + progress * 0.8);
        match cam.world_to_viewport(cam_tf, world) {
            Ok(screen) => {
                *vis = Visibility::Visible;
                node.left = Val::Px(screen.x - 12.0);
                node.top = Val::Px(screen.y - progress * 30.0);
                let alpha = (1.0 - progress).max(0.0);
                let c = color.0.to_linear();
                color.0 = Color::LinearRgba(LinearRgba::new(c.red, c.green, c.blue, alpha));
            }
            Err(_) => *vis = Visibility::Hidden,
        }
    }
}

fn log_deaths(mut deaths: MessageReader<DeathEvent>) {
    for ev in deaths.read() {
        info!("death: {:?}", ev.entity);
    }
}

/// On `OnExit(InGame)`: drop index entries (their UI nodes get despawned by
/// the scene teardown's generic UI-roots sweep).
fn clear_nameplates(mut index: ResMut<NameplateIndex>, mut commands: Commands) {
    for (_, plate) in index.0.drain() {
        if let Ok(mut ec) = commands.get_entity(plate) {
            ec.despawn();
        }
    }
}
