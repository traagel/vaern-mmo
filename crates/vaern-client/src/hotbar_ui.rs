//! Hotbar + spellbook UI (egui).
//!
//! The `Hotbar` resource is populated by draining `MessageReceiver<HotbarSnapshot>`
//! after server spawns the player. Cooldowns are tracked locally (optimistic)
//! on keypress; if the server rejects a cast, the timer still ticks but nothing
//! else fires — minor UX cost that avoids extra cooldown replication.
//!
//! Icons are loaded on-demand from the repo's `icons/` directory, one PNG per
//! ability id (e.g. `arcana.damage.50.fire.firebolt.png`). Decoded egui
//! textures are cached in `IconCache`.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use lightyear::prelude::client::*;
use lightyear::prelude::*;
use vaern_protocol::{HotbarSlotInfo, HotbarSnapshot};

use crate::menu::AppState;

pub struct HotbarUiPlugin;

impl Plugin for HotbarUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Hotbar>()
            .init_resource::<SpellbookState>()
            .insert_resource(IconCache::default())
            .add_message::<CastAttempted>()
            .add_systems(
                Update,
                (
                    ingest_hotbar_snapshot,
                    track_keypress_cooldowns,
                    toggle_spellbook_hotkey,
                    heartbeat_debug,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                EguiPrimaryContextPass,
                load_pending_icons.run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                EguiPrimaryContextPass,
                hotbar_ui_system.run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                EguiPrimaryContextPass,
                spellbook_ui_system.run_if(in_state(AppState::InGame)),
            )
            .add_systems(OnExit(AppState::InGame), reset_hotbar);
    }
}

/// Update-schedule heartbeat that proves HotbarUiPlugin's run-condition fires.
/// If you see this log but no "[hotbar_ui] system running", the problem is
/// scoped to EguiPrimaryContextPass specifically.
fn heartbeat_debug(
    time: Res<Time>,
    mut timer: Local<f32>,
    hotbar: Res<Hotbar>,
) {
    *timer += time.delta_secs();
    if *timer >= 3.0 {
        *timer = 0.0;
        info!(
            "[hotbar_ui:heartbeat] plugin Update systems alive; slots in resource = {}",
            hotbar.slots.len()
        );
    }
}

// ─── resources ──────────────────────────────────────────────────────────────

#[derive(Resource, Default, Debug)]
pub struct Hotbar {
    pub slots: Vec<HotbarSlotEntry>,
}

/// Emitted by `track_keypress_cooldowns` when a hotbar key is pressed AND
/// both the slot's cooldown and the global cooldown are clear. Downstream
/// systems (`handle_ability_input`, `spawn_cast_flashes`) read this instead
/// of re-checking cooldown state themselves.
#[derive(Message, Debug, Clone, Copy)]
pub struct CastAttempted {
    pub slot_idx: u8,
}

#[derive(Debug, Clone)]
pub struct HotbarSlotEntry {
    pub info: HotbarSlotInfo,
    /// Seconds of cooldown remaining (local, optimistic). 0.0 = ready.
    pub cooldown_remaining: f32,
}

#[derive(Resource, Default, Debug)]
struct SpellbookState {
    open: bool,
}

/// Cache of ability-id → loaded egui texture. Also tracks ids we attempted
/// but failed to load (missing icon file) so we don't retry every frame.
#[derive(Resource, Default)]
pub struct IconCache {
    pub textures: HashMap<String, egui::TextureHandle>,
    pub failed: HashSet<String>,
}

/// Absolute path to the repo's `icons/` dir, relative to CARGO_MANIFEST_DIR.
fn icons_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../icons")
}

// ─── systems ────────────────────────────────────────────────────────────────

fn ingest_hotbar_snapshot(
    mut receivers: Query<&mut MessageReceiver<HotbarSnapshot>, With<Client>>,
    mut hotbar: ResMut<Hotbar>,
) {
    let Ok(mut rx) = receivers.single_mut() else { return };
    for snap in rx.receive() {
        hotbar.slots = snap
            .slots
            .into_iter()
            .map(|info| HotbarSlotEntry {
                info,
                cooldown_remaining: 0.0,
            })
            .collect();
        info!("hotbar snapshot: {} slots", hotbar.slots.len());
    }
}

