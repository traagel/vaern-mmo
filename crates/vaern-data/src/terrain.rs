//! Lore-aware terrain tagging for hubs and landmarks.
//!
//! The procedural heightfield (in `vaern-cartography::heightfield`) takes
//! a list of [`TerrainStamp`]s — Gaussian peaks/dips placed at hub +
//! landmark world positions — and adds them on top of the biome SDF
//! blend. This module is the source of truth for which feature each hub
//! / landmark gets.
//!
//! Resolution priority:
//! 1. Explicit `terrain:` field on the YAML wins (added on `Hub` and
//!    `Landmark` in [`crate::world`] / [`crate::landmark`]).
//! 2. Otherwise, [`auto_derive_feature`] keyword-scans `name +
//!    description`. Token priority: `ford/bridge/crossing` beats
//!    `keep/fortress` (so "Ford Keep" stays a Ford), then ridge/cairn
//!    keywords beat building keywords, then valleys/grove/fen, then
//!    explicit "flat" keywords like `waypost`. Default is `Flat`.

use serde::{Deserialize, Serialize};

use crate::{Coord2, LandmarkIndex, World, WorldLayout};

/// Symbolic ground shape at a hub or landmark.
///
/// `Forest` and `Fen` add no height stamp on their own — they're hints
/// to the heightfield's noise modulation pass and to scatter rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerrainFeature {
    /// Small hill / ridge / cairn / mound — modest peak.
    Hill,
    /// Capital-class peak — keeps and citadels.
    BigHill,
    /// Sunken farmstead / hollow / dale — gentle dip.
    Valley,
    /// Where a road crosses a river — soft basin to receive the river
    /// channel cleanly.
    Ford,
    /// Wooded biome modifier — no height stamp; influences scatter +
    /// noise.
    Forest,
    /// Marshy biome modifier — slight depression handled by the biome
    /// SDF; no Gaussian stamp.
    Fen,
    /// Long ridge feature — handled like Hill but with anisotropic
    /// stamping (currently same shape as Hill until anisotropic mode
    /// lands).
    Ridge,
    /// No terrain stamp. The default when nothing matches.
    Flat,
}

impl TerrainFeature {
    /// Gaussian amplitude in metres for this feature's stamp. Zero for
    /// features that contribute via biome / mask only.
    pub fn stamp_amplitude_m(self) -> f32 {
        match self {
            Self::Hill => 8.0,
            Self::BigHill => 18.0,
            Self::Valley => -5.0,
            Self::Ford => -2.0,
            Self::Ridge => 8.0,
            Self::Forest | Self::Fen | Self::Flat => 0.0,
        }
    }

    /// Stamp footprint radius in metres (Gaussian σ ≈ radius / 3 so the
    /// 3-sigma support fits inside).
    pub fn stamp_radius_m(self) -> f32 {
        match self {
            Self::Hill => 80.0,
            Self::BigHill => 140.0,
            Self::Valley => 100.0,
            Self::Ford => 30.0,
            Self::Ridge => 90.0,
            Self::Forest | Self::Fen | Self::Flat => 0.0,
        }
    }

    /// `true` if this feature contributes a non-zero Gaussian to
    /// `terrain_stamp_field`. Lets the heightfield skip iteration over
    /// stamps with no effect.
    pub fn has_stamp(self) -> bool {
        self.stamp_amplitude_m() != 0.0
    }
}

/// One placed terrain stamp in world-space coordinates.
#[derive(Debug, Clone)]
pub struct TerrainStamp {
    /// Source identifier for stable iteration and debugging
    /// (`hub:dalewatch_keep`, `landmark:sidlow_cairn`, ...).
    pub source_id: String,
    /// World-space position (x, z). The heightfield evaluates Gaussian
    /// distance from this point.
    pub world_pos: Coord2,
    pub feature: TerrainFeature,
}

