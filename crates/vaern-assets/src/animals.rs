//! EverythingLibrary Animals catalog.
//!
//! Scans `assets/extracted/animals/*.glb` at startup and exposes a map
//! from species basename (e.g. `"GrayWolf"`) to Bevy asset path.
//! 178 static-mesh species ship in this pack — mammals, birds,
//! reptiles, insects, plus a small `Imaginary/` bucket (Unicorn,
//! MidnightDeer, Batto, Curow, Entraine) and `AnimalParts/` bits
//! (EyeBall, Skeleton) that double as fantasy-monster stand-ins.
//!
//! Pure data, no rig. Consumers (server NPC spawn, future museum
//! panel) look up a species and spawn the GLB as a SceneRoot child
//! of the NPC entity.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use bevy::prelude::*;

/// Folder (relative to the asset root) containing species GLBs.
pub const ANIMALS_FOLDER_REL: &str = "extracted/animals";

/// One entry in the catalog — asset-server path + display label.
#[derive(Clone, Debug)]
pub struct AnimalEntry {
    /// Bevy asset-server-relative path,
    /// e.g. `"extracted/animals/GrayWolf.glb"`.
    pub path: String,
    /// Display-friendly label ("GrayWolf" → "Gray Wolf").
    pub label: String,
}

#[derive(Resource, Default, Debug)]
pub struct AnimalCatalog {
    species: BTreeMap<String, AnimalEntry>,
}

impl AnimalCatalog {
    /// Scan `{assets_root}/extracted/animals/` for `*.glb` files.
    pub fn scan(assets_root: impl AsRef<Path>) -> Self {
        let root = assets_root.as_ref();
        let folder_abs = root.join(ANIMALS_FOLDER_REL);
        let mut species = BTreeMap::new();
        let Ok(read) = fs::read_dir(&folder_abs) else {
            warn!(
                "AnimalCatalog: couldn't read {} — no animal meshes available",
                folder_abs.display()
            );
            return Self::default();
        };
        for entry in read.flatten() {
            let Ok(fname) = entry.file_name().into_string() else { continue };
            let Some(stem) = fname.strip_suffix(".glb") else { continue };
            species.insert(
                stem.to_string(),
                AnimalEntry {
                    path: format!("{ANIMALS_FOLDER_REL}/{fname}"),
                    label: humanize(stem),
                },
            );
        }
        info!("AnimalCatalog built — {} species", species.len());
        Self { species }
    }

    /// Resolve a species by basename. `None` if unknown — caller
    /// should log + fall back (cuboid, closest-match, etc.).
    pub fn get(&self, basename: &str) -> Option<&AnimalEntry> {
        self.species.get(basename)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &AnimalEntry)> {
        self.species.iter().map(|(k, v)| (k.as_str(), v))
    }

    pub fn len(&self) -> usize {
        self.species.len()
    }

    pub fn is_empty(&self) -> bool {
        self.species.is_empty()
    }
}

/// `GrayWolf` → `Gray Wolf`, `BlueMorphoButterfly` → `Blue Morpho Butterfly`.
fn humanize(stem: &str) -> String {
    let mut out = String::with_capacity(stem.len() + 4);
    let mut prev_lower = false;
    for (i, c) in stem.chars().enumerate() {
        if i > 0 && c.is_ascii_uppercase() && prev_lower {
            out.push(' ');
        }
        out.push(c);
        prev_lower = c.is_ascii_lowercase();
    }
    out
}
