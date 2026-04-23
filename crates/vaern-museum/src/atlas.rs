//! vaern-atlas — one-shot grid view of every Meshtint piece-node, body
//! overlay, and weapon, with taxonomy labels floating above each entry.
//!
//! No composer UI, no menus. Spawn everything at once, orbit-camera
//! around the grid. For authoring taxonomy classifications without
//! clicking through menus in the museum.
//!
//! Run:  `cargo run -p vaern-museum --bin vaern-atlas`
//!
//! Layout (Z axis, rows descend into -Z):
//!
//!   Male · Torso        [18 mannequins]
//!   Male · Bottom       [20]
//!   Male · Feet         [6]
//!   Male · Hand         [4]
//!   Male · Belt         [11 incl. 0=None]
//!   — gap —
//!   Female · Torso      [20]
//!   Female · Bottom     [21]
//!   Female · Feet       [6]
//!   Female · Hand       [4]
//!   Female · Belt       [11]
//!   — big gap —
//!   Male · Hair…Poleyn  [13 rows, varying widths]
//!   — gap —
//!   Female · Hair…Poleyn [13 rows]
//!   — big gap —
//!   Weapons             [wrapped across multiple rows]

mod camera;

use std::path::Path;

use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};
use vaern_assets::{
    BELT_MAX, BodyOverlay, BodySlot, FEET_MAX, Gender, HAND_MAX, MeshtintCatalog,
    MeshtintCharacterBundle, MeshtintPieceTaxonomy, OutfitPieces, PieceCategory,
    VaernAssetsPlugin, WeaponGrips, WeaponOverlay,
};

use camera::{OrbitCamera, orbit_camera_apply, orbit_camera_input};

const COL_SPACING: f32 = 2.5;
const ROW_SPACING: f32 = 5.0;
const GENDER_GAP: f32 = 5.0;
const SECTION_GAP: f32 = 12.0;

/// World-space label. Drawn as egui text at the projected screen
/// position each frame.
#[derive(Component)]
struct AtlasLabel {
    text: String,
    /// Y offset above the entity's world position (characters get 2.5m,
    /// row headers get 0).
    y_offset: f32,
    size: f32,
}

impl AtlasLabel {
    fn over_character(text: String) -> Self {
        Self { text, y_offset: 2.5, size: 11.0 }
    }
    fn header(text: String) -> Self {
        Self { text, y_offset: 2.5, size: 14.0 }
    }
}

fn main() {
    let assets_root_abs = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets")
        .canonicalize()
        .expect("assets/ folder must exist at <workspace>/assets");
    let asset_root = assets_root_abs.to_string_lossy().into_owned();

    let catalog = MeshtintCatalog::scan(&assets_root_abs);
    let weapon_grips = WeaponGrips::load_yaml(assets_root_abs.join("meshtint_weapon_grips.yaml"))
        .unwrap_or_else(|e| {
            warn!("weapon grips load failed ({e}); weapons fall back to identity");
            WeaponGrips::default()
        });
    let taxonomy =
        MeshtintPieceTaxonomy::load_yaml(assets_root_abs.join("meshtint_piece_taxonomy.yaml"))
            .unwrap_or_else(|e| {
                warn!("taxonomy load failed ({e}); labels fall back to catalog names");
                MeshtintPieceTaxonomy::default()
            });

    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Vaern Atlas".into(),
                        resolution: (1600u32, 900u32).into(),
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
        .insert_resource(weapon_grips)
        .insert_resource(taxonomy)
        .add_systems(Startup, (setup_world, spawn_atlas).chain())
        .add_systems(Update, (orbit_camera_input, orbit_camera_apply).chain())
        .add_systems(EguiPrimaryContextPass, draw_labels)
        .run();
}

fn setup_world(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Big ground plane sized to host the whole grid.
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(400.0, 0.2, 400.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.09, 0.10, 0.12),
            perceptual_roughness: 0.95,
            ..default()
        })),
        Transform::from_xyz(0.0, -0.1, -150.0),
    ));

    commands.spawn((
        DirectionalLight {
            illuminance: 10_000.0,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(
            EulerRot::XYZ,
            -std::f32::consts::FRAC_PI_3,
            std::f32::consts::FRAC_PI_4,
            0.0,
        )),
    ));

    let orbit = OrbitCamera {
        focus: Vec3::new(26.0, 1.0, -80.0),
        yaw: 0.3,
        pitch: 0.9,
        distance: 130.0,
    };
    let mut tf = Transform::default();
    orbit.write_transform(&mut tf);
    commands.spawn((
        Camera3d::default(),
        tf,
        AmbientLight {
            color: Color::WHITE,
            brightness: 400.0,
            ..default()
        },
        orbit,
    ));
}

fn spawn_atlas(
    mut commands: Commands,
    assets: Res<AssetServer>,
    catalog: Res<MeshtintCatalog>,
    taxonomy: Res<MeshtintPieceTaxonomy>,
) {
    let mut row_z = 0.0f32;

    // Base piece-node categories per gender.
    for &gender in Gender::ALL {
        for category in [
            PieceCategory::Torso,
            PieceCategory::Bottom,
            PieceCategory::Feet,
            PieceCategory::Hand,
            PieceCategory::Belt,
        ] {
            spawn_piece_row(&mut commands, &assets, &taxonomy, gender, category, row_z);
            row_z -= ROW_SPACING;
        }
        row_z -= GENDER_GAP;
    }

    row_z -= SECTION_GAP;

    // Body overlays per gender.
    for &gender in Gender::ALL {
        for &slot in BodySlot::ALL {
            if catalog.body(gender, slot).is_empty() {
                continue;
            }
            spawn_overlay_row(&mut commands, &assets, &catalog, &taxonomy, gender, slot, row_z);
            row_z -= ROW_SPACING;
        }
        row_z -= GENDER_GAP;
    }

    row_z -= SECTION_GAP;

    // Weapons — gender-agnostic.
    spawn_weapon_block(&mut commands, &assets, &catalog, row_z);
}

