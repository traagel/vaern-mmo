//! Top-left self unit frame: race portrait, character name, level number,
//! HP bar, XP bar. Reads combat/progression state from the Replicated copy
//! of our own player (same pattern as `combat_ui.rs` — Predicted doesn't
//! update combat state because the client has no local sim).
//!
//! Portraits load on demand from the repo's `characters/<race>.<sex>.png`,
//! decoded with `image` and registered with egui — same mechanism as ability
//! icons in `hotbar_ui.rs`.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use lightyear::prelude::client::Client;
use lightyear::prelude::*;
use vaern_character::{PlayerRace, XpCurve};
use vaern_core::pillar::Pillar;
use vaern_protocol::{PlayerStateSnapshot, PlayerTag};

use crate::menu::{AppState, SelectedCharacter, pillar_display};

pub struct UnitFramePlugin;

impl Plugin for UnitFramePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(PortraitCache::default())
            .insert_resource(ClientXpCurve(XpCurve::default()))
            .insert_resource(OwnPlayerState::default())
            .add_systems(
                Update,
                ingest_player_state.run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                EguiPrimaryContextPass,
                (load_pending_portrait, unit_frame_ui)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(OnExit(AppState::InGame), reset);
    }
}

/// Latest snapshot of the owning player's combat + progression state,
/// populated by `ingest_player_state` from `PlayerStateSnapshot` messages.
/// `received` flips `true` the first time a message arrives — until then the
/// unit frame renders in placeholder mode.
#[derive(Resource, Debug, Default, Clone)]
pub struct OwnPlayerState {
    pub received: bool,
    pub snap: PlayerStateSnapshot,
}

fn ingest_player_state(
    mut receivers: Query<&mut MessageReceiver<PlayerStateSnapshot>, With<Client>>,
    mut state: ResMut<OwnPlayerState>,
) {
    let Ok(mut rx) = receivers.single_mut() else { return };
    for snap in rx.receive() {
        state.received = true;
        state.snap = snap;
    }
}

/// Client-side XP curve. Parallel to the server's resource; loaded from the
/// same yaml so the bar fills correctly. Wrapped in a newtype to keep the
/// crate-level `XpCurve` reusable by other systems later without collision.
#[derive(Resource, Debug, Default, Clone)]
struct ClientXpCurve(XpCurve);

/// Race portrait id → loaded egui texture. `failed` blocks retry on missing
/// file (e.g. race has no character art yet).
#[derive(Resource, Default)]
struct PortraitCache {
    textures: HashMap<String, egui::TextureHandle>,
    failed: HashSet<String>,
}

fn characters_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../characters")
}

fn progression_yaml() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../src/generated/world/progression/xp_curve.yaml")
}

fn load_image_from_disk(path: &Path) -> Result<egui::ColorImage, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let img = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Ok(egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()))
}

