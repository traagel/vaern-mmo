//! Inventory + paper-doll UI. Single egui window toggled by `I`.
//!
//! Layout (one window, two vertical columns):
//!
//! * **Inventory grid** — 30 slots laid out 3 × 10. Each slot shows the
//!   resolved item name colored by rarity; hover reveals a tooltip card
//!   with stats + resists + soulbound + weight; left-click auto-equips
//!   to the first valid slot for that item's kind.
//!
//! * **Paper doll** — 20 equipment slots in two sub-columns. Left
//!   sub-column holds body armor (head → feet); right holds jewelry
//!   + weapons + focus. Each slot is a fixed-size button; right-click
//!   unequips.
//!
//! Data flow:
//!   Server → `InventorySnapshot` / `EquippedSnapshot` → `OwnInventory`
//!     / `OwnEquipped` resources → panel reads these each frame.
//!   Click → `EquipRequest` / `UnequipRequest` messages → server.

use std::collections::HashMap;

use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use lightyear::prelude::*;
use lightyear::prelude::client::Client;

use vaern_core::DamageType;
use vaern_equipment::EquipSlot;
use vaern_items::{
    ContentRegistry, ItemInstance, ItemKind, Rarity, ResolvedItem, WeaponGrip,
};
use vaern_inventory::BELT_SLOTS;
use vaern_protocol::{
    BindBeltSlotRequest, Channel1, ConsumeItemRequest, EquipRequest, EquippedSnapshot,
    InventorySlotEntry, InventorySnapshot, UnequipRequest,
};

use crate::item_icons::{
    CELL_SIZE, ItemIconCache, paint_empty_slot, paint_item_cell, rarity_color,
};
use crate::menu::AppState;

/// Loaded at startup — mirrors the server's `GameData.content` registry
/// so the UI can resolve instances into display-ready `ResolvedItem`s
/// locally. Ships the same YAML tree; client loads off disk via
/// `CARGO_MANIFEST_DIR` like the other data loaders in `data.rs`.
#[derive(Resource)]
pub struct ClientContent(pub ContentRegistry);

/// Latest inventory snapshot from the server.
#[derive(Resource, Default)]
pub struct OwnInventory {
    pub capacity: u32,
    pub slots: Vec<Option<InventorySlotEntry>>,
}

/// Latest equipped snapshot from the server.
#[derive(Resource, Default)]
pub struct OwnEquipped {
    pub slots: HashMap<EquipSlot, ItemInstance>,
}

/// Toggle state for the inventory window.
#[derive(Resource, Default)]
pub struct InventoryWindowOpen(pub bool);

pub struct InventoryUiPlugin;

impl Plugin for InventoryUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OwnInventory>()
            .init_resource::<OwnEquipped>()
            .init_resource::<InventoryWindowOpen>()
            .add_systems(Startup, load_client_content)
            // Input + snapshot ingestion stay in Update — they don't touch
            // egui. The draw system MUST run in EguiPrimaryContextPass or
            // egui's context isn't ready and clicks silently no-op.
            .add_systems(
                Update,
                (
                    toggle_inventory_window,
                    ingest_inventory_snapshot,
                    ingest_equipped_snapshot,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                EguiPrimaryContextPass,
                draw_inventory_window.run_if(in_state(AppState::InGame)),
            );
    }
}

/// Load the content registry once at startup. Same path-resolution as
/// the server — walks up from `CARGO_MANIFEST_DIR` to `src/generated/items`.
/// If it fails, panics loud so the dev catches the missing tree early.
fn load_client_content(mut commands: Commands) {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.join("../../src/generated/items");
    let mut reg = ContentRegistry::new();
    match reg.load_tree(&root) {
        Ok(c) => {
            println!(
                "[client] content registry loaded: {} bases, {} materials, {} qualities",
                c.bases, c.materials, c.qualities
            );
        }
        Err(e) => panic!("[client] failed to load content tree at {}: {e}", root.display()),
    }
    commands.insert_resource(ClientContent(reg));
}

fn toggle_inventory_window(
    keys: Res<ButtonInput<KeyCode>>,
    mut open: ResMut<InventoryWindowOpen>,
) {
    if keys.just_pressed(KeyCode::KeyI) {
        open.0 = !open.0;
    }
}

