//! World-space nameplates + rising damage numbers for every living entity.
//! Positioned each frame by projecting world pos → screen via the main camera.

use std::collections::HashMap;

use bevy::prelude::*;
use vaern_combat::{AnimState, DeathEvent, DisplayName, Health, NpcKind};
use vaern_protocol::PlayerTag;

use crate::chat_ui::{ChatBubbleEvent, ChatInputFocused};
use crate::menu::AppState;
use crate::menu::pillar_display;
use crate::scene::CastFiredLocal;
use crate::shared::MainCamera;

/// Max world-space distance (metres/units) at which a nameplate stays
/// visible. Beyond this the plate hides — matches MMO norms where
/// dense crowds don't turn into a letter soup.
pub const NAMEPLATE_MAX_RANGE: f32 = 60.0;

pub struct NameplatesPlugin;

impl Plugin for NameplatesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NameplateIndex>()
            .init_resource::<NameplatesVisible>()
            .add_systems(OnExit(AppState::InGame), clear_nameplates)
            .add_systems(
                Update,
                (
                    toggle_nameplates_hotkey,
                    spawn_nameplates,
                    update_nameplates,
                    tick_damage_numbers,
                    spawn_chat_bubbles,
                    tick_chat_bubbles,
                    log_deaths,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

/// Global visibility toggle for nameplates. `true` = shown (default);
/// `false` = all plates forced hidden. Toggled by the `V` key.
#[derive(Resource, Debug, Clone, Copy)]
pub struct NameplatesVisible(pub bool);

impl Default for NameplatesVisible {
    fn default() -> Self {
        Self(true)
    }
}

fn toggle_nameplates_hotkey(
    keys: Res<ButtonInput<KeyCode>>,
    chat_focused: Res<ChatInputFocused>,
    mut vis: ResMut<NameplatesVisible>,
) {
    // Suppress while typing so someone wanting "I have" in chat doesn't
    // accidentally toggle plates off.
    if chat_focused.0 {
        return;
    }
    if keys.just_pressed(KeyCode::KeyV) {
        vis.0 = !vis.0;
        info!("[nameplates] {}", if vis.0 { "shown" } else { "hidden" });
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
        // Prefer the character's display name for both players and
        // NPCs — that's what makes the world legible in a party.
        // Fall back to pillar label only for anonymous spawns (empty
        // DisplayName, common in headless / test-harness clients).
        let label = match (display, tag) {
            (Some(d), _) if !d.0.is_empty() => d.0.clone(),
            (_, Some(t)) => pillar_display(t.core_pillar).to_string(),
            (Some(d), _) => d.0.clone(),
            (None, None) => "NPC".to_string(),
        };
        let label_color = match kind {
            Some(NpcKind::QuestGiver) => Color::srgb(1.0, 0.85, 0.35), // gold
            Some(NpcKind::Vendor) => Color::srgb(0.60, 0.80, 0.95),    // cool blue
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

        // "!" / "?" indicator above the name label. QuestGivers get the
        // classic gold "!"; QuestPoi waypoints get a teal "?" to read as
        // "investigate here". Other NPC kinds get no marker.
        let quest_marker = match kind {
            Some(NpcKind::QuestGiver) => Some(
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
            ),
            Some(NpcKind::QuestPoi) => Some(
                commands
                    .spawn((
                        Text::new("?"),
                        TextFont {
                            font_size: 22.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.35, 0.85, 0.95)),
                    ))
                    .id(),
            ),
            _ => None,
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
    nameplates_visible: Res<NameplatesVisible>,
) {
    let Ok((cam, cam_tf)) = cams.single() else {
        return;
    };
    let cam_pos = cam_tf.translation();
    let range_sq = NAMEPLATE_MAX_RANGE * NAMEPLATE_MAX_RANGE;
    for (plate, mut node, mut vis) in &mut plates {
        // Global toggle wins — when hidden, skip projection work too.
        if !nameplates_visible.0 {
            *vis = Visibility::Hidden;
            continue;
        }
        let Ok((target_tf, hp, state)) = targets.get(plate.target) else {
            *vis = Visibility::Hidden;
            continue;
        };
        // Cull by distance before projecting — cheap early-out that
        // also prevents distant plates from cluttering the HUD.
        if target_tf.translation().distance_squared(cam_pos) > range_sq {
            *vis = Visibility::Hidden;
            continue;
        }
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

// ─── chat bubbles ──────────────────────────────────────────────────────────

/// How long a chat bubble lingers above its speaker's head before it
/// fades out. Matches the "read a one-liner" window — long enough to
/// catch a glance, short enough not to pile up.
const CHAT_BUBBLE_SECS: f32 = 5.0;
/// Max visible bubbles per speaker. New bubbles replace the oldest
/// live one instead of stacking forever — reads like a speech balloon
/// re-filling, not a thought tower.
const CHAT_BUBBLE_MAX_PER_TARGET: usize = 1;
/// Max characters per bubble; longer messages truncate with an ellipsis.
/// Scrollback still has the full text in the chat history panel.
const CHAT_BUBBLE_MAX_CHARS: usize = 72;

#[derive(Component, Debug, Clone)]
struct ChatBubble {
    target: Entity,
    remaining: f32,
    total: f32,
}

fn truncate_for_bubble(text: &str) -> String {
    let count = text.chars().count();
    if count <= CHAT_BUBBLE_MAX_CHARS {
        return text.to_string();
    }
    let mut out: String = text.chars().take(CHAT_BUBBLE_MAX_CHARS - 1).collect();
    out.push('…');
    out
}

/// Resolve each `ChatBubbleEvent` to a speaker entity by
/// case-insensitive `DisplayName` match, then spawn a bubble UI node
/// tracking that entity. Also culls older bubbles above the same
/// target so only `CHAT_BUBBLE_MAX_PER_TARGET` show at once.
fn spawn_chat_bubbles(
    mut events: MessageReader<ChatBubbleEvent>,
    players: Query<(Entity, &DisplayName), With<Health>>,
    existing: Query<(Entity, &ChatBubble)>,
    mut commands: Commands,
) {
    for ev in events.read() {
        // Resolve speaker by display name — case-insensitive so chat
        // typing doesn't require exact-casing the recipient.
        let Some((speaker, _)) = players
            .iter()
            .find(|(_, name)| name.0.eq_ignore_ascii_case(&ev.from))
        else {
            continue;
        };

        // Enforce the per-target cap before spawning a new one.
        let mut same_target: Vec<(Entity, f32)> = existing
            .iter()
            .filter(|(_, b)| b.target == speaker)
            .map(|(e, b)| (e, b.remaining))
            .collect();
        // Sort so the smallest-remaining (oldest) bubbles are first.
        same_target.sort_by(|a, b| a.1.total_cmp(&b.1));
        while same_target.len() >= CHAT_BUBBLE_MAX_PER_TARGET {
            let (old, _) = same_target.remove(0);
            if let Ok(mut ec) = commands.get_entity(old) {
                ec.despawn();
            }
        }

        let text = truncate_for_bubble(&ev.text);
        // Build the bubble entity across two inserts to stay inside
        // Bevy's bundle-arity for tuples of heterogeneous Components.
        let bubble = commands
            .spawn((
                Text::new(text),
                TextFont {
                    font_size: 13.0,
                    ..default()
                },
                TextColor(Color::srgb(1.0, 1.0, 1.0)),
                BackgroundColor(Color::srgba(0.08, 0.10, 0.14, 0.82)),
                Node {
                    position_type: PositionType::Absolute,
                    padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    max_width: Val::Px(260.0),
                    ..default()
                },
                BorderColor::all(Color::srgba(0.45, 0.55, 0.70, 0.85)),
            ))
            .id();
        commands.entity(bubble).insert((
            Visibility::Hidden,
            Pickable::IGNORE,
            ChatBubble {
                target: speaker,
                remaining: CHAT_BUBBLE_SECS,
                total: CHAT_BUBBLE_SECS,
            },
        ));
    }
}

/// Tick remaining life on every bubble, project to screen, fade out
/// over the last ~1 second. Despawns expired bubbles and any whose
/// target entity disappeared.
fn tick_chat_bubbles(
    time: Res<Time>,
    cams: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    targets: Query<&GlobalTransform>,
    nameplates_visible: Res<NameplatesVisible>,
    mut bubbles: Query<(
        Entity,
        &mut ChatBubble,
        &mut Node,
        &mut Visibility,
        &mut TextColor,
        &mut BackgroundColor,
        &mut BorderColor,
    )>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();
    let cam = cams.single().ok();
    for (entity, mut bubble, mut node, mut vis, mut text_color, mut bg, mut border) in &mut bubbles
    {
        bubble.remaining -= dt;
        if bubble.remaining <= 0.0 {
            if let Ok(mut ec) = commands.get_entity(entity) {
                ec.despawn();
            }
            continue;
        }
        // Same global hide rule as nameplates — V key toggles both.
        if !nameplates_visible.0 {
            *vis = Visibility::Hidden;
            continue;
        }
        let Ok(target_tf) = targets.get(bubble.target) else {
            *vis = Visibility::Hidden;
            continue;
        };
        let Some((cam, cam_tf)) = cam else {
            *vis = Visibility::Hidden;
            continue;
        };
        // Anchor a bit higher than the nameplate so the two don't
        // overlap — plate sits at +2.1, bubble at +2.8.
        let head = target_tf.translation() + Vec3::Y * 2.8;
        match cam.world_to_viewport(cam_tf, head) {
            Ok(screen) => {
                *vis = Visibility::Visible;
                node.left = Val::Px(screen.x - 130.0);
                node.top = Val::Px(screen.y - 40.0);
                // Fade the last 1.0 seconds. Keep full opacity before
                // that so short reads don't blink.
                let fade_start = 1.0;
                let alpha = if bubble.remaining > fade_start {
                    1.0
                } else {
                    (bubble.remaining / fade_start).clamp(0.0, 1.0)
                };
                let c = text_color.0.to_linear();
                text_color.0 =
                    Color::LinearRgba(LinearRgba::new(c.red, c.green, c.blue, alpha));
                let bgc = bg.0.to_linear();
                bg.0 = Color::LinearRgba(LinearRgba::new(
                    bgc.red,
                    bgc.green,
                    bgc.blue,
                    (bgc.alpha * alpha).clamp(0.0, 1.0),
                ));
                // BorderColor has per-side colors in Bevy 0.18; sample
                // top as representative and rewrite all four uniformly.
                let bc = border.top.to_linear();
                *border = BorderColor::all(Color::LinearRgba(LinearRgba::new(
                    bc.red,
                    bc.green,
                    bc.blue,
                    (bc.alpha * alpha).clamp(0.0, 1.0),
                )));
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
