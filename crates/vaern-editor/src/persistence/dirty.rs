//! Per-zone dirty tracking.
//!
//! V1: data structure only. Once mutating modes (place, brush, etc.)
//! land, they should call `DirtyZones::mark` so the toolbar's Save
//! button can highlight pending zones and the close-without-saving
//! path can warn.

use bevy::prelude::*;
use std::collections::HashSet;

#[derive(Resource, Debug, Default, Clone)]
pub struct DirtyZones {
    zones: HashSet<String>,
}

impl DirtyZones {
    pub fn mark(&mut self, zone_id: impl Into<String>) {
        self.zones.insert(zone_id.into());
    }

    pub fn clear(&mut self, zone_id: &str) {
        self.zones.remove(zone_id);
    }

    pub fn is_dirty(&self, zone_id: &str) -> bool {
        self.zones.contains(zone_id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.zones.iter().map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.zones.len()
    }

    pub fn is_empty(&self) -> bool {
        self.zones.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mark_and_clear_round_trip() {
        let mut d = DirtyZones::default();
        assert!(!d.is_dirty("dalewatch_marches"));
        d.mark("dalewatch_marches");
        assert!(d.is_dirty("dalewatch_marches"));
        d.clear("dalewatch_marches");
        assert!(!d.is_dirty("dalewatch_marches"));
    }
}