fn ingest_inventory_snapshot(
    mut rx: Query<&mut MessageReceiver<InventorySnapshot>, With<Client>>,
    mut inv: ResMut<OwnInventory>,
) {
    for mut receiver in &mut rx {
        if let Some(snap) = receiver.receive().last() {
            inv.capacity = snap.capacity;
            inv.slots = snap.slots;
        }
    }
}

fn ingest_equipped_snapshot(
    mut rx: Query<&mut MessageReceiver<EquippedSnapshot>, With<Client>>,
    mut eq: ResMut<OwnEquipped>,
) {
    for mut receiver in &mut rx {
        if let Some(snap) = receiver.receive().last() {
            eq.slots = snap
                .entries
                .into_iter()
                .map(|e| (e.slot, e.instance))
                .collect();
        }
    }
}

// ─── display helpers ───────────────────────────────────────────────────────

fn rarity_label(r: Rarity) -> &'static str {
    match r {
        Rarity::Junk => "Junk",
        Rarity::Common => "Common",
        Rarity::Uncommon => "Uncommon",
        Rarity::Rare => "Rare",
        Rarity::Epic => "Epic",
        Rarity::Legendary => "Legendary",
    }
}

fn kind_label(k: &ItemKind) -> String {
    match k {
        ItemKind::Weapon { grip, school } => {
            let g = match grip {
                WeaponGrip::Light => "Light",
                WeaponGrip::OneHanded => "One-Handed",
                WeaponGrip::TwoHanded => "Two-Handed",
            };
            format!("{g} {school}")
        }
        ItemKind::Armor {
            slot, layer, armor_type, ..
        } => format!("{armor_type:?} {slot} · {layer:?} layer"),
        ItemKind::Shield { .. } => "Shield".into(),
        ItemKind::Rune { school } => format!("Rune · {school}"),
        ItemKind::Consumable { charges, .. } => {
            if *charges > 1 {
                format!("Consumable · {} charges", charges)
            } else {
                "Consumable".into()
            }
        }
        ItemKind::Reagent => "Reagent".into(),
        ItemKind::Trinket => "Trinket".into(),
        ItemKind::Quest => "Quest Item".into(),
        ItemKind::Material => "Material".into(),
        ItemKind::Currency => "Currency".into(),
        ItemKind::Misc => "Misc".into(),
    }
}

fn slot_short_label(s: EquipSlot) -> &'static str {
    match s {
        EquipSlot::Head => "Head",
        EquipSlot::Shoulders => "Shoulders",
        EquipSlot::Chest => "Chest",
        EquipSlot::Shirt => "Shirt",
        EquipSlot::Tabard => "Tabard",
        EquipSlot::Back => "Back",
        EquipSlot::Wrists => "Wrists",
        EquipSlot::Hands => "Hands",
        EquipSlot::Waist => "Waist",
        EquipSlot::Legs => "Legs",
        EquipSlot::Feet => "Feet",
        EquipSlot::Neck => "Neck",
        EquipSlot::Ring1 => "Ring",
        EquipSlot::Ring2 => "Ring",
        EquipSlot::Trinket1 => "Trinket",
        EquipSlot::Trinket2 => "Trinket",
        EquipSlot::MainHand => "Main Hand",
        EquipSlot::OffHand => "Off Hand",
        EquipSlot::Ranged => "Ranged",
        EquipSlot::Focus => "Focus",
    }
}

/// 3–5 character abbreviation for empty paper-doll cells. The full
/// name appears in the tooltip-on-hover; this is just a glyph the
/// player can scan to confirm "yes, that's the helm slot".
fn slot_abbrev(s: EquipSlot) -> &'static str {
    match s {
        EquipSlot::Head => "HEAD",
        EquipSlot::Shoulders => "SHLD",
        EquipSlot::Chest => "CHST",
        EquipSlot::Shirt => "SHRT",
        EquipSlot::Tabard => "TBRD",
        EquipSlot::Back => "BACK",
        EquipSlot::Wrists => "WRST",
        EquipSlot::Hands => "HAND",
        EquipSlot::Waist => "WAST",
        EquipSlot::Legs => "LEGS",
        EquipSlot::Feet => "FEET",
        EquipSlot::Neck => "NECK",
        EquipSlot::Ring1 | EquipSlot::Ring2 => "RING",
        EquipSlot::Trinket1 | EquipSlot::Trinket2 => "TRKT",
        EquipSlot::MainHand => "MH",
        EquipSlot::OffHand => "OH",
        EquipSlot::Ranged => "RNG",
        EquipSlot::Focus => "FOC",
    }
}

