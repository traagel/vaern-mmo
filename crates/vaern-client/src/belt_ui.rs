//! Consumable belt UI — 4 hotkey slots (keys 7/8/9/0) for potions.
//!
//! Shows below the hotbar as a compact 4-slot strip. Each slot displays
//! the bound potion's short name + the count remaining in the bag. Keys
//! 7/8/9/0 fire `ConsumeBeltRequest` for the matching slot.
//!
//! Binding happens from the inventory UI's right-click context menu
//! (see `inventory_ui.rs`) — this module only owns the snapshot
//! ingestion, the hotkey wiring, and the draw call.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use lightyear::prelude::*;
use lightyear::prelude::client::Client;

use vaern_inventory::BELT_SLOTS;
use vaern_items::{ContentRegistry, ItemInstance};
use vaern_protocol::{
    Channel1, ConsumableBeltSnapshot, ConsumeBeltRequest,
};

use crate::inventory_ui::{ClientContent, OwnInventory};
use crate::item_icons::{ItemIconCache, paint_empty_slot, paint_item_cell};
use crate::menu::AppState;

/// Latest belt snapshot from the server. `slots[i]` = bound template
/// or None. Client looks up counts from `OwnInventory` locally.
#[derive(Resource, Default)]
pub struct OwnConsumableBelt {
    pub slots: [Option<ItemInstance>; BELT_SLOTS],
}

pub struct BeltUiPlugin;

impl Plugin for BeltUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OwnConsumableBelt>()
            .add_systems(
                Update,
                (ingest_belt_snapshot, belt_hotkey_input)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                EguiPrimaryContextPass,
                draw_belt_strip.run_if(in_state(AppState::InGame)),
            );
    }
}

fn ingest_belt_snapshot(
    mut rx: Query<&mut MessageReceiver<ConsumableBeltSnapshot>, With<Client>>,
    mut belt: ResMut<OwnConsumableBelt>,
) {
    for mut receiver in &mut rx {
        if let Some(snap) = receiver.receive().last() {
            // Snapshot ships Vec<Option<…>>; normalize to fixed array.
            let mut next: [Option<ItemInstance>; BELT_SLOTS] = Default::default();
            for (i, entry) in snap.slots.into_iter().enumerate() {
                if i < BELT_SLOTS {
                    next[i] = entry;
                }
            }
            belt.slots = next;
        }
    }
}

/// Keys 7/8/9/0 → belt slots 0..=3. Sends `ConsumeBeltRequest`.
/// Doesn't gate on local "are you bound" — server is authoritative
/// and silently no-ops unbound slots.
fn belt_hotkey_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut tx: Query<&mut MessageSender<ConsumeBeltRequest>, With<Client>>,
) {
    let slot = if keys.just_pressed(KeyCode::Digit7) {
        Some(0u8)
    } else if keys.just_pressed(KeyCode::Digit8) {
        Some(1)
    } else if keys.just_pressed(KeyCode::Digit9) {
        Some(2)
    } else if keys.just_pressed(KeyCode::Digit0) {
        Some(3)
    } else {
        None
    };
    let Some(slot_idx) = slot else { return };
    if let Ok(mut sender) = tx.single_mut() {
        let _ = sender.send::<Channel1>(ConsumeBeltRequest { slot_idx });
    }
}

fn draw_belt_strip(
    mut contexts: EguiContexts,
    belt: Res<OwnConsumableBelt>,
    inv: Res<OwnInventory>,
    content: Option<Res<ClientContent>>,
    icons: Res<ItemIconCache>,
) {
    let Ok(ctx) = contexts.ctx_mut() else { return };
    let Some(content) = content else { return };

    // Anchored just above the hotbar; tight 4-slot strip with keys in
    // grey below. Invisible frame (no border) so it doesn't compete
    // visually with the hotbar.
    egui::Window::new("consumable_belt")
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .movable(false)
        .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -100.0))
        .frame(
            egui::Frame::default()
                .fill(egui::Color32::from_rgba_unmultiplied(15, 18, 22, 200))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(70)))
                .inner_margin(egui::Margin::symmetric(6, 4))
                .corner_radius(4.0),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                for (i, bound) in belt.slots.iter().enumerate() {
                    belt_slot_widget(ui, i, bound.as_ref(), &inv, &content.0, &icons);
                }
            });
        });
}

const SLOT_KEYS: [&str; BELT_SLOTS] = ["7", "8", "9", "0"];
const BELT_CELL: f32 = 44.0;

fn belt_slot_widget(
    ui: &mut egui::Ui,
    idx: usize,
    bound: Option<&ItemInstance>,
    inv: &OwnInventory,
    content: &ContentRegistry,
    icons: &ItemIconCache,
) {
    ui.vertical(|ui| {
        let (rect, resp) =
            ui.allocate_exact_size(egui::vec2(BELT_CELL, BELT_CELL), egui::Sense::hover());
        let painter = ui.painter().clone();

        match bound {
            None => {
                paint_empty_slot(&painter, rect, "");
            }
            Some(inst) => {
                let resolved = content.resolve(inst).ok();
                let count: u32 = inv
                    .slots
                    .iter()
                    .filter_map(|s| s.as_ref())
                    .filter(|s| {
                        s.instance.base_id == inst.base_id
                            && s.instance.material_id == inst.material_id
                            && s.instance.quality_id == inst.quality_id
                            && s.instance.affixes == inst.affixes
                    })
                    .map(|s| s.count)
                    .sum();
                if let Some(r) = &resolved {
                    paint_item_cell(
                        &painter,
                        rect,
                        &inst.base_id,
                        &r.kind,
                        r.rarity,
                        icons.get(&inst.base_id),
                        count,
                    );
                    // Out-of-stock dim overlay so empty bound slots
                    // read clearly even with the icon present.
                    if count == 0 {
                        painter.rect_filled(
                            rect,
                            4.0,
                            egui::Color32::from_rgba_unmultiplied(0, 0, 0, 160),
                        );
                    }
                } else {
                    paint_empty_slot(&painter, rect, "?");
                }
                let tooltip_name = resolved
                    .as_ref()
                    .map(|r| r.display_name.clone())
                    .unwrap_or_else(|| inst.base_id.clone());
                resp.on_hover_text(if count > 0 {
                    format!("{tooltip_name} ×{count}")
                } else {
                    format!("{tooltip_name} (out of stock)")
                });
            }
        }

        ui.add_space(-2.0);
        ui.label(
            egui::RichText::new(SLOT_KEYS[idx])
                .size(10.0)
                .color(egui::Color32::from_gray(140)),
        );
    });
}
