//! Zone YAML I/O.
//!
//! V1: load only (delegates to `vaern_data::load_world`). The save
//! path writes a sidecar in `~/.cache/vaern-editor/<zone>.yaml` and
//! warns that in-place save is not yet implemented.
//!
//! When the V2 save path lands, the canonical location is
//! `<workspace>/src/generated/world/zones/<zone>/...` and the helper
//! [`atomic::write_atomic`] should be used to avoid half-written files
//! mid-save.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use vaern_data::{load_world, load_world_layout, AuthoredProp, World, WorldLayout};

use super::atomic::write_atomic;

/// Workspace-relative path to the world YAML root. Hard-coded for V1
/// so the editor can be launched without env-var setup; mirrors the
/// pattern in `vaern-client/src/scene/dressing.rs`.
pub fn world_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src/generated/world")
}

/// Load every zone in the workspace world tree. The editor only edits
/// one at a time but keeping the global view simplifies cross-zone
/// references (e.g. landmarks shared with the active zone).
pub fn load_world_for_editor() -> Result<World> {
    let root = world_root();
    load_world(&root).with_context(|| format!("loading world from {root:?}"))
}

/// Load `world.yaml` zone-placements (Voronoi anchors + coastline)
/// alongside the per-zone YAMLs. Mirrors the cartography pipeline used
/// by `vaern-client::scene::dressing` and `vaern-server::data` so the
/// editor renders zones at the same world-space coordinates as the
/// runtime — without this, the legacy `ZONE_RING_RADIUS = 2800`
/// fallback teleports the camera ~6.7 km from where the zone content
/// actually lives.
pub fn load_world_layout_for_editor() -> Result<WorldLayout> {
    let root = world_root();
    load_world_layout(&root).with_context(|| format!("loading world layout from {root:?}"))
}

/// Replace just the `props:` array of a hub YAML, preserving every
/// other field verbatim. Avoids round-tripping the full Hub struct
/// through serde (which would re-order keys and might bake in
/// `#[serde(default)]` values that weren't in the source file).
///
/// Strategy: parse the file into a `serde_yaml::Value` (preserves the
/// full mapping shape), serialize the new `props:` to a Value, splice
/// it in, dump back to YAML, atomic-write.
pub fn save_hub_props(path: &Path, props: &[AuthoredProp]) -> Result<()> {
    let original = std::fs::read_to_string(path)
        .with_context(|| format!("reading hub yaml at {path:?}"))?;
    let mut value: serde_yaml::Value =
        serde_yaml::from_str(&original).context("parsing hub yaml as Value")?;
    let map = value
        .as_mapping_mut()
        .context("hub yaml root must be a mapping")?;
    let props_value = serde_yaml::to_value(props).context("serializing AuthoredProp list")?;
    map.insert(serde_yaml::Value::String("props".to_string()), props_value);
    let output = serde_yaml::to_string(&value).context("re-serializing hub yaml")?;
    write_atomic(path, output.as_bytes()).with_context(|| format!("atomic write to {path:?}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use vaern_data::PropOffset;

    #[test]
    fn world_root_resolves_under_workspace() {
        let root = world_root();
        let s = root.to_string_lossy();
        assert!(s.contains("src/generated/world"));
    }

    #[test]
    fn save_hub_props_round_trips_props_array() {
        let dir = std::env::temp_dir().join(format!("vaern_zone_io_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test_hub.yaml");
        let starter = "id: test_hub\nzone: test_zone\nname: Test Hub\nrole: outpost\n\
                       quest_givers: 2\nbiome: grass\nprops: []\n";
        std::fs::write(&path, starter).unwrap();

        let props = vec![AuthoredProp {
            slug: "wooden_barrels_01".into(),
            offset: PropOffset { x: 1.5, z: -2.0 },
            rotation_y_deg: 45.0,
            scale: 1.0,
            absolute_y: None,
        }];
        save_hub_props(&path, &props).unwrap();

        let written = std::fs::read_to_string(&path).unwrap();
        assert!(written.contains("wooden_barrels_01"));
        assert!(written.contains("rotation_y_deg"));
        // Other fields preserved.
        assert!(written.contains("id: test_hub"));
        assert!(written.contains("Test Hub"));

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }
}