fn spawn_piece_row(
    commands: &mut Commands,
    assets: &AssetServer,
    taxonomy: &MeshtintPieceTaxonomy,
    gender: Gender,
    category: PieceCategory,
    row_z: f32,
) {
    let (max, category_name) = match category {
        PieceCategory::Torso => (gender.torso_max(), "Torso"),
        PieceCategory::Bottom => (gender.bottom_max(), "Bottom"),
        PieceCategory::Feet => (FEET_MAX, "Feet"),
        PieceCategory::Hand => (HAND_MAX, "Hand"),
        PieceCategory::Belt => (BELT_MAX, "Belt"),
    };
    // Belt allows 0 = none.
    let start_n = if matches!(category, PieceCategory::Belt) { 0 } else { 1 };

    // Row header — sits at the left of the row, in-line with the chars.
    commands.spawn((
        Transform::from_xyz(-6.0, 0.0, row_z),
        AtlasLabel::header(format!("{} · {}", gender.label(), category_name)),
    ));

    for (col, n) in (start_n..=max).enumerate() {
        let x = col as f32 * COL_SPACING;
        let outfit = match category {
            PieceCategory::Torso => OutfitPieces { torso: n, ..default() },
            PieceCategory::Bottom => OutfitPieces { bottom: n, ..default() },
            PieceCategory::Feet => OutfitPieces { feet: n, ..default() },
            PieceCategory::Hand => OutfitPieces { hand: n, ..default() },
            PieceCategory::Belt => OutfitPieces { belt: n, ..default() },
        };
        commands
            .spawn(MeshtintCharacterBundle::new(assets, gender, 1))
            .insert((
                Transform::from_xyz(x, 0.0, row_z),
                outfit,
                AtlasLabel::over_character(taxonomy.label(gender, category, n)),
            ));
    }
}

fn spawn_overlay_row(
    commands: &mut Commands,
    assets: &AssetServer,
    catalog: &MeshtintCatalog,
    taxonomy: &MeshtintPieceTaxonomy,
    gender: Gender,
    slot: BodySlot,
    row_z: f32,
) {
    let variants = catalog.body(gender, slot);
    if variants.is_empty() {
        return;
    }

    commands.spawn((
        Transform::from_xyz(-6.0, 0.0, row_z),
        AtlasLabel::header(format!("{} · {}", gender.label(), slot.label())),
    ));

    for (col, variant) in variants.iter().enumerate() {
        let x = col as f32 * COL_SPACING;
        let char = commands
            .spawn(MeshtintCharacterBundle::new(assets, gender, 1))
            .insert((
                Transform::from_xyz(x, 0.0, row_z),
                AtlasLabel::over_character(
                    taxonomy.overlay_label(gender, slot, variant.number),
                ),
            ))
            .id();

        commands.spawn(BodyOverlay {
            target: char,
            gender,
            slot,
            variant: variant.number,
            mirror_x: false,
        });
        if slot.has_mirrored_pair() {
            commands.spawn(BodyOverlay {
                target: char,
                gender,
                slot,
                variant: variant.number,
                mirror_x: true,
            });
        }
    }
}

fn spawn_weapon_block(
    commands: &mut Commands,
    assets: &AssetServer,
    catalog: &MeshtintCatalog,
    top_row_z: f32,
) {
    const WEAPONS_PER_ROW: usize = 20;

    commands.spawn((
        Transform::from_xyz(-6.0, 0.0, top_row_z),
        AtlasLabel::header("Weapons".into()),
    ));

    let mut total = 0usize;
    for category in catalog.weapon_categories() {
        for variant in catalog.weapon(category) {
            let col = total % WEAPONS_PER_ROW;
            let row = total / WEAPONS_PER_ROW;
            let x = col as f32 * COL_SPACING;
            let z = top_row_z - row as f32 * ROW_SPACING;

            let char = commands
                .spawn(MeshtintCharacterBundle::new(assets, Gender::Male, 1))
                .insert((
                    Transform::from_xyz(x, 0.0, z),
                    AtlasLabel::over_character(variant.label.clone()),
                ))
                .id();
            commands.spawn(WeaponOverlay {
                target: char,
                category: category.to_string(),
                variant: variant.number,
            });

            total += 1;
        }
    }
}

/// Each frame, project every `AtlasLabel` entity's world position to
/// screen space and draw its text via egui's painter. `Area::interactable(false)`
/// makes the overlay transparent to pointer input so the orbit camera
/// still gets drag events.
fn draw_labels(
    mut contexts: EguiContexts,
    camera: Query<(&Camera, &GlobalTransform)>,
    labels: Query<(&GlobalTransform, &AtlasLabel)>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let Ok((camera, cam_tf)) = camera.single() else {
        return;
    };

    egui::Area::new(egui::Id::new("atlas_labels"))
        .anchor(egui::Align2::LEFT_TOP, [0.0, 0.0])
        .interactable(false)
        .show(ctx, |ui| {
            let painter = ui.painter();
            for (tf, lbl) in &labels {
                let world = tf.translation() + Vec3::Y * lbl.y_offset;
                if let Ok(screen) = camera.world_to_viewport(cam_tf, world) {
                    painter.text(
                        egui::pos2(screen.x, screen.y),
                        egui::Align2::CENTER_BOTTOM,
                        &lbl.text,
                        egui::FontId::proportional(lbl.size),
                        egui::Color32::WHITE,
                    );
                }
            }
        });
}