fn damage_type_label(dt: DamageType) -> &'static str {
    match dt {
        DamageType::Slashing => "slashing",
        DamageType::Piercing => "piercing",
        DamageType::Bludgeoning => "bludgeoning",
        DamageType::Fire => "fire",
        DamageType::Cold => "cold",
        DamageType::Lightning => "lightning",
        DamageType::Force => "force",
        DamageType::Radiant => "radiant",
        DamageType::Necrotic => "necrotic",
        DamageType::Blood => "blood",
        DamageType::Poison => "poison",
        DamageType::Acid => "acid",
    }
}

/// Tooltip card body rendered inside an `on_hover_ui` closure. Header
/// is rarity-colored name; body lists each nonzero stat / resist; footer
/// shows soulbound + weight. Hidden stats (e.g. crit = 0) are skipped
/// so common items don't show a wall of "0.0" placeholders.
fn item_tooltip(ui: &mut egui::Ui, resolved: &ResolvedItem) {
    ui.set_max_width(300.0);

    ui.label(
        egui::RichText::new(&resolved.display_name)
            .strong()
            .size(14.0)
            .color(rarity_color(resolved.rarity)),
    );
    ui.label(
        egui::RichText::new(format!(
            "{} · {}",
            rarity_label(resolved.rarity),
            kind_label(&resolved.kind)
        ))
        .size(10.0)
        .color(egui::Color32::from_rgb(170, 170, 170)),
    );

    let s = &resolved.stats;
    let mut stat_lines: Vec<String> = Vec::new();
    if s.armor > 0 {
        stat_lines.push(format!("{} armor", s.armor));
    }
    if s.weapon_max_dmg > 0.0 {
        stat_lines.push(format!(
            "Weapon Damage {:.0}–{:.0}",
            s.weapon_min_dmg, s.weapon_max_dmg
        ));
    }
    if s.crit_rating_pct > 0.0 {
        stat_lines.push(format!("+{:.0}% crit", s.crit_rating_pct));
    }
    if s.haste_rating_pct > 0.0 {
        stat_lines.push(format!("+{:.0}% haste", s.haste_rating_pct));
    }
    if s.block_chance_pct > 0.0 {
        stat_lines.push(format!("+{:.0}% block chance", s.block_chance_pct));
    }
    if s.block_value > 0 {
        stat_lines.push(format!("{} block value", s.block_value));
    }
    if s.mp5.abs() > 0.01 {
        if s.mp5 < 0.0 {
            stat_lines.push(format!("{:.1} mp5 (upkeep drain)", s.mp5));
        } else {
            stat_lines.push(format!("+{:.1} mp5", s.mp5));
        }
    }
    if s.fortune_pct > 0.0 {
        stat_lines.push(format!("+{:.0}% fortune", s.fortune_pct));
    }
    if !stat_lines.is_empty() {
        ui.separator();
        for line in &stat_lines {
            ui.label(
                egui::RichText::new(line)
                    .size(12.0)
                    .color(egui::Color32::from_rgb(230, 230, 230)),
            );
        }
    }

    let mut resist_lines: Vec<String> = Vec::new();
    for dt in DamageType::ALL {
        let v = s.resists[dt.index()];
        if v.abs() < 0.1 {
            continue;
        }
        if v >= 0.0 {
            resist_lines.push(format!("+{:.0} {} resist", v, damage_type_label(dt)));
        } else {
            resist_lines.push(format!("{:.0} {} resist", v, damage_type_label(dt)));
        }
    }
    if !resist_lines.is_empty() {
        ui.separator();
        for line in &resist_lines {
            ui.label(
                egui::RichText::new(line)
                    .size(11.0)
                    .color(egui::Color32::from_rgb(180, 210, 255)),
            );
        }
    }

    if resolved.soulbound {
        ui.separator();
        ui.label(
            egui::RichText::new("Soulbound")
                .italics()
                .color(egui::Color32::from_rgb(220, 180, 60)),
        );
    }

    ui.separator();
    ui.label(
        egui::RichText::new(format!("{:.1} kg", resolved.weight_kg))
            .size(10.0)
            .color(egui::Color32::from_rgb(140, 140, 140)),
    );
}

