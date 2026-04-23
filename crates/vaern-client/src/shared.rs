//! Client-wide shared types: marker components used across modules, the
//! character-height mesh offset, and a small school-color helper that
//! multiple UI systems need.

use bevy::prelude::*;

// ─── markers ───────────────────────────────────────────────────────────────

/// Own-player marker added to the predicted copy when we render it.
#[derive(Component)]
pub struct Player;

/// Any server-replicated NPC (no PlayerTag). Client-local marker.
#[derive(Component)]
pub struct Npc;

/// A remote player (someone else) replicated in as an Interpolated copy.
#[derive(Component)]
pub struct RemotePlayer;

/// The 3D gameplay camera.
#[derive(Component)]
pub struct MainCamera;

/// The 2D camera that keeps egui drawing while the menu is up (order=-1
/// so the gameplay 3D camera renders on top once the game starts).
#[derive(Component)]
pub struct MenuCamera;

/// Tag for world entities spawned by `setup_scene`; despawned by teardown.
#[derive(Component)]
pub struct GameWorld;

/// Tag added once a visual mesh has been attached as a child. Gates the
/// per-entity render systems so rollback / transient Children changes don't
/// cause mesh re-spawns (the prior `Without<Children>` filter flickered).
#[derive(Component)]
pub struct ModelAttached;

/// Resource holding our own client id so UI can distinguish self from peers.
#[derive(Resource, Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct OwnClientId(pub u64);

// ─── mesh attach ───────────────────────────────────────────────────────────

/// Half-height offset for placing the cuboid body above the feet-position
/// parent Transform. Matches `Cuboid::new(W, 1.8, D)` so the bottom face sits
/// exactly on y=0.
pub const MESH_Y_OFFSET: f32 = 0.9;

/// Attach a visible cuboid mesh as a CHILD of `parent` so the entity's
/// feet-position Transform (y=0) is preserved even on the first
/// pre-replication frame.
pub fn attach_mesh_child(
    parent: Entity,
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    commands: &mut Commands,
) {
    let child = commands
        .spawn((
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::from_xyz(0.0, MESH_Y_OFFSET, 0.0),
        ))
        .id();
    commands.entity(parent).add_child(child);
}

// ─── small lookups ─────────────────────────────────────────────────────────

/// RGB world-color per school id (used for cast beams, impact flashes, and
/// some UI accents). `hotbar_ui` has its own egui-color variant.
pub fn school_color(school: &str) -> Color {
    match school {
        "fire" => Color::srgb(1.0, 0.45, 0.10),
        "frost" => Color::srgb(0.35, 0.75, 1.0),
        "shadow" => Color::srgb(0.60, 0.20, 0.90),
        "light" => Color::srgb(1.0, 0.95, 0.55),
        "lightning" => Color::srgb(0.85, 0.85, 1.0),
        "blade" | "spear" | "blunt" | "shield" => Color::srgb(0.80, 0.80, 0.80),
        "poison" => Color::srgb(0.40, 0.90, 0.30),
        "blood" => Color::srgb(0.75, 0.10, 0.15),
        _ => Color::WHITE,
    }
}

