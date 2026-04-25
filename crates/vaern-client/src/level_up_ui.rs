//! Level-up feedback overlay.
//!
//! Watches `OwnPlayerState` for an `xp_level` increase. On rise, queues a
//! 2.5-second centered "Level N" banner with a fade-in/out + a brief
//! screen flash. Pure egui — no audio yet (post-pre-alpha).

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::menu::AppState;
use crate::unit_frame::OwnPlayerState;

const BANNER_LIFETIME_SECS: f32 = 2.5;
const FLASH_LIFETIME_SECS: f32 = 0.35;

#[derive(Resource, Default)]
struct LastSeenLevel(u32);

#[derive(Resource, Default)]
struct LevelUpFlash {
    /// New level number to display.
    level: u32,
    /// Seconds remaining on the banner. 0 = inactive.
    banner_remaining: f32,
    /// Seconds remaining on the screen flash. 0 = inactive.
    flash_remaining: f32,
}

pub struct LevelUpEffectsPlugin;

impl Plugin for LevelUpEffectsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LastSeenLevel>()
            .init_resource::<LevelUpFlash>()
            .add_systems(
                Update,
                (detect_level_up, render_banner, tick_lifetimes)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(OnExit(AppState::InGame), reset);
    }
}

fn reset(mut last: ResMut<LastSeenLevel>, mut flash: ResMut<LevelUpFlash>) {
    *last = LastSeenLevel::default();
    *flash = LevelUpFlash::default();
}

fn detect_level_up(
    state: Res<OwnPlayerState>,
    mut last: ResMut<LastSeenLevel>,
    mut flash: ResMut<LevelUpFlash>,
) {
    if !state.received {
        return;
    }
    let cur = state.snap.xp_level.max(1);
    if last.0 == 0 {
        // First snapshot after enter-game: just record. Don't fire a banner
        // for the initial syncing of the persisted level.
        last.0 = cur;
        return;
    }
    if cur > last.0 {
        info!("[level-up] {} → {}", last.0, cur);
        flash.level = cur;
        flash.banner_remaining = BANNER_LIFETIME_SECS;
        flash.flash_remaining = FLASH_LIFETIME_SECS;
        last.0 = cur;
    } else if cur < last.0 {
        // De-level (server reset or character switch). Clear our tracker.
        last.0 = cur;
    }
}

fn tick_lifetimes(time: Res<Time>, mut flash: ResMut<LevelUpFlash>) {
    let dt = time.delta_secs();
    flash.banner_remaining = (flash.banner_remaining - dt).max(0.0);
    flash.flash_remaining = (flash.flash_remaining - dt).max(0.0);
}

fn render_banner(mut contexts: EguiContexts, flash: Res<LevelUpFlash>) {
    if flash.banner_remaining <= 0.0 {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else { return };

    // Banner alpha: fade in over the first 0.25s, fade out over the last 0.6s.
    let elapsed = BANNER_LIFETIME_SECS - flash.banner_remaining;
    let banner_alpha = if elapsed < 0.25 {
        elapsed / 0.25
    } else if flash.banner_remaining < 0.6 {
        flash.banner_remaining / 0.6
    } else {
        1.0
    }
    .clamp(0.0, 1.0);

    // Screen-wide gold flash overlay (additive feel — semi-transparent gold panel).
    if flash.flash_remaining > 0.0 {
        let flash_alpha = (flash.flash_remaining / FLASH_LIFETIME_SECS).clamp(0.0, 1.0) * 0.15;
        let painter = ctx.layer_painter(egui::LayerId::background());
        painter.rect_filled(
            ctx.content_rect(),
            0.0,
            egui::Color32::from_rgba_unmultiplied(255, 220, 110, (flash_alpha * 255.0) as u8),
        );
    }

    // Centered banner text.
    let screen = ctx.content_rect();
    let center = screen.center();
    let banner_pos = egui::pos2(center.x, screen.top() + screen.height() * 0.32);

    egui::Area::new(egui::Id::new("level_up_banner"))
        .fixed_pos(banner_pos)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let alpha = (banner_alpha * 255.0) as u8;
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new("LEVEL UP")
                        .size(38.0)
                        .strong()
                        .color(egui::Color32::from_rgba_unmultiplied(255, 220, 110, alpha)),
                );
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new(format!("Level {}", flash.level))
                        .size(56.0)
                        .strong()
                        .color(egui::Color32::from_rgba_unmultiplied(255, 245, 200, alpha)),
                );
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new("+1 pillar point granted to your primary pillar")
                        .size(16.0)
                        .italics()
                        .color(egui::Color32::from_rgba_unmultiplied(220, 220, 220, alpha)),
                );
            });
        });
}
