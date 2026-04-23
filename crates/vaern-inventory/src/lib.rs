//! Player inventory — fixed-capacity slot grid holding `ItemInstance` stacks.
//!
//! Stores the worn-but-not-equipped state: a bag. `Equipped` (in
//! vaern-equipment) holds what's currently in slots; pickups / drops /
//! vendor-buy / loot land here first.
//!
//! Leaf crate: core + vaern-items deps only. No ECS plugin registration
//! — server wires the component and systems.
//!
//! ## Stack semantics
//!
//! A stack is identified by its [`StackKey`] = (base_id, material_id,
//! quality_id, affixes). Adding an instance that matches an existing
//! stack merges into it up to the base's `stack_max`. Non-stackable
//! bases always take a fresh slot.
//!
//! ## Slot layout
//!
//! Inventory is a flat `Vec<Option<InventorySlot>>` sized to its
//! `capacity`. Empty slots are `None` so indices stay stable for UI
//! drag-and-drop.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use vaern_items::{ContentRegistry, ItemInstance};

/// Default starter inventory size. 30 feels generous for early game;
/// bag-expansion quests / crafted containers can grow it later.
pub const DEFAULT_CAPACITY: usize = 30;

/// One occupied inventory slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventorySlot {
    pub instance: ItemInstance,
    pub count: u32,
}

impl InventorySlot {
    pub fn new(instance: ItemInstance, count: u32) -> Self {
        Self { instance, count }
    }

    fn stack_key(&self) -> StackKey<'_> {
        StackKey {
            base_id: &self.instance.base_id,
            material_id: self.instance.material_id.as_deref(),
            quality_id: &self.instance.quality_id,
            affixes: &self.instance.affixes,
        }
    }
}

/// Stack identity — two instances stack iff every field matches.
/// Affixes differ by position as well as content (the full Vec).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StackKey<'a> {
    base_id: &'a str,
    material_id: Option<&'a str>,
    quality_id: &'a str,
    affixes: &'a [String],
}

/// Player inventory component. Server-authoritative; clients see it
/// via `InventorySnapshot` messages (to be added in session 7 with UI).
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct PlayerInventory {
    slots: Vec<Option<InventorySlot>>,
}

impl Default for PlayerInventory {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }
}

