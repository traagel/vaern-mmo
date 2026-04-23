//! Compositional item model (Dwarf Fortress-style).
//!
//! An item in the world is an `ItemInstance` = `(base, material, quality,
//! affixes)`. The registry stores these three axes as small orthogonal
//! tables, and the resolver folds a concrete instance into a
//! `ResolvedItem` with a composed display name and rolled stats.
//!
//! Why this over a flat catalog: balance iteration (tweak "steel" once
//! and every steel item rebalances), memory (≈220 KB vs ≈100 MB once
//! affixes arrive), network (ship 30-byte instance tuples instead of
//! full item defs), and procedural drops (loot tables pick a base + a
//! material-compatible material + a quality distribution, no static
//! manifest required).
//!
//! Legacy flat `Item` / `ItemRegistry` in `lib.rs` stays for equipment
//! and economy consumers until session 2 migrates them onto instances.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use vaern_core::DAMAGE_TYPE_COUNT;

use crate::{ArmorLayer, ArmorType, BodyZone, ItemKind, Rarity, SecondaryStats, SizeClass, WeaponGrip};

// ---------------------------------------------------------------------------
// Base — shape of an item, independent of material + quality
// ---------------------------------------------------------------------------

/// A *base* is the piece shape: "longsword", "breastplate", "cowl",
/// "healing_potion". Carries size + unscaled weight + the parametric
/// stat hooks that `Material` and `Quality` modulate at resolve time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemBase {
    /// Stable piece id referenced by `ItemInstance.base_id`.
    pub id: String,
    /// Human-readable piece noun ("Longsword", "Hauberk"). Joined with
    /// material + quality at display time.
    pub piece_name: String,
    pub size: SizeClass,
    /// Unscaled weight. Multiplied by `Material.weight_mult` at resolve.
    pub base_weight_kg: f32,
    /// Volume fallback — takes precedence over size-class default if set.
    #[serde(default)]
    pub volume_l: Option<f32>,
    /// Stackable consumables / raw materials stack here; equipment
    /// doesn't. Drives ItemInstance stack semantics in inventories.
    #[serde(default)]
    pub stackable: bool,
    #[serde(default = "default_stack_max")]
    pub stack_max: u32,
    #[serde(default)]
    pub no_vendor: bool,
    #[serde(default)]
    pub soulbound: bool,
    /// Explicit vendor base price override. When absent, resolver
    /// derives from weight × volume × rarity at query time.
    #[serde(default)]
    pub vendor_base_price: Option<u32>,
    pub kind: BaseKind,
}

fn default_stack_max() -> u32 {
    1
}

/// Parametric counterpart to `ItemKind`. Carries *pre-material* stat
/// hooks (base_armor_class, base_min_dmg, ...) that get scaled when
/// the instance resolves.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BaseKind {
    Weapon {
        grip: WeaponGrip,
        /// School id (blade/blunt/spear/...) — resolved against vaern-data's
        /// school registry at runtime.
        school: String,
        base_min_dmg: f32,
        base_max_dmg: f32,
    },
    Armor {
        slot: String,
        armor_type: ArmorType,
        layer: ArmorLayer,
        #[serde(default)]
        coverage: Vec<BodyZone>,
        base_armor_class: f32,
    },
    Shield {
        base_armor_class: f32,
        /// Base block chance before material/quality mults. In pct.
        #[serde(default = "default_block_chance")]
        base_block_chance_pct: f32,
        #[serde(default = "default_block_value")]
        base_block_value: f32,
    },
    Rune {
        /// DamageType channel this rune wards. Matches `vaern_core::DamageType`.
        school: String,
        /// Pre-quality resist on the warded channel.
        base_resist: f32,
        /// Pre-quality mp5 drain (NEGATIVE — upkeep cost).
        base_mp5_drain: f32,
    },
    Consumable {
        #[serde(default = "default_charges")]
        charges: u8,
        /// What the item does on use. `None` = inert. Resolved straight
        /// through into `ItemKind::Consumable.effect`.
        #[serde(default)]
        effect: crate::ConsumeEffect,
    },
    Reagent,
    Trinket,
    Quest,
    Material,
    Currency,
    Misc,
}