/// Tick local cooldowns. On a hotbar keypress / mouse-button press where the
/// slot's cooldown is ready, emit `CastAttempted` and arm the slot CD. No
/// GCD — New World-style combat lets abilities interleave freely.
///
/// Slot mapping:
///   Digit1..=Digit6 → slots 0..=5 (visible hotbar)
///   MouseLeft       → slot 6      (light auto-attack)
///   MouseRight      → slot 7      (heavy auto-attack)
fn track_keypress_cooldowns(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut hotbar: ResMut<Hotbar>,
    mut attempts: MessageWriter<CastAttempted>,
) {
    let dt = time.delta_secs();
    for slot in &mut hotbar.slots {
        if slot.cooldown_remaining > 0.0 {
            slot.cooldown_remaining = (slot.cooldown_remaining - dt).max(0.0);
        }
    }
    let pressed_slot = if keys.just_pressed(KeyCode::Digit1) {
        Some(0usize)
    } else if keys.just_pressed(KeyCode::Digit2) {
        Some(1)
    } else if keys.just_pressed(KeyCode::Digit3) {
        Some(2)
    } else if keys.just_pressed(KeyCode::Digit4) {
        Some(3)
    } else if keys.just_pressed(KeyCode::Digit5) {
        Some(4)
    } else if keys.just_pressed(KeyCode::Digit6) {
        Some(5)
    } else if mouse.just_pressed(MouseButton::Left) {
        Some(6) // light auto-attack
    } else if mouse.just_pressed(MouseButton::Right) {
        Some(7) // heavy auto-attack
    } else {
        None
    };
    let Some(i) = pressed_slot else { return };
    let Some(slot) = hotbar.slots.get_mut(i) else { return };
    if slot.cooldown_remaining > 0.0 {
        return;
    }
    slot.cooldown_remaining = slot.info.cooldown_secs;
    attempts.write(CastAttempted { slot_idx: i as u8 });
}

fn toggle_spellbook_hotkey(
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<SpellbookState>,
) {
    if keys.just_pressed(KeyCode::KeyK) {
        state.open = !state.open;
    }
}

fn reset_hotbar(
    mut hotbar: ResMut<Hotbar>,
    mut spell: ResMut<SpellbookState>,
    mut icons: ResMut<IconCache>,
) {
    hotbar.slots.clear();
    spell.open = false;
    // Keep textures cached across logouts — they're expensive to reload and
    // the same character likely uses the same icons. Clear only the failed
    // set so a retry can happen on a fresh run.
    icons.failed.clear();
}

/// For each hotbar slot whose `ability_id` is not yet in `IconCache`, try to
/// load `icons/<id>.png` → decode → register with egui. Runs once-per-slot
/// because we check `contains_key` / `failed` before reading.
fn load_pending_icons(
    hotbar: Res<Hotbar>,
    mut icons: ResMut<IconCache>,
    mut contexts: EguiContexts,
) {
    if hotbar.slots.is_empty() {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else { return };
    let root = icons_root();
    for slot in &hotbar.slots {
        let id = &slot.info.ability_id;
        if icons.textures.contains_key(id) || icons.failed.contains(id) {
            continue;
        }
        let path = root.join(format!("{}.png", id));
        match load_icon_from_disk(&path) {
            Ok(color_image) => {
                let handle = ctx.load_texture(
                    format!("icon:{id}"),
                    color_image,
                    egui::TextureOptions::LINEAR,
                );
                icons.textures.insert(id.clone(), handle);
            }
            Err(e) => {
                debug!("icon load failed for {}: {}", path.display(), e);
                icons.failed.insert(id.clone());
            }
        }
    }
}

fn load_icon_from_disk(path: &Path) -> Result<egui::ColorImage, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let img = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Ok(egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()))
}

// ─── UI ─────────────────────────────────────────────────────────────────────

const SLOT_SIZE: f32 = 64.0;
const SLOT_PADDING: f32 = 6.0;