impl PlayerInventory {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            slots: vec![None; capacity],
        }
    }

    pub fn capacity(&self) -> usize {
        self.slots.len()
    }

    /// Number of occupied slots (not the total item count across stacks).
    pub fn len(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.iter().all(Option::is_none)
    }

    pub fn is_full(&self) -> bool {
        self.slots.iter().all(Option::is_some)
    }

    pub fn get(&self, idx: usize) -> Option<&InventorySlot> {
        self.slots.get(idx).and_then(Option::as_ref)
    }

    pub fn iter(&self) -> impl Iterator<Item = (usize, &InventorySlot)> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.as_ref().map(|sl| (i, sl)))
    }

    /// Add `count` of `instance` to the inventory. Stacks into an
    /// existing matching slot (up to the base's `stack_max`), then
    /// places remainder into empty slots. Returns the number NOT
    /// placed (because inventory filled up) — 0 means everything fit.
    ///
    /// Unknown bases resolve to `count` leftover (couldn't add anything).
    /// Caller can fall back to dropping on the ground.
    pub fn add(
        &mut self,
        instance: ItemInstance,
        count: u32,
        registry: &ContentRegistry,
    ) -> u32 {
        if count == 0 {
            return 0;
        }
        let Some(base) = registry.get_base(&instance.base_id) else {
            return count;
        };
        let stack_max = if base.stackable { base.stack_max.max(1) } else { 1 };
        let mut remaining = count;

        // Phase 1: merge into existing compatible stacks.
        if stack_max > 1 {
            let key = StackKey {
                base_id: &instance.base_id,
                material_id: instance.material_id.as_deref(),
                quality_id: &instance.quality_id,
                affixes: &instance.affixes,
            };
            for slot in self.slots.iter_mut().filter_map(Option::as_mut) {
                if slot.stack_key() == key {
                    let room = stack_max.saturating_sub(slot.count);
                    let take = remaining.min(room);
                    slot.count += take;
                    remaining -= take;
                    if remaining == 0 {
                        return 0;
                    }
                }
            }
        }

        // Phase 2: fill empty slots with fresh stacks (up to stack_max each).
        for empty in self.slots.iter_mut().filter(|s| s.is_none()) {
            let take = remaining.min(stack_max);
            *empty = Some(InventorySlot::new(instance.clone(), take));
            remaining -= take;
            if remaining == 0 {
                return 0;
            }
        }

        remaining
    }

    /// Remove up to `count` from `slot_idx`. Returns (instance, amount)
    /// actually taken. Returns None if the slot is empty.
    pub fn take(&mut self, slot_idx: usize, count: u32) -> Option<(ItemInstance, u32)> {
        let slot = self.slots.get_mut(slot_idx)?.as_mut()?;
        let taken = count.min(slot.count);
        slot.count -= taken;
        let instance = slot.instance.clone();
        if slot.count == 0 {
            self.slots[slot_idx] = None;
        }
        Some((instance, taken))
    }

    /// Take the entire slot contents. Returns None if empty.
    pub fn take_all(&mut self, slot_idx: usize) -> Option<InventorySlot> {
        let slot = self.slots.get_mut(slot_idx)?;
        slot.take()
    }

    /// Total weight — resolves each occupied instance through the registry.
    pub fn total_weight_kg(&self, registry: &ContentRegistry) -> f32 {
        self.iter()
            .filter_map(|(_, slot)| {
                registry
                    .resolve(&slot.instance)
                    .ok()
                    .map(|r| r.weight_kg * slot.count as f32)
            })
            .sum()
    }

    /// Find the first slot whose instance matches `template` (all four
    /// identity fields: base_id, material_id, quality_id, affixes).
    /// Used by the consumable belt to locate a bound potion's stack
    /// when the hotkey fires.
    pub fn find_matching(&self, template: &ItemInstance) -> Option<usize> {
        self.slots.iter().enumerate().find_map(|(i, s)| {
            s.as_ref()
                .filter(|slot| instance_matches(&slot.instance, template))
                .map(|_| i)
        })
    }

    /// Total count across every slot whose instance matches `template`.
    /// Belt UI shows this as the "N potions remaining" badge.
    pub fn total_matching(&self, template: &ItemInstance) -> u32 {
        self.iter()
            .filter_map(|(_, slot)| {
                if instance_matches(&slot.instance, template) {
                    Some(slot.count)
                } else {
                    None
                }
            })
            .sum()
    }

    /// Decrement one unit from the first matching stack. Returns true
    /// on success, false if no matching stack exists.
    pub fn consume_matching(&mut self, template: &ItemInstance) -> bool {
        match self.find_matching(template) {
            Some(idx) => self.take(idx, 1).is_some(),
            None => false,
        }
    }
}

/// Instance-equality by the same 4-tuple StackKey uses. Two instances
/// match iff every identity field matches.
pub fn instance_matches(a: &ItemInstance, b: &ItemInstance) -> bool {
    a.base_id == b.base_id
        && a.material_id == b.material_id
        && a.quality_id == b.quality_id
        && a.affixes == b.affixes
}

/// Consumable belt — 4 hotkey-bound potion slots. Stores the
/// `ItemInstance` template only (binding survives stack rearrangement
/// in the bag); server searches the inventory for a matching stack at
/// activation time. `None` = unbound.
#[derive(Component, Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConsumableBelt {
    pub slots: [Option<ItemInstance>; BELT_SLOTS],
}

/// Number of hotkey-bound consumable slots. Matches client keys 7/8/9/0.
pub const BELT_SLOTS: usize = 4;

impl ConsumableBelt {
    pub fn bind(&mut self, slot_idx: usize, instance: ItemInstance) -> bool {
        if slot_idx >= BELT_SLOTS {
            return false;
        }
        self.slots[slot_idx] = Some(instance);
        true
    }

    pub fn clear(&mut self, slot_idx: usize) -> bool {
        if slot_idx >= BELT_SLOTS {
            return false;
        }
        let had = self.slots[slot_idx].is_some();
        self.slots[slot_idx] = None;
        had
    }

