//! Player identity + progression components. Leaf crate (core-only deps) so
//! `vaern-sim` and `vaern-combat` don't pull it. Cross-boundary systems
//! (e.g. level-up bumps `Health.max`) live in the server, not here, to keep
//! character free of any combat dependency.

use std::{collections::HashMap, fs, path::Path};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Player progression state. `current` is XP banked toward the next level;
/// when it reaches `XpCurve::to_next(level)`, the level-up system subtracts
/// that threshold and increments `level`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Experience {
    pub current: u32,
    pub level: u32,
}

impl Default for Experience {
    fn default() -> Self {
        Self { current: 0, level: 1 }
    }
}

/// Replicated race id (e.g. `"mannin"`, `"skarn"`). Lives on the player
/// entity so clients can pick race-specific assets (portrait, voice lines)
/// without needing a second round-trip.
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerRace(pub String);

/// Loaded XP-to-next-level table, usually read from `world/progression/xp_curve.yaml`.
/// Keyed by level `L`; value = XP needed to advance `L → L+1`.
#[derive(Resource, Debug, Clone, Default)]
pub struct XpCurve {
    table: HashMap<u32, u32>,
}

impl XpCurve {
    /// XP needed to advance from `level` to `level + 1`. Falls back to the
    /// quadratic formula baked into the yaml (`400*L + 120*L^2`) for levels
    /// past the explicit table, so the curve stays monotonic forever.
    pub fn to_next(&self, level: u32) -> u32 {
        if let Some(&v) = self.table.get(&level) {
            return v;
        }
        let l = level as f64;
        (400.0 * l + 120.0 * l * l) as u32
    }

    /// Load the curve from a YAML file shaped like `world/progression/xp_curve.yaml`
    /// (fields: `table: { L: xp, ... }`).
    pub fn load_yaml(path: impl AsRef<Path>) -> Result<Self, LoadXpCurveError> {
        #[derive(Deserialize)]
        struct XpCurveYaml {
            #[serde(default)]
            table: HashMap<u32, u32>,
        }
        let path = path.as_ref();
        let text = fs::read_to_string(path).map_err(|e| LoadXpCurveError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        let parsed: XpCurveYaml =
            serde_yaml::from_str(&text).map_err(|e| LoadXpCurveError::Yaml {
                path: path.display().to_string(),
                source: e,
            })?;
        Ok(Self { table: parsed.table })
    }
}

#[derive(Debug, Error)]
pub enum LoadXpCurveError {
    #[error("io at {path}: {source}")]
    Io { path: String, source: std::io::Error },
    #[error("yaml parse at {path}: {source}")]
    Yaml {
        path: String,
        source: serde_yaml::Error,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_experience_is_level_one() {
        let xp = Experience::default();
        assert_eq!(xp.level, 1);
        assert_eq!(xp.current, 0);
    }

    #[test]
    fn curve_falls_back_to_formula_beyond_table() {
        // Empty curve — every lookup goes through the formula.
        let c = XpCurve::default();
        // 400*1 + 120*1 = 520 (matches L1 table value).
        assert_eq!(c.to_next(1), 520);
        // 400*30 + 120*900 = 12_000 + 108_000 = 120_000 (matches L30 yaml value).
        assert_eq!(c.to_next(30), 120_000);
    }

    #[test]
    fn curve_prefers_table_when_present() {
        let mut c = XpCurve::default();
        c.table.insert(5, 999);
        assert_eq!(c.to_next(5), 999);
        // Absent levels still use formula.
        assert_eq!(c.to_next(1), 520);
    }
}
