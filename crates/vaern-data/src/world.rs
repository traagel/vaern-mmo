//! Zone / Hub / Mob loaders. The world layout lives under
//! `src/generated/world/zones/<zone>/{core.yaml, hubs/*.yaml, mobs/*.yaml}`.

use std::{collections::HashMap, fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::{read_dir, spatial::CoordinateSystem, Bounds, Coord2, LoadError};

// ─── shared ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct LevelRange {
    pub min: u32,
    pub max: u32,
}

// ─── biome + continent ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct Biome {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub climate: String,
    #[serde(default)]
    pub hazards: Vec<String>,
    #[serde(default)]
    pub typical_flora: Vec<String>,
    #[serde(default)]
    pub typical_fauna: Vec<String>,
    #[serde(default)]
    pub faction_affinity: String,
}

pub fn load_biomes(root: impl AsRef<Path>) -> Result<Vec<Biome>, LoadError> {
    let root = root.as_ref();
    let mut out = Vec::new();
    for path in read_dir(root)? {
        if !is_loadable_yaml(&path) {
            continue;
        }
        let text = fs::read_to_string(&path).map_err(|e| LoadError::Io {
            path: path.clone(),
            source: e,
        })?;
        out.push(serde_yaml::from_str(&text).map_err(|e| LoadError::Yaml {
            path: path.clone(),
            source: e,
        })?);
    }
    out.sort_by(|a: &Biome, b| a.id.cmp(&b.id));
    Ok(out)
}

// ─── zone ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct ZoneBudgetHours {
    pub solo: f32,
    pub duo: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ZoneBudget {
    pub quest_count_target: u32,
    pub unique_mob_types: u32,
    pub mob_kills_to_complete: u32,
    pub estimated_hours_to_complete: ZoneBudgetHours,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Zone {
    pub id: String,
    pub name: String,
    pub faction_control: String,
    pub biome: String,
    pub region: String,
    pub tier: String,
    pub level_range: LevelRange,
    #[serde(default)]
    pub starter_race: Option<String>,
    pub hub_count: u32,
    pub budget: ZoneBudget,
    #[serde(default)]
    pub notes: String,
    /// Axis-aligned bounding box in zone-local meters. Optional during
    /// the schema migration; required by the cartography validator.
    #[serde(default)]
    pub bounds: Option<Bounds>,
    /// Names which hub is at `(0, 0)` and declares the engine axis
    /// convention. Optional during migration; required by the validator.
    #[serde(default)]
    pub coordinate_system: Option<CoordinateSystem>,
    /// World-dressing scatter rules applied to the voxel ground inside
    /// this zone's footprint. Rendered client-side only.
    #[serde(default)]
    pub scatter: Vec<ScatterRule>,
}

/// One dressing-scatter rule. Client-side scatter samples Poisson-disk
/// positions within the zone's footprint where the local biome matches
/// `biome` and the terrain slope is <= `max_slope_deg`, then picks a
/// random [`crate::world::...`] slug from the Poly Haven catalog matching
/// `category`.
///
/// Deterministic across clients via `zone_seed` XOR `seed_salt` so every
/// player sees the same world without per-prop replication.
#[derive(Debug, Clone, Deserialize)]
pub struct ScatterRule {
    /// Biome key to match (e.g. `"river_valley"`, `"grass"`, `"stone"`).
    /// Set to `"*"` to match any biome.
    pub biome: String,
    /// Poly Haven category: `tree` / `dead_wood` / `rock` / `ground_cover`
    /// / `shrub`. Drives mesh selection from the catalog.
    pub category: String,
    /// Props per 100 m² footprint at the nominal density setting.
    pub density_per_100m2: f32,
    /// Minimum spacing between instances of this rule, in meters.
    pub min_spacing: f32,
    /// Cull placements on terrain steeper than this, in degrees.
    #[serde(default = "default_max_slope")]
    pub max_slope_deg: f32,
    /// Keep this many meters clear around every hub center.
    #[serde(default)]
    pub exclude_radius_from_hubs: f32,
    /// Per-rule seed offset so two rules over the same biome don't overlap.
    #[serde(default)]
    pub seed_salt: u32,
}

fn default_max_slope() -> f32 {
    45.0
}

// ─── hub ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct Hub {
    pub id: String,
    pub zone: String,
    pub name: String,
    pub role: String,
    #[serde(default)]
    pub amenities: Vec<String>,
    pub quest_givers: u32,
    /// Position relative to the zone's world origin. When `None`, the
    /// server falls back to a tiny radial layout (legacy behavior).
    /// Expressed as `(x, z)` — Y is sampled from the shared terrain.
    #[serde(default)]
    pub offset_from_zone_origin: Option<Coord2>,
    /// Biome key for the client's Voronoi region renderer. Drives the
    /// per-hub floor-patch texture. Unknown biomes fall back to `grass`.
    /// Default is `grass` so existing zones without explicit biomes
    /// keep their current look.
    #[serde(default = "default_biome")]
    pub biome: String,
    /// Authored prop placements rendered around this hub, in hub-local
    /// coordinates (meters, +X east, +Z south). Y is sampled from the
    /// voxel terrain at spawn time.
    #[serde(default)]
    pub props: Vec<AuthoredProp>,
}

