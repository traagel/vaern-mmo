//! Vendor UI — egui window with Buy + Sell tabs.
//!
//! Data flow:
//!   Server `VendorWindowSnapshot` → `ActiveVendor` resource.
//!   Server `VendorClosedNotice` → clears `ActiveVendor`.
//!   F-press near a vendor → `VendorOpenRequest(entity)`.
//!   "Buy" click → `VendorBuyRequest { vendor_id, listing_idx }`.
//!   "Sell" click → `VendorSellRequest { vendor_id, inventory_idx }`.
//!
//! The Sell tab reads `OwnInventory` directly (no separate snapshot) —
//! after each sell the server broadcasts a fresh `InventorySnapshot`
//! and `WalletSnapshot`, so the UI re-renders with the new state
//! automatically.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use lightyear::prelude::*;
use lightyear::prelude::client::Client;

use vaern_combat::{DisplayName, NpcKind};
use vaern_economy::{QualityMod, VendorPricing, format_copper_as_gsc, vendor_sell_price};
use vaern_items::{ContentRegistry, ResolvedItem};
use vaern_protocol::{
    Channel1, VendorBuyRequest, VendorClosedNotice, VendorOpenRequest, VendorSellRequest,
    VendorWindowSnapshot,
};

use crate::inventory_ui::{ClientContent, OwnInventory, OwnWallet};
use crate::item_icons::rarity_color;
use crate::menu::AppState;
use crate::shared::Player;

/// Range at which "[F] Trade with X" shows + F sends VendorOpenRequest.
/// Matches server `VENDOR_INTERACT_RANGE` (5.0u).
pub const VENDOR_INTERACT_RANGE: f32 = 5.0;

pub struct VendorUiPlugin;

impl Plugin for VendorUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NearbyVendor>()
            .init_resource::<ActiveVendor>()
            .init_resource::<VendorTab>()
            .add_systems(
                Update,
                (
                    detect_nearby_vendor,
                    open_vendor_on_f,
                    ingest_vendor_snapshot,
                    ingest_vendor_closed,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                EguiPrimaryContextPass,
                (vendor_prompt_ui, vendor_window_ui).run_if(in_state(AppState::InGame)),
            )
            .add_systems(OnExit(AppState::InGame), reset_state);
    }
}

#[derive(Resource, Default, Debug)]
pub struct NearbyVendor {
    pub entity: Option<Entity>,
    pub name: String,
}

/// Open vendor window state. `None` = no window. Stores the latest
/// snapshot directly — server pushes updates on every buy.
#[derive(Resource, Default, Debug)]
pub struct ActiveVendor {
    pub snapshot: Option<VendorWindowSnapshot>,
}

#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum VendorTab {
    #[default]
    Buy,
    Sell,
}

fn detect_nearby_vendor(
    player: Query<&Transform, With<Player>>,
    vendors: Query<(Entity, &Transform, &NpcKind, Option<&DisplayName>)>,
    mut nearby: ResMut<NearbyVendor>,
) {
    let Ok(player_tf) = player.single() else {
        nearby.entity = None;
        nearby.name.clear();
        return;
    };
    let range_sq = VENDOR_INTERACT_RANGE * VENDOR_INTERACT_RANGE;
    let best = vendors
        .iter()
        .filter(|(_, _, kind, _)| matches!(*kind, NpcKind::Vendor))
        .map(|(e, tf, _, name)| {
            (
                e,
                tf.translation.distance_squared(player_tf.translation),
                name,
            )
        })
        .filter(|(_, d_sq, _)| *d_sq <= range_sq)
        .min_by(|a, b| a.1.total_cmp(&b.1));
    match best {
        Some((e, _, name)) => {
            nearby.entity = Some(e);
            nearby.name = name
                .map(|n| n.0.clone())
                .unwrap_or_else(|| "Vendor".into());
        }
        None => {
            nearby.entity = None;
            nearby.name.clear();
        }
    }
}

fn open_vendor_on_f(
    keys: Res<ButtonInput<KeyCode>>,
    nearby: Res<NearbyVendor>,
    active: Res<ActiveVendor>,
    mut tx: Query<&mut MessageSender<VendorOpenRequest>, With<Client>>,
) {
    if !keys.just_pressed(KeyCode::KeyF) {
        return;
    }
    // Don't re-open if we already have a window up — quest-giver F
    // routing might also fire this frame; priority goes to whichever
    // system runs first. For the simple pre-alpha case, no conflict.
    if active.snapshot.is_some() {
        return;
    }
    let Some(e) = nearby.entity else { return };
    let Ok(mut sender) = tx.single_mut() else { return };
    let _ = sender.send::<Channel1>(VendorOpenRequest { vendor: e });
}

