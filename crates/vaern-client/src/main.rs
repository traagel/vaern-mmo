//! Vaern client. Scaffolding split into focused modules:
//!
//!   menu      — egui character-select / connect / logout
//!   hotbar_ui — egui ability hotbar + spellbook (K)
//!   net       — lightyear client entity + ClientHello
//!   scene     — 3D ground/sun/camera, mesh attachment, follow-camera, teardown
//!   input     — WASD, auto-target, 1..=6 hotkey → CastIntent
//!   combat_ui — Bevy-native HP/resource/cast/target bars
//!   vfx       — impact flashes, cast beam gizmos
//!   nameplates — world-space HP bars + floating damage numbers
//!   diagnostic — periodic + boundary logging
//!   shared    — marker components, school color, class label helpers

mod attack_viz;
mod belt_ui;
mod chat_ui;
mod combat_ui;
mod diagnostic;
mod hotbar_ui;
mod hud;
mod input;
mod interact;
mod harvest_ui;
mod inventory_ui;
mod item_icons;
mod level_up_ui;
mod loot_ui;
mod menu;
mod nameplates;
mod net;
mod party_ui;
mod quests;
mod scene;
mod shared;
mod stat_screen;
mod unit_frame;
mod vendor_ui;
mod vfx;
mod voxel_biomes;
mod voxel_demo;

use core::time::Duration;
use std::path::Path;

use bevy::prelude::*;
use vaern_assets::{
    AnimalCatalog, MegakitCatalog, MeshtintAnimationCatalog, QuaterniusGrips, VaernAssetsPlugin,
};
use vaern_persistence::HumanoidArchetypeTable;

/// Bevy `Resource` newtype around the archetype table so it can be
/// injected into client systems. Kept here rather than in
/// `vaern-persistence` so the persistence crate stays Bevy-free.
#[derive(bevy::prelude::Resource, Debug, Default, Clone)]
pub struct ArchetypeTableRes(pub HumanoidArchetypeTable);