    pub fn get(&self, slot_idx: usize) -> Option<&ItemInstance> {
        self.slots.get(slot_idx).and_then(Option::as_ref)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vaern_items::{
        ArmorType, BaseKind, ItemBase, Material, Quality, Rarity, SizeClass, WeaponGrip,
    };

    fn weapon_base() -> ItemBase {
        ItemBase {
            id: "sword".into(),
            piece_name: "Sword".into(),
            size: SizeClass::Medium,
            base_weight_kg: 2.0,
            volume_l: None,
            stackable: false,
            stack_max: 1,
            no_vendor: false,
            soulbound: false,
            vendor_base_price: None,
            kind: BaseKind::Weapon {
                grip: WeaponGrip::OneHanded,
                school: "blade".into(),
                base_min_dmg: 4.0,
                base_max_dmg: 8.0,
            },
        }
    }

    fn potion_base() -> ItemBase {
        ItemBase {
            id: "healing_potion".into(),
            piece_name: "Healing Potion".into(),
            size: SizeClass::Tiny,
            base_weight_kg: 0.3,
            volume_l: None,
            stackable: true,
            stack_max: 20,
            no_vendor: false,
            soulbound: false,
            vendor_base_price: None,
            kind: BaseKind::Consumable {
                charges: 1,
                effect: vaern_items::ConsumeEffect::None,
            },
        }
    }

    fn test_registry() -> ContentRegistry {
        let mut r = ContentRegistry::new();
        r.insert_base(weapon_base());
        r.insert_base(potion_base());
        r.insert_material(Material {
            id: "iron".into(),
            display: "Iron".into(),
            tier: 3,
            weight_mult: 1.0,
            ac_mult: 1.0,
            dmg_mult: 1.0,
            resist_adds: [0.0; 12],
            valid_for: vec![ArmorType::Mail, ArmorType::Plate],
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

    fn sword_inst() -> ItemInstance {
        ItemInstance::new("sword", "iron", "regular")
    }

    fn potion_inst() -> ItemInstance {
        ItemInstance::materialless("healing_potion", "regular")
    }

    #[test]
    fn empty_inventory_reports_capacity_and_empty() {
        let inv = PlayerInventory::with_capacity(5);
        assert_eq!(inv.capacity(), 5);
        assert_eq!(inv.len(), 0);
        assert!(inv.is_empty());
        assert!(!inv.is_full());
    }

    #[test]
    fn non_stackable_items_occupy_distinct_slots() {
        let reg = test_registry();
        let mut inv = PlayerInventory::with_capacity(3);
        assert_eq!(inv.add(sword_inst(), 1, &reg), 0);
        assert_eq!(inv.add(sword_inst(), 1, &reg), 0);
        // Each sword takes its own slot (stack_max=1).
        assert_eq!(inv.len(), 2);
        assert!(inv.get(0).is_some());
        assert!(inv.get(1).is_some());
        assert_eq!(inv.get(0).unwrap().count, 1);
    }

    #[test]
    fn stackable_items_merge_up_to_stack_max() {
        let reg = test_registry();
        let mut inv = PlayerInventory::with_capacity(3);
        // Adding 15 potions fits in one stack (max 20).
        assert_eq!(inv.add(potion_inst(), 15, &reg), 0);
        assert_eq!(inv.len(), 1);
        assert_eq!(inv.get(0).unwrap().count, 15);
        // Adding 10 more: 5 completes first stack to 20, remaining 5 spill into slot 1.
        assert_eq!(inv.add(potion_inst(), 10, &reg), 0);
        assert_eq!(inv.len(), 2);
        assert_eq!(inv.get(0).unwrap().count, 20);
        assert_eq!(inv.get(1).unwrap().count, 5);
    }

    #[test]
    fn add_returns_leftover_when_inventory_full() {
        let reg = test_registry();
        let mut inv = PlayerInventory::with_capacity(1);
        // First sword fits.
        assert_eq!(inv.add(sword_inst(), 1, &reg), 0);
        // Second has nowhere to go (non-stackable, 1 slot).
        assert_eq!(inv.add(sword_inst(), 1, &reg), 1);
        assert!(inv.is_full());
    }

    #[test]
    fn take_removes_partial_and_full_counts() {
        let reg = test_registry();
        let mut inv = PlayerInventory::with_capacity(3);
        inv.add(potion_inst(), 20, &reg);
        // Take 5 — slot retains 15.
        let (inst, n) = inv.take(0, 5).unwrap();
        assert_eq!(n, 5);
        assert_eq!(inst.base_id, "healing_potion");
        assert_eq!(inv.get(0).unwrap().count, 15);
        // Take more than remaining — clamp.
        let (_, n) = inv.take(0, 99).unwrap();
        assert_eq!(n, 15);
        assert!(inv.get(0).is_none());
    }

    #[test]
    fn take_all_clears_slot_returns_contents() {
        let reg = test_registry();
        let mut inv = PlayerInventory::with_capacity(3);
        inv.add(sword_inst(), 1, &reg);
        let slot = inv.take_all(0).unwrap();
        assert_eq!(slot.count, 1);
        assert_eq!(slot.instance.base_id, "sword");
        assert!(inv.get(0).is_none());
    }

    #[test]
    fn unknown_base_returns_full_count_leftover() {
        let reg = test_registry();
        let mut inv = PlayerInventory::with_capacity(3);
        let phantom = ItemInstance::new("nonexistent", "iron", "regular");
        assert_eq!(inv.add(phantom, 5, &reg), 5);
        assert!(inv.is_empty());
    }

    #[test]
    fn different_qualities_do_not_stack() {
        let reg = test_registry();
        let mut inv = PlayerInventory::with_capacity(5);
        inv.add(potion_inst(), 5, &reg);
        // Different quality = different stack key.
        // (Only one quality defined in test registry; simulate by
        // changing the tuple directly.)
        let mut q2 = ItemInstance::materialless("healing_potion", "regular");
        q2.affixes = vec!["enchanted".into()]; // affix differs
        inv.add(q2, 5, &reg);
        assert_eq!(inv.len(), 2);
    }

    #[test]
    fn total_weight_sums_across_stacks_with_counts() {
        let reg = test_registry();
        let mut inv = PlayerInventory::with_capacity(3);
        inv.add(sword_inst(), 1, &reg);   // 1 × 2.0 kg = 2.0
        inv.add(potion_inst(), 10, &reg); // 10 × 0.3 kg = 3.0
        // Total: 5.0 kg.
        assert!((inv.total_weight_kg(&reg) - 5.0).abs() < 1e-5);
    }

    #[test]
    fn find_matching_finds_by_full_identity_tuple() {
        let reg = test_registry();
        let mut inv = PlayerInventory::with_capacity(5);
        inv.add(potion_inst(), 5, &reg);
        assert_eq!(inv.find_matching(&potion_inst()), Some(0));
        // A differently-affixed instance is a different identity.
        let mut other = potion_inst();
        other.affixes = vec!["enchanted".into()];
        assert_eq!(inv.find_matching(&other), None);
    }

    #[test]
    fn total_matching_sums_counts_across_all_slots() {
        let reg = test_registry();
        let mut inv = PlayerInventory::with_capacity(5);
        // Two stacks of 20 each (stack_max=20 on potion_base).
        inv.add(potion_inst(), 25, &reg);
        assert_eq!(inv.total_matching(&potion_inst()), 25);
    }

    #[test]
    fn consume_matching_decrements_by_one() {
        let reg = test_registry();
        let mut inv = PlayerInventory::with_capacity(5);
        inv.add(potion_inst(), 3, &reg);
        assert!(inv.consume_matching(&potion_inst()));
        assert_eq!(inv.total_matching(&potion_inst()), 2);
    }

    #[test]
    fn consume_matching_returns_false_when_none_in_inventory() {
        let reg = test_registry();
        let mut inv = PlayerInventory::with_capacity(5);
        inv.add(sword_inst(), 1, &reg);
        assert!(!inv.consume_matching(&potion_inst()));
        // Sword stays put.
        assert_eq!(inv.len(), 1);
    }

    #[test]
    fn consume_matching_removes_empty_slot_on_last_charge() {
        let reg = test_registry();
        let mut inv = PlayerInventory::with_capacity(5);
        inv.add(potion_inst(), 1, &reg);
        assert!(inv.consume_matching(&potion_inst()));
        assert!(inv.is_empty());
    }

    #[test]
    fn belt_bind_clear_get_roundtrip() {
        let mut belt = ConsumableBelt::default();
        assert!(belt.bind(0, potion_inst()));
        assert_eq!(belt.get(0).map(|i| i.base_id.as_str()), Some("healing_potion"));
        assert!(belt.clear(0));
        assert!(belt.get(0).is_none());
        // Clearing an already-empty slot returns false.
        assert!(!belt.clear(0));
        // Out-of-range indices refuse both bind and clear.
        assert!(!belt.bind(BELT_SLOTS, potion_inst()));
        assert!(!belt.clear(BELT_SLOTS));
    }

    #[test]
    fn belt_rebind_overwrites_previous_template() {
        let mut belt = ConsumableBelt::default();
        belt.bind(1, potion_inst());
        let mut other = potion_inst();
        other.affixes = vec!["enchanted".into()];
        belt.bind(1, other.clone());
        assert_eq!(belt.get(1), Some(&other));
    }
}