fn ingest_vendor_snapshot(
    mut rx: Query<&mut MessageReceiver<VendorWindowSnapshot>, With<Client>>,
    mut active: ResMut<ActiveVendor>,
) {
    let Ok(mut receiver) = rx.single_mut() else { return };
    for snap in receiver.receive() {
        active.snapshot = Some(snap);
    }
}

fn ingest_vendor_closed(
    mut rx: Query<&mut MessageReceiver<VendorClosedNotice>, With<Client>>,
    mut active: ResMut<ActiveVendor>,
) {
    let Ok(mut receiver) = rx.single_mut() else { return };
    for notice in receiver.receive() {
        if active
            .snapshot
            .as_ref()
            .map(|s| s.vendor_id == notice.vendor_id)
            .unwrap_or(false)
        {
            active.snapshot = None;
        }
    }
}

fn vendor_prompt_ui(
    mut contexts: EguiContexts,
    nearby: Res<NearbyVendor>,
    active: Res<ActiveVendor>,
) {
    // Suppress prompt while the window is open so it doesn't overlap.
    if active.snapshot.is_some() || nearby.entity.is_none() {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else { return };
    egui::Area::new(egui::Id::new("vendor_prompt"))
        .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -100.0))
        .show(ctx, |ui| {
            egui::Frame::default()
                .fill(egui::Color32::from_rgba_unmultiplied(20, 25, 30, 220))
                .stroke(egui::Stroke::new(
                    1.0,
                    egui::Color32::from_rgb(150, 200, 240),
                ))
                .inner_margin(egui::Margin::symmetric(14, 8))
                .corner_radius(4.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("[F]")
                                .strong()
                                .color(egui::Color32::from_rgb(255, 220, 120))
                                .size(14.0),
                        );
                        ui.label(
                            egui::RichText::new(format!("Trade with {}", nearby.name))
                                .color(egui::Color32::from_gray(220))
                                .size(13.0),
                        );
                    });
                });
        });
}

#[allow(clippy::too_many_arguments)]
fn vendor_window_ui(
    mut contexts: EguiContexts,
    mut active: ResMut<ActiveVendor>,
    mut tab: ResMut<VendorTab>,
    wallet: Res<OwnWallet>,
    inv: Res<OwnInventory>,
    content: Option<Res<ClientContent>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut buy_tx: Query<
        &mut MessageSender<VendorBuyRequest>,
        (With<Client>, Without<MessageSender<VendorSellRequest>>),
    >,
    mut sell_tx: Query<
        &mut MessageSender<VendorSellRequest>,
        (With<Client>, Without<MessageSender<VendorBuyRequest>>),
    >,
) {
    let Some(snap) = active.snapshot.as_ref() else { return };
    let Some(content) = content else { return };
    let Ok(ctx) = contexts.ctx_mut() else { return };

    if keys.just_pressed(KeyCode::Escape) {
        active.snapshot = None;
        return;
    }

    let mut close = false;
    let mut buy_idx: Option<u32> = None;
    let mut sell_idx: Option<u32> = None;
    let vendor_id = snap.vendor_id;
    let pricing = VendorPricing::default();

    egui::Window::new(format!("— {} —", snap.vendor_name))
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .resizable(false)
        .collapsible(false)
        .default_width(560.0)
        .show(ctx, |ui| {
            // Header: wallet + tab selector.
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("Gold  {}", format_copper_as_gsc(wallet.copper)))
                        .strong()
                        .color(egui::Color32::from_rgb(230, 200, 100)),
                );
                ui.add_space(16.0);
                if ui
                    .selectable_label(*tab == VendorTab::Buy, "Buy")
                    .clicked()
                {
                    *tab = VendorTab::Buy;
                }
                if ui
                    .selectable_label(*tab == VendorTab::Sell, "Sell")
                    .clicked()
                {
                    *tab = VendorTab::Sell;
                }
            });
            ui.separator();

            egui::ScrollArea::vertical()
                .max_height(360.0)
                .show(ui, |ui| match *tab {
                    VendorTab::Buy => {
                        for listing in &snap.listings {
                            render_buy_row(
                                ui,
                                listing,
                                &content.0,
                                wallet.copper,
                                &mut buy_idx,
                            );
                        }
                        if snap.listings.is_empty() {
                            ui.label(
                                egui::RichText::new("— nothing to sell —")
                                    .italics()
                                    .color(egui::Color32::from_gray(170)),
                            );
                        }
                    }
                    VendorTab::Sell => {
                        let mut any = false;
                        for (idx, entry) in inv.slots.iter().enumerate() {
                            let Some(entry) = entry.as_ref() else { continue };
                            any = true;
                            render_sell_row(
                                ui,
                                idx as u32,
                                entry.count,
                                &entry.instance,
                                &content.0,
                                &pricing,
                                &mut sell_idx,
                            );
                        }
                        if !any {
                            ui.label(
                                egui::RichText::new("— nothing in your inventory —")
                                    .italics()
                                    .color(egui::Color32::from_gray(170)),
                            );
                        }
                    }
                });

            ui.separator();
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("[Esc] close")
                        .small()
                        .color(egui::Color32::from_gray(130)),
                );
                if ui.button("Close").clicked() {
                    close = true;
                }
            });
        });

    if let Some(idx) = buy_idx {
        if let Ok(mut sender) = buy_tx.single_mut() {
            let _ = sender.send::<Channel1>(VendorBuyRequest {
                vendor_id,
                listing_idx: idx,
            });
        }
    }
    if let Some(idx) = sell_idx {
        if let Ok(mut sender) = sell_tx.single_mut() {
            let _ = sender.send::<Channel1>(VendorSellRequest {
                vendor_id,
                inventory_idx: idx,
            });
        }
    }
    if close {
        active.snapshot = None;
    }
}