impl std::ops::Deref for ArchetypeTableRes {
    type Target = HumanoidArchetypeTable;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
use vaern_combat::CombatPlugin;
use vaern_protocol::{FIXED_TIMESTEP_HZ, SharedPlugin};

use crate::attack_viz::AttackVizPlugin;
use crate::belt_ui::BeltUiPlugin;
use crate::combat_ui::CombatUiPlugin;
use crate::diagnostic::DiagnosticsPlugin;
use crate::hotbar_ui::HotbarUiPlugin;
use crate::hud::HudPlugin;
use crate::input::ClientInputPlugin;
use crate::interact::InteractPlugin;
use crate::level_up_ui::LevelUpEffectsPlugin;
use crate::harvest_ui::HarvestUiPlugin;
use crate::inventory_ui::InventoryUiPlugin;
use crate::item_icons::ItemIconsPlugin;
use crate::loot_ui::LootUiPlugin;
use crate::menu::MenuPlugin;
use crate::stat_screen::StatScreenPlugin;
use crate::nameplates::NameplatesPlugin;
use crate::net::NetworkingPlugin;
use crate::quests::QuestsPlugin;
use crate::scene::ScenePlugin;
use crate::unit_frame::UnitFramePlugin;
use crate::vfx::VfxPlugin;
use crate::voxel_demo::VoxelDemoPlugin;

fn main() {
    let tick = Duration::from_secs_f64(1.0 / FIXED_TIMESTEP_HZ);
    // Point Bevy's AssetServer at the workspace-level assets/ (Quaternius
    // glTFs + textures live there). Default path is `assets/` next to the
    // binary, which doesn't exist for a workspace crate. Mirrors how
    // vaern-museum resolves it.
    let asset_root_abs = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets")
        .canonicalize()
        .expect("assets/ folder must exist at <workspace>/assets");
    let asset_root = asset_root_abs.to_string_lossy().into_owned();
    // Scan the UAL GLBs so the MeshtintAnimations graph builder in
    // MeshtintPlugin has clips to load. Without this resource, the
    // shared AnimationGraph comes up empty and the character stays in
    // bind pose.
    let anim_catalog = MeshtintAnimationCatalog::scan(&asset_root_abs);
    // MEGAKIT props + their calibrated Quaternius-rig grips. Drives
    // the weapon overlay spawn path for own + remote players. Missing
    // YAML falls back to identity grips (warns, doesn't fail).
    let megakit_catalog = MegakitCatalog::scan(&asset_root_abs);
    let animal_catalog = AnimalCatalog::scan(&asset_root_abs);
    let quaternius_grips =
        QuaterniusGrips::load_yaml(asset_root_abs.join("quaternius_weapon_grips.yaml"))
            .unwrap_or_else(|e| {
                warn!(
                    "failed to load quaternius_weapon_grips.yaml ({e}); \
                     Quaternius weapons will render at identity grip"
                );
                QuaterniusGrips::default()
            });
    // Humanoid-NPC archetype table — used to expand the replicated
    // `NpcAppearance.archetype` key into a full cosmetic bundle at
    // render time. Missing / malformed → empty table, every humanoid
    // NPC falls back to a cuboid. Same YAML the server loads, just
    // the `humanoid_archetypes:` section.
    let archetype_table =
        HumanoidArchetypeTable::load_yaml(asset_root_abs.join("npc_mesh_map.yaml"))
            .unwrap_or_else(|e| {
                warn!(
                    "failed to load npc_mesh_map.yaml archetypes ({e}); \
                     humanoid NPCs will render as cuboids"
                );
                HumanoidArchetypeTable::default()
            });
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Vaern — scaffold".into(),
                        resolution: (1280u32, 720u32).into(),
                        ..default()
                    }),
                    ..default()
                })
                .set(AssetPlugin {
                    file_path: asset_root,
                    ..default()
                }),
        )
        .add_plugins(lightyear::prelude::client::ClientPlugins { tick_duration: tick })
        .add_plugins(SharedPlugin)
        .add_plugins(CombatPlugin)
        // Character rendering: Quaternius modular outfits on UE Mannequin.
        // Drives the own-player render path in scene.rs.
        .add_plugins(VaernAssetsPlugin)
        .insert_resource(anim_catalog)
        .insert_resource(megakit_catalog)
        .insert_resource(animal_catalog)
        .insert_resource(quaternius_grips)
        .insert_resource(ArchetypeTableRes(archetype_table))
        // Menu + egui-driven UIs
        .add_plugins(MenuPlugin)
        .add_plugins(HotbarUiPlugin)
        .add_plugins(HudPlugin)
        .add_plugins(QuestsPlugin)
        .add_plugins(InteractPlugin)
        .add_plugins(InventoryUiPlugin)
        .add_plugins(vendor_ui::VendorUiPlugin)
        .add_plugins(chat_ui::ChatUiPlugin)
        .add_plugins(party_ui::PartyUiPlugin)
        .add_plugins(BeltUiPlugin)
        .add_plugins(LootUiPlugin)
        .add_plugins(ItemIconsPlugin)
        .add_plugins(HarvestUiPlugin)
        .add_plugins(StatScreenPlugin)
        // Game systems — each gated internally on AppState::InGame.
        .add_plugins(NetworkingPlugin)
        .add_plugins(ScenePlugin)
        .add_plugins(ClientInputPlugin)
        .add_plugins(CombatUiPlugin)
        .add_plugins(UnitFramePlugin)
        .add_plugins(LevelUpEffectsPlugin)
        .add_plugins(AttackVizPlugin)
        .add_plugins(VfxPlugin)
        .add_plugins(NameplatesPlugin)
        .add_plugins(DiagnosticsPlugin)
        // Voxel world — SDF chunks streamed around the camera, F10
        // carves a debug crater. Coexists with the existing ground
        // plane today; will retire it once server-side authority lands.
        .add_plugins(VoxelDemoPlugin)
        .insert_resource(ClearColor(Color::srgb(0.05, 0.07, 0.10)))
        .run();
}
