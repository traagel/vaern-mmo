//! Resource-node rendering + H-to-harvest interaction.
//!
//! Nodes are replicated entities carrying `NodeKind` + `NodeState`.
//! Client-side:
//!   * `draw_node_markers` — gizmo sphere per node, colored by kind
//!     family (ore = orange, herb = green, wood = brown). Dim when
//!     state is Harvested.
//!   * `handle_harvest_input` — H pressed + player within 3.5u of an
//!     Available node → `HarvestRequest` message.
//!
//! Result lands in inventory via the existing `InventorySnapshot`
//! broadcast — no new S→C message for this.

use bevy::prelude::*;
use lightyear::prelude::client::Client;
use lightyear::prelude::*;

use vaern_professions::{NodeKind, NodeState};
use vaern_protocol::{Channel1, HarvestRequest};

use crate::menu::AppState;
use crate::shared::Player;

const HARVEST_RANGE: f32 = 3.5;

pub struct HarvestUiPlugin;

impl Plugin for HarvestUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (draw_node_markers, handle_harvest_input).run_if(in_state(AppState::InGame)),
        );
    }
}

fn kind_color(kind: NodeKind) -> Color {
    match kind {
        // Ores: warm amber/copper tones
        NodeKind::CopperVein => Color::srgb(0.85, 0.45, 0.20),
        NodeKind::IronVein => Color::srgb(0.60, 0.55, 0.50),
        NodeKind::SilverVein => Color::srgb(0.85, 0.85, 0.90),
        NodeKind::MithrilVein => Color::srgb(0.55, 0.75, 0.85),
        NodeKind::AdamantineVein => Color::srgb(0.30, 0.45, 0.65),
        // Herbs: greens + flavor
        NodeKind::StanchweedPatch => Color::srgb(0.40, 0.75, 0.35),
        NodeKind::SunleafPatch => Color::srgb(0.85, 0.85, 0.30),
        NodeKind::BlightrootPatch => Color::srgb(0.35, 0.50, 0.25),
        NodeKind::SilverfrondPatch => Color::srgb(0.70, 0.85, 0.75),
        NodeKind::EmberbloomPatch => Color::srgb(0.95, 0.50, 0.30),
        NodeKind::GhostcapPatch => Color::srgb(0.85, 0.80, 0.95),
        // Trees: brown
        NodeKind::PineTree => Color::srgb(0.45, 0.60, 0.35),
        NodeKind::OakTree => Color::srgb(0.55, 0.40, 0.25),
        NodeKind::YewTree => Color::srgb(0.65, 0.55, 0.40),
        NodeKind::IronwoodTree => Color::srgb(0.40, 0.35, 0.30),
    }
}

fn draw_node_markers(
    mut gizmos: Gizmos,
    nodes: Query<(&Transform, &NodeKind, &NodeState)>,
) {
    for (tf, kind, state) in &nodes {
        let base_pos = tf.translation + Vec3::new(0.0, 0.6, 0.0);
        let mut color = kind_color(*kind);
        let (radius_inner, radius_outer) = match state {
            NodeState::Available => (0.35, 0.55),
            NodeState::Harvested { .. } => {
                // Dim the color and shrink when harvested.
                color = Color::srgba(0.3, 0.3, 0.3, 0.4);
                (0.18, 0.28)
            }
        };
        gizmos.sphere(base_pos, radius_inner, color);
        if matches!(state, NodeState::Available) {
            gizmos.sphere(
                base_pos,
                radius_outer,
                Color::srgba(color.to_linear().red, color.to_linear().green, color.to_linear().blue, 0.25),
            );
        }
    }
}

fn handle_harvest_input(
    keys: Res<ButtonInput<KeyCode>>,
    player: Query<&Transform, With<Player>>,
    nodes: Query<(Entity, &Transform, &NodeState), With<NodeKind>>,
    mut sender: Query<&mut MessageSender<HarvestRequest>, With<Client>>,
) {
    if !keys.just_pressed(KeyCode::KeyH) {
        return;
    }
    let Ok(player_tf) = player.single() else { return };
    // Closest Available node within range.
    let mut best: Option<(Entity, f32)> = None;
    for (e, tf, state) in &nodes {
        if !matches!(state, NodeState::Available) {
            continue;
        }
        let d = player_tf.translation.distance(tf.translation);
        if d <= HARVEST_RANGE && best.map_or(true, |(_, bd)| d < bd) {
            best = Some((e, d));
        }
    }
    let Some((node_entity, _)) = best else { return };
    if let Ok(mut tx) = sender.single_mut() {
        let _ = tx.send::<Channel1>(HarvestRequest { node: node_entity });
    }
}
