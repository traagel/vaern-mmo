//! Drop items that no longer resolve against the content registry.
//!
//! The item model references bases / materials / qualities / affixes by
//! string id. If the `src/generated/items/` YAML tables get re-seeded
//! with renamed ids, old saved inventories still point at the defunct
//! strings. Rather than hard-fail the load, the server walks the
//! freshly-loaded state through `ContentRegistry::resolve` at rehydration
//! time and drops anything that doesn't resolve. The list of dropped
//! ids flows back to the caller for logging.
//!
//! Saves keep flowing (resolved items survive; unresolved stacks are
//! vaporized). A future "dead-item amnesty" PR can vendor-refund or
//! park the dropped items in a graveyard; for the close-friend household
//! target the warning + drop is enough.

use vaern_equipment::Equipped;
use vaern_inventory::{ConsumableBelt, PlayerInventory};
use vaern_items::ContentRegistry;

/// Ids of items that were dropped during a sanitization pass. The
/// `source` is a short tag ("inventory" / "equipped" / "belt") so the
/// caller can log which collection each drop came from.
#[derive(Debug, Clone)]
pub struct DroppedItem {
    pub source: &'static str,
    pub base_id: String,
}

/// Walk every stored `ItemInstance` through `registry.resolve()` and
/// drop those that error. Returns the list of dropped ids.
pub fn sanitize_loadout(
    inventory: &mut PlayerInventory,
    equipped: &mut Equipped,
    belt: &mut ConsumableBelt,
    registry: &ContentRegistry,
) -> Vec<DroppedItem> {
    let mut dropped = Vec::new();

    // Inventory: collect bad slot indices first (can't mutate while iterating).
    let bad_inv: Vec<usize> = inventory
        .iter()
        .filter_map(|(i, slot)| {
            if registry.resolve(&slot.instance).is_err() {
                dropped.push(DroppedItem {
                    source: "inventory",
                    base_id: slot.instance.base_id.clone(),
                });
                Some(i)
            } else {
                None
            }
        })
        .collect();
    for idx in bad_inv {
        let _ = inventory.take_all(idx);
    }

    // Equipped: same pattern.
    let bad_eq: Vec<vaern_equipment::EquipSlot> = equipped
        .iter()
        .filter_map(|(slot, instance)| {
            if registry.resolve(instance).is_err() {
                dropped.push(DroppedItem {
                    source: "equipped",
                    base_id: instance.base_id.clone(),
                });
                Some(slot)
            } else {
                None
            }
        })
        .collect();
    for slot in bad_eq {
        let _ = equipped.unequip(slot);
    }

    // Belt: overwrite unresolvable bindings with None. Belt stores a
    // template, not an inventory index, so this doesn't touch the bag.
    for i in 0..belt.slots.len() {
        if let Some(instance) = &belt.slots[i] {
            if registry.resolve(instance).is_err() {
                dropped.push(DroppedItem {
                    source: "belt",
                    base_id: instance.base_id.clone(),
                });
                belt.slots[i] = None;
            }
        }
    }

    dropped
}
