//! Item icon cache + shared cell painter.
//!
//! Mirrors the ability-icon pipeline in `hotbar_ui.rs`. One `ItemIconCache`
//! resource holds `base_id → egui::TextureHandle`. Icons are loaded
//! lazily off disk from `<workspace>/icons/items/<base_id>.png` whenever
//! a new base_id appears in any visible item source (inventory, paper
//! doll, consumable belt, loot window).
//!
//! Missing icons are not an error: the shared `paint_item_cell` helper
//! falls back to a kind-colored square with a short text label, so the
//! UI layout stays uniform whether art has shipped for a given base or
//! not. This is the same fallback shape the hotbar uses while ability
//! icons are still being authored.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};

use vaern_items::{ItemKind, Rarity};

use crate::belt_ui::OwnConsumableBelt;
use crate::inventory_ui::{OwnEquipped, OwnInventory};
use crate::loot_ui::LootWindow;
use crate::menu::AppState;

pub struct ItemIconsPlugin;

impl Plugin for ItemIconsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ItemIconCache>().add_systems(
            EguiPrimaryContextPass,
            load_pending_item_icons.run_if(in_state(AppState::InGame)),
        );
    }
}

/// Cache of `base_id → loaded egui texture`. `failed` records ids we
/// already tried and couldn't load, so we don't hammer the disk every
/// frame for missing PNGs (most bases will be missing until the SDXL
/// pass lands).
#[derive(Resource, Default)]
pub struct ItemIconCache {
    pub textures: HashMap<String, egui::TextureHandle>,
    pub failed: HashSet<String>,
}

impl ItemIconCache {
    pub fn get(&self, base_id: &str) -> Option<&egui::TextureHandle> {
        self.textures.get(base_id)
    }
}

/// Absolute path to `<workspace>/icons/items/`. Mirrors `icons_root()`
/// in hotbar_ui.rs but scoped to the items subdirectory so the ability
/// + item icon sets don't collide.
fn item_icons_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../icons/items")
}

/// Walk every visible item source and lazy-load any base_id that
/// hasn't been touched yet. Cheap: contains_key + contains lookups
/// short-circuit, and the actual disk read only fires on first sight.
fn load_pending_item_icons(
    inv: Res<OwnInventory>,
    eq: Res<OwnEquipped>,
    belt: Res<OwnConsumableBelt>,
    loot: Res<LootWindow>,
    mut icons: ResMut<ItemIconCache>,
    mut contexts: EguiContexts,
) {
    let Ok(ctx) = contexts.ctx_mut() else { return };
    let root = item_icons_root();

    // Build a single dedup'd set of base_ids visible this frame.
    let mut wanted: HashSet<&str> = HashSet::new();
    for slot in inv.slots.iter().flatten() {
        wanted.insert(slot.instance.base_id.as_str());
    }
    for inst in eq.slots.values() {
        wanted.insert(inst.base_id.as_str());
    }
    for inst in belt.slots.iter().flatten() {
        wanted.insert(inst.base_id.as_str());
    }
    for entry in &loot.slots {
        wanted.insert(entry.instance.base_id.as_str());
    }

    for base_id in wanted {
        if icons.textures.contains_key(base_id) || icons.failed.contains(base_id) {
            continue;
        }
        let path = root.join(format!("{base_id}.png"));
        match load_icon_from_disk(&path) {
            Ok(color_image) => {
                let handle = ctx.load_texture(
                    format!("item-icon:{base_id}"),
                    color_image,
                    egui::TextureOptions::LINEAR,
                );
                icons.textures.insert(base_id.to_string(), handle);
            }
            Err(_) => {
                icons.failed.insert(base_id.to_string());
            }
        }
    }
}

/// Target atlas size for cached item icons. SDXL writes 1024² but cells
/// render at 48px — keep 128² (2.6× oversample) so high-DPI monitors
/// and future larger cells stay crisp while cutting per-icon VRAM from
/// ~4 MB to ~64 KB. Lanczos3 downsample preserves the painted detail.
const ICON_ATLAS_SIZE: u32 = 128;

fn load_icon_from_disk(path: &Path) -> Result<egui::ColorImage, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let img = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
    let img = img.resize_exact(
        ICON_ATLAS_SIZE,
        ICON_ATLAS_SIZE,
        image::imageops::FilterType::Lanczos3,
    );
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Ok(egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()))
}

// ─── shared cell painter ────────────────────────────────────────────────────

pub const CELL_SIZE: f32 = 48.0;
pub const CELL_CORNER: f32 = 4.0;