fn default_block_chance() -> f32 {
    10.0
}
fn default_block_value() -> f32 {
    10.0
}
fn default_charges() -> u8 {
    1
}

// ---------------------------------------------------------------------------
// Material — the substance side of an item
// ---------------------------------------------------------------------------

/// A material modulates stats (weight, armor, damage, resist adds) and
/// contributes to the display name. `valid_for` + `weapon_eligible` +
/// `shield_eligible` gate which bases a material can combine with.
///
/// Material-specific mechanical effects (silver vs necrotic, dragonscale
/// vs fire) land as `resist_adds` — flat per-channel additions on top
/// of the armor_type's base profile. No new combat code needed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Material {
    pub id: String,
    pub display: String,
    /// Progression gate — loot tables filter by tier for encounter
    /// appropriate drops. 1-10 scale, with 1 = copper/linen and
    /// 6+ = mithril/adamantine/dragonhide.
    pub tier: u8,
    pub weight_mult: f32,
    pub ac_mult: f32,
    pub dmg_mult: f32,
    pub resist_adds: [f32; DAMAGE_TYPE_COUNT],
    /// Which ArmorType families accept this material. A material not
    /// in an armor's valid_for triggers `ResolveError::InvalidPairing`.
    #[serde(default)]
    pub valid_for: Vec<ArmorType>,
    #[serde(default)]
    pub weapon_eligible: bool,
    #[serde(default)]
    pub shield_eligible: bool,
    /// Base rarity at material-tier × regular-quality. Quality offset
    /// stacks on top.
    pub base_rarity: Rarity,
}

// ---------------------------------------------------------------------------
// Quality — the craft-roll side of an item
// ---------------------------------------------------------------------------

/// A quality modifies rolled stat magnitude and rarity. Orthogonal to
/// material — a `crude mithril dagger` and a `masterful iron dagger`
/// are both valid and interesting outcomes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quality {
    pub id: String,
    /// Empty for "regular" (display omits the prefix). Non-empty
    /// qualities prepend to the name: "Masterful Steel Longsword".
    pub display: String,
    pub stat_mult: f32,
    /// Applied atop `Material.base_rarity` — a masterful common-material
    /// item can climb to rare, a crude epic-material item drops to
    /// uncommon.
    pub rarity_offset: i8,
}

// ---------------------------------------------------------------------------
// Affix — the "of Warding" / "Enchanted" / "of the Frostwarden" layer
// ---------------------------------------------------------------------------

/// Whether an affix displays as a prefix (before material) or a suffix
/// (after piece). "Enchanted Steel Longsword" = prefix, "Longsword of
/// Warding" = suffix. An item can stack both: "Enchanted Steel
/// Longsword of the Bear of Warding".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AffixPosition {
    Prefix,
    Suffix,
}

/// A modifier applied on top of a (base, material, quality) tuple.
/// Sources: random roll on world drops, crafter-applied reagents,
/// boss-dropped shards (deterministic, soulbinding), rare gathered
/// reagents. All four produce the same `Affix` landing on
/// `ItemInstance.affixes: Vec<String>`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Affix {
    pub id: String,
    /// Display fragment woven into the resolved name. For suffixes
    /// include the connector ("of Warding"), prefixes are bare
    /// ("Enchanted"). Empty display = silent affix (set tags, etc.).
    pub display: String,
    pub position: AffixPosition,
    /// Which base kinds the affix can roll on. Strings matching
    /// `BaseKind` variant names in lowercase: "weapon", "armor",
    /// "shield", "rune". A resolve with a base kind absent from this
    /// list returns `ResolveError::InvalidAffix`.
    #[serde(default)]
    pub applies_to: Vec<String>,
    /// Tier range for random-roll eligibility. Loot tables filter by
    /// `(material.tier >= min_tier && ... <= max_tier)`. Ignored for
    /// shard-only (weight 0) affixes since they're placed explicitly.
    #[serde(default)]
    pub min_tier: u8,
    #[serde(default = "default_max_tier")]
    pub max_tier: u8,
    /// Stat delta folded into the final `ResolvedItem.stats` alongside
    /// material + quality contributions. Zero-default means "flavor
    /// only" affix (e.g. pure visual/narrative tags).
    #[serde(default)]
    pub stat_delta: SecondaryStats,
    /// Loot-roll weight. `0` = this affix never rolls randomly; only
    /// applied deterministically via boss shards, quest rewards, or
    /// crafter rites. Higher weight = more common at random.
    #[serde(default)]
    pub weight: u32,
    /// When true, applying this affix marks the item soulbound. Boss-
    /// shard-sourced affixes use this so raid gear is non-tradeable
    /// once imprinted. Random-rolled affixes leave it false so gear
    /// stays on the player market.
    #[serde(default)]
    pub soulbinds: bool,
}

