//! Loot container UI — pending-loot tracking + loot window.
//!
//! Flow:
//!   * `ingest_pending_loots` pulls `PendingLootsSnapshot` into the
//!     `NearbyLoot` resource every tick — client now knows which
//!     containers exist and where.
//!   * `sync_loot_markers` spawns a bag mesh at each container
//!     position and despawns bags whose container is gone.
//!   * `handle_loot_interact_input` listens for `G`; when pressed
//!     near a container, sends `LootOpenRequest` for the closest.
//!   * `ingest_loot_window` pulls `LootWindowSnapshot` → updates the
//!     `LootWindow` resource.
//!   * `draw_loot_window` renders the egui panel. Click an item to
//!     send `LootTakeRequest`; "Take all" sends `LootTakeAllRequest`.
//!   * `ingest_loot_closed` pulls `LootClosedNotice` → auto-closes
//!     the window if it was showing this container.

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use lightyear::prelude::client::Client;
use lightyear::prelude::*;

use vaern_items::{ContentRegistry, ItemInstance};
use vaern_protocol::{
    Channel1, LootClosedNotice, LootContainerSummary, LootId, LootOpenRequest,
    LootTakeAllRequest, LootTakeRequest, LootWindowEntry, LootWindowSnapshot,
    PendingLootsSnapshot,
};

use crate::inventory_ui::ClientContent;
use crate::item_icons::{CELL_SIZE, ItemIconCache, paint_item_cell, rarity_color};
use crate::menu::AppState;
use crate::shared::{GameWorld, Player};

/// Max distance from player to container for `G` to work. Must match
/// the server's `LOOT_OPEN_RANGE` constant.
const LOOT_OPEN_RANGE: f32 = 5.0;

/// Latest `PendingLootsSnapshot` from the server.
#[derive(Resource, Default, Debug)]
pub struct NearbyLoot {
    pub containers: Vec<LootContainerSummary>,
}

/// Live bag-mesh entities, keyed by `LootId`. Reconciled against
/// `NearbyLoot.containers` every tick by `sync_loot_markers`: new
/// loot ids get a bag spawned, vanished ones get their bag despawned.
#[derive(Resource, Default, Debug)]
pub struct LootMarkers {
    entities: HashMap<LootId, Entity>,
}

/// State of the open loot window. `loot_id == 0` means no window open.
#[derive(Resource, Default, Debug)]
pub struct LootWindow {
    pub loot_id: LootId,
    pub open: bool,
    pub slots: Vec<LootWindowEntry>,
}

impl LootWindow {
    pub fn is_open(&self) -> bool {
        self.open && self.loot_id != 0
    }
}

pub struct LootUiPlugin;

impl Plugin for LootUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NearbyLoot>()
            .init_resource::<LootWindow>()
            .init_resource::<LootMarkers>()
            .add_systems(
                Update,
                (
                    ingest_pending_loots,
                    ingest_loot_window,
                    ingest_loot_closed,
                    sync_loot_markers,
                    handle_loot_interact_input,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                EguiPrimaryContextPass,
                draw_loot_window.run_if(in_state(AppState::InGame)),
            );
    }
}

fn ingest_pending_loots(
    mut rx: Query<&mut MessageReceiver<PendingLootsSnapshot>, With<Client>>,
    mut nearby: ResMut<NearbyLoot>,
) {
    for mut receiver in &mut rx {
        if let Some(snap) = receiver.receive().last() {
            nearby.containers = snap.containers;
        }
    }
}

fn ingest_loot_window(
    mut rx: Query<&mut MessageReceiver<LootWindowSnapshot>, With<Client>>,
    mut window: ResMut<LootWindow>,
) {
    for mut receiver in &mut rx {
        if let Some(snap) = receiver.receive().last() {
            window.loot_id = snap.loot_id;
            window.slots = snap.slots;
            window.open = true;
            // If the window just transitioned to empty, close it
            // (server follows up with LootClosedNotice for the same id,
            // but closing proactively avoids a one-frame flash).
            if window.slots.is_empty() {
                window.open = false;
            }
        }
    }
}

fn ingest_loot_closed(
    mut rx: Query<&mut MessageReceiver<LootClosedNotice>, With<Client>>,
    mut window: ResMut<LootWindow>,
) {
    for mut receiver in &mut rx {
        for notice in receiver.receive() {
            if window.loot_id == notice.loot_id {
                window.open = false;
                window.slots.clear();
                window.loot_id = 0;
            }
        }
    }
}

/// Spawn a bag mesh at each pending-loot position; despawn it when the
/// server stops reporting that container. Tagged `GameWorld` so the
/// on-disconnect teardown sweeps any leftovers.
fn sync_loot_markers(
    nearby: Res<NearbyLoot>,
    mut markers: ResMut<LootMarkers>,
    assets: Res<AssetServer>,
    mut commands: Commands,
) {
    let current: HashSet<LootId> = nearby.containers.iter().map(|c| c.loot_id).collect();

    markers.entities.retain(|id, entity| {
        if current.contains(id) {
            true
        } else {
            if let Ok(mut ec) = commands.get_entity(*entity) {
                ec.despawn();
            }
            false
        }
    });

    for c in &nearby.containers {
        if markers.entities.contains_key(&c.loot_id) {
            continue;
        }
        let pos = Vec3::new(c.pos_x, c.pos_y, c.pos_z);
        let entity = commands
            .spawn((
                Name::new(format!("loot-bag-{}", c.loot_id)),
                SceneRoot(assets.load("extracted/props/Bag.gltf#Scene0")),
                Transform::from_translation(pos),
                GameWorld,
            ))
            .id();
        markers.entities.insert(c.loot_id, entity);
    }
}

