//! Poly Haven PBR world-dressing pack.
//!
//! CC0 PBR models downloaded into `assets/polyhaven/{slug}/` via
//! `scripts/download_polyhaven.py`. Each folder contains a
//! `{slug}_1k.gltf` plus its `.bin` buffer and `textures/*`.
//!
//! This module exposes a [`PolyHavenCatalog`] resource that classifies
//! every pack entry by category (tree / rock / ground cover / hub prop
//! / …) so the world-dressing scatter system can pick appropriate
//! meshes per biome.
//!
//! Unlike the Quaternius character outfits (which are composed at
//! runtime from multiple part meshes), each Poly Haven asset is a
//! single self-contained scene and is spawned with
//! `SceneRoot(assets.load(entry.scene_path()))`.

use std::collections::BTreeMap;

use bevy::prelude::*;

pub const POLYHAVEN_FOLDER_REL: &str = "polyhaven";

/// Coarse category — drives scatter rules + biome affinity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum PolyHavenCategory {
    /// Standing trees + saplings. Scatter sparse, large collision.
    Tree,
    /// Dead logs, stumps, roots, dried branches. Scatter medium, small collision.
    DeadWood,
    /// Boulders + cliff faces + stones. Scatter rare, big collision.
    Rock,
    /// Grass / moss / fern / flower ground cover. Scatter dense, no collision.
    GroundCover,
    /// Bushes + shrubs. Scatter medium, small collision.
    Shrub,
    /// Authored-placement hub dressing: barrels, wells, banners, doors.
    HubProp,
    /// Decorative weapon-rack props (shields, swords, hammers) that sit on a rack.
    WeaponRackDressing,
}

impl PolyHavenCategory {
    /// Default collision opt-in — `true` means the asset should get a
    /// collider when spawned so players can't walk through it.
    pub fn has_collision(self) -> bool {
        matches!(self, Self::Tree | Self::Rock | Self::HubProp)
    }
}

#[derive(Clone, Debug)]
pub struct PolyHavenEntry {
    /// Lowercased slug used as the folder name and asset key.
    pub slug: String,
    /// Human-readable label, derived from the slug.
    pub label: String,
    /// Bevy asset-server-relative path to the `.gltf`, e.g.
    /// `"polyhaven/pine_tree_01/pine_tree_01_1k.gltf"`.
    pub gltf_path: String,
    pub category: PolyHavenCategory,
}

impl PolyHavenEntry {
    /// Asset-server path to the default scene inside the glTF.
    pub fn scene_path(&self) -> String {
        format!("{}#Scene0", self.gltf_path)
    }
}

#[derive(Resource, Default, Debug)]
pub struct PolyHavenCatalog {
    entries: BTreeMap<String, PolyHavenEntry>,
}

impl PolyHavenCatalog {
    /// Build the catalog from the static slug → category table.
    ///
    /// Does not touch disk at build time — it just declares the pack's
    /// manifest. Actual asset loading happens when consumers call
    /// `asset_server.load(entry.scene_path())`. A missing `.gltf` on
    /// disk surfaces at that point as a Bevy asset-load warning.
    pub fn new() -> Self {
        let mut entries = BTreeMap::new();
        for (slug, category) in CURATED {
            entries.insert(
                (*slug).to_string(),
                PolyHavenEntry {
                    slug: (*slug).to_string(),
                    label: slug_to_label(slug),
                    gltf_path: format!("{POLYHAVEN_FOLDER_REL}/{slug}/{slug}_1k.gltf"),
                    category: *category,
                },
            );
        }
        Self { entries }
    }

    pub fn get(&self, slug: &str) -> Option<&PolyHavenEntry> {
        self.entries.get(slug)
    }

    pub fn iter(&self) -> impl Iterator<Item = &PolyHavenEntry> {
        self.entries.values()
    }