fn default_max_tier() -> u8 {
    10
}

/// Maximum number of affix slots a rarity can hold. Pre-rolled drops
/// typically fill `max - 1` slots, leaving one open for crafter work.
/// Tier-set pieces override this (always fully slotted from the rite).
pub fn rarity_to_max_slots(r: Rarity) -> u8 {
    match r {
        Rarity::Junk | Rarity::Common => 0,
        Rarity::Uncommon => 1,
        Rarity::Rare => 2,
        Rarity::Epic => 3,
        Rarity::Legendary => 4,
    }
}

// ---------------------------------------------------------------------------
// Instance — the 4-tuple that stores in inventories / drops / Equipped
// ---------------------------------------------------------------------------

/// What the world actually holds: a tuple into the registry + an
/// optional affix list (empty until the affix system lands). Stored
/// in inventories, Equipped, on-the-ground drops, and over the wire.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ItemInstance {
    pub base_id: String,
    /// Absent for bases with no material axis (consumables, reagents,
    /// materials themselves, runes — runes carry their school on the
    /// base, not on a metal).
    #[serde(default)]
    pub material_id: Option<String>,
    pub quality_id: String,
    #[serde(default)]
    pub affixes: Vec<String>,
}

impl ItemInstance {
    /// Quick constructor for equipment without a material axis (runes,
    /// consumables). Instance identity uses quality only.
    pub fn materialless(base_id: impl Into<String>, quality_id: impl Into<String>) -> Self {
        Self {
            base_id: base_id.into(),
            material_id: None,
            quality_id: quality_id.into(),
            affixes: Vec::new(),
        }
    }

