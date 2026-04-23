//! vaern-museum v2 — Meshtint character composer.
//!
//! Thin egui frontend over `vaern-assets::meshtint`. One mannequin at
//! the origin; user picks gender, outfit piece-nodes, body overlays
//! (hair/helmet/pauldron/…), weapon/shield, and optional DS palette.
//! Weapon grip is live-editable via sliders and mirrors the YAML
//! calibration in `assets/meshtint_weapon_grips.yaml`.
//!
//! Module split:
//!   - [`composer`]  user-picks resource + sync systems (picks ↔ ECS)
//!   - [`scene`]     ground plane, sun, orbit-camera spawn, palette cache
//!   - [`camera`]    orbit-camera component + input
//!   - [`ui`]        egui Composer window

mod camera;
mod composer;
mod scene;
mod ui;

use std::path::Path;

use bevy::prelude::*;
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};
use vaern_assets::{
    MegakitCatalog, MeshtintAnimationCatalog, MeshtintCatalog, MeshtintPieceTaxonomy,
    QuaterniusGrips, VaernAssetsPlugin, WeaponGrips,
};

use camera::{orbit_camera_apply, orbit_camera_input};
use composer::{
    Composer, MegakitPropList, PaletteCache, WeaponList, apply_overlay_colors, push_quaternius_grip,
    push_weapon_grip, sync_character, sync_overlays, sync_palette, sync_quaternius_weapon,
    sync_selected_clip,
};
use scene::{setup_palettes, setup_world};
use ui::ui_panel;

fn main() {
    let assets_root_abs = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets")
        .canonicalize()
        .expect("assets/ folder must exist at <workspace>/assets");
    let asset_root = assets_root_abs.to_string_lossy().into_owned();

    // Load the Meshtint catalog + weapon-grip registry synchronously at
    // startup. Both are small YAML/filesystem reads and the UI needs
    // them immediately.
    let catalog = MeshtintCatalog::scan(&assets_root_abs);
    let anim_catalog = MeshtintAnimationCatalog::scan(&assets_root_abs);
    let weapon_list = WeaponList::build(&catalog);
    let megakit_catalog = MegakitCatalog::scan(&assets_root_abs);
    let prop_list = MegakitPropList::build(&megakit_catalog);
    let weapon_grips = WeaponGrips::load_yaml(assets_root_abs.join("meshtint_weapon_grips.yaml"))
        .unwrap_or_else(|e| {
            warn!(
                "failed to load meshtint_weapon_grips.yaml ({e}); \
                 weapons will fall back to mainhand + identity grip"
            );
            WeaponGrips::default()
        });
    let quaternius_grips =
        QuaterniusGrips::load_yaml(assets_root_abs.join("quaternius_weapon_grips.yaml"))
            .unwrap_or_else(|e| {
                warn!(
                    "failed to load quaternius_weapon_grips.yaml ({e}); \
                     Quaternius weapons will fall back to mainhand + identity grip"
                );
                QuaterniusGrips::default()
            });
    let piece_taxonomy =
        MeshtintPieceTaxonomy::load_yaml(assets_root_abs.join("meshtint_piece_taxonomy.yaml"))
            .unwrap_or_else(|e| {
                warn!(
                    "failed to load meshtint_piece_taxonomy.yaml ({e}); \
                     outfit sliders will show bare variant numbers"
                );
                MeshtintPieceTaxonomy::default()
            });

    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Vaern Museum v2".into(),
                        resolution: (1280u32, 800u32).into(),
                        ..default()
                    }),
                    ..default()
                })
                .set(AssetPlugin {
                    file_path: asset_root,
                    ..default()
                }),
        )
        .add_plugins(EguiPlugin::default())
        .add_plugins(VaernAssetsPlugin)
        .insert_resource(ClearColor(Color::srgb(0.05, 0.06, 0.09)))
        .insert_resource(catalog)
        .insert_resource(anim_catalog)
        .insert_resource(weapon_list)
        .insert_resource(megakit_catalog)
        .insert_resource(prop_list)
        .insert_resource(weapon_grips)
        .insert_resource(quaternius_grips)
        .insert_resource(piece_taxonomy)
        .init_resource::<Composer>()
        .init_resource::<PaletteCache>()
        .add_systems(Startup, (setup_world, setup_palettes))
        .add_systems(
            Update,
            (
                sync_character,
                sync_overlays,
                sync_quaternius_weapon,
                sync_palette,
                push_weapon_grip,
                push_quaternius_grip,
                sync_selected_clip,
                orbit_camera_input,
                orbit_camera_apply,
            )
                .chain(),
        )
        // Runs after vaern-assets' palette walker so its per-overlay
        // flat-colour MeshMaterial3d insertions override the palette.
        .add_systems(
            Update,
            apply_overlay_colors.after(vaern_assets::meshtint::palette::apply_palette_override),
        )
        .add_systems(EguiPrimaryContextPass, ui_panel)
        .run();
}
