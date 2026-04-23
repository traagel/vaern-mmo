//! Player equipment slots + the `Equipped` component that holds what's
//! currently worn.
//!
//! Session 5 note: migrated from `slot → String` (legacy flat Item id)
//! to `slot → ItemInstance` (Model B composition tuple). All inspection
//! helpers now resolve through the `ContentRegistry` on demand. This is
//! the "network-cheap storage, fold on read" pattern.
//!
//! Inventory storage is NOT in this crate — equipment is the worn state
//! only. The future `vaern-inventory` crate will own bags and stacks.

use std::collections::HashMap;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use vaern_items::{ContentRegistry, ItemInstance, ItemKind, ResolveError, ResolvedItem, WeaponGrip};

/// Canonical equipment slots. Offhand gates on the mainhand's grip
/// (TwoHanded excludes offhand). Ring1/Ring2 + Trinket1/Trinket2 pairs
/// are addressed directly; callers that want "any empty ring" should
/// iterate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EquipSlot {
    Head,
    Shoulders,
    Chest,
    /// Cosmetic / transmog layer — accepts armor items declaring
    /// `slot: "shirt"`. Phase A holds the Under-armor layer.
    Shirt,
    /// Faction/institution emblem worn over chest. Reputation-gain
    /// hooks (institution tabards accelerating rep) live in server,
    /// not here — this is just the equipment slot.
    Tabard,
    Back,
    Wrists,
    Hands,
    Waist,
    Legs,
    Feet,
    Neck,
    Ring1,
    Ring2,
    Trinket1,
    Trinket2,
    MainHand,
    OffHand,
    Ranged,
    /// Caster-only magical ward generator slot. Accepts only
    /// `ItemKind::Rune` items. The rune provides magical absorption
    /// (via `ResolvedItem.stats.resists`) at a mana upkeep (negative
    /// `stats.mp5`) — the "Gandalf magic-tank" build surface.
    Focus,
}

impl EquipSlot {
    /// All slots in a canonical order, for iteration (UI layout, full
    /// unequip, encumbrance totals).
    pub const ALL: [EquipSlot; 20] = [
        EquipSlot::Head,
        EquipSlot::Shoulders,
        EquipSlot::Chest,
        EquipSlot::Shirt,
        EquipSlot::Tabard,
        EquipSlot::Back,
        EquipSlot::Wrists,
        EquipSlot::Hands,
        EquipSlot::Waist,
        EquipSlot::Legs,
        EquipSlot::Feet,
        EquipSlot::Neck,
        EquipSlot::Ring1,
        EquipSlot::Ring2,
        EquipSlot::Trinket1,
        EquipSlot::Trinket2,
        EquipSlot::MainHand,
        EquipSlot::OffHand,
        EquipSlot::Ranged,
        EquipSlot::Focus,
    ];

    /// Armor `slot` string used in base YAMLs (`kind: {type: armor, slot: "chest"}`).
    /// Used to validate armor-base → slot placement.
    fn armor_slot_id(self) -> Option<&'static str> {
        Some(match self {
            Self::Head => "head",
            Self::Shoulders => "shoulders",
            Self::Chest => "chest",
            Self::Shirt => "shirt",
            Self::Tabard => "tabard",
            Self::Back => "back",
            Self::Wrists => "wrists",
            Self::Hands => "hands",
            Self::Waist => "waist",
            Self::Legs => "legs",
            Self::Feet => "feet",
            Self::Neck => "neck",
            Self::Ring1 | Self::Ring2 => "ring",
            Self::Trinket1 | Self::Trinket2 => "trinket",
            Self::MainHand | Self::OffHand | Self::Ranged | Self::Focus => return None,
        })
    }
}

/// Equipped state per player entity. Slots store `ItemInstance` tuples
/// — resolve through the registry for display or stat inspection.
///
/// Not replicated directly; server stays authoritative and clients see
/// resolved stats via a snapshot message (same pattern as
/// `PlayerStateSnapshot`).
#[derive(Component, Debug, Default, Clone, Serialize, Deserialize)]
pub struct Equipped {
    slots: HashMap<EquipSlot, ItemInstance>,
}