/// Given an item kind, pick the first slot it's allowed to occupy.
/// Naive — for weapons picks MainHand regardless of grip; offhand
/// handling etc. is a v2 feature (proper class-aware auto-equip).
fn default_slot_for(kind: &ItemKind) -> Option<EquipSlot> {
    match kind {
        ItemKind::Weapon { grip, .. } => match grip {
            WeaponGrip::Light | WeaponGrip::OneHanded | WeaponGrip::TwoHanded => {
                Some(EquipSlot::MainHand)
            }
        },
        ItemKind::Shield { .. } => Some(EquipSlot::OffHand),
        ItemKind::Rune { .. } => Some(EquipSlot::Focus),
        ItemKind::Armor { slot, .. } => match slot.as_str() {
            "head" => Some(EquipSlot::Head),
            "shoulders" => Some(EquipSlot::Shoulders),
            "chest" => Some(EquipSlot::Chest),
            "shirt" => Some(EquipSlot::Shirt),
            "tabard" => Some(EquipSlot::Tabard),
            "back" => Some(EquipSlot::Back),
            "wrists" => Some(EquipSlot::Wrists),
            "hands" => Some(EquipSlot::Hands),
            "waist" => Some(EquipSlot::Waist),
            "legs" => Some(EquipSlot::Legs),
            "feet" => Some(EquipSlot::Feet),
            "neck" => Some(EquipSlot::Neck),
            "ring" => Some(EquipSlot::Ring1),
            "trinket" => Some(EquipSlot::Trinket1),
            _ => None,
        },
        _ => None, // Consumables, reagents, materials, etc. don't equip.
    }
}

// ─── slot widgets ──────────────────────────────────────────────────────────

/// One inventory slot — a square icon cell. Left-click dispatches by
/// item kind:
///
/// * `Consumable` → send `ConsumeItemRequest`. Server applies the
///   effect and decrements one charge.
/// * Gear (weapon / armor / shield / rune) → send `EquipRequest` to
///   the default slot for that kind.
/// * Everything else (reagents, materials, quest items) → no action.
///
/// Right-click on a `Consumable` opens a context menu to bind it to a
/// belt slot (keys 7/8/9/0). Hover always shows the tooltip card.
#[allow(clippy::too_many_arguments)]
fn inventory_slot_ui(
    ui: &mut egui::Ui,
    idx: usize,
    slot: Option<&InventorySlotEntry>,
    content: &ContentRegistry,
    icons: &ItemIconCache,
    equip_tx: &mut Query<&mut MessageSender<EquipRequest>, With<Client>>,
    consume_tx: &mut Query<&mut MessageSender<ConsumeItemRequest>, With<Client>>,
    bind_tx: &mut Query<&mut MessageSender<BindBeltSlotRequest>, With<Client>>,
) {
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(CELL_SIZE, CELL_SIZE), egui::Sense::click());
    let painter = ui.painter().clone();

    let Some(entry) = slot else {
        paint_empty_slot(&painter, rect, "");
        return;
    };

    let resolved = match content.resolve(&entry.instance) {
        Ok(r) => r,
        Err(_) => {
            // Unknown base_id: paint a red error square so the dev sees
            // it and the slot is still hover-debuggable.
            painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(80, 20, 20));
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "?",
                egui::FontId::proportional(18.0),
                egui::Color32::from_rgb(240, 200, 200),
            );
            resp.on_hover_text(format!("unresolved base: {}", entry.instance.base_id));
            return;
        }
    };

    paint_item_cell(
        &painter,
        rect,
        &entry.instance.base_id,
        &resolved.kind,
        resolved.rarity,
        icons.get(&entry.instance.base_id),
        entry.count,
    );

    let resp = resp.on_hover_ui(|ui| item_tooltip(ui, &resolved));

    // Right-click context menu on Consumables: bind to belt slot.
    if matches!(resolved.kind, ItemKind::Consumable { .. }) {
        let instance = entry.instance.clone();
        resp.clone().context_menu(|ui| {
            ui.label(
                egui::RichText::new("Bind to belt slot")
                    .size(10.0)
                    .color(egui::Color32::from_gray(180)),
            );
            ui.separator();
            for i in 0..BELT_SLOTS {
                let key = match i {
                    0 => "7",
                    1 => "8",
                    2 => "9",
                    _ => "0",
                };
                if ui.button(format!("Slot {} ({key})", i + 1)).clicked() {
                    if let Ok(mut sender) = bind_tx.single_mut() {
                        let _ = sender.send::<Channel1>(BindBeltSlotRequest {
                            slot_idx: i as u8,
                            instance: instance.clone(),
                        });
                    }
                    ui.close();
                }
            }
        });
    }

    if resp.clicked() {
        match &resolved.kind {
            ItemKind::Consumable { .. } => {
                if let Ok(mut sender) = consume_tx.single_mut() {
                    let _ = sender.send::<Channel1>(ConsumeItemRequest {
                        inventory_idx: idx as u32,
                    });
                }
            }
            _ => {
                if let Some(target) = default_slot_for(&resolved.kind) {
                    if let Ok(mut sender) = equip_tx.single_mut() {
                        let _ = sender.send::<Channel1>(EquipRequest {
                            slot: target,
                            inventory_idx: idx as u32,
                        });
                    }
                }
            }
        }
    }
}

