//! YAML-driven weapon grip registry.
//!
//! Every weapon/shield/tool overlay is rigid and parented to a hand bone
//! on the humanoid rig. Meshtint's source FBX has no canonical origin or
//! orientation for weapon meshes, so each category needs a hand-calibrated
//! `(translation, rotation, flip)` offset to seat the grip in the palm
//! with the blade/haft pointed the right way.
//!
//! [`WeaponGrips`] holds the full registry, loaded from a YAML file
//! authored against the Meshtint rig. See
//! `assets/meshtint_weapon_grips.yaml` for the canonical calibration.

use std::collections::HashMap;
use std::path::Path;

use bevy::prelude::*;
use serde::Deserialize;
use thiserror::Error;

/// Which skeleton attach point a weapon category docks to.
///
/// The Meshtint palm bones are `RigRPalm` (mainhand) and `RigLPalm`
/// (offhand); `Back` targets `RigSpine3` as a placeholder for quivers.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AttachBone {
    #[default]
    Mainhand,
    Offhand,
    Back,
}

/// One grip calibration — translation (metres, bone-local), Euler rotation
/// (degrees, applied XYZ before flips), and 180° post-multiply flips.
///
/// Missing YAML fields default to zero/false so overrides can be sparse.
#[derive(Clone, Copy, Debug, Default, Deserialize, Resource)]
pub struct GripSpec {
    #[serde(default)]
    pub tx: f32,
    #[serde(default)]
    pub ty: f32,
    #[serde(default)]
    pub tz: f32,
    #[serde(default)]
    pub rx: f32,
    #[serde(default)]
    pub ry: f32,
    #[serde(default)]
    pub rz: f32,
    #[serde(default)]
    pub flip_x: bool,
    #[serde(default)]
    pub flip_y: bool,
    #[serde(default)]
    pub flip_z: bool,
}

impl GripSpec {
    /// Fold the spec into a local `Transform` for the overlay scene-root.
    pub fn transform(&self) -> Transform {
        let mut q = Quat::from_euler(
            EulerRot::XYZ,
            self.rx.to_radians(),
            self.ry.to_radians(),
            self.rz.to_radians(),
        );
        if self.flip_x {
            q *= Quat::from_rotation_x(std::f32::consts::PI);
        }
        if self.flip_y {
            q *= Quat::from_rotation_y(std::f32::consts::PI);
        }
        if self.flip_z {
            q *= Quat::from_rotation_z(std::f32::consts::PI);
        }
        Transform {
            translation: Vec3::new(self.tx, self.ty, self.tz),
            rotation: q,
            scale: Vec3::ONE,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct CategoryGrip {
    #[serde(default)]
    pub attach: AttachBone,
    pub default: GripSpec,
    #[serde(default)]
    pub overrides: HashMap<u32, GripSpec>,
}

impl CategoryGrip {
    /// Per-variant grip — falls back to the category default if no
    /// override exists for `variant`.
    pub fn grip_for(&self, variant: u32) -> GripSpec {
        self.overrides.get(&variant).copied().unwrap_or(self.default)
    }
}

/// Registry of every calibrated weapon/shield/tool category. Typically
/// loaded once at startup via [`WeaponGrips::load_yaml`] and inserted as
/// a Bevy resource.
#[derive(Clone, Debug, Default, Deserialize, Resource)]
pub struct WeaponGrips {
    pub categories: HashMap<String, CategoryGrip>,
}

impl WeaponGrips {
    /// `(attach_bone, grip_spec)` for `(category, variant)`, or `None`
    /// if the category isn't registered.
    pub fn lookup(&self, category: &str, variant: u32) -> Option<(AttachBone, GripSpec)> {
        let cat = self.categories.get(category)?;
        Some((cat.attach, cat.grip_for(variant)))
    }

    /// Parse a YAML calibration file off disk.
    pub fn load_yaml<P: AsRef<Path>>(path: P) -> Result<Self, WeaponGripsLoadError> {
        let bytes = std::fs::read(path.as_ref())?;
        let grips: WeaponGrips = serde_yaml::from_slice(&bytes)?;
        Ok(grips)
    }
}

#[derive(Debug, Error)]
pub enum WeaponGripsLoadError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("yaml: {0}")]
    Yaml(#[from] serde_yaml::Error),
}