fn render_buy_row(
    ui: &mut egui::Ui,
    listing: &vaern_protocol::VendorWindowListing,
    content: &ContentRegistry,
    wallet_copper: u64,
    buy_idx: &mut Option<u32>,
) {
    let resolved: Option<ResolvedItem> = content.resolve(&listing.instance).ok();
    let (name, rarity) = match resolved.as_ref() {
        Some(r) => (r.display_name.clone(), r.rarity),
        None => (listing.instance.base_id.clone(), vaern_items::Rarity::Common),
    };
    let affordable = wallet_copper >= listing.price_copper as u64;
    let has_stock = listing.stock.map(|n| n > 0).unwrap_or(true);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(&name)
                .color(rarity_color(rarity))
                .size(13.0),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut btn = egui::Button::new(
                egui::RichText::new(format!(
                    "Buy  {}",
                    format_copper_as_gsc(listing.price_copper as u64)
                ))
                .size(12.0),
            );
            if !affordable || !has_stock {
                btn = btn.sense(egui::Sense::hover());
            }
            let resp = ui.add_enabled(affordable && has_stock, btn);
            if resp.clicked() {
                *buy_idx = Some(listing.idx);
            }
            if let Some(n) = listing.stock {
                ui.label(
                    egui::RichText::new(format!("×{n}"))
                        .size(11.0)
                        .color(egui::Color32::from_gray(150)),
                );
            }
        });
    });
}

fn render_sell_row(
    ui: &mut egui::Ui,
    inventory_idx: u32,
    count: u32,
    instance: &vaern_items::ItemInstance,
    content: &ContentRegistry,
    pricing: &VendorPricing,
    sell_idx: &mut Option<u32>,
) {
    let Ok(resolved) = content.resolve(instance) else { return };
    let price = match vendor_sell_price(&resolved, pricing, QualityMod::default()) {
        Ok(p) => p,
        Err(_) => {
            // Soulbound / quest-item — show grey "no sale" row so the
            // player sees why it isn't sellable.
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(&resolved.display_name)
                        .color(egui::Color32::from_gray(120))
                        .size(13.0),
                );
                ui.label(
                    egui::RichText::new("(soulbound / no sale)")
                        .color(egui::Color32::from_gray(100))
                        .size(11.0),
                );
            });
            return;
        }
    };
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(&resolved.display_name)
                .color(rarity_color(resolved.rarity))
                .size(13.0),
        );
        if count > 1 {
            ui.label(
                egui::RichText::new(format!("×{count}"))
                    .size(11.0)
                    .color(egui::Color32::from_gray(150)),
            );
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button(
                    egui::RichText::new(format!("Sell  {}", format_copper_as_gsc(price as u64)))
                        .size(12.0),
                )
                .clicked()
            {
                *sell_idx = Some(inventory_idx);
            }
        });
    });
}

fn reset_state(
    mut nearby: ResMut<NearbyVendor>,
    mut active: ResMut<ActiveVendor>,
    mut tab: ResMut<VendorTab>,
) {
    nearby.entity = None;
    nearby.name.clear();
    active.snapshot = None;
    *tab = VendorTab::Buy;
}
