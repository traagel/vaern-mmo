use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;
use thiserror::Error;
use vaern_core::{Pillar, School};

pub mod bestiary;
pub mod boss_drops;
pub mod dungeon;
pub mod flavored;
pub mod landmark;
pub mod quest;
pub mod race;
pub mod world;

pub use bestiary::{load_bestiary, Affinities, ArmorClass, Bestiary, CreatureType};
pub use boss_drops::{load_all_boss_drops, BossDropEntry, BossDrops};
pub use dungeon::{load_dungeons, Boss, Dungeon};
pub use flavored::{
    load_flavored, FlavoredAbility, FlavoredEffect, FlavoredEffectKind, FlavoredIndex,
    FlavoredShape,
};
pub use landmark::{load_all_landmarks, Landmark, LandmarkIndex, LandmarkOffset};
pub use quest::{
    load_all_side_quests, HubSideQuests, SideQuest, SideQuestIndex,
};
pub use quest::{
    load_all_chains, ItemRequirement, ItemReward, QuestChain, QuestChainFinalReward, QuestIndex,
    QuestNpc, QuestObjective, QuestStep,
};
pub use race::{load_races, Race, RacePillarAffinity};
pub use world::{
    load_biomes, load_world, AuthoredProp, Biome, Hub, HubOffset, LevelRange, Mob, PropOffset,
    ScatterRule, World, Zone,
};

#[derive(Debug, Error)]
pub enum LoadError {
    #[error("io at {path:?}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
    #[error("yaml parse at {path:?}: {source}")]
    Yaml { path: PathBuf, source: serde_yaml::Error },
}

/// Template loader. Still a stub — the roster yaml schema is in flux.
#[derive(Debug, Deserialize)]
pub struct RosterEntry {
    pub position: [u8; 3],
    pub label: String,
}

#[derive(Debug, Deserialize)]
pub struct Roster {
    pub classes: Vec<RosterEntry>,
}