fn hotbar_ui_system(
    mut contexts: EguiContexts,
    hotbar: Res<Hotbar>,
    icons: Res<IconCache>,
    // Own-player Transform (Predicted copy carries it). Used to compute the
    // distance to the current target so we can dim out-of-range slots.
    own_player: Query<(&Transform, Option<&vaern_combat::Target>), With<crate::shared::Player>>,
    transforms: Query<&Transform>,
    mut first_run: Local<bool>,
) {
    let ctx = match contexts.ctx_mut() {
        Ok(c) => c,
        Err(e) => {
            if !*first_run {
                info!("[hotbar_ui] ctx_mut failed: {e:?}");
                *first_run = true;
            }
            return;
        }
    };
    if !*first_run {
        info!("[hotbar_ui] system running; slots={}", hotbar.slots.len());
        *first_run = true;
    }

    // Use Window instead of Area — Window is draw-order-guaranteed on top
    // and resilient to anchor / layer quirks. Configured as non-movable /
    // no title so it still reads as a HUD element.
    egui::Window::new("hotbar")
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .movable(false)
        .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -24.0))
        .frame(
            egui::Frame::default()
                .fill(egui::Color32::from_rgba_unmultiplied(0, 0, 0, 200))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(90)))
                .inner_margin(egui::Margin::same(SLOT_PADDING as i8))
                .corner_radius(6.0),
        )
        .show(ctx, |ui| {
            if hotbar.slots.is_empty() {
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new("waiting for hotbar from server…")
                        .italics()
                        .color(egui::Color32::from_gray(180)),
                );
                ui.add_space(6.0);
                return;
            }
            // Current target distance (if any). None means no target selected
            // OR we don't have a player Transform yet — out-of-range dim stays off.
            let target_dist = own_player.single().ok().and_then(|(ptf, target)| {
                let target_e = target?.0;
                let ttf = transforms.get(target_e).ok()?;
                Some(ptf.translation.distance(ttf.translation))
            });

            ui.horizontal(|ui| {
                // Only the keyboard-bound slots (0..=5) are rendered as
                // buttons; slots 6/7 are the light/heavy auto-attacks driven
                // by LMB/RMB and don't occupy hotbar real-estate.
                for (i, slot) in hotbar.slots.iter().enumerate().take(6) {
                    let texture = icons.textures.get(&slot.info.ability_id);
                    let out_of_range = match (target_dist, slot.info.shape.as_str()) {
                        // Self-AoE ignores range — caster is the origin.
                        (_, "aoe_on_self") => false,
                        // Cone / Line / Projectile need a target for aim direction
                        // AND the target must be within reach.
                        (Some(d), "cone" | "line" | "projectile") => d > slot.info.range,
                        // Target / AoE-on-target: same rule.
                        (Some(d), _) => d > slot.info.range,
                        // No target selected — leave bright (UX: avoid gray
                        // hotbar in open world).
                        (None, _) => false,
                    };
                    draw_slot(ui, i, slot, texture, out_of_range);
                }
            });
        });

    // Debug marker as a Window too — bright red, top-center. If neither the
    // hotbar Window nor this one appear, egui rendering itself is broken.
    egui::Window::new("hotbar_debug")
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .movable(false)
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 60.0))
        .frame(
            egui::Frame::default()
                .fill(egui::Color32::from_rgb(220, 40, 40))
                .inner_margin(egui::Margin::symmetric(10, 4)),
        )
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new(format!(
                    "HOTBAR DEBUG · slots={}",
                    hotbar.slots.len()
                ))
                .color(egui::Color32::WHITE)
                .strong(),
            );
        });
}