/// Load the portrait for the current race on first frame it's needed. Also
/// lazy-loads the XP curve yaml the first time a player frame renders — the
/// server has its own copy; the client loads independently because it never
/// receives a "xp to next" number, just the raw `current`.
fn load_pending_portrait(
    selected: Option<Res<SelectedCharacter>>,
    mut cache: ResMut<PortraitCache>,
    mut curve: ResMut<ClientXpCurve>,
    mut contexts: EguiContexts,
    mut tried_curve: Local<bool>,
) {
    if !*tried_curve {
        *tried_curve = true;
        match XpCurve::load_yaml(progression_yaml()) {
            Ok(c) => curve.0 = c,
            Err(e) => warn!("[unit_frame] xp curve load failed: {e} (formula fallback in use)"),
        }
    }

    let Some(selected) = selected else { return };
    let id = portrait_id(&selected.race_id);
    if cache.textures.contains_key(&id) || cache.failed.contains(&id) {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else { return };
    let path = characters_root().join(format!("{id}.png"));
    match load_image_from_disk(&path) {
        Ok(img) => {
            let handle = ctx.load_texture(
                format!("portrait:{id}"),
                img,
                egui::TextureOptions::LINEAR,
            );
            cache.textures.insert(id, handle);
        }
        Err(e) => {
            warn!("[unit_frame] portrait load failed for {}: {e}", path.display());
            cache.failed.insert(id);
        }
    }
}

/// `<race>.male` — the generic race portrait (always rendered by the asset
/// pipeline). Class-specific `<race>.class_NN.faction_X.<sex>.png` variants
/// exist too but not for every combination; fall back to race-generic.
fn portrait_id(race_id: &str) -> String {
    let race = if race_id.is_empty() { "mannin" } else { race_id };
    format!("{race}.male")
}

fn reset(mut cache: ResMut<PortraitCache>) {
    cache.textures.clear();
    cache.failed.clear();
}

fn unit_frame_ui(
    time: Res<Time>,
    mut contexts: EguiContexts,
    selected: Option<Res<SelectedCharacter>>,
    cache: Res<PortraitCache>,
    curve: Res<ClientXpCurve>,
    state: Res<OwnPlayerState>,
    // We still read `PlayerTag` off *any* copy of own player purely to get
    // the class_id (static, set at spawn — so stale-copy read is fine).
    // Everything dynamic (HP / XP) comes from `OwnPlayerState` via
    // `PlayerStateSnapshot` messages.
    players: Query<(&PlayerTag, Option<&PlayerRace>)>,
    mut debug_timer: Local<f32>,
) {
    let Ok(ctx) = contexts.ctx_mut() else { return };

    *debug_timer += time.delta_secs();
    if *debug_timer >= 3.0 {
        *debug_timer = 0.0;
        info!(
            "[unit_frame] state_received={} hp={}/{} xp={}/{} L{}",
            state.received,
            state.snap.hp_current,
            state.snap.hp_max,
            state.snap.xp_current,
            state.snap.xp_to_next,
            state.snap.xp_level,
        );
    }

    // Pillar + race: pick any player copy we can find. PlayerTag.core_pillar
    // is static, so even a stale Predicted copy is fine.
    let selected_ref = selected.as_deref();
    let core_pillar = players
        .iter()
        .next()
        .map(|(t, _)| t.core_pillar)
        .or_else(|| selected_ref.map(|s| s.core_pillar))
        .unwrap_or(Pillar::Might);
    let race_component = players.iter().find_map(|(_, r)| r);
    let race_id = race_component
        .map(|r| r.0.as_str())
        .or(selected_ref.map(|s| s.race_id.as_str()))
        .unwrap_or("mannin")
        .to_string();

    draw_frame(
        ctx,
        &cache,
        &curve,
        selected_ref,
        core_pillar,
        &race_id,
        &state.snap,
        !state.received,
    );
}

fn draw_frame(
    ctx: &egui::Context,
    cache: &PortraitCache,
    curve: &ClientXpCurve,
    selected: Option<&SelectedCharacter>,
    core_pillar: Pillar,
    race_id: &str,
    snap: &PlayerStateSnapshot,
    placeholder: bool,
) {
    let name = selected
        .map(|s| s.name.clone())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| pillar_display(core_pillar).to_string());
    let portrait = cache.textures.get(&portrait_id(race_id));
    let to_next = snap
        .xp_to_next
        .max(if placeholder { curve.0.to_next(1) } else { 1 });

    // Use Window (not Area) — same reason hotbar_ui does: Window is
    // draw-order-guaranteed and resilient to anchor / layer quirks that
    // otherwise render an Area behind the 3D camera.
    egui::Window::new("unit_frame_self")
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .movable(false)
        .anchor(egui::Align2::LEFT_TOP, egui::vec2(16.0, 16.0))
        .frame(
            egui::Frame::default()
                .fill(egui::Color32::from_rgba_unmultiplied(15, 18, 22, 230))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(90)))
                .inner_margin(egui::Margin::symmetric(10, 8))
                .corner_radius(6.0),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                match portrait {
                    Some(tex) => {
                        ui.add(
                            egui::Image::new((tex.id(), egui::vec2(72.0, 72.0)))
                                .corner_radius(4.0),
                        );
                    }
                    None => {
                        let (rect, _) = ui.allocate_exact_size(
                            egui::vec2(72.0, 72.0),
                            egui::Sense::hover(),
                        );
                        ui.painter().rect_filled(
                            rect,
                            4.0,
                            egui::Color32::from_rgb(40, 40, 50),
                        );
                    }
                }
                ui.add_space(10.0);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(&name)
                                .size(16.0)
                                .strong()
                                .color(egui::Color32::from_rgb(240, 240, 240)),
                        );
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new(format!("L{}", snap.xp_level.max(1)))
                                .size(14.0)
                                .color(egui::Color32::from_rgb(230, 200, 100)),
                        );
                    });
                    ui.label(
                        egui::RichText::new(format!(
                            "{} · {}",
                            titlecase(race_id),
                            pillar_display(core_pillar),
                        ))
                        .size(11.0)
                        .color(egui::Color32::from_rgb(180, 180, 180)),
                    );
                    ui.add_space(4.0);

                    let hp_max = if snap.hp_max > 0.0 { snap.hp_max } else { 100.0 };
                    let hp_pct = (snap.hp_current / hp_max).clamp(0.0, 1.0);
                    let hp_label = if placeholder {
                        "HP — waiting…".into()
                    } else {
                        format!("HP {:.0} / {:.0}", snap.hp_current, snap.hp_max)
                    };
                    bar(
                        ui,
                        hp_pct,
                        egui::Color32::from_rgb(200, 50, 50),
                        &hp_label,
                    );
                    let xp_pct = (snap.xp_current as f32 / to_next as f32).clamp(0.0, 1.0);
                    let xp_label = if placeholder {
                        "XP — waiting…".into()
                    } else {
                        format!("XP {} / {}", snap.xp_current, to_next)
                    };
                    bar(
                        ui,
                        xp_pct,
                        egui::Color32::from_rgb(160, 110, 210),
                        &xp_label,
                    );

                    // Stamina bar — drains while blocking, per-hit chunk
                    // taken by successful parries. Empty = can't stance.
                    if !placeholder && snap.stamina_max > 0.0 {
                        let stam_pct =
                            (snap.stamina_current / snap.stamina_max).clamp(0.0, 1.0);
                        let stam_label = format!(
                            "STA {:.0} / {:.0}",
                            snap.stamina_current, snap.stamina_max
                        );
                        let fill = if snap.is_blocking {
                            // Slightly desaturated / amber when actively
                            // draining — visual cue that stamina is moving.
                            egui::Color32::from_rgb(220, 170, 60)
                        } else {
                            egui::Color32::from_rgb(110, 180, 120)
                        };
                        bar(ui, stam_pct, fill, &stam_label);

                        if snap.is_blocking || snap.is_parrying {
                            let (text, color) = if snap.is_parrying {
                                ("PARRY", egui::Color32::from_rgb(255, 240, 140))
                            } else {
                                ("BLOCKING", egui::Color32::from_rgb(255, 210, 100))
                            };
                            ui.label(
                                egui::RichText::new(text)
                                    .size(11.0)
                                    .strong()
                                    .color(color),
                            );
                        }
                    }
                });
            });
        });
}

fn bar(ui: &mut egui::Ui, pct: f32, fill: egui::Color32, label: &str) {
    let desired = egui::vec2(220.0, 16.0);
    let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, egui::Color32::from_rgb(25, 25, 30));
    let mut fill_rect = rect;
    fill_rect.set_width(rect.width() * pct);
    painter.rect_filled(fill_rect, 2.0, fill);
    painter.rect_stroke(
        rect,
        2.0,
        egui::Stroke::new(1.0, egui::Color32::from_rgb(60, 60, 70)),
        egui::StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(11.0),
        egui::Color32::WHITE,
    );
}

fn titlecase(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for part in s.split('_') {
        if !out.is_empty() {
            out.push(' ');
        }
        let mut c = part.chars();
        if let Some(first) = c.next() {
            out.extend(first.to_uppercase());
            out.push_str(c.as_str());
        }
    }
    out
}