/// Keyword-scan a hub or landmark's `name` (and optional description) to
/// infer a [`TerrainFeature`]. The order of checks is the priority — the
/// first match wins.
pub fn auto_derive_feature(name: &str, description: Option<&str>) -> TerrainFeature {
    let mut text = name.to_ascii_lowercase();
    if let Some(d) = description {
        text.push(' ');
        text.push_str(&d.to_ascii_lowercase());
    }

    // Ford/bridge keywords beat everything else — a ford-shaped basin
    // wins over the building's outline.
    if contains_word(&text, &["ford", "bridge", "crossing", "ferry"])
        || contains_phrase(&text, &["river bank", "eastern bank", "western bank"])
    {
        return TerrainFeature::Ford;
    }

    // Ridge / cairn / scarp / barrow / downs / scrub uplift — smaller
    // peaks. "Mound" alone is too generic so it must also see "barrow"
    // or "ridge".
    if contains_word(
        &text,
        &["cairn", "ridge", "scarp", "downs", "barrow", "tor", "knoll"],
    ) {
        return TerrainFeature::Hill;
    }

    // Mine cut into a hillside.
    if contains_word(&text, &["mine", "quarry"]) {
        return TerrainFeature::Hill;
    }

    // Big hub features. Must be checked AFTER ford/cairn so that "Ford
    // Keep" / "Cairn Keep" route to the more terrain-specific feature.
    if contains_word(&text, &["keep", "fortress", "citadel", "watchtower"]) {
        return TerrainFeature::BigHill;
    }

    // Valley / hollow / sheltered croft.
    if contains_word(&text, &["croft", "hollow", "dell", "dale"]) {
        return TerrainFeature::Valley;
    }

    // Forest indicators — no height contribution, just a tag.
    if contains_word(
        &text,
        &["grove", "copse", "woods", "thicket", "wood"],
    ) {
        return TerrainFeature::Forest;
    }

    // Fen / marsh / mire / bog / brake.
    if contains_word(
        &text,
        &["fen", "fens", "marsh", "mire", "bog", "swamp", "brake"],
    ) {
        return TerrainFeature::Fen;
    }

    // Explicit flatness hints — waypost / road / track. These are
    // optional but help authors signal "this hub is on flat ground".
    if contains_word(&text, &["waypost", "road", "track"]) {
        return TerrainFeature::Flat;
    }

    TerrainFeature::Flat
}

fn contains_word(haystack: &str, needles: &[&str]) -> bool {
    for n in needles {
        // Word-boundary scan: token may be at start/end or surrounded by
        // non-alphanumerics. Avoids "fortress" matching inside "uniform".
        let mut start = 0usize;
        while let Some(off) = haystack[start..].find(n) {
            let pos = start + off;
            let before_ok = pos == 0
                || !haystack[..pos]
                    .chars()
                    .last()
                    .map(|c| c.is_ascii_alphanumeric())
                    .unwrap_or(false);
            let after_ok = pos + n.len() == haystack.len()
                || !haystack[pos + n.len()..]
                    .chars()
                    .next()
                    .map(|c| c.is_ascii_alphanumeric())
                    .unwrap_or(false);
            if before_ok && after_ok {
                return true;
            }
            start = pos + n.len();
        }
    }
    false
}

fn contains_phrase(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| haystack.contains(n))
}

