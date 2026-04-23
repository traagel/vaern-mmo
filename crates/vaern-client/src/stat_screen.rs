//! Character stat screen — folds pillar-derived primaries + equipped
//! gear stats into a `CombinedStats` view and renders it. Toggled by `C`.
//!
//! Everything the screen shows is already on the client:
//!   * Pillar scores / caps / XP — from `PlayerStateSnapshot` via
//!     `OwnPlayerState`.
//!   * Equipped instances — from `EquippedSnapshot` via `OwnEquipped`.
//!   * Their stat rolls — resolved locally via `ClientContent`.
//!
//! No new protocol messages required; the fold happens client-side.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};

use vaern_core::DamageType;
use vaern_stats::{
    CombinedStats, PillarScores, SecondaryStats, TertiaryStats, combine, derive_primaries,
    xp_to_next_point,
};

use crate::inventory_ui::{ClientContent, OwnEquipped};
use crate::menu::AppState;
use crate::unit_frame::OwnPlayerState;

/// Toggle state for the stat screen. Separate from `InventoryWindowOpen`
/// so both can be open at once; the cursor-free check OR's them.
#[derive(Resource, Default)]
pub struct StatScreenOpen(pub bool);

pub struct StatScreenPlugin;

impl Plugin for StatScreenPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StatScreenOpen>()
            .add_systems(
                Update,
                toggle_stat_screen.run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                EguiPrimaryContextPass,
                draw_stat_screen.run_if(in_state(AppState::InGame)),
            );
    }
}

fn toggle_stat_screen(keys: Res<ButtonInput<KeyCode>>, mut open: ResMut<StatScreenOpen>) {
    if keys.just_pressed(KeyCode::KeyC) {
        open.0 = !open.0;
    }
}

/// Fold pillar-derived + gear + tertiary stats into a single view.
/// Tertiary defaults to zero until that roll source exists.
fn compute_combined(
    player: &OwnPlayerState,
    equipped: &OwnEquipped,
    content: &ClientContent,
) -> (PillarScores, CombinedStats) {
    let pillars = PillarScores {
        might: player.snap.might,
        finesse: player.snap.finesse,
        arcana: player.snap.arcana,
    };
    let derived = derive_primaries(&pillars);

    let mut gear = SecondaryStats::default();
    for inst in equipped.slots.values() {
        if let Ok(r) = content.0.resolve(inst) {
            gear.add(&r.stats);
        }
    }
    let tertiary = TertiaryStats::default();
    (pillars, combine(&derived, &gear, &tertiary))
}