impl Equipped {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, slot: EquipSlot) -> Option<&ItemInstance> {
        self.slots.get(&slot)
    }

    /// Borrow the underlying slot map. Useful for cheap read-only
    /// consumers (asset resolver, snapshot broadcast) that don't want
    /// to wrap every lookup through `get`.
    pub fn slots(&self) -> &HashMap<EquipSlot, ItemInstance> {
        &self.slots
    }

    pub fn is_empty_slot(&self, slot: EquipSlot) -> bool {
        !self.slots.contains_key(&slot)
    }

    pub fn iter(&self) -> impl Iterator<Item = (EquipSlot, &ItemInstance)> {
        self.slots.iter().map(|(s, inst)| (*s, inst))
    }

    /// Equip `instance` into `slot`, resolving it against the registry
    /// for validation. Returns the previously equipped instance if any.
    ///
    /// Two-handed mainhand forces offhand unequip; the unseated offhand
    /// is returned via `displaced_offhand` so the caller can move it
    /// back to inventory.
    pub fn equip(
        &mut self,
        slot: EquipSlot,
        instance: ItemInstance,
        registry: &ContentRegistry,
    ) -> Result<EquipResult, EquipError> {
        let resolved = registry.resolve(&instance).map_err(EquipError::Resolve)?;
        validate_slot_for_item(slot, &resolved)?;

        let mut displaced_offhand = None;
        if slot == EquipSlot::MainHand {
            if let ItemKind::Weapon {
                grip: WeaponGrip::TwoHanded,
                ..
            } = &resolved.kind
            {
                displaced_offhand = self.slots.remove(&EquipSlot::OffHand);
            }
        }
        if slot == EquipSlot::OffHand {
            if let Some(mh_inst) = self.slots.get(&EquipSlot::MainHand) {
                if let Ok(mh) = registry.resolve(mh_inst) {
                    if let ItemKind::Weapon {
                        grip: WeaponGrip::TwoHanded,
                        ..
                    } = &mh.kind
                    {
                        return Err(EquipError::OffHandBlockedByTwoHander);
                    }
                }
            }
        }
        let previous = self.slots.insert(slot, instance);
        Ok(EquipResult {
            previous,
            displaced_offhand,
        })
    }

    /// Clear a slot; return the previously equipped instance, if any.
    pub fn unequip(&mut self, slot: EquipSlot) -> Option<ItemInstance> {
        self.slots.remove(&slot)
    }

    /// Total worn weight in kilograms. Unresolvable instances (stale
    /// material/base reference) are skipped silently — caller sees a
    /// registry gap, not a crash.
    pub fn total_weight_kg(&self, registry: &ContentRegistry) -> f32 {
        self.slots
            .values()
            .filter_map(|inst| registry.resolve(inst).ok())
            .map(|r| r.weight_kg)
            .sum()
    }

    /// Total worn armor class (armor + shield contributions via
    /// `ResolvedItem.stats.armor`, which fold_base fills for both kinds).
    pub fn total_armor_class(&self, registry: &ContentRegistry) -> u32 {
        self.slots
            .values()
            .filter_map(|inst| registry.resolve(inst).ok())
            .map(|r| r.stats.armor)
            .sum()
    }
}

/// Returned by a successful `equip`. `previous` is the instance that
/// was in the target slot (for the caller to push back to inventory).
/// `displaced_offhand` is set when a two-handed weapon kicked the
/// offhand out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EquipResult {
    pub previous: Option<ItemInstance>,
    pub displaced_offhand: Option<ItemInstance>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum EquipError {
    #[error("item kind doesn't match slot")]
    SlotKindMismatch,
    #[error("armor piece declared slot `{declared}`, asked to equip to `{target}`")]
    ArmorSlotMismatch { declared: String, target: String },
    #[error("can't equip to offhand while a two-handed weapon is in the main hand")]
    OffHandBlockedByTwoHander,
    #[error("shields go in the offhand, not a weapon slot")]
    ShieldNotInMainHand,
    #[error("weapon of grip `{grip:?}` can't go in slot `{slot:?}`")]
    GripSlotMismatch { grip: WeaponGrip, slot: EquipSlot },
    #[error("couldn't resolve instance: {0}")]
    Resolve(ResolveError),
}