/// Find the closest pending-loot container within `LOOT_OPEN_RANGE`
/// of the player. Returns its `loot_id`, or `None` if nothing is in
/// reach.
fn closest_in_range(player_pos: Vec3, nearby: &NearbyLoot) -> Option<LootId> {
    let mut best: Option<(LootId, f32)> = None;
    for c in &nearby.containers {
        let pos = Vec3::new(c.pos_x, c.pos_y, c.pos_z);
        let d = player_pos.distance(pos);
        if d <= LOOT_OPEN_RANGE {
            if best.map_or(true, |(_, bd)| d < bd) {
                best = Some((c.loot_id, d));
            }
        }
    }
    best.map(|(id, _)| id)
}

fn handle_loot_interact_input(
    keys: Res<ButtonInput<KeyCode>>,
    nearby: Res<NearbyLoot>,
    player: Query<&Transform, With<Player>>,
    mut sender: Query<&mut MessageSender<LootOpenRequest>, With<Client>>,
) {
    if !keys.just_pressed(KeyCode::KeyG) {
        return;
    }
    let Ok(player_tf) = player.single() else { return };
    let Some(loot_id) = closest_in_range(player_tf.translation, &nearby) else {
        return;
    };
    if let Ok(mut tx) = sender.single_mut() {
        let _ = tx.send::<Channel1>(LootOpenRequest { loot_id });
    }
}

fn describe(inst: &ItemInstance, reg: &ContentRegistry) -> String {
    reg.resolve(inst)
        .map(|r| r.display_name)
        .unwrap_or_else(|_| format!("<unresolved {}>", inst.base_id))
}

fn draw_loot_window(
    mut contexts: EguiContexts,
    mut window: ResMut<LootWindow>,
    content: Option<Res<ClientContent>>,
    icons: Res<ItemIconCache>,
    mut take_tx: Query<&mut MessageSender<LootTakeRequest>, With<Client>>,
    mut take_all_tx: Query<&mut MessageSender<LootTakeAllRequest>, With<Client>>,
) {
    if !window.is_open() {
        return;
    }
    let Some(content) = content else { return };
    let Ok(ctx) = contexts.ctx_mut() else { return };

    let mut want_close = false;

    egui::Window::new("Loot")
        .default_size(egui::vec2(360.0, 320.0))
        .show(ctx, |ui| {
            ui.label(format!("Container #{}", window.loot_id));
            ui.separator();
            if window.slots.is_empty() {
                ui.label(egui::RichText::new("— empty —").color(egui::Color32::DARK_GRAY));
            } else {
                egui::ScrollArea::vertical()
                    .max_height(220.0)
                    .id_salt("loot_scroll")
                    .show(ui, |ui| {
                        for (idx, entry) in window.slots.iter().enumerate() {
                            ui.horizontal(|ui| {
                                let resolved = content.0.resolve(&entry.instance).ok();
                                let (rect, resp) = ui.allocate_exact_size(
                                    egui::vec2(CELL_SIZE, CELL_SIZE),
                                    egui::Sense::click(),
                                );
                                let painter = ui.painter().clone();
                                if let Some(r) = &resolved {
                                    paint_item_cell(
                                        &painter,
                                        rect,
                                        &entry.instance.base_id,
                                        &r.kind,
                                        r.rarity,
                                        icons.get(&entry.instance.base_id),
                                        entry.count,
                                    );
                                } else {
                                    painter.rect_filled(
                                        rect,
                                        4.0,
                                        egui::Color32::from_rgb(80, 20, 20),
                                    );
                                }
                                if resp.clicked() {
                                    if let Ok(mut tx) = take_tx.single_mut() {
                                        let _ = tx.send::<Channel1>(LootTakeRequest {
                                            loot_id: window.loot_id,
                                            slot_idx: idx as u32,
                                        });
                                    }
                                }
                                ui.vertical(|ui| {
                                    let name = describe(&entry.instance, &content.0);
                                    let color = resolved
                                        .as_ref()
                                        .map(|r| rarity_color(r.rarity))
                                        .unwrap_or(egui::Color32::LIGHT_RED);
                                    ui.label(
                                        egui::RichText::new(&name).color(color).size(13.0),
                                    );
                                    if entry.count > 1 {
                                        ui.label(
                                            egui::RichText::new(format!("×{}", entry.count))
                                                .size(10.0)
                                                .color(egui::Color32::from_gray(170)),
                                        );
                                    }
                                });
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.button("Take").clicked() {
                                            if let Ok(mut tx) = take_tx.single_mut() {
                                                let _ = tx.send::<Channel1>(LootTakeRequest {
                                                    loot_id: window.loot_id,
                                                    slot_idx: idx as u32,
                                                });
                                            }
                                        }
                                    },
                                );
                            });
                        }
                    });
            }
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Take all").clicked() {
                    if let Ok(mut tx) = take_all_tx.single_mut() {
                        let _ = tx.send::<Channel1>(LootTakeAllRequest {
                            loot_id: window.loot_id,
                        });
                    }
                }
                if ui.button("Close").clicked() {
                    want_close = true;
                }
            });
        });

    if want_close {
        window.open = false;
    }
}
