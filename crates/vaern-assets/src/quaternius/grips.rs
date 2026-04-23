//! YAML-driven weapon grip registry for the Quaternius / UE Mannequin
//! rig.
//!
//! Separate from `crate::meshtint::grips` — the Quaternius skeleton has
//! different bone-local axes (UE Mannequin conventions vs Meshtint's
//! custom rig), so the calibrated `(translation, rotation, flip)` for
//! the same underlying weapon mesh differs between the two.
//!
//! Keyed by MEGAKIT prop basename (e.g. `"Sword_Bronze"`), **not** by
//! weapon category — MEGAKIT ships discrete named props, one per
//! weapon, unlike Meshtint's `Sword_01..05` series.
//!
//! Authored via the museum's Quaternius weapon-grip panel; saved to
//! `assets/quaternius_weapon_grips.yaml` and loaded once at startup.

use std::collections::HashMap;
use std::path::Path;

use bevy::prelude::*;
use serde::Deserialize;
use thiserror::Error;

/// Which hand a prop parents to.
///
/// Mainhand → [`super::bones::BONE_MAINHAND`] (`hand_r`).
/// Offhand  → [`super::bones::BONE_OFFHAND`] (`hand_l`), used for
/// shields, torches, secondary weapons.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AttachHand {
    #[default]
    Mainhand,
    Offhand,
}

/// Bone-local offset for one prop. Units mirror the Meshtint grip
/// format so the museum panel can reuse the same slider UX: metres
/// for translation, degrees (Euler XYZ) for rotation applied before
/// 180° flips. Missing YAML fields default to zero / false.
#[derive(Clone, Copy, Debug, Default, Deserialize, Resource, PartialEq)]
pub struct QuaterniusGripSpec {
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

impl QuaterniusGripSpec {
    /// Fold the spec into a local `Transform` for the overlay scene
    /// root. Identical shape to `meshtint::grips::GripSpec::transform`,
    /// but the bone-local frame this lands in is the UE Mannequin hand
    /// — different convention from Meshtint's `RigRPalm`.
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

/// Per-prop grip entry — which hand to attach to, plus the bone-local
/// spec.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct PropGrip {
    #[serde(default)]
    pub attach: AttachHand,
    #[serde(default)]
    pub spec: QuaterniusGripSpec,
}

/// Registry of every calibrated MEGAKIT prop. Populated at startup
/// from `assets/quaternius_weapon_grips.yaml`.
#[derive(Clone, Debug, Default, Deserialize, Resource)]
pub struct QuaterniusGrips {
    /// prop basename → grip entry. Absent entries get an identity
    /// mainhand fallback on lookup, with a warn-level log.
    pub props: HashMap<String, PropGrip>,
}

impl QuaterniusGrips {
    /// `(attach_hand, grip_spec)` for `prop_id`, falling back to
    /// mainhand + identity if the prop isn't in the registry (unknown
    /// prop = "render something" rather than hard-fail).
    pub fn lookup(&self, prop_id: &str) -> (AttachHand, QuaterniusGripSpec) {
        self.props
            .get(prop_id)
            .map(|g| (g.attach, g.spec))
            .unwrap_or_default()
    }

    /// Parse a YAML calibration file off disk. The YAML layout is:
    /// ```yaml
    /// props:
    ///   Sword_Bronze:
    ///     attach: mainhand
    ///     spec: { tx: 0.0, ty: 0.0, tz: 0.0, rx: 0, ry: 0, rz: 90 }
    ///   Shield_Wooden:
    ///     attach: offhand
    ///     spec: { ... }
    /// ```
    pub fn load_yaml<P: AsRef<Path>>(path: P) -> Result<Self, QuaterniusGripsLoadError> {
        let bytes = std::fs::read(path.as_ref())?;
        let grips: QuaterniusGrips = serde_yaml::from_slice(&bytes)?;
        Ok(grips)
    }
}

#[derive(Debug, Error)]
pub enum QuaterniusGripsLoadError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("yaml: {0}")]
    Yaml(#[from] serde_yaml::Error),
}
