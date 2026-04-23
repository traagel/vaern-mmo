//! First-spawn starter kits, keyed by the character's core pillar.
//!
//! Characters commit to a pillar at creation and evolve into an archetype /
//! Order through play — so starter gear is pillar-themed (melee, caster,
//! skirmisher) rather than class-specific. Kit composition is intentionally
//! minimal; per-race flavoring and Order-tier sets come later.

use vaern_core::pillar::Pillar;
use vaern_inventory::PlayerInventory;
use vaern_items::{ContentRegistry, ItemInstance};

/// One entry: `(base_id, material, quality, count)`. `material=None` for
/// materialless bases (consumables, runes).
type KitEntry = (&'static str, Option<&'static str>, &'static str, u32);

/// Might starter — melee/tank shape. Sword + buckler to exercise Block,
/// gambeson body + legs + cap so the frontal-block damage reduction math
/// has real armor behind it. Heavier stamina pots because Block/Parry
/// drain stamina; fewer mana pots (Might kits are nearly mana-free).
const MIGHT_KIT: &[KitEntry] = &[
    ("sword",                 Some("iron"),           "regular", 1),
    ("buckler",               Some("bronze"),         "regular", 1),
    ("gambeson_gambeson",     Some("linen_padding"),  "regular", 1),
    ("gambeson_arming_cap",   Some("linen_padding"),  "regular", 1),
    ("gambeson_breeches",     Some("linen_padding"),  "regular", 1),
    ("minor_healing_potion",  None,                   "regular", 5),
    ("minor_stamina_potion",  None,                   "regular", 5),
    ("minor_mana_potion",     None,                   "regular", 2),
];

/// Arcana starter — caster / magic-tank shape. Wand for a cast-time
/// baseline, a fire rune in the Focus slot so the ward-layer mitigation
/// has a real channel, cloth robe + under layer + trousers. Mana-heavy
/// potion mix because Arcana spells cost mana.
const ARCANA_KIT: &[KitEntry] = &[
    ("wand",                  Some("iron"),  "regular", 1),
    ("rune_of_fire",          None,          "regular", 1),
    ("cloth_robe",            Some("linen"), "regular", 1),
    ("cloth_shirt",           Some("linen"), "regular", 1),
    ("cloth_trousers",        Some("linen"), "regular", 1),
    ("cloth_cowl",            Some("linen"), "regular", 1),
    ("minor_healing_potion",  None,          "regular", 3),
    ("minor_mana_potion",     None,          "regular", 5),
    ("minor_stamina_potion",  None,          "regular", 2),
];

/// Finesse starter — duelist / skirmisher shape. Dagger (parry-capable,
/// no shield) + shortbow for ranged option, leather set for balanced
/// mitigation. Balanced potion mix.
const FINESSE_KIT: &[KitEntry] = &[
    ("dagger",                Some("iron"),     "regular", 1),
    ("shortbow",              Some("iron"),     "regular", 1),
    ("leather_jerkin",        Some("boarhide"), "regular", 1),
    ("leather_hood",          Some("boarhide"), "regular", 1),
    ("leather_leggings",      Some("boarhide"), "regular", 1),
    ("minor_healing_potion",  None,             "regular", 4),
    ("minor_stamina_potion",  None,             "regular", 4),
    ("minor_mana_potion",     None,             "regular", 2),
];

fn kit_for(pillar: Pillar) -> &'static [KitEntry] {
    match pillar {
        Pillar::Might => MIGHT_KIT,
        Pillar::Arcana => ARCANA_KIT,
        Pillar::Finesse => FINESSE_KIT,
    }
}

/// Build a default-capacity `PlayerInventory` pre-loaded with the
/// pillar's starter kit.
pub fn build_starter_inventory_for_pillar(
    pillar: Pillar,
    registry: &ContentRegistry,
) -> PlayerInventory {
    let mut inv = PlayerInventory::default();
    grant(&mut inv, pillar, registry);
    inv
}

/// Push a pillar's starter kit into an existing inventory. Logs any
/// failures without aborting — preserves whatever of the kit succeeded.
pub fn grant(inv: &mut PlayerInventory, pillar: Pillar, registry: &ContentRegistry) {
    for (base_id, material, quality_id, count) in kit_for(pillar) {
        let instance = match material {
            Some(m) => ItemInstance::new(*base_id, *m, *quality_id),
            None => ItemInstance::materialless(*base_id, *quality_id),
        };
        if let Err(e) = registry.resolve(&instance) {
            println!(
                "[starter-gear] skipping {base_id} ({material:?}/{quality_id}): {e}"
            );
            continue;
        }
        let leftover = inv.add(instance, *count, registry);
        if leftover > 0 {
            println!(
                "[starter-gear] inventory full, {leftover} of {base_id} didn't fit"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn registry() -> ContentRegistry {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let root = manifest.join("../../src/generated/items").canonicalize().unwrap();
        let mut reg = ContentRegistry::new();
        reg.load_tree(&root).unwrap();
        reg
    }

    fn kit_base_ids(pillar: Pillar) -> Vec<String> {
        let reg = registry();
        let inv = build_starter_inventory_for_pillar(pillar, &reg);
        inv.iter()
            .map(|(_, slot)| slot.instance.base_id.clone())
            .collect()
    }

    fn all_kit_entries_resolve(pillar: Pillar) {
        let reg = registry();
        let inv = build_starter_inventory_for_pillar(pillar, &reg);
        assert!(inv.iter().count() > 0, "pillar {pillar:?} starter inventory empty");
        for (_, slot) in inv.iter() {
            reg.resolve(&slot.instance).unwrap_or_else(|e| {
                panic!(
                    "pillar {pillar:?} item {:?} failed to resolve: {e}",
                    slot.instance
                )
            });
        }
    }

    #[test]
    fn might_starter_resolves() {
        all_kit_entries_resolve(Pillar::Might);
    }

    #[test]
    fn arcana_starter_resolves() {
        all_kit_entries_resolve(Pillar::Arcana);
    }

    #[test]
    fn finesse_starter_resolves() {
        all_kit_entries_resolve(Pillar::Finesse);
    }

    #[test]
    fn might_kit_contains_shield_and_sword() {
        let ids = kit_base_ids(Pillar::Might);
        assert!(ids.iter().any(|i| i == "sword"), "Might kit missing sword: {ids:?}");
        assert!(ids.iter().any(|i| i == "buckler"), "Might kit missing buckler: {ids:?}");
    }

    #[test]
    fn arcana_kit_contains_wand_and_rune() {
        let ids = kit_base_ids(Pillar::Arcana);
        assert!(ids.iter().any(|i| i == "wand"), "Arcana kit missing wand: {ids:?}");
        assert!(
            ids.iter().any(|i| i == "rune_of_fire"),
            "Arcana kit missing rune: {ids:?}"
        );
    }

    #[test]
    fn finesse_kit_contains_dagger_and_bow() {
        let ids = kit_base_ids(Pillar::Finesse);
        assert!(ids.iter().any(|i| i == "dagger"), "Finesse missing dagger: {ids:?}");
        assert!(ids.iter().any(|i| i == "shortbow"), "Finesse missing shortbow: {ids:?}");
    }
}
