//! Filesystem scan enumerating every Meshtint GLB variant shipped in
//! `assets/extracted/meshtint/`. Consumers (museum UI, game client)
//! hold a single [`MeshtintCatalog`] resource and look up variants by
//! `(gender, body-slot)` or `(weapon-category, variant-number)`.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use bevy::prelude::*;

use super::overlay::BodySlot;
use super::Gender;

/// Every weapon category currently shipped in the Meshtint Polygonal
/// Fantasy Pack. `scan_variants` walks `{category}_NN.glb` matches under
/// `weapons/` for each entry.
pub const WEAPON_CATEGORIES: &[&str] = &[
    "Sword",
    "Axe",
    "Dagger",
    "Hammer",
    "Mace",
    "Staff",
    "Wand",
    "Bow",
    "Arrow",
    "Quiver",
    "Shield",
    "Pick_Axe",
    "Sickle",
    "Scythe",
    "Spade",
    "Rake",
    "Blacksmith_Hammer",
    "Bag",
    "Basket",
];

#[derive(Clone, Debug)]
pub struct Variant {
    /// The `NN` in `{category}_NN.glb`.
    pub number: u32,
    /// Bevy asset-server-relative path, e.g. `"extracted/meshtint/weapons/Sword_01.glb"`.
    pub path: String,
    /// Display-friendly label, e.g. `"Sword 01"`, `"Pick Axe 03"`.
    pub label: String,
}

#[derive(Resource, Default, Debug)]
pub struct MeshtintCatalog {
    body: BTreeMap<(Gender, BodySlot), Vec<Variant>>,
    weapons: BTreeMap<&'static str, Vec<Variant>>,
    /// Available `{Gender}_NN.glb` base variants, sorted. Present so the
    /// UI can drive a "base variant" picker; the Polygonal Fantasy Pack
    /// 1.4 ships `_01` only, but the scan is forward-compatible.
    bases: BTreeMap<Gender, Vec<u32>>,
}

impl MeshtintCatalog {
    /// Scan `{assets_root}/extracted/meshtint/` and build the catalog.
    /// `assets_root` is the on-disk path to the assets directory (e.g.
    /// `<workspace>/assets`) — *not* the Bevy asset-server prefix.
    pub fn scan(assets_root: impl AsRef<Path>) -> Self {
        let root = assets_root.as_ref();
        let mut cat = Self::default();

        // Bases — per gender.
        for &g in Gender::ALL {
            let folder_abs = root.join(g.folder_rel());
            let bases = scan_prefix(&folder_abs, g.base_file_prefix(), g.folder_rel())
                .into_iter()
                .map(|v| v.number)
                .collect::<Vec<_>>();
            cat.bases.insert(g, bases);
        }

        // Body overlays — per gender, per slot.
        for &g in Gender::ALL {
            let folder_rel = g.folder_rel();
            let folder_abs = root.join(folder_rel);
            for &slot in BodySlot::ALL {
                let Some(prefix) = slot.file_prefix(g) else {
                    cat.body.insert((g, slot), Vec::new());
                    continue;
                };
                let variants = scan_prefix(&folder_abs, prefix, folder_rel);
                cat.body.insert((g, slot), variants);
            }
        }

        // Weapons — gender-agnostic, per category.
        let wfolder_rel = "extracted/meshtint/weapons";
        let wfolder_abs = root.join(wfolder_rel);
        for &cat_name in WEAPON_CATEGORIES {
            let variants = scan_prefix(&wfolder_abs, cat_name, wfolder_rel);
            cat.weapons.insert(cat_name, variants);
        }

        let total_body: usize = cat.body.values().map(|v| v.len()).sum();
        let total_weapons: usize = cat.weapons.values().map(|v| v.len()).sum();
        let total_bases: usize = cat.bases.values().map(|v| v.len()).sum();
        info!(
            "MeshtintCatalog built — {total_bases} bases, {total_body} body overlays, {total_weapons} weapons"
        );
        cat
    }

    pub fn body(&self, gender: Gender, slot: BodySlot) -> &[Variant] {
        self.body
            .get(&(gender, slot))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn weapon(&self, category: &str) -> &[Variant] {
        self.weapons
            .get(category)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn body_variant(&self, gender: Gender, slot: BodySlot, variant: u32) -> Option<&Variant> {
        self.body(gender, slot).iter().find(|v| v.number == variant)
    }

    pub fn weapon_variant(&self, category: &str, variant: u32) -> Option<&Variant> {
        self.weapon(category).iter().find(|v| v.number == variant)
    }

    pub fn weapon_categories(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.weapons.keys().copied()
    }

    /// Available base variant numbers for the given gender, sorted. Empty
    /// if no matching `{Gender}_NN.glb` was found on disk.
    pub fn base_variants(&self, gender: Gender) -> &[u32] {
        self.bases.get(&gender).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Largest available base variant for the gender, or `1` if none
    /// were found (so default UI picks stay in range).
    pub fn base_variant_max(&self, gender: Gender) -> u32 {
        self.base_variants(gender).last().copied().unwrap_or(1)
    }
}

fn scan_prefix(dir: &Path, prefix: &str, asset_folder_rel: &str) -> Vec<Variant> {
    let Ok(read) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let pat_prefix = format!("{prefix}_");
    let mut hits: Vec<(u32, String)> = Vec::new();
    for entry in read.flatten() {
        let Ok(fname) = entry.file_name().into_string() else {
            continue;
        };
        let Some(stem) = fname.strip_suffix(".glb") else {
            continue;
        };
        let Some(rest) = stem.strip_prefix(&pat_prefix) else {
            continue;
        };
        let Ok(n) = rest.parse::<u32>() else {
            continue;
        };
        hits.push((n, fname));
    }
    hits.sort_by_key(|(n, _)| *n);
    hits.into_iter()
        .map(|(n, fname)| Variant {
            number: n,
            path: format!("{asset_folder_rel}/{fname}"),
            // Pick_Axe_03 → "Pick Axe 03" for display.
            label: format!("{prefix} {:02}", n).replace('_', " "),
        })
        .collect()
}
