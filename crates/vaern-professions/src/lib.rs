//! Gathering + crafting profession primitives.
//!
//! Leaf crate — no Bevy plugin, no world logic. Server wires resource
//! nodes + harvest handlers; client renders markers + sends requests.
//! This crate just owns the types both sides share.
//!
//! Two cooperating layers:
//!
//!   * **`Profession`** — what a player is trained in. Each player
//!     carries `ProfessionSkills` with a per-profession skill value
//!     (0..=500). Skill gates which recipes / nodes they can use.
//!
//!   * **`NodeKind`** — what a world resource node offers. A
//!     `copper_vein` demands Mining ≥ 0, yields copper_ore on harvest.
//!     `stanchweed_patch` demands Herbalism ≥ 0, yields stanchweed.
//!     Mapping from kind → (profession, min_skill, yields) lives on
//!     `NodeDef`, authored in YAML at `src/generated/world/nodes.yaml`
//!     (planned — v1 uses hardcoded defaults in
//!     `vaern-server::resource_nodes`).
//!
//! Crafting professions (Blacksmithing/Leatherworking/Tailoring/etc.)
//! reuse the same `Profession` enum + `ProfessionSkills` component,
//! just with recipes instead of world nodes. Both wire later.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Every profession — gathering + crafting. Kept in one enum so a
/// player's `ProfessionSkills` slot-map covers all of them uniformly.
/// The "2 profession cap" is social design, not enforced here; the
/// content layer gates skill growth.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Profession {
    // Gathering
    Mining,
    Herbalism,
    Skinning,
    Logging,
    // Crafting — populate as recipe systems land
    Blacksmithing,
    Leatherworking,
    Tailoring,
    Alchemy,
    Enchanting,
    Jewelcrafting,
    Bowyery,
}

impl Profession {
    /// Display label (title case).
    pub fn display(self) -> &'static str {
        match self {
            Self::Mining => "Mining",
            Self::Herbalism => "Herbalism",
            Self::Skinning => "Skinning",
            Self::Logging => "Logging",
            Self::Blacksmithing => "Blacksmithing",
            Self::Leatherworking => "Leatherworking",
            Self::Tailoring => "Tailoring",
            Self::Alchemy => "Alchemy",
            Self::Enchanting => "Enchanting",
            Self::Jewelcrafting => "Jewelcrafting",
            Self::Bowyery => "Bowyery",
        }
    }

    /// All profession variants, for iteration.
    pub const ALL: [Profession; 11] = [
        Profession::Mining,
        Profession::Herbalism,
        Profession::Skinning,
        Profession::Logging,
        Profession::Blacksmithing,
        Profession::Leatherworking,
        Profession::Tailoring,
        Profession::Alchemy,
        Profession::Enchanting,
        Profession::Jewelcrafting,
        Profession::Bowyery,
    ];
}

/// Per-player skill levels across professions. 0 = untrained. Levels
/// scale to 500 for master crafters; trainer NPCs raise the cap in
/// tiered steps (50 → 150 → 300 → 500). A player with Mining 0 can
/// still mine copper (tier-1 nodes are universal); higher-tier nodes
/// gate on skill.
///
/// Stored as a flat array of `u16` keyed by `Profession as usize` so
/// per-profession lookups are O(1) without a hash.
#[derive(Component, Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfessionSkills {
    pub levels: [u16; 11],
}

impl ProfessionSkills {
    pub fn get(&self, p: Profession) -> u16 {
        self.levels[p as usize]
    }
    pub fn set(&mut self, p: Profession, v: u16) {
        self.levels[p as usize] = v;
    }
    pub fn add(&mut self, p: Profession, delta: i32) {
        let cur = self.levels[p as usize] as i32;
        self.levels[p as usize] = (cur + delta).clamp(0, 500) as u16;
    }
}

// ---------------------------------------------------------------------------
// Resource nodes
// ---------------------------------------------------------------------------

/// What a harvestable node offers. Maps 1:1 to a profession +
/// item-yield spec. Node YAML and seed-time config reference these
/// by variant.
///
/// Tier reflects the material tier the node belongs to — a
/// `CopperVein` is tier 1, `MithrilVein` tier 5, `Adamantine` tier 6.
/// Loot-table material_tier uses this to match zone content progression.
#[derive(
    Component, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    // Mining nodes
    CopperVein,
    IronVein,
    SilverVein,
    MithrilVein,
    AdamantineVein,
    // Herbalism nodes
    StanchweedPatch,
    SunleafPatch,
    BlightrootPatch,
    SilverfrondPatch,
    EmberbloomPatch,
    GhostcapPatch,
    // Logging nodes
    PineTree,
    OakTree,
    YewTree,
    IronwoodTree,
    // (Skinning uses mob-corpse interaction, not world nodes.)
}

impl NodeKind {
    pub fn profession(self) -> Profession {
        match self {
            Self::CopperVein
            | Self::IronVein
            | Self::SilverVein
            | Self::MithrilVein
            | Self::AdamantineVein => Profession::Mining,
            Self::StanchweedPatch
            | Self::SunleafPatch
            | Self::BlightrootPatch
            | Self::SilverfrondPatch
            | Self::EmberbloomPatch
            | Self::GhostcapPatch => Profession::Herbalism,
            Self::PineTree | Self::OakTree | Self::YewTree | Self::IronwoodTree => {
                Profession::Logging
            }
        }
    }