/// One paper-doll slot — square icon cell. Empty slots show the slot's
/// 3–4 char abbreviation (HEAD / CHST / MH / FOC) so the player can
/// scan the layout. Filled slots show the icon + rarity border;
/// hover for tooltip; right-click to unequip.
fn doll_slot_ui(
    ui: &mut egui::Ui,
    slot: EquipSlot,
    equipped: Option<&ItemInstance>,
    content: &ContentRegistry,
    icons: &ItemIconCache,
    unequip_tx: &mut Query<&mut MessageSender<UnequipRequest>, With<Client>>,
) {
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(CELL_SIZE, CELL_SIZE), egui::Sense::click());
    let painter = ui.painter().clone();

    let Some(inst) = equipped else {
        paint_empty_slot(&painter, rect, slot_abbrev(slot));
        resp.on_hover_text(slot_short_label(slot));
        return;
    };

    let resolved = match content.resolve(inst) {
        Ok(r) => r,
        Err(_) => {
            painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(80, 20, 20));
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "?",
                egui::FontId::proportional(18.0),
                egui::Color32::from_rgb(240, 200, 200),
            );
            resp.on_hover_text(format!("unresolved base: {}", inst.base_id));
            return;
        }
    };

    paint_item_cell(
        &painter,
        rect,
        &inst.base_id,
        &resolved.kind,
        resolved.rarity,
        icons.get(&inst.base_id),
        1, // doll slots never stack
    );

    let resp = resp.on_hover_ui(|ui| {
        item_tooltip(ui, &resolved);
        ui.separator();
        ui.label(
            egui::RichText::new("Right-click to unequip")
                .color(egui::Color32::from_rgb(140, 140, 140))
                .size(9.0),
        );
    });

    if resp.secondary_clicked() {
        if let Ok(mut sender) = unequip_tx.single_mut() {
            let _ = sender.send::<Channel1>(UnequipRequest { slot });
        }
    }
}

// ─── main window ───────────────────────────────────────────────────────────

/// Armor slots in the left doll column, top-to-bottom.
const DOLL_LEFT_SLOTS: [EquipSlot; 11] = [
    EquipSlot::Head,
    EquipSlot::Shoulders,
    EquipSlot::Chest,
    EquipSlot::Shirt,
    EquipSlot::Tabard,
    EquipSlot::Back,
    EquipSlot::Wrists,
    EquipSlot::Hands,
    EquipSlot::Waist,
    EquipSlot::Legs,
    EquipSlot::Feet,
];

