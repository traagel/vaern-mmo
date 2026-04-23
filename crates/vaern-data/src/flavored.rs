//! Flavored-ability loader. One yaml per (pillar, category) at
//! `flavored/<pillar>/<category>.yaml`, each containing `variants[tier][school]`
//! entries with name/description/damage_type. The derived `id` is
//! `{pillar}.{category}.{tier}.{school}.{name}` and matches the icon filename
//! convention under `icons/`.

use std::{collections::HashMap, fs, path::Path};

use serde::Deserialize;
use vaern_core::Pillar;

use crate::{read_dir, LoadError};

/// Ability shape override from YAML. Maps 1:1 to `vaern_combat::AbilityShape`
/// but lives in the data crate to keep vaern-data decoupled from bevy/combat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlavoredShape {
    Target,
    AoeOnTarget,
    AoeOnSelf,
    Cone,
    Line,
    Projectile,
}

/// Status-effect rider parsed from YAML. Mirrors `vaern_combat::EffectSpec`
/// but stays in vaern-data so this crate doesn't pull in bevy. Converted
/// to the combat type at `apply_flavored_overrides` time.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct FlavoredEffect {
    pub id: String,
    pub duration_secs: f32,
    #[serde(flatten)]
    pub kind: FlavoredEffectKind,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum FlavoredEffectKind {
    Dot {
        dps: f32,
        tick_interval: f32,
        #[serde(default)]
        school: Option<String>,
    },
    Slow {
        speed_mult: f32,
    },
}

#[derive(Debug, Clone, Deserialize)]
struct FlavoredVariantRaw {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    baseline_slot: Option<String>,
    #[serde(default)]
    school: Option<String>,
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    morality: String,
    #[serde(default)]
    damage_type: String,
    // Optional stat overrides. Each replaces the tier/pillar default when set.
    #[serde(default)]
    damage: Option<f32>,
    #[serde(default)]
    cast_secs: Option<f32>,
    #[serde(default)]
    cooldown_secs: Option<f32>,
    #[serde(default)]
    resource_cost: Option<f32>,
    #[serde(default)]
    range: Option<f32>,
    #[serde(default)]
    shape: Option<FlavoredShape>,
    #[serde(default)]
    aoe_radius: Option<f32>,
    #[serde(default)]
    cone_half_angle_deg: Option<f32>,
    #[serde(default)]
    line_width: Option<f32>,
    #[serde(default)]
    projectile_speed: Option<f32>,
    #[serde(default)]
    projectile_radius: Option<f32>,
    #[serde(default)]
    applies_effect: Option<FlavoredEffect>,
}

#[derive(Debug, Clone, Deserialize)]
struct FlavoredFile {
    pillar: Pillar,
    category: String,
    /// Outer key: tier (25/50/75/100). Inner key: school id.
    variants: HashMap<u8, HashMap<String, FlavoredVariantRaw>>,
}

#[derive(Debug, Clone)]
pub struct FlavoredAbility {
    /// `{pillar}.{category}.{tier}.{school}.{name}` — also the icon base name.
    pub id: String,
    pub pillar: Pillar,
    pub category: String,
    pub tier: u8,
    pub school: String,
    pub name: String,
    pub description: String,
    pub damage_type: String,
    pub morality: String,
    // Optional stat overrides. `None` = use tier/pillar default.
    pub damage: Option<f32>,
    pub cast_secs: Option<f32>,
    pub cooldown_secs: Option<f32>,
    pub resource_cost: Option<f32>,
    pub range: Option<f32>,
    pub shape: Option<FlavoredShape>,
    pub aoe_radius: Option<f32>,
    pub cone_half_angle_deg: Option<f32>,
    pub line_width: Option<f32>,
    pub projectile_speed: Option<f32>,
    pub projectile_radius: Option<f32>,
    /// Optional status-effect applied to hit targets (DoT / Slow / …).
    pub applies_effect: Option<FlavoredEffect>,
}

impl FlavoredAbility {
    /// "icons/<id>.png" relative to the generated root.
    pub fn icon_basename(&self) -> String {
        format!("{}.png", self.id)
    }
}

/// Keyed by `(pillar, category, tier, school)`.
#[derive(Debug, Default, Clone)]
pub struct FlavoredIndex {
    by_key: HashMap<(Pillar, String, u8, String), FlavoredAbility>,
}

impl FlavoredIndex {
    pub fn get(
        &self,
        pillar: Pillar,
        category: &str,
        tier: u8,
        school: &str,
    ) -> Option<&FlavoredAbility> {
        self.by_key
            .get(&(pillar, category.to_owned(), tier, school.to_owned()))
    }

    pub fn len(&self) -> usize {
        self.by_key.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_key.is_empty()
    }
}

fn pillar_str(p: Pillar) -> &'static str {
    match p {
        Pillar::Might => "might",
        Pillar::Arcana => "arcana",
        Pillar::Finesse => "finesse",
    }
}

/// Load every `flavored/<pillar>/<category>.yaml`. Each yaml unpacks into N
/// (tier × school) variants; the derived id is the canonical handle.
pub fn load_flavored(root: impl AsRef<Path>) -> Result<FlavoredIndex, LoadError> {
    let root = root.as_ref();
    let mut out = FlavoredIndex::default();
    for pillar_dir in read_dir(root)? {
        if !pillar_dir.is_dir() {
            continue;
        }
        for path in read_dir(&pillar_dir)? {
            if path.extension().is_none_or(|e| e != "yaml") {
                continue;
            }
            let text = fs::read_to_string(&path).map_err(|e| LoadError::Io {
                path: path.clone(),
                source: e,
            })?;
            let file: FlavoredFile =
                serde_yaml::from_str(&text).map_err(|e| LoadError::Yaml {
                    path: path.clone(),
                    source: e,
                })?;
            for (tier, by_school) in file.variants {
                for (school_key, raw) in by_school {
                    // `school` key in the outer map is authoritative; raw.school may be absent.
                    let school = raw.school.clone().unwrap_or_else(|| school_key.clone());
                    let id = raw.id.clone().unwrap_or_else(|| {
                        format!(
                            "{}.{}.{}.{}.{}",
                            pillar_str(file.pillar),
                            file.category,
                            tier,
                            school,
                            raw.name
                        )
                    });
                    let _ = raw.baseline_slot; // presently unused; preserved in yaml for tooling
                    let ability = FlavoredAbility {
                        id,
                        pillar: file.pillar,
                        category: file.category.clone(),
                        tier,
                        school: school.clone(),
                        name: raw.name,
                        description: raw.description,
                        damage_type: raw.damage_type,
                        morality: raw.morality,
                        damage: raw.damage,
                        cast_secs: raw.cast_secs,
                        cooldown_secs: raw.cooldown_secs,
                        resource_cost: raw.resource_cost,
                        range: raw.range,
                        shape: raw.shape,
                        aoe_radius: raw.aoe_radius,
                        cone_half_angle_deg: raw.cone_half_angle_deg,
                        line_width: raw.line_width,
                        projectile_speed: raw.projectile_speed,
                        projectile_radius: raw.projectile_radius,
                        applies_effect: raw.applies_effect,
                    };
                    out.by_key.insert(
                        (file.pillar, file.category.clone(), tier, school),
                        ability,
                    );
                }
            }
        }
    }
    Ok(out)
}