fn draw_slot(
    ui: &mut egui::Ui,
    index: usize,
    slot: &HotbarSlotEntry,
    icon: Option<&egui::TextureHandle>,
    out_of_range: bool,
) {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(SLOT_SIZE, SLOT_SIZE),
        egui::Sense::hover(),
    );
    let painter = ui.painter();
    let school_color = school_egui_color(&slot.info.school);
    let base_bg = egui::Color32::from_rgba_unmultiplied(
        (school_color[0] as f32 * 0.35) as u8,
        (school_color[1] as f32 * 0.35) as u8,
        (school_color[2] as f32 * 0.35) as u8,
        220,
    );
    // Slot background (always drawn, in case icon has transparency)
    painter.rect_filled(rect, 4.0, base_bg);

    // Icon if available — inset slightly so border + hotkey remain legible.
    let inner = rect.shrink(3.0);
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
            // Icon not loaded: fall back to name text centered in the slot.
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                truncate_name(&slot.info.name, 10),
                egui::FontId::proportional(12.0),
                egui::Color32::WHITE,
            );
        }
    }

    // Out-of-range dim: dark translucent overlay + red-tinted border so the
    // slot reads as unavailable without hiding the icon.
    if out_of_range {
        painter.rect_filled(
            rect,
            4.0,
            egui::Color32::from_rgba_unmultiplied(0, 0, 0, 150),
        );
    }

    // Border on top of icon — red if out of range, gray otherwise.
    let border_color = if out_of_range {
        egui::Color32::from_rgb(200, 60, 60)
    } else {
        egui::Color32::from_gray(120)
    };
    painter.rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(1.0, border_color),
        egui::StrokeKind::Outside,
    );

    // School swatch strip along the left edge
    let swatch = egui::Rect::from_min_size(rect.min, egui::vec2(3.0, rect.height()));
    painter.rect_filled(
        swatch,
        0.0,
        egui::Color32::from_rgba_unmultiplied(
            school_color[0],
            school_color[1],
            school_color[2],
            240,
        ),
    );

    // Hotkey in top-left, with a dark backdrop so it stays readable on bright icons
    let key_pos = rect.min + egui::vec2(6.0, 4.0);
    let key_text = format!("{}", index + 1);
    painter.rect_filled(
        egui::Rect::from_min_size(key_pos - egui::vec2(2.0, 2.0), egui::vec2(14.0, 16.0)),
        2.0,
        egui::Color32::from_rgba_unmultiplied(0, 0, 0, 180),
    );
    painter.text(
        key_pos,
        egui::Align2::LEFT_TOP,
        key_text,
        egui::FontId::monospace(13.0),
        egui::Color32::from_rgb(255, 230, 150),
    );

    // Resource cost in bottom-right
    if slot.info.resource_cost > 0.0 {
        painter.text(
            rect.max - egui::vec2(6.0, 4.0),
            egui::Align2::RIGHT_BOTTOM,
            format!("{:.0}", slot.info.resource_cost),
            egui::FontId::proportional(11.0),
            egui::Color32::from_rgb(120, 180, 230),
        );
    }

    // Cooldown sweep — dark overlay shrinking bottom-up
    if slot.cooldown_remaining > 0.0 && slot.info.cooldown_secs > 0.0 {
        let frac = (slot.cooldown_remaining / slot.info.cooldown_secs).clamp(0.0, 1.0);
        let overlay_height = rect.height() * frac;
        let overlay = egui::Rect::from_min_size(
            egui::pos2(rect.min.x, rect.max.y - overlay_height),
            egui::vec2(rect.width(), overlay_height),
        );
        painter.rect_filled(
            overlay,
            0.0,
            egui::Color32::from_rgba_unmultiplied(0, 0, 0, 160),
        );
        // Remaining-seconds readout
        painter.text(
            rect.center() + egui::vec2(0.0, 14.0),
            egui::Align2::CENTER_CENTER,
            format!("{:.1}s", slot.cooldown_remaining),
            egui::FontId::proportional(11.0),
            egui::Color32::from_rgb(255, 210, 120),
        );
    }

    // Tooltip on hover
    response.on_hover_ui_at_pointer(|ui| {
        ui.set_min_width(260.0);
        ui.label(
            egui::RichText::new(&slot.info.name)
                .size(14.0)
                .strong()
                .color(egui::Color32::from_rgb(
                    school_color[0],
                    school_color[1],
                    school_color[2],
                )),
        );
        ui.label(
            egui::RichText::new(format!(
                "Tier {} · {} · {}",
                slot.info.tier,
                prettify(&slot.info.pillar),
                prettify(&slot.info.category),
            ))
            .italics()
            .color(egui::Color32::from_gray(170)),
        );
        if !slot.info.description.is_empty() {
            ui.separator();
            ui.label(&slot.info.description);
        }
        ui.separator();
        ui.label(format!("School: {}", prettify(&slot.info.school)));
        if !slot.info.damage_type.is_empty() {
            ui.label(format!("Damage type: {}", prettify(&slot.info.damage_type)));
        }
        ui.label(format!("Damage: {:.0}", slot.info.damage));
        if slot.info.cast_secs > 0.0 {
            ui.label(format!("Cast time: {:.1}s", slot.info.cast_secs));
        } else {
            ui.label("Cast time: instant");
        }
        ui.label(format!("Cooldown: {:.1}s", slot.info.cooldown_secs));
        ui.label(format!("Resource cost: {:.0}", slot.info.resource_cost));
        let range_label = match slot.info.shape.as_str() {
            "aoe_on_self" => format!(
                "Self-AoE · radius {:.1}u",
                slot.info.aoe_radius
            ),
            "aoe_on_target" => format!(
                "Range {:.0}u · AoE radius {:.1}u",
                slot.info.range, slot.info.aoe_radius
            ),
            "cone" => format!(
                "Cone {:.0}u · {:.0}° spread",
                slot.info.range,
                slot.info.cone_half_angle_deg * 2.0,
            ),
            "line" => format!(
                "Line {:.0}u · {:.1}u wide",
                slot.info.range, slot.info.line_width,
            ),
            "projectile" => format!(
                "Projectile · {:.0}u @ {:.0}u/s (blockable)",
                slot.info.range, slot.info.projectile_speed,
            ),
            _ => format!("Range {:.0}u", slot.info.range),
        };
        ui.label(if out_of_range {
            egui::RichText::new(format!("{range_label}  (out of range)"))
                .color(egui::Color32::from_rgb(240, 120, 120))
        } else {
            egui::RichText::new(range_label)
        });
        ui.separator();
        ui.label(
            egui::RichText::new(format!("Press {}", index + 1))
                .italics()
                .color(egui::Color32::from_gray(170)),
        );
        ui.label(
            egui::RichText::new(&slot.info.ability_id)
                .size(10.0)
                .monospace()
                .color(egui::Color32::from_gray(120)),
        );
    });
}