fn draw_stat_screen(
    mut contexts: EguiContexts,
    open: Res<StatScreenOpen>,
    player: Res<OwnPlayerState>,
    equipped: Res<OwnEquipped>,
    content: Option<Res<ClientContent>>,
) {
    if !open.0 {
        return;
    }
    let Some(content) = content else { return };
    let Ok(ctx) = contexts.ctx_mut() else { return };
    let (pillars, stats) = compute_combined(&player, &equipped, &content);
    let caps = (
        player.snap.might_cap,
        player.snap.finesse_cap,
        player.snap.arcana_cap,
    );
    let pillar_xp = (
        player.snap.might_xp,
        player.snap.finesse_xp,
        player.snap.arcana_xp,
    );

    egui::Window::new("Character")
        .default_size(egui::vec2(460.0, 580.0))
        .show(ctx, |ui| {
            ui.label("Press C to close.");
            ui.separator();

            // ── Level + XP ──────────────────────────────────────────────
            ui.heading(format!("Level {}", player.snap.xp_level));
            ui.horizontal(|ui| {
                ui.label(format!(
                    "XP: {} / {}",
                    player.snap.xp_current, player.snap.xp_to_next
                ));
            });
            ui.separator();

            // ── Pillars + per-pillar XP ─────────────────────────────────
            ui.heading("Pillars");
            pillar_row(ui, "Might",   pillars.might,   caps.0, pillar_xp.0);
            pillar_row(ui, "Finesse", pillars.finesse, caps.1, pillar_xp.1);
            pillar_row(ui, "Arcana",  pillars.arcana,  caps.2, pillar_xp.2);
            ui.separator();

            // ── Vitals ──────────────────────────────────────────────────
            ui.heading("Vitals");
            ui.label(format!(
                "HP:    {} / {}",
                player.snap.hp_current as u32, stats.hp_max
            ));
            ui.label(format!(
                "Mana:  {} / {}",
                player.snap.pool_current as u32, stats.mana_max
            ));
            ui.separator();

            // ── Offense ─────────────────────────────────────────────────
            ui.heading("Offense");
            ui.label(format!("Melee mult:     ×{:.2}", stats.melee_mult));
            ui.label(format!("Spell mult:     ×{:.2}", stats.spell_mult));
            ui.label(format!(
                "Weapon dmg:     {:.1} – {:.1}",
                stats.weapon_min_dmg, stats.weapon_max_dmg
            ));
            ui.label(format!("Crit:           {:.1}%", stats.total_crit_pct));
            ui.label(format!("Haste:          {:.1}%", stats.total_haste_pct));
            ui.label(format!("Fortune:        {:.1}%", stats.fortune_pct));
            ui.separator();

            // ── Defense ─────────────────────────────────────────────────
            ui.heading("Defense");
            ui.label(format!("Armor:          {}", stats.armor));
            ui.label(format!("Dodge:          {:.1}%", stats.total_dodge_pct));
            ui.label(format!(
                "Parry window:   ×{:.2} (Might-scaled)",
                stats.total_parry_pct
            ));
            ui.label(format!(
                "Block chance:   {:.1}%   Block value: {}",
                stats.block_chance_pct, stats.block_value
            ));
            ui.separator();

            // ── Per-channel resists ─────────────────────────────────────
            ui.heading("Resists");
            egui::Grid::new("resists_grid")
                .num_columns(4)
                .spacing([10.0, 2.0])
                .show(ui, |ui| {
                    for (i, dt) in DamageType::ALL.iter().enumerate() {
                        ui.label(format!("{:>10}", format!("{:?}", dt)));
                        ui.label(format!("{:>5.1}", stats.resist_total[i]));
                        if (i + 1) % 2 == 0 {
                            ui.end_row();
                        }
                    }
                });
            ui.separator();

            // ── Utility + tertiary ──────────────────────────────────────
            ui.heading("Utility");
            ui.label(format!(
                "MP5:            {:+.1} {}",
                stats.mp5,
                if stats.mp5 < 0.0 { "(rune drain)" } else { "" }
            ));
            ui.label(format!("Carry:          {:.0} kg", stats.carry_kg));
            ui.label(format!("Luck:           {}", stats.luck));
            ui.label(format!("Leech:          {:.1}%", stats.leech_pct));
            ui.label(format!("Move speed:     {:+.1}%", stats.move_speed_pct));
            ui.label(format!("Avoidance:      {:.1}%", stats.avoidance_pct));
        });
}

/// One pillar row with a compact progress bar toward the next pillar
/// point. `current_xp` is banked XP; `xp_to_next` derives from current.
fn pillar_row(ui: &mut egui::Ui, name: &str, current: u16, cap: u16, current_xp: u32) {
    let to_next = xp_to_next_point(current);
    let frac = if to_next == 0 {
        1.0
    } else {
        (current_xp as f32 / to_next as f32).clamp(0.0, 1.0)
    };
    ui.horizontal(|ui| {
        ui.label(format!("{name:<8} {current:>3} / {cap:<3}"));
        let bar = egui::ProgressBar::new(frac)
            .desired_width(160.0)
            .text(format!("{current_xp}/{to_next}"));
        ui.add(bar);
    });
}