pub fn load_roster(path: impl AsRef<Path>) -> Result<Roster, LoadError> {
    let path = path.as_ref();
    let text = fs::read_to_string(path).map_err(|e| LoadError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    serde_yaml::from_str(&text).map_err(|e| LoadError::Yaml {
        path: path.to_path_buf(),
        source: e,
    })
}

/// Load every school yaml under `root` (expected layout: `root/{pillar}/{name}.yaml`).
/// Returns all schools as a Vec; use `into_index` to build an id-keyed lookup.
pub fn load_schools(root: impl AsRef<Path>) -> Result<Vec<School>, LoadError> {
    let root = root.as_ref();
    let mut schools = Vec::new();

    for pillar_entry in read_dir(root)? {
        let pillar_dir = pillar_entry;
        if !pillar_dir.is_dir() {
            continue;
        }
        for school_entry in read_dir(&pillar_dir)? {
            let path = school_entry;
            if path.extension().is_some_and(|e| e == "yaml") {
                let text = fs::read_to_string(&path).map_err(|e| LoadError::Io {
                    path: path.clone(),
                    source: e,
                })?;
                let school: School = serde_yaml::from_str(&text).map_err(|e| LoadError::Yaml {
                    path: path.clone(),
                    source: e,
                })?;
                schools.push(school);
            }
        }
    }

    Ok(schools)
}

pub fn into_index(schools: Vec<School>) -> HashMap<String, School> {
    schools.into_iter().map(|s| (s.id.clone(), s)).collect()
}

// ─── classes ────────────────────────────────────────────────────────────────

/// Per-pillar data on a class (both `active_tiers` and `capabilities` use this).
#[derive(Debug, Clone, Deserialize)]
pub struct PillarList<T> {
    #[serde(default)]
    pub might: Vec<T>,
    #[serde(default)]
    pub arcana: Vec<T>,
    #[serde(default)]
    pub finesse: Vec<T>,
}

impl<T> PillarList<T> {
    pub fn for_pillar(&self, pillar: Pillar) -> &[T] {
        match pillar {
            Pillar::Might => &self.might,
            Pillar::Arcana => &self.arcana,
            Pillar::Finesse => &self.finesse,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct PositionYaml {
    pub might: u8,
    pub arcana: u8,
    pub finesse: u8,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClassDef {
    pub class_id: u8,
    pub internal_label: String,
    pub position: PositionYaml,
    #[serde(default)]
    pub primary_roles: Vec<String>,
    pub active_tiers: PillarList<u8>,
}

impl ClassDef {
    /// Highest active tier for the given pillar, or `None` if the class has no
    /// access to that pillar.
    pub fn max_tier(&self, pillar: Pillar) -> Option<u8> {
        self.active_tiers.for_pillar(pillar).iter().copied().max()
    }
}

/// Load every archetype yaml under `root`. Current layout:
/// `root/NN_label/core.yaml`. Skips files/dirs whose name starts with `_`.
pub fn load_classes(root: impl AsRef<Path>) -> Result<Vec<ClassDef>, LoadError> {
    let root = root.as_ref();
    let mut classes = Vec::new();
    for path in read_dir(root)? {
        let is_meta = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with('_'));
        if is_meta {
            continue;
        }
        // Current layout: one directory per archetype, with `core.yaml` inside.
        let core = if path.is_dir() {
            path.join("core.yaml")
        } else if path.extension().is_some_and(|e| e == "yaml") {
            path.clone()
        } else {
            continue;
        };
        if !core.exists() {
            continue;
        }
        let text = fs::read_to_string(&core).map_err(|e| LoadError::Io {
            path: core.clone(),
            source: e,
        })?;
        let class: ClassDef = serde_yaml::from_str(&text).map_err(|e| LoadError::Yaml {
            path: core.clone(),
            source: e,
        })?;
        classes.push(class);
    }
    classes.sort_by_key(|c| c.class_id);
    Ok(classes)
}

// ─── abilities ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct AbilityVariant {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AbilityDef {
    pub pillar: Pillar,
    pub category: String,
    /// Map of tier (25/50/75/100) → list of variants. Scaffold uses variant[0].
    pub tiers: HashMap<u8, Vec<AbilityVariant>>,
}

impl AbilityDef {
    /// Pick a variant at the given tier. For scaffold we take the first entry.
    pub fn variant_at(&self, tier: u8) -> Option<&AbilityVariant> {
        self.tiers.get(&tier).and_then(|v| v.first())
    }

    /// Highest tier defined for this category that is ≤ `cap`.
    pub fn highest_tier_at_or_below(&self, cap: u8) -> Option<u8> {
        self.tiers.keys().copied().filter(|t| *t <= cap).max()
    }
}

/// `(pillar, category) → AbilityDef`. Built by `load_abilities`.
#[derive(Debug, Default, Clone)]
pub struct AbilityIndex(pub HashMap<(Pillar, String), AbilityDef>);

impl AbilityIndex {
    pub fn get(&self, pillar: Pillar, category: &str) -> Option<&AbilityDef> {
        self.0.get(&(pillar, category.to_owned()))
    }
}

/// Load every ability yaml under `root` (layout: `root/{pillar}/{category}.yaml`).
pub fn load_abilities(root: impl AsRef<Path>) -> Result<AbilityIndex, LoadError> {
    let root = root.as_ref();
    let mut idx: HashMap<(Pillar, String), AbilityDef> = HashMap::new();
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
            let def: AbilityDef = serde_yaml::from_str(&text).map_err(|e| LoadError::Yaml {
                path: path.clone(),
                source: e,
            })?;
            idx.insert((def.pillar, def.category.clone()), def);
        }
    }
    Ok(AbilityIndex(idx))
}

pub(crate) fn read_dir(path: &Path) -> Result<Vec<PathBuf>, LoadError> {
    let iter = fs::read_dir(path).map_err(|e| LoadError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    let mut out = Vec::new();
    for entry in iter {
        let entry = entry.map_err(|e| LoadError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;
        out.push(entry.path());
    }
    out.sort();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Resolve path to the project's src/generated relative to this crate.
    fn generated_root() -> PathBuf {
        // CARGO_MANIFEST_DIR = crates/vaern-data
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        manifest.join("../../src/generated").canonicalize().unwrap()
    }

    #[test]
    fn loads_all_schools() {
        let schools = load_schools(generated_root().join("schools")).unwrap();
        // 10 arcana + 7 might + 10 finesse = 27 (added alchemy after initial spec)
        assert_eq!(schools.len(), 27, "expected 27 schools, got {}", schools.len());
    }

    #[test]
    fn school_index_has_expected_entries() {
        let schools = load_schools(generated_root().join("schools")).unwrap();
        let idx = into_index(schools);
        assert!(idx.contains_key("fire"));
        assert!(idx.contains_key("blade"));
        assert!(idx.contains_key("poison"));
    }

    #[test]
    fn loads_all_fifteen_classes() {
        let classes = load_classes(generated_root().join("archetypes")).unwrap();
        assert_eq!(classes.len(), 15);
        assert_eq!(classes[0].internal_label, "Fighter");
        assert_eq!(classes[4].internal_label, "Wizard");
        assert_eq!(classes[14].internal_label, "Warden");
    }

    #[test]
    fn class_max_tier_matches_pillar_weight() {
        let classes = load_classes(generated_root().join("archetypes")).unwrap();
        let wizard = &classes[4];
        assert_eq!(wizard.max_tier(Pillar::Arcana), Some(100));
        assert_eq!(wizard.max_tier(Pillar::Might), None);
        let paladin = &classes[1]; // 75 / 25 / 0
        assert_eq!(paladin.max_tier(Pillar::Might), Some(75));
        assert_eq!(paladin.max_tier(Pillar::Arcana), Some(25));
        assert_eq!(paladin.max_tier(Pillar::Finesse), None);
    }

    // ─── bestiary + cross-reference tests ───────────────────────────────────

    #[test]
    fn loads_bestiary() {
        let b = load_bestiary(generated_root().join("bestiary")).unwrap();
        assert_eq!(b.creature_types.len(), 11, "expected 11 creature types");
        assert_eq!(b.armor_classes.len(), 10, "expected 10 armor classes");
        // Living-construct is the playable-gravewrought type
        assert!(b.creature_type("living_construct").is_some());
        assert!(b.creature_type("beast").is_some());
        assert!(b.armor_class("plate").is_some());
    }

    #[test]
    fn bestiary_hp_scaling_is_geometric() {
        let b = load_bestiary(generated_root().join("bestiary")).unwrap();
        let beast = b.creature_type("beast").unwrap();
        // 30 * 1.15^0 = 30 at L1
        assert_eq!(beast.base_hp_at_level(1), 30);
        // 30 * 1.15^9 = 30 * 3.517 ≈ 106 at L10
        assert_eq!(beast.base_hp_at_level(10), 106);
        // HP must grow monotonically with level
        assert!(beast.base_hp_at_level(30) > beast.base_hp_at_level(29));
    }

    #[test]
    fn bestiary_affinities_gate_schools() {
        let b = load_bestiary(generated_root().join("bestiary")).unwrap();
        let beast = b.creature_type("beast").unwrap();
        assert!(beast.affinities.is_legal("blade"));
        assert!(!beast.affinities.is_legal("arcane"));
        assert!(!beast.affinities.is_legal("light"));
        let humanoid = b.creature_type("humanoid").unwrap();
        // Humanoids should accept all well-known schools
        assert!(humanoid.affinities.is_legal("arcane"));
        assert!(humanoid.affinities.is_legal("blade"));
        assert!(humanoid.affinities.is_legal("poison"));
    }

    #[test]
    fn loads_all_races() {
        let races = load_races(generated_root().join("races")).unwrap();
        assert_eq!(races.len(), 10, "expected 10 playable races");
        assert!(races.iter().any(|r| r.id == "mannin"));
        assert!(races.iter().any(|r| r.id == "gravewrought"));
    }

    #[test]
    fn every_race_creature_type_resolves() {
        let b = load_bestiary(generated_root().join("bestiary")).unwrap();
        let races = load_races(generated_root().join("races")).unwrap();
        for r in &races {
            assert!(
                b.creature_type(&r.creature_type).is_some(),
                "race {} references unknown creature_type {:?}",
                r.id,
                r.creature_type
            );
        }
    }

    #[test]
    fn every_race_school_bonus_is_legal_for_its_type() {
        let b = load_bestiary(generated_root().join("bestiary")).unwrap();
        let races = load_races(generated_root().join("races")).unwrap();
        for r in &races {
            let ct = b.creature_type(&r.creature_type).unwrap();
            for school in &r.racial_school_bonuses {
                assert!(
                    !ct.affinities.forbidden.iter().any(|s| s == school),
                    "race {} grants bonus to {:?} but creature_type {} forbids that school",
                    r.id,
                    school,
                    ct.id
                );
            }
        }
    }

    // ─── world tests ────────────────────────────────────────────────────────

    #[test]
    fn loads_world_scaffold() {
        let w = load_world(generated_root().join("world")).unwrap();
        assert_eq!(w.biomes.len(), 9, "expected 9 biomes");
        assert_eq!(w.zones.len(), 28, "expected 28 zones");
        // 76 baseline + 3 new hubs from the dalewatch redesign
        // (harriers_rest, kingsroad_waypost, ford_of_ashmere).
        assert_eq!(w.hubs.len(), 79, "expected 79 hubs");
        // 603 baseline + 9 new mobs from the dalewatch redesign + 3 Slice 6
        // Drifter's Lair trash adds (drifter_brute L8, drifter_acolyte L9,
        // drifter_fanatic L10).
        assert_eq!(w.mobs.len(), 615, "expected 615 mobs");
    }

    #[test]
    fn every_mob_references_valid_bestiary() {
        let b = load_bestiary(generated_root().join("bestiary")).unwrap();
        let w = load_world(generated_root().join("world")).unwrap();
        for m in &w.mobs {
            let ct = b.creature_type(&m.creature_type).unwrap_or_else(|| {
                panic!("mob {} has unknown creature_type {:?}", m.id, m.creature_type)
            });
            b.armor_class(&m.armor_class).unwrap_or_else(|| {
                panic!("mob {} has unknown armor_class {:?}", m.id, m.armor_class)
            });
            // primary_school must be legal under the creature_type's affinities
            assert!(
                ct.affinities.is_legal(&m.damage.primary_school),
                "mob {} has illegal primary_school {:?} for creature_type {}",
                m.id,
                m.damage.primary_school,
                ct.id
            );
        }
    }

    #[test]
    fn every_hub_and_mob_references_valid_zone() {
        let w = load_world(generated_root().join("world")).unwrap();
        let zone_ids: std::collections::HashSet<&str> =
            w.zones.iter().map(|z| z.id.as_str()).collect();
        for h in &w.hubs {
            assert!(
                zone_ids.contains(h.zone.as_str()),
                "hub {} references unknown zone {}",
                h.id,
                h.zone
            );
        }
        for m in &w.mobs {
            assert!(
                zone_ids.contains(m.zone.as_str()),
                "mob {} references unknown zone {}",
                m.id,
                m.zone
            );
        }
    }

    #[test]
    fn zone_hub_counts_match_core_field() {
        let w = load_world(generated_root().join("world")).unwrap();
        for z in &w.zones {
            let actual = w.hubs_in_zone(&z.id).count() as u32;
            assert_eq!(
                actual, z.hub_count,
                "zone {} declares hub_count={} but has {} hub files",
                z.id, z.hub_count, actual
            );
        }
    }

    // ─── dungeon tests ──────────────────────────────────────────────────────

    #[test]
    fn loads_all_dungeons() {
        let d = load_dungeons(generated_root().join("world").join("dungeons")).unwrap();
        // 32 baseline + Slice 6 Drifter's Lair (Dalewatch L9-L10 capstone).
        assert_eq!(d.len(), 33, "expected 33 instances");
        let five_mans = d.iter().filter(|i| i.group_size == 5).count();
        let ten_mans = d.iter().filter(|i| i.group_size == 10).count();
        let twenty_mans = d.iter().filter(|i| i.group_size == 20).count();
        assert_eq!(five_mans, 27);
        assert_eq!(ten_mans, 4);
        assert_eq!(twenty_mans, 2);
    }

    #[test]
    fn every_dungeon_resolves_zone_and_hub() {
        let w = load_world(generated_root().join("world")).unwrap();
        let d = load_dungeons(generated_root().join("world").join("dungeons")).unwrap();
        let zone_ids: std::collections::HashSet<&str> =
            w.zones.iter().map(|z| z.id.as_str()).collect();
        let hub_ids: std::collections::HashSet<&str> =
            w.hubs.iter().map(|h| h.id.as_str()).collect();
        for inst in &d {
            assert!(
                zone_ids.contains(inst.zone.as_str()),
                "dungeon {} references unknown zone {}",
                inst.id,
                inst.zone
            );
            assert!(
                hub_ids.contains(inst.entrance_hub.as_str()),
                "dungeon {} references unknown hub {}",
                inst.id,
                inst.entrance_hub
            );
        }
    }

    #[test]
    fn every_dungeon_boss_count_matches_bosses_file() {
        let d = load_dungeons(generated_root().join("world").join("dungeons")).unwrap();
        for inst in &d {
            assert_eq!(
                inst.bosses.len() as u32, inst.boss_count,
                "dungeon {} declares boss_count={} but bosses.yaml has {} entries",
                inst.id,
                inst.boss_count,
                inst.bosses.len()
            );
        }
    }

    // ─── Slice 6 — Drifter's Lair guards ────────────────────────────────────

    #[test]
    fn drifter_valenn_authors_level_10() {
        let w = load_world(generated_root().join("world")).unwrap();
        let v = w
            .mobs
            .iter()
            .find(|m| m.id == "mob_dalewatch_marches_named_drifter_valenn")
            .expect("Valenn mob must exist");
        assert_eq!(v.level, 10, "Valenn is the L10 capstone boss for Slice 6");
    }

    #[test]
    fn drifter_halen_authors_level_9() {
        let w = load_world(generated_root().join("world")).unwrap();
        let h = w
            .mobs
            .iter()
            .find(|m| m.id == "mob_dalewatch_marches_named_drifter_master")
            .expect("Halen mob must exist");
        assert_eq!(h.level, 9, "Halen is the L9 mini-boss for Slice 6");
    }

    #[test]
    fn drifters_lair_dungeon_yaml_loads_with_two_bosses() {
        let d = load_dungeons(generated_root().join("world").join("dungeons")).unwrap();
        let lair = d
            .iter()
            .find(|i| i.id == "drifters_lair")
            .expect("drifters_lair dungeon must exist");
        assert_eq!(lair.zone, "dalewatch_marches");
        assert_eq!(lair.entrance_hub, "ford_of_ashmere");
        assert_eq!(lair.level_range.min, 9);
        assert_eq!(lair.level_range.max, 10);
        assert_eq!(lair.bosses.len(), 2);
        assert!(lair.bosses.iter().any(|b| b.id == "master_drifter_halen"));
        assert!(lair.bosses.iter().any(|b| b.id == "grand_drifter_valenn"));
    }

    #[test]
    fn first_ride_step_10_targets_valenn_at_level_10() {
        let chains = load_all_chains(generated_root().join("world")).unwrap();
        let chain = chains
            .chains
            .get("chain_dalewatch_first_ride")
            .expect("first-ride chain must exist");
        let step10 = chain
            .steps
            .iter()
            .find(|s| s.step == 10)
            .expect("step 10 must exist");
        assert_eq!(step10.level, 10, "step 10 (kill Valenn) must be the L10 capstone");
        assert_eq!(
            step10.objective.mob_id.as_deref(),
            Some("mob_dalewatch_marches_named_drifter_valenn"),
        );
    }

    #[test]
    fn ability_index_covers_expected_categories() {
        let idx = load_abilities(generated_root().join("abilities")).unwrap();
        assert!(idx.get(Pillar::Arcana, "damage").is_some());
        assert!(idx.get(Pillar::Might, "threat").is_some());
        assert!(idx.get(Pillar::Finesse, "precision").is_some());
        // Check tier resolution
        let damage = idx.get(Pillar::Arcana, "damage").unwrap();
        assert_eq!(damage.variant_at(50).map(|v| v.name.as_str()), Some("firebolt"));
        assert_eq!(damage.highest_tier_at_or_below(75), Some(75));
    }

    // ─── world dressing schema ────────────────────────────────────────────────

    #[test]
    fn dalewatch_zone_has_scatter_rules() {
        let world = load_world(generated_root().join("world")).unwrap();
        let dalewatch = world.zone("dalewatch_marches").expect("dalewatch zone");
        assert!(
            dalewatch.scatter.len() >= 3,
            "expected >=3 scatter rules, got {}",
            dalewatch.scatter.len()
        );
        // Every rule must target a valid category
        for rule in &dalewatch.scatter {
            assert!(
                matches!(
                    rule.category.as_str(),
                    "tree" | "dead_wood" | "rock" | "ground_cover" | "shrub"
                ),
                "unknown scatter category {:?}",
                rule.category
            );
            assert!(rule.density_per_100m2 > 0.0);
            assert!(rule.min_spacing > 0.0);
        }
    }

    // ─── side quests + givers ─────────────────────────────────────────────────

    #[test]
    fn dalewatch_side_quests_have_givers() {
        let idx = load_all_side_quests(generated_root().join("world")).unwrap();
        let hubs: Vec<&str> = idx
            .by_zone
            .get("dalewatch_marches")
            .map(|v| v.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default();
        assert_eq!(hubs.len(), 5, "expected 5 dalewatch hubs with side quests, got {hubs:?}");
        for hub_id in &hubs {
            let bundle = idx.for_hub(hub_id).unwrap();
            assert!(!bundle.quests.is_empty(), "hub {hub_id} has no side quests");
            let giver = bundle.giver.as_ref().unwrap_or_else(|| {
                panic!("hub {hub_id} side-quest bundle has no giver — Slice 2 regression")
            });
            assert!(!giver.id.is_empty());
            assert!(!giver.display_name.is_empty());
            assert_eq!(
                giver.hub_id.as_deref().unwrap_or(*hub_id),
                *hub_id,
                "giver hub_id must match bundle hub"
            );
        }
    }

    #[test]
    fn dalewatch_hubs_have_authored_props() {
        let world = load_world(generated_root().join("world")).unwrap();
        let keep = world.hub("dalewatch_keep").expect("keep hub");
        assert!(
            keep.props.len() >= 15,
            "capital expected >=15 props, got {}",
            keep.props.len()
        );
        // Every slug must resolve in the Poly Haven catalog — defensive
        // check so authoring typos surface at test time.
        let known: std::collections::HashSet<&str> =
            POLYHAVEN_CURATED_SLUGS.iter().copied().collect();
        for prop in &keep.props {
            assert!(
                known.contains(prop.slug.as_str()),
                "unknown polyhaven slug {:?} in dalewatch_keep props",
                prop.slug
            );
        }

        for id in [
            "harriers_rest",
            "kingsroad_waypost",
            "miller_crossing",
            "ford_of_ashmere",
        ] {
            let hub = world.hub(id).unwrap_or_else(|| panic!("hub {id}"));
            assert!(!hub.props.is_empty(), "hub {id} should have props");
            for prop in &hub.props {
                assert!(
                    known.contains(prop.slug.as_str()),
                    "unknown polyhaven slug {:?} in {id} props",
                    prop.slug
                );
            }
        }
    }

    /// Mirror of `crates/vaern-assets/src/polyhaven/mod.rs::CURATED` slug
    /// list. Kept inline here to avoid a dependency edge from `vaern-data`
    /// to `vaern-assets`. If the Poly Haven pack changes, update both.
    const POLYHAVEN_CURATED_SLUGS: &[&str] = &[
        "pine_sapling_small",
        "fir_sapling",
        "fir_sapling_medium",
        "dead_tree_trunk",
        "dead_tree_trunk_02",
        "tree_stump_01",
        "tree_stump_02",
        "pine_roots",
        "root_cluster_01",
        "single_root",
        "dry_branches_medium_01",
        "boulder_01",
        "rock_07",
        "rock_09",
        "rock_face_01",
        "rock_face_02",
        "rock_moss_set_01",
        "rock_moss_set_02",
        "stone_01",
        "mountainside",
        "coast_rocks_01",
        "grass_medium_01",
        "grass_medium_02",
        "grass_bermuda_01",
        "moss_01",
        "fern_02",
        "dandelion_01",
        "shrub_01",
        "shrub_02",
        "shrub_03",
        "shrub_04",
        "celandine_01",
        "wooden_barrels_01",
        "wooden_crate_02",
        "wooden_bucket_01",
        "wooden_bucket_02",
        "wooden_bowl_01",
        "wooden_lantern_01",
        "Lantern_01",
        "vintage_oil_lamp",
        "wooden_candlestick",
        "lantern_chandelier_01",
        "treasure_chest",
        "WoodenTable_01",
        "WoodenChair_01",
        "large_castle_door",
        "large_iron_gate",
        "modular_fort_01",
        "stone_fire_pit",
        "spinning_wheel_01",
        "horse_statue_01",
        "katana_stand_01",
        "antique_estoc",
        "kite_shield",
        "ornate_medieval_dagger",
        "ornate_medieval_mace",
        "ornate_war_hammer",
    ];
}
