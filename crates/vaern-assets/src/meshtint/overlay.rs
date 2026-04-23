//! Rigid overlay spawning.
//!
//! **Body overlays** (hair / beard / helmet / pauldron / …) parent to
//! the character entity; every Meshtint mesh-node carries a `-90°X`
//! rotation baked in by fbx2gltf (Z-up → Y-up), so rigid overlays get a
//! `+90°X` counter-rotation at the scene root to render upright.
//!
//! **Weapon overlays** parent to a hand bone resolved via [`NamedRegions`]
//! on the character. The local transform comes from the [`WeaponGrips`]
//! registry — the calibrated `(translation, rotation, flip)` that seats
//! the grip in the palm.
//!
//! Both flows defer spawning across frames: body overlays until the
//! [`MeshtintCatalog`] resource is present, weapons until additionally
//! the target bone has been cached. Neither over-spawns — the
//! `OverlaySpawned` marker gates re-processing.

use bevy::prelude::*;

use crate::regions::NamedRegions;

use super::catalog::MeshtintCatalog;
use super::grips::{AttachBone, GripSpec, WeaponGrips};
use super::{BONE_BACK, BONE_MAINHAND, BONE_OFFHAND, Gender};

/// Meshtint body-overlay slot. Each maps to a `{prefix}_NN.glb` under
/// the gender's asset folder. See [`BodySlot::file_prefix`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BodySlot {
    Hair,
    Beard,
    Brow,
    Eyes,
    Mouth,
    Helmet,
    Hat,
    Headband,
    Earring,
    Necklace,
    Pauldron,
    Bracer,
    Poleyn,
}

impl BodySlot {
    pub const ALL: &'static [BodySlot] = &[
        BodySlot::Hair,
        BodySlot::Beard,
        BodySlot::Brow,
        BodySlot::Eyes,
        BodySlot::Mouth,
        BodySlot::Helmet,
        BodySlot::Hat,
        BodySlot::Headband,
        BodySlot::Earring,
        BodySlot::Necklace,
        BodySlot::Pauldron,
        BodySlot::Bracer,
        BodySlot::Poleyn,
    ];

    /// Meshtint ships these slots as single-sided geometry anchored to
    /// one side of the body (right shoulder pauldron, right ear earring,
    /// etc.). Spawn a mirrored copy (`mirror_x: true`) alongside the
    /// primary overlay to get a symmetric pair.
    pub fn has_mirrored_pair(self) -> bool {
        matches!(
            self,
            BodySlot::Pauldron | BodySlot::Bracer | BodySlot::Poleyn | BodySlot::Earring
        )
    }

    /// Slots that cannot coexist with `self` on the same character.
    /// Consumers should zero out these slots when the user picks a
    /// variant for `self`. Examples: helmet hides hair/hat/headband;
    /// headband is fine with hair but not with a helmet or hat.
    pub fn conflicts_with(self) -> &'static [BodySlot] {
        match self {
            BodySlot::Helmet => &[BodySlot::Hat, BodySlot::Hair, BodySlot::Headband],
            BodySlot::Hat => &[BodySlot::Helmet, BodySlot::Hair, BodySlot::Headband],
            BodySlot::Hair => &[BodySlot::Helmet, BodySlot::Hat],
            BodySlot::Headband => &[BodySlot::Helmet, BodySlot::Hat],
            _ => &[],
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            BodySlot::Hair => "Hair",
            BodySlot::Beard => "Beard",
            BodySlot::Brow => "Brow",
            BodySlot::Eyes => "Eyes",
            BodySlot::Mouth => "Mouth",
            BodySlot::Helmet => "Helmet",
            BodySlot::Hat => "Hat",
            BodySlot::Headband => "Headband",
            BodySlot::Earring => "Earring",
            BodySlot::Necklace => "Necklace",
            BodySlot::Pauldron => "Pauldron",
            BodySlot::Bracer => "Bracer",
            BodySlot::Poleyn => "Poleyn",
        }
    }

    /// Filename prefix inside the gender's folder. `None` = this slot
    /// has no variants for the given gender (e.g. Beard on Female).
    pub fn file_prefix(self, g: Gender) -> Option<&'static str> {
        match self {
            BodySlot::Hair => Some(match g {
                Gender::Male => "Hair_Male",
                Gender::Female => "Hair_Female",
            }),
            BodySlot::Beard => (g == Gender::Male).then_some("Beard"),
            BodySlot::Brow => Some("Brow"),
            BodySlot::Eyes => Some("Eyes"),
            BodySlot::Mouth => Some("Mouth"),
            BodySlot::Helmet => Some("Helmet"),
            BodySlot::Hat => Some("Hat"),
            BodySlot::Headband => Some("Headband"),
            BodySlot::Earring => Some("Earring"),
            BodySlot::Necklace => Some("Necklace"),
            BodySlot::Pauldron => Some("Pauldron"),
            BodySlot::Bracer => Some("Bracer"),
            BodySlot::Poleyn => Some("Poleyn"),
        }
    }
}

