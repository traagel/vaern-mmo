//! Game-data loading. `GameData` is a catalog of YAML content the server
//! needs at runtime (classes, abilities, world, bestiary, quests) + a derived
//! zone-ring layout built at startup. `XpCurve` lives in its own resource so
//! xp-award systems don't need a full `GameData` handle.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use vaern_character::XpCurve;
use vaern_core::School;
use vaern_data::{
    AbilityIndex, BossDrops, ClassDef, LandmarkIndex, QuestIndex, Race, SideQuestIndex, into_index,
    load_abilities, load_all_boss_drops, load_all_landmarks, load_all_side_quests, load_classes,
    load_races, load_schools,
};
use vaern_items::ContentRegistry;

/// Catalog of design data the server needs at runtime.
#[derive(Resource)]
pub struct GameData {
    /// 15 archetype definitions — reserved for the archetype-unlock path.
    /// Starter characters commit to a pillar only.
    #[allow(dead_code)]
    pub classes: Vec<ClassDef>,
    pub abilities: AbilityIndex,
    pub flavored: vaern_data::FlavoredIndex,
    /// Full world snapshot (zones, hubs, mobs) loaded from `world/`.
    pub world: vaern_data::World,
    /// Bestiary — used to resolve per-mob HP scaling via creature_type.
    pub bestiary: vaern_data::Bestiary,
    /// All main-chain quest definitions. Keyed by chain_id.
    pub quests: QuestIndex,
    /// Per-hub side-quest bundles. Each bundle has an authored giver
    /// NPC who hands out every side quest at that hub.
    pub side_quests: SideQuestIndex,
    /// Per-zone landmark registry. Used to anchor `QuestPoi` waypoints
    /// for `investigate` / `explore` quest steps.
    pub landmarks: LandmarkIndex,
    /// Playable races loaded from `src/generated/races/<id>/core.yaml`.
    /// Used by spawn to derive `PillarCaps` from `affinity`.
    pub races: Vec<Race>,
    /// Which starter zone hosts a race's spawn. Built by inverting each
    /// zone's `starter_race` field.
    pub race_to_zone: HashMap<String, String>,
    /// Where in world-space each starter zone lives. Populated starter zones
    /// are laid out on a 800u ring so two players picking different races
    /// land in distinct, spatially separated encounters.
    pub zone_offsets: HashMap<String, Vec3>,
    /// Compositional item registry — bases × materials × qualities.
    /// Loot tables generate `ItemInstance` tuples; `content.resolve(inst)`
    /// folds them into a display-ready `ResolvedItem`. Small on-disk,
    /// explosive variety at play time.
    pub content: ContentRegistry,
    /// School id → School lookup. Used by combat XP to credit pillar
    /// points to the caster based on the ability's school. Loaded from
    /// `src/generated/schools/{might,finesse,arcana}/*.yaml`.
    pub schools: HashMap<String, School>,
    /// Vendor NPC authoring — one entry per vendor, seeded into world
    /// hubs during `seed_npc_spawns`. Empty if the YAML is missing
    /// (dev-path; ships a warn log).
    pub vendors: Vec<VendorDef>,
    /// Slice 6 boss-drop ladder. Maps `MobSourceId` → guaranteed
    /// `ItemReward`s the named mob drops on kill. Loaded from
    /// `world/dungeons/<id>/loot.yaml`. Drops are merged into the
    /// post-kill `LootContainer` ahead of the random table roll;
    /// shared Need-Before-Greed-Pass distribution lands in Phase C.
    pub boss_drops: BossDrops,
}

/// Authored vendor NPC — placed at `hub_id` in `zone_id`, stocks the
/// listed items. Loaded from `src/generated/vendors.yaml`.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct VendorDef {
    pub id: String,
    pub display_name: String,
    pub zone_id: String,
    pub hub_id: String,
    /// Optional humanoid-archetype hint for the mesh (maps into
    /// `assets/npc_mesh_map.yaml`'s humanoid_archetypes table). Falls
    /// back to hashed-archetype if unset, same as other quest-givers.
    #[serde(default)]
    pub archetype: Option<String>,
    pub listings: Vec<VendorListingDef>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct VendorListingDef {
    pub base_id: String,
    #[serde(default)]
    pub material_id: Option<String>,
    #[serde(default)]
    pub quality_id: Option<String>,
    /// Optional stock cap. Unset = infinite supply.
    #[serde(default)]
    pub stock: Option<u32>,
}