    pub fn tier(self) -> u8 {
        match self {
            Self::CopperVein | Self::StanchweedPatch | Self::PineTree => 1,
            Self::IronVein | Self::SunleafPatch | Self::OakTree => 2,
            Self::SilverVein | Self::BlightrootPatch | Self::SilverfrondPatch | Self::YewTree => 3,
            Self::MithrilVein | Self::EmberbloomPatch | Self::IronwoodTree => 5,
            Self::AdamantineVein | Self::GhostcapPatch => 6,
        }
    }

    /// Item id that this node yields on successful harvest. Must match
    /// a material-kind `ItemBase` id in the content registry. Yields
    /// one unit at v1 balance; richer curves (ore grades, herb yield
    /// per skill level) come in a balance pass.
    pub fn yield_item_id(self) -> &'static str {
        match self {
            Self::CopperVein => "copper_ingot",
            Self::IronVein => "iron_ingot",
            Self::SilverVein => "silver_ingot",
            Self::MithrilVein => "mithril_ingot",
            Self::AdamantineVein => "adamantine_ingot",
            Self::StanchweedPatch => "stanchweed",
            Self::SunleafPatch => "sunleaf",
            Self::BlightrootPatch => "blightroot",
            Self::SilverfrondPatch => "silverfrond",
            Self::EmberbloomPatch => "emberbloom",
            Self::GhostcapPatch => "ghostcap",
            Self::PineTree => "pine_plank",
            Self::OakTree => "oak_plank",
            Self::YewTree => "yew_plank",
            Self::IronwoodTree => "ironwood_plank",
        }
    }

    /// Minimum profession skill needed. Tier 1 nodes are always open;
    /// higher tiers gate.
    pub fn min_skill(self) -> u16 {
        match self.tier() {
            1 => 0,
            2 => 25,
            3 => 75,
            4 => 150,
            5 => 225,
            _ => 300,
        }
    }

    /// How long the node takes to respawn after harvest, in seconds.
    /// Higher-tier nodes respawn slower to keep them meaningful.
    pub fn respawn_secs(self) -> f32 {
        match self.tier() {
            1 => 60.0,
            2 => 90.0,
            3 => 150.0,
            4 => 240.0,
            5 => 360.0,
            _ => 600.0,
        }
    }

    pub fn display(self) -> &'static str {
        match self {
            Self::CopperVein => "Copper Vein",
            Self::IronVein => "Iron Vein",
            Self::SilverVein => "Silver Vein",
            Self::MithrilVein => "Mithril Vein",
            Self::AdamantineVein => "Adamantine Vein",
            Self::StanchweedPatch => "Stanchweed",
            Self::SunleafPatch => "Sunleaf",
            Self::BlightrootPatch => "Blightroot",
            Self::SilverfrondPatch => "Silverfrond",
            Self::EmberbloomPatch => "Emberbloom",
            Self::GhostcapPatch => "Ghostcap",
            Self::PineTree => "Pine Tree",
            Self::OakTree => "Oak Tree",
            Self::YewTree => "Yew Tree",
            Self::IronwoodTree => "Ironwood Tree",
        }
    }
}

/// Node state. Available nodes are harvestable; Harvested nodes wait
/// for their respawn timer. Rendered differently client-side.
#[derive(Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeState {
    Available,
    Harvested { remaining_secs: f32 },
}

impl Default for NodeState {
    fn default() -> Self {
        Self::Available
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profession_skills_get_set_default_zero() {
        let mut s = ProfessionSkills::default();
        assert_eq!(s.get(Profession::Mining), 0);
        s.set(Profession::Mining, 100);
        assert_eq!(s.get(Profession::Mining), 100);
    }

    #[test]
    fn profession_skills_add_clamps_to_range() {
        let mut s = ProfessionSkills::default();
        s.add(Profession::Mining, 600);
        assert_eq!(s.get(Profession::Mining), 500);
        s.add(Profession::Mining, -700);
        assert_eq!(s.get(Profession::Mining), 0);
    }

    #[test]
    fn tier1_nodes_open_to_untrained() {
        assert_eq!(NodeKind::CopperVein.min_skill(), 0);
        assert_eq!(NodeKind::StanchweedPatch.min_skill(), 0);
        assert_eq!(NodeKind::PineTree.min_skill(), 0);
    }

    #[test]
    fn higher_tier_nodes_gate_on_skill() {
        assert!(NodeKind::MithrilVein.min_skill() > NodeKind::IronVein.min_skill());
        assert!(NodeKind::AdamantineVein.min_skill() > NodeKind::MithrilVein.min_skill());
    }

    #[test]
    fn all_nodes_yield_valid_material_item_ids() {
        // Spot-check: yield_item_id strings must match what seed_items.py
        // emits for crafting material bases. Compile-time test: &'static
        // str guarantees they're literal; runtime verification against
        // the content registry lives in vaern-server integration tests.
        assert_eq!(NodeKind::CopperVein.yield_item_id(), "copper_ingot");
        assert_eq!(NodeKind::StanchweedPatch.yield_item_id(), "stanchweed");
        assert_eq!(NodeKind::PineTree.yield_item_id(), "pine_plank");
    }
}
