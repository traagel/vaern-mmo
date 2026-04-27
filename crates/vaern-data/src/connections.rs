//! Cross-zone connection edges. Loaded from
//! `world/zones/<zone>/connections.yaml`. Each file lists the zones
//! adjacent to its owner; the union of all files forms the world graph.

use std::{collections::HashMap, fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::{read_dir, Cardinal, Coord2, LoadError};

/// One outgoing edge from a zone.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Connection {
    pub to_zone: String,
    pub direction: Cardinal,
    /// Connection medium: e.g. `kingsroad`, `forest_track`, `cave_link`.
    /// Keys into `cartography_style.yaml::roads`.
    #[serde(rename = "type")]
    pub type_: String,
    /// Where on this zone the connection exits. In zone-local meters.
    pub border_position: Coord2,
    #[serde(default)]
    pub border_label: String,
    #[serde(default)]
    pub level_continuous: bool,
    #[serde(default)]
    pub pvp_safe: bool,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub warnings: Vec<String>,
}

/// One zone's connections file.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConnectionsFile {
    pub id: String,
    pub zone: String,
    #[serde(default)]
    pub connections: Vec<Connection>,
}

/// All zone connections, keyed by source zone id.
#[derive(Debug, Default, Clone)]
pub struct ConnectionsIndex {
    pub by_zone: HashMap<String, Vec<Connection>>,
}

impl ConnectionsIndex {
    pub fn get(&self, zone_id: &str) -> &[Connection] {
        self.by_zone
            .get(zone_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Iterate all edges in deterministic order — sorted by source
    /// zone id so callers (renderers, validators) emit byte-stable
    /// output across processes despite the underlying HashMap.
    pub fn all_edges(&self) -> impl Iterator<Item = (&str, &Connection)> {
        let mut keys: Vec<&str> = self.by_zone.keys().map(String::as_str).collect();
        keys.sort();
        keys.into_iter().flat_map(move |z| {
            self.by_zone[z].iter().map(move |e| (z, e))
        })
    }
}

/// Walk `world_root/zones/<zone>/connections.yaml`. Zones without the
/// file are silently skipped.
pub fn load_all_connections(
    world_root: impl AsRef<Path>,
) -> Result<ConnectionsIndex, LoadError> {
    let world_root = world_root.as_ref();
    let zones_dir = world_root.join("zones");
    let mut out = ConnectionsIndex::default();
    if !zones_dir.exists() {
        return Ok(out);
    }
    for zone_dir in read_dir(&zones_dir)? {
        if !zone_dir.is_dir() {
            continue;
        }
        let path = zone_dir.join("connections.yaml");
        if !path.exists() {
            continue;
        }
        let text = fs::read_to_string(&path).map_err(|e| LoadError::Io {
            path: path.clone(),
            source: e,
        })?;
        let file: ConnectionsFile = serde_yaml::from_str(&text).map_err(|e| LoadError::Yaml {
            path: path.clone(),
            source: e,
        })?;
        out.by_zone.insert(file.zone, file.connections);
    }
    Ok(out)
}