fn spellbook_ui_system(
    mut contexts: EguiContexts,
    mut state: ResMut<SpellbookState>,
    hotbar: Res<Hotbar>,
    icons: Res<IconCache>,
) {
    let ctx = match contexts.ctx_mut() {
        Ok(c) => c,
        Err(_) => return,
    };
    if !state.open {
        return;
    }
    egui::Window::new("Spellbook [K]")
        .default_width(420.0)
        .default_pos(egui::pos2(40.0, 80.0))
        .open(&mut state.open)
        .show(ctx, |ui| {
            if hotbar.slots.is_empty() {
                ui.label(
                    egui::RichText::new("(hotbar not yet received from server)")
                        .italics(),
                );
                return;
            }
            ui.label("Your assigned abilities:");
            ui.add_space(6.0);
            for (i, slot) in hotbar.slots.iter().enumerate() {
                let color = school_egui_color(&slot.info.school);
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        if let Some(handle) = icons.textures.get(&slot.info.ability_id) {
                            ui.image((handle.id(), egui::vec2(48.0, 48.0)));
                        } else {
                            let (rect, _) = ui.allocate_exact_size(
                                egui::vec2(48.0, 48.0),
                                egui::Sense::hover(),
                            );
                            ui.painter().rect_filled(
                                rect,
                                4.0,
                                egui::Color32::from_gray(40),
                            );
                        }
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(format!("[{}]", i + 1))
                                        .monospace()
                                        .color(egui::Color32::from_gray(160)),
                                );
                                ui.label(
                                    egui::RichText::new(&slot.info.name)
                                        .strong()
                                        .color(egui::Color32::from_rgb(
                                            color[0], color[1], color[2],
                                        )),
                                );
                                ui.label(
                                    egui::RichText::new(format!("T{}", slot.info.tier))
                                        .small()
                                        .color(egui::Color32::from_gray(160)),
                                );
                            });
                            if !slot.info.description.is_empty() {
                                ui.label(
                                    egui::RichText::new(&slot.info.description)
                                        .color(egui::Color32::from_gray(210)),
                                );
                            }
                            ui.label(format!(
                                "{} · {} · {}",
                                prettify(&slot.info.pillar),
                                prettify(&slot.info.category),
                                prettify(&slot.info.school),
                            ));
                            ui.horizontal(|ui| {
                                ui.label(format!("DMG {:.0}", slot.info.damage));
                                ui.separator();
                                if slot.info.cast_secs > 0.0 {
                                    ui.label(format!("Cast {:.1}s", slot.info.cast_secs));
                                } else {
                                    ui.label("Instant");
                                }
                                ui.separator();
                                ui.label(format!("CD {:.1}s", slot.info.cooldown_secs));
                                ui.separator();
                                ui.label(format!("Cost {:.0}", slot.info.resource_cost));
                            });
                        });
                    });
                });
                ui.add_space(2.0);
            }
        });
}

// ─── helpers ────────────────────────────────────────────────────────────────

/// RGB color for each known school id. Mirrors the in-scene `school_color`
/// in main.rs but returns raw bytes for egui.
fn school_egui_color(school: &str) -> [u8; 3] {
    match school {
        "fire" => [255, 115, 30],
        "frost" => [90, 190, 255],
        "shadow" => [155, 60, 220],
        "light" => [255, 240, 150],
        "lightning" => [220, 220, 255],
        "nature" => [110, 200, 90],
        "earth" => [170, 130, 70],
        "arcane" => [200, 120, 255],
        "devotion" => [255, 220, 160],
        "blood" => [190, 30, 45],
        "blade" | "spear" | "shield" | "blunt" | "unarmed" | "honor" | "fury" => [205, 205, 205],
        "dagger" | "bow" | "thrown" | "silent" | "acrobat" | "trickster" => [180, 220, 180],
        "poison" => [100, 230, 80],
        "tonics" | "alchemy" => [230, 230, 160],
        _ => [220, 220, 220],
    }
}

fn truncate_name(name: &str, max: usize) -> String {
    if name.chars().count() <= max {
        name.to_string()
    } else {
        let mut out: String = name.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

fn prettify(s: &str) -> String {
    s.split(&['_', ' '][..])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(c) => c.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
