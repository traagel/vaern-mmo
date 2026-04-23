//! Museum world setup: ground plane, directional light, orbit camera,
//! and the pre-loaded DS palette materials.

use std::path::Path;

use bevy::prelude::*;
use vaern_assets::MESHTINT_DS_PALETTES;

use crate::camera::OrbitCamera;
use crate::composer::PaletteCache;

pub fn setup_world(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Ground.
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(20.0, 0.2, 20.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.12, 0.14, 0.18),
            perceptual_roughness: 0.9,
            ..default()
        })),
        Transform::from_xyz(0.0, -0.1, 0.0),
    ));

    // Sun.
    commands.spawn((
        DirectionalLight {
            illuminance: 10_000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(
            EulerRot::XYZ,
            -std::f32::consts::FRAC_PI_3,
            std::f32::consts::FRAC_PI_4,
            0.0,
        )),
    ));

    // Camera.
    let orbit = OrbitCamera::default();
    let mut tf = Transform::default();
    orbit.write_transform(&mut tf);
    commands.spawn((
        Camera3d::default(),
        tf,
        AmbientLight {
            color: Color::WHITE,
            brightness: 300.0,
            ..default()
        },
        orbit,
    ));
}

pub fn setup_palettes(
    mut cache: ResMut<PaletteCache>,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    cache.stock_mats = MESHTINT_DS_PALETTES
        .iter()
        .map(|p| {
            let tex = asset_server.load(format!("extracted/meshtint/palettes/{p}.png"));
            materials.add(StandardMaterial {
                base_color_texture: Some(tex),
                base_color: Color::WHITE,
                perceptual_roughness: 0.85,
                metallic: 0.0,
                ..default()
            })
        })
        .collect();

    // Also decode each palette synchronously into raw RGBA bytes so the
    // skin-color override can paint a new texture at runtime without
    // waiting on async asset-server loads.
    let assets_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    cache.raw = MESHTINT_DS_PALETTES
        .iter()
        .map(|p| {
            let path = assets_root.join(format!("extracted/meshtint/palettes/{p}.png"));
            image::open(&path)
                .unwrap_or_else(|e| panic!("failed to open palette {path:?}: {e}"))
                .to_rgba8()
                .into_raw()
        })
        .collect();
}