impl VendorListingDef {
    pub fn to_stock_listing(&self) -> vaern_economy::VendorListing {
        vaern_economy::VendorListing {
            base_id: self.base_id.clone(),
            material_id: self.material_id.clone(),
            quality_id: self.quality_id.clone(),
            supply: match self.stock {
                Some(n) => vaern_economy::VendorSupply::Limited(n),
                None => vaern_economy::VendorSupply::Infinite,
            },
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
struct VendorsFile {
    #[serde(default)]
    vendors: Vec<VendorDef>,
}

fn load_vendors(path: &Path) -> Vec<VendorDef> {
    if !path.exists() {
        warn!("[vendors] {} missing; no vendors will spawn", path.display());
        return Vec::new();
    }
    let text = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            warn!("[vendors] read {} failed: {e}", path.display());
            return Vec::new();
        }
    };
    let parsed: VendorsFile = match serde_yaml::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            warn!("[vendors] parse {} failed: {e}", path.display());
            return Vec::new();
        }
    };
    info!("[vendors] loaded {} from {}", parsed.vendors.len(), path.display());
    parsed.vendors
}

impl GameData {
    /// World-space center of a zone. Defaults to origin if the zone isn't
    /// one of the populated starters.
    pub fn zone_origin(&self, zone_id: &str) -> Vec3 {
        self.zone_offsets.get(zone_id).copied().unwrap_or(Vec3::ZERO)
    }

    /// Starter zone for a given race_id. Falls back to "dalewatch_marches"
    /// for empty / unknown race ids (matches mannin's starter zone).
    pub fn zone_for_race(&self, race_id: &str) -> &str {
        self.race_to_zone
            .get(race_id)
            .map(String::as_str)
            .unwrap_or("dalewatch_marches")
    }
}

/// Resolve `src/generated/` relative to `CARGO_MANIFEST_DIR`. Works for `cargo
/// run` from anywhere in the tree; if the binary is moved outside the repo,
/// set `VAERN_DATA_DIR` explicitly.
pub fn data_root() -> PathBuf {
    if let Ok(env) = std::env::var("VAERN_DATA_DIR") {
        return PathBuf::from(env);
    }
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest.join("../../src/generated")
}

pub fn load_xp_curve() -> XpCurve {
    let path = data_root().join("world/progression/xp_curve.yaml");
    match XpCurve::load_yaml(&path) {
        Ok(c) => {
            println!("loaded xp curve from {}", path.display());
            c
        }
        Err(e) => {
            println!("failed to load xp curve ({e}); using formula fallback only");
            XpCurve::default()
        }
    }
}