    pub fn by_category(
        &self,
        category: PolyHavenCategory,
    ) -> impl Iterator<Item = &PolyHavenEntry> {
        self.entries
            .values()
            .filter(move |e| e.category == category)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn slug_to_label(slug: &str) -> String {
    slug.replace('_', " ")
}

/// Static slug → category table. Mirrors `CURATED` in
/// `scripts/download_polyhaven.py`. Adding a new asset: download it via
/// the script, then add a row here.
const CURATED: &[(&str, PolyHavenCategory)] = &[
    // Trees — only photoscans small enough for density scatter. Hero trees
    // (pine_tree_01, fir_tree_01, island_tree_*) are 90 MB – 905 MB each and
    // must be opted into separately for authored hero-placement only.
    ("pine_sapling_small", PolyHavenCategory::Tree),
    ("fir_sapling", PolyHavenCategory::Tree),
    ("fir_sapling_medium", PolyHavenCategory::Tree),
    // Dead wood
    ("dead_tree_trunk", PolyHavenCategory::DeadWood),
    ("dead_tree_trunk_02", PolyHavenCategory::DeadWood),
    ("tree_stump_01", PolyHavenCategory::DeadWood),
    ("tree_stump_02", PolyHavenCategory::DeadWood),
    ("pine_roots", PolyHavenCategory::DeadWood),
    ("root_cluster_01", PolyHavenCategory::DeadWood),
    ("single_root", PolyHavenCategory::DeadWood),
    ("dry_branches_medium_01", PolyHavenCategory::DeadWood),
    // Rocks
    ("boulder_01", PolyHavenCategory::Rock),
    ("rock_07", PolyHavenCategory::Rock),
    ("rock_09", PolyHavenCategory::Rock),
    ("rock_face_01", PolyHavenCategory::Rock),
    ("rock_face_02", PolyHavenCategory::Rock),
    ("rock_moss_set_01", PolyHavenCategory::Rock),
    ("rock_moss_set_02", PolyHavenCategory::Rock),
    ("stone_01", PolyHavenCategory::Rock),
    ("mountainside", PolyHavenCategory::Rock),
    ("coast_rocks_01", PolyHavenCategory::Rock),
    // Ground cover
    ("grass_medium_01", PolyHavenCategory::GroundCover),
    ("grass_medium_02", PolyHavenCategory::GroundCover),
    ("grass_bermuda_01", PolyHavenCategory::GroundCover),
    ("moss_01", PolyHavenCategory::GroundCover),
    ("fern_02", PolyHavenCategory::GroundCover),
    ("dandelion_01", PolyHavenCategory::GroundCover),
    // Shrubs + flowers
    ("shrub_01", PolyHavenCategory::Shrub),
    ("shrub_02", PolyHavenCategory::Shrub),
    ("shrub_03", PolyHavenCategory::Shrub),
    ("shrub_04", PolyHavenCategory::Shrub),
    ("celandine_01", PolyHavenCategory::Shrub),
    // Hub props
    ("wooden_barrels_01", PolyHavenCategory::HubProp),
    ("wooden_crate_02", PolyHavenCategory::HubProp),
    ("wooden_bucket_01", PolyHavenCategory::HubProp),
    ("wooden_bucket_02", PolyHavenCategory::HubProp),
    ("wooden_bowl_01", PolyHavenCategory::HubProp),
    ("wooden_lantern_01", PolyHavenCategory::HubProp),
    ("Lantern_01", PolyHavenCategory::HubProp),
    ("vintage_oil_lamp", PolyHavenCategory::HubProp),
    ("wooden_candlestick", PolyHavenCategory::HubProp),
    ("lantern_chandelier_01", PolyHavenCategory::HubProp),
    ("treasure_chest", PolyHavenCategory::HubProp),
    ("WoodenTable_01", PolyHavenCategory::HubProp),
    ("WoodenChair_01", PolyHavenCategory::HubProp),
    ("large_castle_door", PolyHavenCategory::HubProp),
    ("large_iron_gate", PolyHavenCategory::HubProp),
    ("modular_fort_01", PolyHavenCategory::HubProp),
    ("stone_fire_pit", PolyHavenCategory::HubProp),
    ("spinning_wheel_01", PolyHavenCategory::HubProp),
    ("horse_statue_01", PolyHavenCategory::HubProp),
    // Weapon-rack dressing
    ("katana_stand_01", PolyHavenCategory::WeaponRackDressing),
    ("antique_estoc", PolyHavenCategory::WeaponRackDressing),
    ("kite_shield", PolyHavenCategory::WeaponRackDressing),
    ("ornate_medieval_dagger", PolyHavenCategory::WeaponRackDressing),
    ("ornate_medieval_mace", PolyHavenCategory::WeaponRackDressing),
    ("ornate_war_hammer", PolyHavenCategory::WeaponRackDressing),
];

pub struct PolyHavenPlugin;

impl Plugin for PolyHavenPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(PolyHavenCatalog::new());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_every_curated_slug() {
        let cat = PolyHavenCatalog::new();
        assert_eq!(cat.len(), CURATED.len());
        for (slug, category) in CURATED {
            let entry = cat.get(slug).unwrap_or_else(|| panic!("missing {slug}"));
            assert_eq!(entry.category, *category);
            assert_eq!(entry.slug, *slug);
            assert!(entry.gltf_path.starts_with(POLYHAVEN_FOLDER_REL));
            assert!(entry.gltf_path.ends_with(".gltf"));
        }
    }

    #[test]
    fn by_category_groups_trees() {
        let cat = PolyHavenCatalog::new();
        let tree_count = cat.by_category(PolyHavenCategory::Tree).count();
        assert!(tree_count >= 2, "expected >=2 trees, got {tree_count}");
        let rock_count = cat.by_category(PolyHavenCategory::Rock).count();
        assert!(rock_count >= 5, "expected >=5 rocks, got {rock_count}");
        let hub_count = cat.by_category(PolyHavenCategory::HubProp).count();
        assert!(hub_count >= 15, "expected >=15 hub props, got {hub_count}");
    }

    #[test]
    fn collision_flags() {
        assert!(PolyHavenCategory::Tree.has_collision());
        assert!(PolyHavenCategory::Rock.has_collision());
        assert!(PolyHavenCategory::HubProp.has_collision());
        assert!(!PolyHavenCategory::GroundCover.has_collision());
        assert!(!PolyHavenCategory::Shrub.has_collision());
    }

    #[test]
    fn scene_path_format() {
        let cat = PolyHavenCatalog::new();
        let boulder = cat.get("boulder_01").unwrap();
        assert_eq!(
            boulder.scene_path(),
            "polyhaven/boulder_01/boulder_01_1k.gltf#Scene0"
        );
    }
}