/// Spawn-request for a rigid body overlay (hair / helmet / pauldron /
/// …). Attach to a fresh entity — the `spawn_body_overlays` system
/// fills in the `SceneRoot`, `Transform`, `ChildOf(target)` and
/// visibility-fix marker once the catalog resolves, then tags the entity
/// with [`OverlaySpawned`] so subsequent frames skip it.
///
/// To change variant: despawn the old overlay entity + spawn a new one.
/// Mutating this component in-place **won't** re-spawn the scene.
#[derive(Component, Clone, Copy, Debug)]
pub struct BodyOverlay {
    pub target: Entity,
    pub gender: Gender,
    pub slot: BodySlot,
    pub variant: u32,
    /// Reflect the overlay through the character's YZ plane (`scale.x =
    /// -1`). Used for slots with [`BodySlot::has_mirrored_pair`] where
    /// Meshtint ships only the right-side version — spawn a second
    /// overlay with `mirror_x: true` to get the left side.
    pub mirror_x: bool,
}

/// Spawn-request for a weapon/shield/tool overlay. Resolved by
/// `spawn_weapon_overlays` via the [`MeshtintCatalog`] (asset path) and
/// [`WeaponGrips`] (grip transform + attach bone).
///
/// Same single-shot semantics as [`BodyOverlay`] — mutating this
/// component won't respawn; despawn + respawn the entity instead.
#[derive(Component, Clone, Debug)]
pub struct WeaponOverlay {
    pub target: Entity,
    pub category: String,
    pub variant: u32,
}

/// Marker inserted on overlay entities once their scene + transform are
/// wired. Prevents re-processing.
#[derive(Component)]
pub struct OverlaySpawned;

/// Counter-rotation that cancels fbx2gltf's baked `-90°X` on every
/// Meshtint mesh-node. Rigid body overlays use this at the scene-root
/// so the mesh renders upright; weapons fold the cancellation into
/// their [`GripSpec`] rotation instead.
///
/// `mirror_x` flips the mesh through the YZ plane — used for symmetric
/// pairs (see [`BodySlot::has_mirrored_pair`]).
fn body_overlay_transform(mirror_x: bool) -> Transform {
    let scale = if mirror_x {
        Vec3::new(-1.0, 1.0, 1.0)
    } else {
        Vec3::ONE
    };
    Transform {
        translation: Vec3::ZERO,
        rotation: Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
        scale,
    }
}

pub(super) fn spawn_body_overlays(
    mut commands: Commands,
    assets: Res<AssetServer>,
    catalog: Option<Res<MeshtintCatalog>>,
    q: Query<(Entity, &BodyOverlay), Without<OverlaySpawned>>,
) {
    let Some(catalog) = catalog else { return };
    for (entity, overlay) in &q {
        let Some(variant) = catalog.body_variant(overlay.gender, overlay.slot, overlay.variant)
        else {
            warn!(
                "no asset for body overlay {:?} {:?} {}",
                overlay.gender, overlay.slot, overlay.variant
            );
            commands.entity(entity).insert(OverlaySpawned);
            continue;
        };
        commands.entity(entity).insert((
            SceneRoot(assets.load(format!("{}#Scene0", variant.path))),
            body_overlay_transform(overlay.mirror_x),
            ChildOf(overlay.target),
            OverlaySpawned,
        ));
    }
}