pub fn load_game_data() -> GameData {
    let root = data_root();
    let classes = load_classes(root.join("archetypes"))
        .expect("vaern-server: failed to load archetype YAMLs from src/generated/archetypes");
    let abilities = load_abilities(root.join("abilities"))
        .expect("vaern-server: failed to load ability YAMLs from src/generated/abilities");
    let flavored = vaern_data::load_flavored(root.join("flavored"))
        .expect("vaern-server: failed to load flavored ability YAMLs");
    let world = vaern_data::load_world(root.join("world"))
        .expect("vaern-server: failed to load world YAMLs from src/generated/world");
    let bestiary = vaern_data::load_bestiary(root.join("bestiary"))
        .expect("vaern-server: failed to load bestiary YAMLs from src/generated/bestiary");
    let quests = vaern_data::load_all_chains(root.join("world"))
        .expect("vaern-server: failed to load quest-chain YAMLs from src/generated/world");
    let side_quests = load_all_side_quests(root.join("world"))
        .expect("vaern-server: failed to load side-quest YAMLs from src/generated/world");
    println!(
        "loaded side quests: {} hubs across {} zones",
        side_quests.by_hub.len(),
        side_quests.by_zone.len()
    );
    let landmarks = load_all_landmarks(root.join("world"))
        .expect("vaern-server: failed to load landmark YAMLs from src/generated/world");
    println!(
        "loaded landmarks: {} entries across {} zones",
        landmarks.by_id.len(),
        landmarks.by_zone.len()
    );
    let races = load_races(root.join("races"))
        .expect("vaern-server: failed to load race YAMLs from src/generated/races");
    let schools_vec = load_schools(root.join("schools"))
        .expect("vaern-server: failed to load schools from src/generated/schools");
    println!("loaded {} schools", schools_vec.len());
    let schools = into_index(schools_vec);

    let mut content = ContentRegistry::new();
    let items_root = root.join("items");
    match content.load_tree(&items_root) {
        Ok(c) => println!(
            "loaded content: {} bases, {} materials, {} qualities, {} affixes from {}",
            c.bases, c.materials, c.qualities, c.affixes, items_root.display()
        ),
        Err(e) => panic!(
            "vaern-server: failed to load content from {}: {e}",
            items_root.display()
        ),
    }
    assert_eq!(classes.len(), 15, "expected 15 classes, got {}", classes.len());
    println!(
        "loaded {} classes, {} ability categories, {} flavored variants",
        classes.len(),
        abilities.0.len(),
        flavored.len()
    );
    println!(
        "loaded world: {} zones, {} hubs, {} mobs; bestiary: {} types / {} armor classes; quests: {} chains across {} zones",
        world.zones.len(),
        world.hubs.len(),
        world.mobs.len(),
        bestiary.creature_types.len(),
        bestiary.armor_classes.len(),
        quests.chains.len(),
        quests.by_zone.len(),
    );
    // Starter zones: anyone with a `starter_race` is a player-landable zone.
    // Lay them out on a big ring so distinct races spawn in distinct spots.
    let mut starters: Vec<_> = world
        .zones
        .iter()
        .filter_map(|z| z.starter_race.as_ref().map(|r| (z.id.clone(), r.clone())))
        .collect();
    starters.sort_by(|a, b| a.0.cmp(&b.0));
    let n_starters = starters.len() as f32;
    // Big-zone layout: each starter zone now carries ~1200u of playable
    // content (dalewatch redesign + follow-ups), so ring radius grew
    // from 800u → 2800u to keep adjacent zones from overlapping.
    let zone_ring_radius = 2800.0_f32;
    let mut zone_offsets: HashMap<String, Vec3> = HashMap::new();
    let mut race_to_zone: HashMap<String, String> = HashMap::new();
    for (i, (zone_id, race_id)) in starters.iter().enumerate() {
        let angle = (i as f32 / n_starters) * std::f32::consts::TAU;
        zone_offsets.insert(
            zone_id.clone(),
            Vec3::new(
                zone_ring_radius * angle.cos(),
                0.0,
                zone_ring_radius * angle.sin(),
            ),
        );
        race_to_zone.insert(race_id.clone(), zone_id.clone());
    }
    println!(
        "zone ring: {} starter zones mapped — {}",
        starters.len(),
        starters
            .iter()
            .map(|(z, r)| format!("{r}→{z}"))
            .collect::<Vec<_>>()
            .join(", "),
    );

    println!("loaded {} races", races.len());

    let vendors = load_vendors(&root.join("vendors.yaml"));

    let boss_drops = load_all_boss_drops(root.join("world").join("dungeons"))
        .expect("vaern-server: failed to load boss-drop YAMLs from src/generated/world/dungeons");
    println!("loaded boss drops: {} mob entries", boss_drops.len());

    GameData {
        classes,
        abilities,
        flavored,
        world,
        bestiary,
        quests,
        side_quests,
        landmarks,
        races,
        race_to_zone,
        zone_offsets,
        content,
        schools,
        vendors,
        boss_drops,
    }
}
