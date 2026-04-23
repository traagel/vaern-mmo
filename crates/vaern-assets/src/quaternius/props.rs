//! Fantasy Props MEGAKIT catalog.
//!
//! Scans `assets/extracted/props/*.gltf` at startup and exposes a map
//! from prop basename (e.g. `"Sword_Bronze"`) to Bevy asset path.
//!
//! Unlike `MeshtintCatalog`, which groups `Sword_01..Sword_05` under a
//! `"Sword"` category, MEGAKIT props are discrete named objects —
//! there's one `Sword_Bronze`, one `Shield_Wooden`, etc. So the
//! catalog is a flat `BTreeMap<String, String>` keyed by basename.
//! Adding more props later means dropping new `.gltf` files in the
//! props folder; the scan picks them up automatically.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use bevy::prelude::*;

/// Folder (relative to the asset root) containing the props.
pub const PROPS_FOLDER_REL: &str = "extracted/props";

/// One prop entry — asset-server path + display label.
#[derive(Clone, Debug)]
pub struct PropEntry {
    /// Bevy asset-server-relative path, e.g.
    /// `"extracted/props/Sword_Bronze.gltf"`.
    pub path: String,
    /// Display-friendly label, e.g. `"Sword Bronze"`.
    pub label: String,
}

#[derive(Resource, Default, Debug)]
pub struct MegakitCatalog {
    /// basename → entry. Sorted by key (BTreeMap) so museum pickers
    /// render the prop list in a stable alphabetical order.
    props: BTreeMap<String, PropEntry>,
}

impl MegakitCatalog {
    /// Scan `{assets_root}/extracted/props/` for `*.gltf` files.
    /// `assets_root` is the on-disk path to the assets directory
    /// (e.g. `<workspace>/assets`) — not the Bevy asset-server prefix.
    pub fn scan(assets_root: impl AsRef<Path>) -> Self {
        let root = assets_root.as_ref();
        let folder_abs = root.join(PROPS_FOLDER_REL);
        let mut props = BTreeMap::new();

        let Ok(read) = fs::read_dir(&folder_abs) else {
            warn!(
                "MegakitCatalog: couldn't read {} — no props will be available",
                folder_abs.display()
            );
            return Self::default();
        };
        for entry in read.flatten() {
            let Ok(fname) = entry.file_name().into_string() else {
                continue;
            };
            let Some(stem) = fname.strip_suffix(".gltf") else {
                continue;
            };
            props.insert(
                stem.to_string(),
                PropEntry {
                    path: format!("{PROPS_FOLDER_REL}/{fname}"),
                    label: stem.replace('_', " "),
                },
            );
        }

        info!("MegakitCatalog built — {} props", props.len());
        Self { props }
    }

    /// Resolve a prop by basename. Returns `None` for unknown ids —
    /// caller should log + skip the overlay, not panic.
    pub fn get(&self, basename: &str) -> Option<&PropEntry> {
        self.props.get(basename)
    }

    /// Every (basename, entry) pair, alphabetical. Museum picker uses
    /// this to populate its dropdown.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &PropEntry)> {
        self.props.iter().map(|(k, v)| (k.as_str(), v))
    }

    pub fn len(&self) -> usize {
        self.props.len()
    }

    pub fn is_empty(&self) -> bool {
        self.props.is_empty()
    }
}