/// One hand-placed prop in a hub. Slug must match a
/// `PolyHavenEntry::slug` in the catalog; unknown slugs are logged and
/// skipped at load time.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthoredProp {
    /// Poly Haven catalog slug (e.g. `"wooden_barrels_01"`).
    pub slug: String,
    /// Hub-local offset in meters. Y is sampled from voxel ground unless
    /// `absolute_y` is set.
    pub offset: Coord2,
    /// Facing in degrees around Y axis. 0 = facing -Z.
    #[serde(default)]
    pub rotation_y_deg: f32,
    /// Uniform scale. Defaults to 1.0.
    #[serde(default = "default_scale")]
    pub scale: f32,
    /// Override automatic voxel-ground Y-snap. Use for lanterns on walls
    /// or banners hung above doorways.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub absolute_y: Option<f32>,
}

fn default_scale() -> f32 {
    1.0
}

fn default_biome() -> String {
    "grass".to_string()
}

// ─── mob ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct MobDamage {
    pub primary_school: String,
    pub attack_range: String,
    pub dps_tier: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MobBehavior {
    pub aggro_range: String,
    pub social_radius: u32,
    pub flee_threshold: f32,
    pub calls_allies: bool,
    pub patrol: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MobDrops {
    pub gold_copper_avg: u32,
    pub item_hint: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Mob {
    pub id: String,
    pub name: String,
    pub zone: String,
    pub level: u32,
    /// Reference into `Bestiary::creature_types`.
    pub creature_type: String,
    /// Reference into `Bestiary::armor_classes`.
    pub armor_class: String,
    pub rarity: String,
    pub role: String,
    pub faction_alignment: String,
    pub hp_tier: String,
    pub damage: MobDamage,
    pub behavior: MobBehavior,
    pub loot_tier: String,
    pub drops: MobDrops,
    #[serde(default)]
    pub biome_context: String,
    #[serde(default)]
    pub chain_target: bool,
}

impl Mob {
    /// Rarity multiplier applied on top of creature_type base hp, per
    /// `bestiary/_schema.yaml`.
    pub fn rarity_hp_multiplier(&self) -> f32 {
        match self.rarity.as_str() {
            "common" => 1.00,
            "elite" => 2.75,
            "rare" => 3.50,
            "named" => 5.00,
            _ => 1.00,
        }
    }
}

// ─── aggregate ───────────────────────────────────────────────────────────────

/// Complete world snapshot loaded from `src/generated/world/`.
#[derive(Debug, Default)]
pub struct World {
    pub biomes: Vec<Biome>,
    pub zones: Vec<Zone>,
    pub hubs: Vec<Hub>,
    pub mobs: Vec<Mob>,
}

impl World {
    pub fn zone(&self, id: &str) -> Option<&Zone> {
        self.zones.iter().find(|z| z.id == id)
    }

    pub fn hub(&self, id: &str) -> Option<&Hub> {
        self.hubs.iter().find(|h| h.id == id)
    }

    pub fn mobs_in_zone(&self, zone_id: &str) -> impl Iterator<Item = &Mob> {
        self.mobs.iter().filter(move |m| m.zone == zone_id)
    }

    pub fn hubs_in_zone(&self, zone_id: &str) -> impl Iterator<Item = &Hub> {
        self.hubs.iter().filter(move |h| h.zone == zone_id)
    }

    pub fn by_zone_index(&self) -> HashMap<&str, &Zone> {
        self.zones.iter().map(|z| (z.id.as_str(), z)).collect()
    }
}

pub fn load_world(world_root: impl AsRef<Path>) -> Result<World, LoadError> {
    let root = world_root.as_ref();

    let biomes = load_biomes(root.join("biomes"))?;
    let mut zones: Vec<Zone> = Vec::new();
    let mut hubs: Vec<Hub> = Vec::new();
    let mut mobs: Vec<Mob> = Vec::new();

    let zones_dir = root.join("zones");
    for zone_dir in read_dir(&zones_dir)? {
        if !zone_dir.is_dir() {
            continue;
        }
        // zone core
        let zone_core = zone_dir.join("core.yaml");
        if zone_core.exists() {
            let text = fs::read_to_string(&zone_core).map_err(|e| LoadError::Io {
                path: zone_core.clone(),
                source: e,
            })?;
            let zone: Zone = serde_yaml::from_str(&text).map_err(|e| LoadError::Yaml {
                path: zone_core.clone(),
                source: e,
            })?;
            zones.push(zone);
        }
        // hubs
        let hubs_dir = zone_dir.join("hubs");
        if hubs_dir.exists() {
            for path in read_dir(&hubs_dir)? {
                if !is_loadable_yaml(&path) {
                    continue;
                }
                let text = fs::read_to_string(&path).map_err(|e| LoadError::Io {
                    path: path.clone(),
                    source: e,
                })?;
                let hub: Hub = serde_yaml::from_str(&text).map_err(|e| LoadError::Yaml {
                    path: path.clone(),
                    source: e,
                })?;
                hubs.push(hub);
            }
        }
        // mobs
        let mobs_dir = zone_dir.join("mobs");
        if mobs_dir.exists() {
            for path in read_dir(&mobs_dir)? {
                if !is_loadable_yaml(&path) {
                    continue;
                }
                let text = fs::read_to_string(&path).map_err(|e| LoadError::Io {
                    path: path.clone(),
                    source: e,
                })?;
                let mob: Mob = serde_yaml::from_str(&text).map_err(|e| LoadError::Yaml {
                    path: path.clone(),
                    source: e,
                })?;
                mobs.push(mob);
            }
        }
    }

    zones.sort_by(|a, b| a.id.cmp(&b.id));
    hubs.sort_by(|a, b| a.id.cmp(&b.id));
    mobs.sort_by(|a, b| a.id.cmp(&b.id));

    Ok(World {
        biomes,
        zones,
        hubs,
        mobs,
    })
}

fn is_loadable_yaml(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "yaml")
        && !path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with('_'))
}