/// Pure validation: does `item` belong in `slot`? Factored out so the
/// server can show a tooltip ("can't equip here") without mutating state.
pub fn validate_slot_for_item(slot: EquipSlot, item: &ResolvedItem) -> Result<(), EquipError> {
    match (&item.kind, slot) {
        (ItemKind::Weapon { grip, .. }, EquipSlot::MainHand) => {
            if matches!(grip, WeaponGrip::Light | WeaponGrip::OneHanded | WeaponGrip::TwoHanded) {
                Ok(())
            } else {
                Err(EquipError::GripSlotMismatch {
                    grip: *grip,
                    slot,
                })
            }
        }
        (ItemKind::Weapon { grip, .. }, EquipSlot::OffHand) => {
            // Only light + one-handed go in the offhand.
            if matches!(grip, WeaponGrip::Light | WeaponGrip::OneHanded) {
                Ok(())
            } else {
                Err(EquipError::GripSlotMismatch {
                    grip: *grip,
                    slot,
                })
            }
        }
        (ItemKind::Weapon { .. }, EquipSlot::Ranged) => Ok(()),
        (ItemKind::Shield { .. }, EquipSlot::OffHand) => Ok(()),
        (ItemKind::Shield { .. }, _) => Err(EquipError::ShieldNotInMainHand),
        (ItemKind::Rune { .. }, EquipSlot::Focus) => Ok(()),
        (ItemKind::Rune { .. }, _) => Err(EquipError::SlotKindMismatch),
        (ItemKind::Armor { slot: declared, .. }, target) => {
            if target.armor_slot_id() == Some(declared.as_str()) {
                Ok(())
            } else {
                Err(EquipError::ArmorSlotMismatch {
                    declared: declared.clone(),
                    target: format!("{target:?}"),
                })
            }
        }
        _ => Err(EquipError::SlotKindMismatch),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vaern_items::{
        ArmorLayer, ArmorType, BaseKind, ItemBase, Material, Quality, Rarity, SizeClass,
    };

    // ---------------------------------------------------------------
    // In-memory registry builder for tests. `regular` quality has
    // stat_mult=1.0 so weight/AC math lines up with the base's raw
    // values — keeps weight assertions readable.
    //
    // Test material "testmetal" is valid_for every ArmorType family +
    // weapon + shield — so tests don't hit InvalidPairing on any base
    // shape. Prod registry has realistic constraints in
    // `scripts/items/materials.py`.
    // ---------------------------------------------------------------

    fn test_registry(bases: Vec<ItemBase>) -> ContentRegistry {
        let mut r = ContentRegistry::new();
        for b in bases {
            r.insert_base(b);
        }
        r.insert_material(Material {
            id: "testmetal".into(),
            display: "Testmetal".into(),
            tier: 1,
            weight_mult: 1.0,
            ac_mult: 1.0,
            dmg_mult: 1.0,
            resist_adds: [0.0; 12],
            valid_for: vec![
                ArmorType::Cloth,
                ArmorType::Gambeson,
                ArmorType::Leather,
                ArmorType::Mail,
                ArmorType::Plate,
            ],
            weapon_eligible: true,
            shield_eligible: true,
            base_rarity: Rarity::Common,
        });
        r.insert_quality(Quality {
            id: "regular".into(),
            display: "".into(),
            stat_mult: 1.0,
            rarity_offset: 0,
        });
        r
    }

    fn weapon_base(id: &str, grip: WeaponGrip) -> ItemBase {
        ItemBase {
            id: id.into(),
            piece_name: id.into(),
            size: SizeClass::Medium,
            base_weight_kg: 3.0,
            volume_l: None,
            stackable: false,
            stack_max: 1,
            no_vendor: false,
            soulbound: false,
            vendor_base_price: None,
            kind: BaseKind::Weapon {
                grip,
                school: "blade".into(),
                base_min_dmg: 4.0,
                base_max_dmg: 8.0,
            },
        }
    }

    fn armor_base(id: &str, slot: &str, base_ac: f32) -> ItemBase {
        ItemBase {
            id: id.into(),
            piece_name: id.into(),
            size: SizeClass::Medium,
            base_weight_kg: 5.0,
            volume_l: None,
            stackable: false,
            stack_max: 1,
            no_vendor: false,
            soulbound: false,
            vendor_base_price: None,
            kind: BaseKind::Armor {
                slot: slot.into(),
                armor_type: ArmorType::Cloth,
                layer: ArmorLayer::Under,
                coverage: vec![],
                base_armor_class: base_ac,
            },
        }
    }

    fn shield_base(id: &str, base_ac: f32) -> ItemBase {
        ItemBase {
            id: id.into(),
            piece_name: id.into(),
            size: SizeClass::Medium,
            base_weight_kg: 4.0,
            volume_l: None,
            stackable: false,
            stack_max: 1,
            no_vendor: false,
            soulbound: false,
            vendor_base_price: None,
            kind: BaseKind::Shield {
                base_armor_class: base_ac,
                base_block_chance_pct: 10.0,
                base_block_value: 5.0,
            },
        }
    }

    fn rune_base(id: &str, school: &str) -> ItemBase {
        ItemBase {
            id: id.into(),
            piece_name: id.into(),
            size: SizeClass::Tiny,
            base_weight_kg: 0.2,
            volume_l: None,
            stackable: false,
            stack_max: 1,
            no_vendor: false,
            soulbound: false,
            vendor_base_price: None,
            kind: BaseKind::Rune {
                school: school.into(),
                base_resist: 15.0,
                base_mp5_drain: -1.0,
            },
        }
    }

    /// Materialled instance (armor/weapon/shield).
    fn inst(base: &str) -> ItemInstance {
        ItemInstance::new(base, "testmetal", "regular")
    }

    /// Materialless instance (rune/consumable).
    fn rune_inst(base: &str) -> ItemInstance {
        ItemInstance::materialless(base, "regular")
    }

    #[test]
    fn armor_slot_must_match_declared() {
        let reg = test_registry(vec![armor_base("chain_chest", "chest", 5.0)]);
        let mut eq = Equipped::new();
        assert!(eq.equip(EquipSlot::Chest, inst("chain_chest"), &reg).is_ok());
        // Wrong slot rejected.
        let mut eq = Equipped::new();
        assert!(matches!(
            eq.equip(EquipSlot::Head, inst("chain_chest"), &reg),
            Err(EquipError::ArmorSlotMismatch { .. })
        ));
    }

    #[test]
    fn two_handed_displaces_offhand() {
        let reg = test_registry(vec![
            weapon_base("sword", WeaponGrip::OneHanded),
            weapon_base("greatsword", WeaponGrip::TwoHanded),
            shield_base("buckler", 2.0),
        ]);
        let mut eq = Equipped::new();
        eq.equip(EquipSlot::MainHand, inst("sword"), &reg).unwrap();
        eq.equip(EquipSlot::OffHand, inst("buckler"), &reg).unwrap();
        let result = eq
            .equip(EquipSlot::MainHand, inst("greatsword"), &reg)
            .unwrap();
        assert_eq!(result.previous.as_ref().map(|i| i.base_id.as_str()), Some("sword"));
        assert_eq!(
            result.displaced_offhand.as_ref().map(|i| i.base_id.as_str()),
            Some("buckler")
        );
        assert!(eq.is_empty_slot(EquipSlot::OffHand));
    }

    #[test]
    fn offhand_blocked_while_two_hander_is_equipped() {
        let reg = test_registry(vec![
            weapon_base("greatsword", WeaponGrip::TwoHanded),
            shield_base("buckler", 2.0),
        ]);
        let mut eq = Equipped::new();
        eq.equip(EquipSlot::MainHand, inst("greatsword"), &reg).unwrap();
        assert_eq!(
            eq.equip(EquipSlot::OffHand, inst("buckler"), &reg),
            Err(EquipError::OffHandBlockedByTwoHander)
        );
    }

    #[test]
    fn shield_rejects_mainhand() {
        let reg = test_registry(vec![shield_base("buckler", 2.0)]);
        let mut eq = Equipped::new();
        assert_eq!(
            eq.equip(EquipSlot::MainHand, inst("buckler"), &reg),
            Err(EquipError::ShieldNotInMainHand)
        );
    }

    #[test]
    fn rune_equips_only_in_focus() {
        let reg = test_registry(vec![rune_base("flame_rune", "fire")]);
        let mut eq = Equipped::new();
        assert!(eq.equip(EquipSlot::Focus, rune_inst("flame_rune"), &reg).is_ok());
        // Rejected in any other slot.
        let mut eq = Equipped::new();
        assert_eq!(
            eq.equip(EquipSlot::MainHand, rune_inst("flame_rune"), &reg),
            Err(EquipError::SlotKindMismatch)
        );
        let mut eq = Equipped::new();
        assert_eq!(
            eq.equip(EquipSlot::Trinket1, rune_inst("flame_rune"), &reg),
            Err(EquipError::SlotKindMismatch)
        );
    }

    #[test]
    fn focus_rejects_non_rune_items() {
        let reg = test_registry(vec![
            weapon_base("sword", WeaponGrip::OneHanded),
            armor_base("helm", "head", 2.0),
        ]);
        let mut eq = Equipped::new();
        assert!(eq.equip(EquipSlot::Focus, inst("sword"), &reg).is_err());
        assert!(eq.equip(EquipSlot::Focus, inst("helm"), &reg).is_err());
    }

    #[test]
    fn totals_sum_across_slots() {
        let reg = test_registry(vec![
            armor_base("chain_chest", "chest", 5.0),
            armor_base("iron_helm", "head", 2.0),
            shield_base("buckler", 2.0),
        ]);
        let mut eq = Equipped::new();
        eq.equip(EquipSlot::Chest, inst("chain_chest"), &reg).unwrap();
        eq.equip(EquipSlot::Head, inst("iron_helm"), &reg).unwrap();
        eq.equip(EquipSlot::OffHand, inst("buckler"), &reg).unwrap();
        // Weights: 5 (chest) + 5 (helm base) + 4 (buckler) = 14.
        assert_eq!(eq.total_weight_kg(&reg), 14.0);
        // Armor: 5 + 2 + 2 = 9.
        assert_eq!(eq.total_armor_class(&reg), 9);
    }

    #[test]
    fn unknown_base_resolves_to_error() {
        let reg = test_registry(vec![weapon_base("sword", WeaponGrip::OneHanded)]);
        let mut eq = Equipped::new();
        let result = eq.equip(EquipSlot::MainHand, inst("does_not_exist"), &reg);
        assert!(matches!(result, Err(EquipError::Resolve(_))));
    }
}