pub(super) fn spawn_weapon_overlays(
    mut commands: Commands,
    assets: Res<AssetServer>,
    catalog: Option<Res<MeshtintCatalog>>,
    grips: Option<Res<WeaponGrips>>,
    q: Query<(Entity, &WeaponOverlay), Without<OverlaySpawned>>,
    regions: Query<&NamedRegions>,
) {
    let Some(catalog) = catalog else { return };

    for (entity, overlay) in &q {
        // Grip lookup — falls back to mainhand + identity if the category
        // isn't calibrated (game client is expected to ship a full grip
        // registry; this fallback is a last-resort "render something").
        let (attach, spec) = grips
            .as_deref()
            .and_then(|g| g.lookup(&overlay.category, overlay.variant))
            .unwrap_or((AttachBone::Mainhand, GripSpec::default()));

        let bone_name = match attach {
            AttachBone::Mainhand => BONE_MAINHAND,
            AttachBone::Offhand => BONE_OFFHAND,
            AttachBone::Back => BONE_BACK,
        };

        // Defer until the character's skeleton has been walked by
        // NamedRegions and the requested bone is cached.
        let Ok(character_regions) = regions.get(overlay.target) else {
            continue;
        };
        let Some(bone) = character_regions.entity(bone_name) else {
            continue;
        };

        let Some(variant) = catalog.weapon_variant(&overlay.category, overlay.variant) else {
            warn!(
                "no asset for weapon overlay {} {}",
                overlay.category, overlay.variant
            );
            commands.entity(entity).insert(OverlaySpawned);
            continue;
        };

        commands.entity(entity).insert((
            SceneRoot(assets.load(format!("{}#Scene0", variant.path))),
            spec.transform(),
            ChildOf(bone),
            OverlaySpawned,
        ));
    }
}

/// Mirror (`scale.x = -1`) flips the determinant of the model matrix,
/// which inverts triangle winding and backface-normal direction. Left
/// uncorrected, mirrored overlays render with shading visibly inside-out
/// ("normals flipped"). `StandardMaterial::double_sided` tells Bevy's
/// PBR shader to flip the interpolated normal on the backside, restoring
/// correct lighting; pairing it with `cull_mode = None` avoids relying
/// on Bevy's negative-determinant culling auto-flip.
///
/// Runs every frame. Walks each mirrored overlay's mesh descendants,
/// reaches through to the `StandardMaterial` asset, and mutates it in
/// place. The `!mat.double_sided` guard skips idle frames without
/// bumping change-detection.
///
/// Mutating the shared glTF material affects the non-mirrored twin too
/// — acceptable, since `double_sided` is visually neutral on a
/// correctly-wound mesh (just renders both sides at slight extra cost).
pub(super) fn fix_mirrored_overlay_normals(
    mut materials: ResMut<Assets<StandardMaterial>>,
    overlays: Query<(Entity, &BodyOverlay)>,
    children: Query<&Children>,
    mesh_mats: Query<&MeshMaterial3d<StandardMaterial>>,
) {
    for (root, overlay) in &overlays {
        if !overlay.mirror_x {
            continue;
        }
        let mut stack = vec![root];
        while let Some(e) = stack.pop() {
            if let Ok(handle) = mesh_mats.get(e) {
                if let Some(mat) = materials.get_mut(&handle.0) {
                    if !mat.double_sided {
                        mat.double_sided = true;
                        mat.cull_mode = None;
                    }
                }
            }
            if let Ok(kids) = children.get(e) {
                for &c in kids {
                    stack.push(c);
                }
            }
        }
    }
}