/// WoW-standard rarity palette. Mirrors `rarity_color` in inventory_ui.rs;
/// re-exported here so all icon sites pull from one source.
pub fn rarity_color(r: Rarity) -> egui::Color32 {
    match r {
        Rarity::Junk => egui::Color32::from_rgb(120, 120, 120),
        Rarity::Common => egui::Color32::from_rgb(240, 240, 240),
        Rarity::Uncommon => egui::Color32::from_rgb(30, 220, 30),
        Rarity::Rare => egui::Color32::from_rgb(50, 130, 255),
        Rarity::Epic => egui::Color32::from_rgb(170, 70, 230),
        Rarity::Legendary => egui::Color32::from_rgb(255, 145, 10),
    }
}

/// Background tint for a missing-icon cell. Disambiguates item kind at
/// a glance even when the PNG hasn't been authored — weapons read red,
/// armor blue, consumables green, etc.
pub fn kind_bg_color(kind: &ItemKind) -> egui::Color32 {
    let [r, g, b] = match kind {
        ItemKind::Weapon { .. } => [120, 40, 40],
        ItemKind::Armor { .. } => [50, 70, 110],
        ItemKind::Shield { .. } => [70, 90, 130],
        ItemKind::Rune { .. } => [110, 60, 140],
        ItemKind::Consumable { .. } => [50, 100, 60],
        ItemKind::Reagent => [100, 90, 50],
        ItemKind::Trinket => [90, 80, 110],
        ItemKind::Quest => [150, 130, 50],
        ItemKind::Material => [80, 70, 60],
        ItemKind::Currency => [140, 120, 50],
        ItemKind::Misc => [70, 70, 70],
    };
    egui::Color32::from_rgb(r, g, b)
}

/// Short label for the missing-icon fallback. Pulled from base_id so
/// distinct bases of the same kind read differently. e.g. "iron_long
/// sword" → "ILS"; "minor_healing_potion" → "MHP".
pub fn fallback_label(base_id: &str) -> String {
    let initials: String = base_id
        .split('_')
        .filter(|w| !w.is_empty())
        .filter_map(|w| w.chars().next())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if initials.is_empty() {
        "?".into()
    } else if initials.len() <= 4 {
        initials
    } else {
        initials.chars().take(4).collect()
    }
}

/// Paint one item cell. Always draws a square: kind-tinted background,
/// either the loaded icon image or a centered fallback label, and a
/// rarity-colored border. Stack count overlays bottom-right when > 1.
///
/// Caller has already allocated `rect` and handles hover/click on the
/// returned `Response`. This helper only paints.
pub fn paint_item_cell(
    painter: &egui::Painter,
    rect: egui::Rect,
    base_id: &str,
    kind: &ItemKind,
    rarity: Rarity,
    icon: Option<&egui::TextureHandle>,
    stack_count: u32,
) {
    // 1. Background — kind tint when no icon, dark plate when icon present
    //    (so transparent edges of the icon don't show clear-color).
    let bg = if icon.is_some() {
        egui::Color32::from_rgba_unmultiplied(0, 0, 0, 200)
    } else {
        kind_bg_color(kind)
    };
    painter.rect_filled(rect, CELL_CORNER, bg);

    // 2. Icon or fallback glyph.
    let inner = rect.shrink(2.0);
    match icon {
        Some(handle) => {
            painter.image(
                handle.id(),
                inner,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        None => {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                fallback_label(base_id),
                egui::FontId::proportional(14.0),
                egui::Color32::from_rgb(235, 235, 235),
            );
        }
    }

    // 3. Rarity border on top.
    painter.rect_stroke(
        rect,
        CELL_CORNER,
        egui::Stroke::new(2.0, rarity_color(rarity)),
        egui::StrokeKind::Outside,
    );

    // 4. Stack count bottom-right with a dark backdrop for legibility.
    if stack_count > 1 {
        let pos = rect.max - egui::vec2(4.0, 3.0);
        let txt = format!("{stack_count}");
        let approx_w = (txt.len() as f32) * 7.0 + 4.0;
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(pos.x - approx_w, pos.y - 14.0),
                egui::pos2(pos.x + 1.0, pos.y + 1.0),
            ),
            2.0,
            egui::Color32::from_rgba_unmultiplied(0, 0, 0, 180),
        );
        painter.text(
            pos,
            egui::Align2::RIGHT_BOTTOM,
            txt,
            egui::FontId::proportional(12.0),
            egui::Color32::from_rgb(255, 235, 180),
        );
    }
}

/// Empty-cell variant: dim square with the slot-name abbreviation.
/// Used for empty paper-doll slots so the player still reads which
/// slot is which.
pub fn paint_empty_slot(painter: &egui::Painter, rect: egui::Rect, label: &str) {
    painter.rect_filled(
        rect,
        CELL_CORNER,
        egui::Color32::from_rgba_unmultiplied(20, 22, 28, 200),
    );
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(11.0),
        egui::Color32::from_rgb(95, 95, 110),
    );
    painter.rect_stroke(
        rect,
        CELL_CORNER,
        egui::Stroke::new(1.0, egui::Color32::from_gray(60)),
        egui::StrokeKind::Outside,
    );
}