/// Build the deterministic, sorted-by-source-id list of terrain stamps
/// for a single zone. Iterates every hub + landmark in the zone, resolves
/// each one's feature (explicit YAML field if present, else
/// [`auto_derive_feature`]), and projects offsets into world coordinates
/// via `layout.zone_origin(zone_id)`.
///
/// Returns an empty Vec for unknown zones / zones with no placement.
pub fn build_zone_stamps(
    zone_id: &str,
    layout: &WorldLayout,
    world: &World,
    landmarks: &LandmarkIndex,
) -> Vec<TerrainStamp> {
    let Some(origin) = layout.zone_origin(zone_id) else {
        return Vec::new();
    };
    let mut stamps: Vec<TerrainStamp> = Vec::new();

    for hub in world.hubs_in_zone(zone_id) {
        let Some(off) = hub.offset_from_zone_origin else {
            continue;
        };
        let feature = hub
            .terrain
            .unwrap_or_else(|| auto_derive_feature(&hub.name, None));
        if !feature.has_stamp() && !matches!(feature, TerrainFeature::Forest | TerrainFeature::Fen)
        {
            // Flat hubs add nothing to the stamp list — saves iteration.
            continue;
        }
        stamps.push(TerrainStamp {
            source_id: format!("hub:{}", hub.id),
            world_pos: Coord2::new(off.x + origin.x, off.z + origin.z),
            feature,
        });
    }

    for lm in landmarks.iter_zone(zone_id) {
        let feature = lm.terrain.unwrap_or_else(|| {
            auto_derive_feature(&lm.name, lm.description.as_deref())
        });
        if !feature.has_stamp() && !matches!(feature, TerrainFeature::Forest | TerrainFeature::Fen)
        {
            continue;
        }
        stamps.push(TerrainStamp {
            source_id: format!("landmark:{}", lm.id),
            world_pos: Coord2::new(
                lm.offset_from_zone_origin.x + origin.x,
                lm.offset_from_zone_origin.z + origin.z,
            ),
            feature,
        });
    }

    // Sort by source_id for byte-stable iteration in the heightfield's
    // Gaussian sum.
    stamps.sort_by(|a, b| a.source_id.cmp(&b.source_id));
    stamps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cairn_resolves_to_hill() {
        assert_eq!(
            auto_derive_feature("Sidlow Cairn", None),
            TerrainFeature::Hill
        );
        assert_eq!(
            auto_derive_feature("Ashmane Cairn", None),
            TerrainFeature::Hill
        );
    }

    #[test]
    fn keep_alone_is_big_hill() {
        assert_eq!(
            auto_derive_feature("Dalewatch Keep", None),
            TerrainFeature::BigHill
        );
    }

    #[test]
    fn ford_beats_keep() {
        // If both ford and keep tokens are present, ford wins so the
        // hub sits in a basin, not on a mound.
        assert_eq!(
            auto_derive_feature("Ford Keep", None),
            TerrainFeature::Ford
        );
        assert_eq!(
            auto_derive_feature(
                "Bridge Citadel",
                Some("Stone bridge fortress straddling the river")
            ),
            TerrainFeature::Ford
        );
    }

    #[test]
    fn ridge_in_description_resolves_to_hill_when_name_is_neutral() {
        // Old Brenn's Croft → ridge keyword in description lifts it to
        // Hill (ridge beats croft because ridge is checked first).
        assert_eq!(
            auto_derive_feature(
                "Old Brenn's Croft",
                Some("set against the western ridge — wolves come from the ridge at night")
            ),
            TerrainFeature::Hill
        );
    }

    #[test]
    fn croft_alone_is_valley() {
        // Without ridge in the description, the croft sits in a hollow.
        assert_eq!(
            auto_derive_feature("Hollow Croft", None),
            TerrainFeature::Valley
        );
    }

    #[test]
    fn grove_resolves_to_forest() {
        assert_eq!(
            auto_derive_feature("Thornroot Grove", None),
            TerrainFeature::Forest
        );
        assert_eq!(
            auto_derive_feature("The Boar Grove", None),
            TerrainFeature::Forest
        );
    }

    #[test]
    fn fen_and_marsh_resolve_to_fen() {
        assert_eq!(
            auto_derive_feature("The Blackwash Fens", None),
            TerrainFeature::Fen
        );
        assert_eq!(
            auto_derive_feature("The Reed-Brake", None),
            TerrainFeature::Fen
        );
    }

    #[test]
    fn mine_resolves_to_hill() {
        assert_eq!(
            auto_derive_feature("Copperstep Mine", None),
            TerrainFeature::Hill
        );
    }

    #[test]
    fn waypost_alone_is_flat() {
        assert_eq!(
            auto_derive_feature("Kingsroad Waypost", None),
            TerrainFeature::Flat
        );
    }

    #[test]
    fn unknown_resolves_to_flat() {
        assert_eq!(
            auto_derive_feature("Some Random Place", None),
            TerrainFeature::Flat
        );
    }

    #[test]
    fn word_boundary_does_not_match_substring() {
        // "performance" contains "ford" as a suffix — must NOT match.
        assert_eq!(
            auto_derive_feature("Performance Hall", None),
            TerrainFeature::Flat
        );
        // "uniform" contains "form" but not "fortress" — must not match
        // BigHill.
        assert_eq!(
            auto_derive_feature("Uniform Square", None),
            TerrainFeature::Flat
        );
    }

    #[test]
    fn stamp_amplitude_zero_for_flat() {
        assert_eq!(TerrainFeature::Flat.stamp_amplitude_m(), 0.0);
        assert_eq!(TerrainFeature::Forest.stamp_amplitude_m(), 0.0);
        assert_eq!(TerrainFeature::Fen.stamp_amplitude_m(), 0.0);
        assert!(TerrainFeature::Hill.stamp_amplitude_m() > 0.0);
        assert!(TerrainFeature::Valley.stamp_amplitude_m() < 0.0);
    }

    #[test]
    fn watchtower_resolves_to_big_hill() {
        // Pact Watchtower — "Concord watchtower on the eastern bank of the Ash"
        // The "eastern bank" phrase wins over watchtower → Ford.
        assert_eq!(
            auto_derive_feature(
                "Pact Watchtower",
                Some("A half-collapsed Concord watchtower on the eastern bank of the Ash.")
            ),
            TerrainFeature::Ford
        );
        // Without the bank phrase, watchtower alone → BigHill.
        assert_eq!(
            auto_derive_feature("Lone Watchtower", None),
            TerrainFeature::BigHill
        );
    }
}