/// Jewelry + weapons in the right doll column. Lined up so weapons
/// sit visually paired at the bottom.
const DOLL_RIGHT_SLOTS: [EquipSlot; 9] = [
    EquipSlot::Neck,
    EquipSlot::Ring1,
    EquipSlot::Ring2,
    EquipSlot::Trinket1,
    EquipSlot::Trinket2,
    EquipSlot::MainHand,
    EquipSlot::OffHand,
    EquipSlot::Ranged,
    EquipSlot::Focus,
];

const INVENTORY_COLS: usize = 6;

#[allow(clippy::too_many_arguments)]
fn draw_inventory_window(
    mut contexts: EguiContexts,
    open: Res<InventoryWindowOpen>,
    inv: Res<OwnInventory>,
    eq: Res<OwnEquipped>,
    content: Option<Res<ClientContent>>,
    icons: Res<ItemIconCache>,
    mut equip_tx: Query<&mut MessageSender<EquipRequest>, With<Client>>,
    mut unequip_tx: Query<&mut MessageSender<UnequipRequest>, With<Client>>,
    mut consume_tx: Query<&mut MessageSender<ConsumeItemRequest>, With<Client>>,
    mut bind_tx: Query<&mut MessageSender<BindBeltSlotRequest>, With<Client>>,
) {
    if !open.0 {
        return;
    }
    let Some(content) = content else { return };
    let Ok(ctx) = contexts.ctx_mut() else { return };

    egui::Window::new("Inventory & Equipment")
        .default_size(egui::vec2(780.0, 560.0))
        .resizable(true)
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new(
                    "Left-click a potion to use  ·  Right-click a potion to bind to belt slot (keys 7/8/9/0)  ·  Left-click gear to equip  ·  Right-click an equipped slot to unequip  ·  I to close",
                )
                .size(10.0)
                .color(egui::Color32::from_rgb(150, 150, 150)),
            );
            ui.separator();
            ui.horizontal_top(|ui| {
                // ── Inventory (left) ────────────────────────────────────
                ui.vertical(|ui| {
                    ui.heading(format!(
                        "Inventory ({}/{})",
                        inv.slots.iter().filter(|s| s.is_some()).count(),
                        inv.capacity
                    ));
                    egui::ScrollArea::vertical()
                        .max_height(480.0)
                        .id_salt("inv_scroll")
                        .show(ui, |ui| {
                            egui::Grid::new("inv_grid")
                                .num_columns(INVENTORY_COLS)
                                .spacing([4.0, 4.0])
                                .show(ui, |ui| {
                                    for idx in 0..inv.capacity as usize {
                                        let slot_ref = inv.slots.get(idx).and_then(|s| s.as_ref());
                                        inventory_slot_ui(
                                            ui,
                                            idx,
                                            slot_ref,
                                            &content.0,
                                            &icons,
                                            &mut equip_tx,
                                            &mut consume_tx,
                                            &mut bind_tx,
                                        );
                                        if idx % INVENTORY_COLS == INVENTORY_COLS - 1 {
                                            ui.end_row();
                                        }
                                    }
                                });
                        });
                });

                ui.separator();

                // ── Paper doll (right) ──────────────────────────────────
                ui.vertical(|ui| {
                    ui.heading("Equipment");
                    ui.add_space(4.0);
                    ui.horizontal_top(|ui| {
                        ui.vertical(|ui| {
                            for slot in DOLL_LEFT_SLOTS {
                                doll_slot_ui(
                                    ui,
                                    slot,
                                    eq.slots.get(&slot),
                                    &content.0,
                                    &icons,
                                    &mut unequip_tx,
                                );
                            }
                        });
                        ui.add_space(8.0);
                        ui.vertical(|ui| {
                            for slot in DOLL_RIGHT_SLOTS {
                                doll_slot_ui(
                                    ui,
                                    slot,
                                    eq.slots.get(&slot),
                                    &content.0,
                                    &icons,
                                    &mut unequip_tx,
                                );
                            }
                        });
                    });
                });
            });
        });
}