    /// Constructor for gear with a material axis.
    pub fn new(
        base_id: impl Into<String>,
        material_id: impl Into<String>,
        quality_id: impl Into<String>,
    ) -> Self {
        Self {
            base_id: base_id.into(),
            material_id: Some(material_id.into()),
            quality_id: quality_id.into(),
            affixes: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Resolved — the composed view, ready for display + combat
// ---------------------------------------------------------------------------

/// Composed, display-ready item view. Built by `ContentRegistry::resolve`.
/// Carries the instance that produced it for round-tripping (network,
/// persistence).
#[derive(Debug, Clone)]
pub struct ResolvedItem {
    pub id: String,
    pub display_name: String,
    pub size: SizeClass,
    pub weight_kg: f32,
    pub rarity: Rarity,
    pub stackable: bool,
    pub stack_max: u32,
    pub no_vendor: bool,
    pub soulbound: bool,
    pub base_price: u32,
    pub stats: SecondaryStats,
    /// Resolved kind — same enum the legacy Item uses, so economy /
    /// equipment see a familiar shape.
    pub kind: ItemKind,
    pub instance: ItemInstance,
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

/// Storage for the four part tables. Populated from
/// `src/generated/items/` at server start; queried by loot systems,
/// inventories, and equipment-display code.
#[derive(Debug, Default, Clone)]
pub struct ContentRegistry {
    bases: HashMap<String, ItemBase>,
    materials: HashMap<String, Material>,
    qualities: HashMap<String, Quality>,
    affixes: HashMap<String, Affix>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct LoadCounts {
    pub bases: usize,
    pub materials: usize,
    pub qualities: usize,
    pub affixes: usize,
}

impl ContentRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_base(&self, id: &str) -> Option<&ItemBase> {
        self.bases.get(id)
    }

    pub fn get_material(&self, id: &str) -> Option<&Material> {
        self.materials.get(id)
    }

    pub fn get_quality(&self, id: &str) -> Option<&Quality> {
        self.qualities.get(id)
    }

    pub fn get_affix(&self, id: &str) -> Option<&Affix> {
        self.affixes.get(id)
    }

    pub fn bases(&self) -> impl Iterator<Item = &ItemBase> {
        self.bases.values()
    }
    pub fn materials(&self) -> impl Iterator<Item = &Material> {
        self.materials.values()
    }
    pub fn qualities(&self) -> impl Iterator<Item = &Quality> {
        self.qualities.values()
    }
    pub fn affixes(&self) -> impl Iterator<Item = &Affix> {
        self.affixes.values()
    }

    pub fn counts(&self) -> LoadCounts {
        LoadCounts {
            bases: self.bases.len(),
            materials: self.materials.len(),
            qualities: self.qualities.len(),
            affixes: self.affixes.len(),
        }
    }

    pub fn insert_base(&mut self, b: ItemBase) {
        self.bases.insert(b.id.clone(), b);
    }
    pub fn insert_material(&mut self, m: Material) {
        self.materials.insert(m.id.clone(), m);
    }
    pub fn insert_quality(&mut self, q: Quality) {
        self.qualities.insert(q.id.clone(), q);
    }
    pub fn insert_affix(&mut self, a: Affix) {
        self.affixes.insert(a.id.clone(), a);
    }

    /// Recursive loader. Expected tree:
    /// ```text
    /// <root>/
    ///   bases/          (**/*.yaml — ItemBase payloads)
    ///   materials.yaml  (Material payloads)
    ///   qualities.yaml  (Quality payloads)
    ///   affixes.yaml    (Affix payloads)
    /// ```
    /// Each file is `{<collection_key>: [...]}` — `bases`, `materials`,
    /// `qualities`, `affixes` respectively. The loader detects collection
    /// by file path rather than content, so empty files error early.
    pub fn load_tree(&mut self, root: impl AsRef<Path>) -> Result<LoadCounts, LoadError> {
        let root = root.as_ref();
        let bases_root = root.join("bases");
        if bases_root.exists() {
            self.load_bases_tree(&bases_root)?;
        }
        let mats_path = root.join("materials.yaml");
        if mats_path.exists() {
            self.load_materials_file(&mats_path)?;
        }
        let qual_path = root.join("qualities.yaml");
        if qual_path.exists() {
            self.load_qualities_file(&qual_path)?;
        }
        let affix_path = root.join("affixes.yaml");
        if affix_path.exists() {
            self.load_affixes_file(&affix_path)?;
        }
        Ok(self.counts())
    }

    fn load_bases_tree(&mut self, dir: &Path) -> Result<(), LoadError> {
        let mut stack: Vec<PathBuf> = vec![dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            for entry in fs::read_dir(&d).map_err(|e| LoadError::Io {
                path: d.display().to_string(),
                source: e,
            })? {
                let entry = entry.map_err(|e| LoadError::Io {
                    path: d.display().to_string(),
                    source: e,
                })?;
                let p = entry.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().and_then(|s| s.to_str()) == Some("yaml") {
                    self.load_bases_file(&p)?;
                }
            }
        }
        Ok(())
    }

    fn load_bases_file(&mut self, path: &Path) -> Result<(), LoadError> {
        #[derive(Deserialize)]
        struct Payload {
            bases: Vec<ItemBase>,
        }
        let text = fs::read_to_string(path).map_err(|e| LoadError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        let p: Payload = serde_yaml::from_str(&text).map_err(|e| LoadError::Yaml {
            path: path.display().to_string(),
            source: e,
        })?;
        for b in p.bases {
            self.insert_base(b);
        }
        Ok(())
    }

    fn load_materials_file(&mut self, path: &Path) -> Result<(), LoadError> {
        #[derive(Deserialize)]
        struct Payload {
            materials: Vec<Material>,
        }
        let text = fs::read_to_string(path).map_err(|e| LoadError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        let p: Payload = serde_yaml::from_str(&text).map_err(|e| LoadError::Yaml {
            path: path.display().to_string(),
            source: e,
        })?;
        for m in p.materials {
            self.insert_material(m);
        }
        Ok(())
    }

    fn load_qualities_file(&mut self, path: &Path) -> Result<(), LoadError> {
        #[derive(Deserialize)]
        struct Payload {
            qualities: Vec<Quality>,
        }
        let text = fs::read_to_string(path).map_err(|e| LoadError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        let p: Payload = serde_yaml::from_str(&text).map_err(|e| LoadError::Yaml {
            path: path.display().to_string(),
            source: e,
        })?;
        for q in p.qualities {
            self.insert_quality(q);
        }
        Ok(())
    }

    fn load_affixes_file(&mut self, path: &Path) -> Result<(), LoadError> {
        #[derive(Deserialize)]
        struct Payload {
            affixes: Vec<Affix>,
        }
        let text = fs::read_to_string(path).map_err(|e| LoadError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        let p: Payload = serde_yaml::from_str(&text).map_err(|e| LoadError::Yaml {
            path: path.display().to_string(),
            source: e,
        })?;
        for a in p.affixes {
            self.insert_affix(a);
        }
        Ok(())
    }

    // ---- resolve ----------------------------------------------------------

    /// Fold an instance into a display-ready item. Returns a
    /// `ResolveError` when the tuple is invalid (missing part, a
    /// material that doesn't fit the base's armor_type, or an affix
    /// that doesn't apply to the base kind).
    pub fn resolve(&self, inst: &ItemInstance) -> Result<ResolvedItem, ResolveError> {
        let base = self
            .get_base(&inst.base_id)
            .ok_or_else(|| ResolveError::UnknownBase(inst.base_id.clone()))?;
        let quality = self
            .get_quality(&inst.quality_id)
            .ok_or_else(|| ResolveError::UnknownQuality(inst.quality_id.clone()))?;
        let material = match &inst.material_id {
            Some(id) => Some(
                self.get_material(id)
                    .ok_or_else(|| ResolveError::UnknownMaterial(id.clone()))?,
            ),
            None => None,
        };

        // Material-pairing validation.
        if let Some(m) = material {
            match &base.kind {
                BaseKind::Armor { armor_type, .. } if !m.valid_for.contains(armor_type) => {
                    return Err(ResolveError::InvalidPairing {
                        base: inst.base_id.clone(),
                        material: m.id.clone(),
                    });
                }
                BaseKind::Weapon { .. } if !m.weapon_eligible => {
                    return Err(ResolveError::InvalidPairing {
                        base: inst.base_id.clone(),
                        material: m.id.clone(),
                    });
                }
                BaseKind::Shield { .. } if !m.shield_eligible => {
                    return Err(ResolveError::InvalidPairing {
                        base: inst.base_id.clone(),
                        material: m.id.clone(),
                    });
                }
                _ => {}
            }
        }

        // Resolve affix ids → &Affix refs + validate applies_to.
        // Preserve `inst.affixes` order for deterministic display/id.
        let base_kind_tag = base_kind_tag(&base.kind);
        let mut resolved_affixes: Vec<&Affix> = Vec::with_capacity(inst.affixes.len());
        for aid in &inst.affixes {
            let a = self
                .get_affix(aid)
                .ok_or_else(|| ResolveError::UnknownAffix(aid.clone()))?;
            if !a.applies_to.iter().any(|t| t == base_kind_tag) {
                return Err(ResolveError::InvalidAffix {
                    base: inst.base_id.clone(),
                    affix: a.id.clone(),
                });
            }
            resolved_affixes.push(a);
        }

        let weight_mult = material.map(|m| m.weight_mult).unwrap_or(1.0);
        let ac_mult = material.map(|m| m.ac_mult).unwrap_or(1.0);
        let dmg_mult = material.map(|m| m.dmg_mult).unwrap_or(1.0);
        let resist_adds = material
            .map(|m| m.resist_adds)
            .unwrap_or([0.0; DAMAGE_TYPE_COUNT]);

        let weight_kg = base.base_weight_kg * weight_mult;

        // Rarity = material base + quality offset, clamped.
        let base_rarity = material.map(|m| m.base_rarity).unwrap_or(Rarity::Common);
        let rarity = apply_rarity_offset(base_rarity, quality.rarity_offset);

        // Build stats + kind per base variant, then fold affix stat deltas
        // on top. Affixes stack additively in listed order.
        let (mut stats, kind) =
            fold_base(&base.kind, ac_mult, dmg_mult, quality.stat_mult, resist_adds);
        for a in &resolved_affixes {
            stats.add(&a.stat_delta);
        }

        // Soulbound = base flag OR any applied affix's soulbind flag.
        // Boss-shard-sourced affixes carry `soulbinds: true` so imprinting
        // them makes the item non-tradeable.
        let soulbound = base.soulbound || resolved_affixes.iter().any(|a| a.soulbinds);

        // Compose display + id with affixes woven in.
        let display_name = compose_display(quality, material, base, &resolved_affixes);
        let id = compose_id(inst);

        // Base price — override wins; else formula on resolved weight/vol/rarity.
        let base_price = base.vendor_base_price.unwrap_or_else(|| {
            let vol = base.volume_l.unwrap_or_else(|| base.size.default_volume_l());
            let scalar = (2.0 * weight_kg + 3.0 * vol + 10.0).max(1.0);
            (scalar * rarity.price_multiplier()).ceil() as u32
        });

        Ok(ResolvedItem {
            id,
            display_name,
            size: base.size,
            weight_kg,
            rarity,
            stackable: base.stackable,
            stack_max: base.stack_max,
            no_vendor: base.no_vendor,
            soulbound,
            base_price,
            stats,
            kind,
            instance: inst.clone(),
        })
    }
}

/// Lowercase tag string that `Affix.applies_to` entries are matched
/// against. Keep in lockstep with the BaseKind variant names so YAML
/// authors can write `applies_to: [weapon, armor]` naturally.
fn base_kind_tag(kind: &BaseKind) -> &'static str {
    match kind {
        BaseKind::Weapon { .. } => "weapon",
        BaseKind::Armor { .. } => "armor",
        BaseKind::Shield { .. } => "shield",
        BaseKind::Rune { .. } => "rune",
        BaseKind::Consumable { .. } => "consumable",
        BaseKind::Reagent => "reagent",
        BaseKind::Trinket => "trinket",
        BaseKind::Quest => "quest",
        BaseKind::Material => "material",
        BaseKind::Currency => "currency",
        BaseKind::Misc => "misc",
    }
}

fn apply_rarity_offset(base: Rarity, offset: i8) -> Rarity {
    // Ordinals 0..=5: junk, common, uncommon, rare, epic, legendary.
    let ord = match base {
        Rarity::Junk => 0i16,
        Rarity::Common => 1,
        Rarity::Uncommon => 2,
        Rarity::Rare => 3,
        Rarity::Epic => 4,
        Rarity::Legendary => 5,
    };
    let shifted = (ord + offset as i16).clamp(0, 5);
    match shifted {
        0 => Rarity::Junk,
        1 => Rarity::Common,
        2 => Rarity::Uncommon,
        3 => Rarity::Rare,
        4 => Rarity::Epic,
        _ => Rarity::Legendary,
    }
}

fn fold_base(
    kind: &BaseKind,
    ac_mult: f32,
    dmg_mult: f32,
    q_mult: f32,
    resist_adds: [f32; DAMAGE_TYPE_COUNT],
) -> (SecondaryStats, ItemKind) {
    let mut stats = SecondaryStats::default();
    match kind {
        BaseKind::Armor {
            base_armor_class,
            armor_type,
            slot,
            layer,
            coverage,
        } => {
            let armor = (*base_armor_class * ac_mult * q_mult).max(0.0) as u32;
            stats.armor = armor;
            // Per-channel resist profile = armor_type profile × ac_mult × q_mult
            // (scaled with material) + flat resist_adds (material-specific
            // effect — silver vs necrotic, dragonscale vs fire).
            let profile = armor_type.base_resist_profile();
            for i in 0..DAMAGE_TYPE_COUNT {
                stats.resists[i] = profile[i] * ac_mult * q_mult + resist_adds[i];
            }
            (
                stats,
                ItemKind::Armor {
                    slot: slot.clone(),
                    armor_class: armor.min(u16::MAX as u32) as u16,
                    armor_type: *armor_type,
                    layer: *layer,
                    coverage: coverage.clone(),
                },
            )
        }
        BaseKind::Weapon {
            grip,
            school,
            base_min_dmg,
            base_max_dmg,
        } => {
            stats.weapon_min_dmg = base_min_dmg * dmg_mult * q_mult;
            stats.weapon_max_dmg = base_max_dmg * dmg_mult * q_mult;
            (
                stats,
                ItemKind::Weapon {
                    grip: *grip,
                    school: school.clone(),
                },
            )
        }
        BaseKind::Shield {
            base_armor_class,
            base_block_chance_pct,
            base_block_value,
        } => {
            let armor = (*base_armor_class * ac_mult * q_mult).max(0.0) as u32;
            stats.armor = armor;
            stats.block_chance_pct = base_block_chance_pct * q_mult;
            stats.block_value = (base_block_value * ac_mult * q_mult).max(0.0) as u32;
            (
                stats,
                ItemKind::Shield {
                    armor_class: armor.min(u16::MAX as u32) as u16,
                },
            )
        }
        BaseKind::Rune {
            school,
            base_resist,
            base_mp5_drain,
        } => {
            if let Some(dt) = vaern_core::DamageType::from_str(school) {
                stats.resists[dt.index()] = base_resist * q_mult;
            }
            // Drain is signed — higher quality drains more (the warding
            // is *stronger*, so upkeep is higher). Keeps the "magic-tank"
            // trade meaningful at endgame.
            stats.mp5 = base_mp5_drain * q_mult;
            (
                stats,
                ItemKind::Rune {
                    school: school.clone(),
                },
            )
        }
        BaseKind::Consumable { charges, effect } => (
            stats,
            ItemKind::Consumable {
                charges: *charges,
                effect: effect.clone(),
            },
        ),
        BaseKind::Reagent => (stats, ItemKind::Reagent),
        BaseKind::Trinket => (stats, ItemKind::Trinket),
        BaseKind::Quest => (stats, ItemKind::Quest),
        BaseKind::Material => (stats, ItemKind::Material),
        BaseKind::Currency => (stats, ItemKind::Currency),
        BaseKind::Misc => (stats, ItemKind::Misc),
    }
}

/// Compose the display name with prefixes woven before the material
/// and suffixes after the piece. Ordering: `{quality} {prefixes*}
/// {material} {piece} {suffixes*}`. Quality stays outermost (it's the
/// craft-roll adjective); prefixes are visual/narrative
/// ("Enchanted", "Blessed"); suffixes are the "of X" fragments.
fn compose_display(
    quality: &Quality,
    material: Option<&Material>,
    base: &ItemBase,
    affixes: &[&Affix],
) -> String {
    let mut name = String::new();
    if !quality.display.is_empty() {
        name.push_str(&quality.display);
        name.push(' ');
    }
    for a in affixes.iter().filter(|a| a.position == AffixPosition::Prefix) {
        if !a.display.is_empty() {
            name.push_str(&a.display);
            name.push(' ');
        }
    }
    if let Some(m) = material {
        name.push_str(&m.display);
        name.push(' ');
    }
    name.push_str(&base.piece_name);
    for a in affixes.iter().filter(|a| a.position == AffixPosition::Suffix) {
        if !a.display.is_empty() {
            name.push(' ');
            name.push_str(&a.display);
        }
    }
    name
}

fn compose_id(inst: &ItemInstance) -> String {
    let mut id = match (&inst.material_id, inst.quality_id.as_str()) {
        (Some(m), "regular") => format!("{m}_{}", inst.base_id),
        (Some(m), q) => format!("{q}_{m}_{}", inst.base_id),
        (None, "regular") => inst.base_id.clone(),
        (None, q) => format!("{q}_{}", inst.base_id),
    };
    // Affixes appended with `+` separator so they don't collide with the
    // underscore-delimited base/material/quality segments.
    for aid in &inst.affixes {
        id.push('+');
        id.push_str(aid);
    }
    id
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum LoadError {
    #[error("io at {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("yaml parse at {path}: {source}")]
    Yaml {
        path: String,
        source: serde_yaml::Error,
    },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ResolveError {
    #[error("unknown base `{0}`")]
    UnknownBase(String),
    #[error("unknown material `{0}`")]
    UnknownMaterial(String),
    #[error("unknown quality `{0}`")]
    UnknownQuality(String),
    #[error("unknown affix `{0}`")]
    UnknownAffix(String),
    #[error("invalid pairing: base `{base}` does not accept material `{material}`")]
    InvalidPairing { base: String, material: String },
    #[error("invalid affix: base `{base}` does not accept affix `{affix}`")]
    InvalidAffix { base: String, affix: String },
}
