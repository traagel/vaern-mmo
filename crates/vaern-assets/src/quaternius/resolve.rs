//! Map player `Equipped` state → [`QuaterniusOutfit`] visual.
//!
//! Per-slot rule: if a primary armor slot is empty, render Peasant at
//! [`ColorVariant::Default`] ("naked" identity, undyed rags). If it's
//! occupied, render the armor-type's outfit family at a non-Default
//! color variant so naked and equipped are always visually distinct.
//!
//! Secondary slots (wrists / waist / back / shirt / shoulders) are
//! ignored here — they either collapse into the parent mesh (wrists
//! into Arms), have no dedicated Quaternius mesh (waist / back / shirt),
//! or are pending accessory-slot API (shoulders). The resolver only
//! maps the five primary slots the Quaternius per-slot composer
//! supports: chest, legs, hands, feet, head.
//!
//! Mapping table (unoccupied = Peasant BaseColor "naked"):
//!
//! | Armor type | Body           | Legs / Arms / Feet   | Head        |
//! |------------|----------------|----------------------|-------------|
//! | Cloth      | Wizard V2      | Wizard V2            | RangerHood V2 |
//! | Gambeson   | Peasant V2     | Peasant V2           | *(no mesh)*   |
//! | Leather    | Ranger V2      | Ranger V2            | RangerHood 1 |
//! | Mail       | KnightCloth V3 | Knight V3            | KnightArmet V3 |
//! | Plate      | Knight V2      | Knight V2            | KnightArmet V2 |
//!
//! The Peasant BaseColor palette is reserved for "naked" — no equipped
//! armor family ever uses `(Peasant, Default)`, so the naked silhouette
//! stays unambiguous.

use std::collections::HashMap;

use vaern_equipment::EquipSlot;
use vaern_items::{ArmorType, ContentRegistry, ItemInstance, ItemKind};

use super::character::{
    ColorVariant, HeadPiece, HeadSlot, Outfit, OutfitSlot, QuaterniusOutfit,
};

/// Peasant BaseColor — the reserved "naked slot" identity. Used for any
/// primary slot whose armor is unequipped. No equipped armor family
/// ever maps here.
const NAKED: OutfitSlot = OutfitSlot::new(Outfit::Peasant, ColorVariant::Default);

/// Derive the visual Quaternius outfit from a player's equipped slot
/// map. Accepts any `&HashMap<EquipSlot, ItemInstance>` so both the
/// server-side `Equipped` component (via [`vaern_equipment::Equipped::slots`])
/// and the client's `OwnEquipped` resource can pass through without a
/// wrapper type.
///
/// Fills body / legs / arms / feet / head_piece. Leaves hair + beard
/// unset — callers should set them from the character's appearance
/// picks after calling this.
pub fn outfit_from_equipped(
    slots: &HashMap<EquipSlot, ItemInstance>,
    registry: &ContentRegistry,
) -> QuaterniusOutfit {
    let chest = armor_type_at(slots, registry, EquipSlot::Chest);
    let legs = armor_type_at(slots, registry, EquipSlot::Legs);
    let hands = armor_type_at(slots, registry, EquipSlot::Hands);
    let feet = armor_type_at(slots, registry, EquipSlot::Feet);
    let head = armor_type_at(slots, registry, EquipSlot::Head);

    QuaterniusOutfit {
        body: Some(body_slot(chest)),
        legs: Some(legs_slot(legs)),
        arms: Some(arms_slot(hands)),
        feet: Some(feet_slot(feet)),
        head_piece: head_slot(head),
        hair: None,
        beard: None,
    }
}

fn armor_type_at(
    slots: &HashMap<EquipSlot, ItemInstance>,
    registry: &ContentRegistry,
    slot: EquipSlot,
) -> Option<ArmorType> {
    let inst = slots.get(&slot)?;
    let resolved = registry.resolve(inst).ok()?;
    match resolved.kind {
        ItemKind::Armor { armor_type, .. } => Some(armor_type),
        _ => None,
    }
}

fn body_slot(at: Option<ArmorType>) -> OutfitSlot {
    match at {
        None => NAKED,
        Some(ArmorType::Cloth) => OutfitSlot::new(Outfit::Wizard, ColorVariant::V2),
        Some(ArmorType::Gambeson) => OutfitSlot::new(Outfit::Peasant, ColorVariant::V2),
        Some(ArmorType::Leather) => OutfitSlot::new(Outfit::Ranger, ColorVariant::V2),
        Some(ArmorType::Mail) => OutfitSlot::new(Outfit::KnightCloth, ColorVariant::V3),
        Some(ArmorType::Plate) => OutfitSlot::new(Outfit::Knight, ColorVariant::V2),
    }
}

fn legs_slot(at: Option<ArmorType>) -> OutfitSlot {
    match at {
        None => NAKED,
        Some(ArmorType::Cloth) => OutfitSlot::new(Outfit::Wizard, ColorVariant::V2),
        Some(ArmorType::Gambeson) => OutfitSlot::new(Outfit::Peasant, ColorVariant::V2),
        Some(ArmorType::Leather) => OutfitSlot::new(Outfit::Ranger, ColorVariant::V2),
        Some(ArmorType::Mail) => OutfitSlot::new(Outfit::Knight, ColorVariant::V3),
        Some(ArmorType::Plate) => OutfitSlot::new(Outfit::Knight, ColorVariant::V2),
    }
}

fn arms_slot(at: Option<ArmorType>) -> OutfitSlot {
    match at {
        None => NAKED,
        Some(ArmorType::Cloth) => OutfitSlot::new(Outfit::Wizard, ColorVariant::V2),
        Some(ArmorType::Gambeson) => OutfitSlot::new(Outfit::Peasant, ColorVariant::V2),
        Some(ArmorType::Leather) => OutfitSlot::new(Outfit::Ranger, ColorVariant::V2),
        Some(ArmorType::Mail) => OutfitSlot::new(Outfit::Knight, ColorVariant::V3),
        Some(ArmorType::Plate) => OutfitSlot::new(Outfit::Knight, ColorVariant::V2),
    }
}

fn feet_slot(at: Option<ArmorType>) -> OutfitSlot {
    match at {
        None => NAKED,
        Some(ArmorType::Cloth) => OutfitSlot::new(Outfit::Wizard, ColorVariant::V2),
        Some(ArmorType::Gambeson) => OutfitSlot::new(Outfit::Peasant, ColorVariant::V2),
        Some(ArmorType::Leather) => OutfitSlot::new(Outfit::Ranger, ColorVariant::V2),
        Some(ArmorType::Mail) => OutfitSlot::new(Outfit::Knight, ColorVariant::V3),
        Some(ArmorType::Plate) => OutfitSlot::new(Outfit::Knight, ColorVariant::V2),
    }
}

fn head_slot(at: Option<ArmorType>) -> Option<HeadSlot> {
    Some(match at? {
        ArmorType::Cloth => HeadSlot::new(HeadPiece::RangerHood, ColorVariant::V2),
        // No padded-cap mesh ships in the Quaternius pack.
        ArmorType::Gambeson => return None,
        ArmorType::Leather => HeadSlot::new(HeadPiece::RangerHood, ColorVariant::Default),
        ArmorType::Mail => HeadSlot::new(HeadPiece::KnightArmet, ColorVariant::V3),
        ArmorType::Plate => HeadSlot::new(HeadPiece::KnightArmet, ColorVariant::V2),
    })
}

/// MEGAKIT prop basenames resolved for a player's equipped state.
/// `None` on either slot = render empty-handed for that side.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EquippedProps {
    pub mainhand: Option<String>,
    pub offhand: Option<String>,
}

/// Derive the visible MEGAKIT prop basenames (Sword_Bronze, etc) for
/// each hand from a player's equipped state. MEGAKIT only ships 5
/// weapon meshes, so the mapping is lossy: every blade reads as a
/// sword, every blunt weapon as an axe. When MEGAKIT grows we can
/// replace this with a richer school → prop table or add a prop_id
/// to each weapon's base YAML.
///
/// Mainhand:
/// - `ItemKind::Weapon { school: "blade" | .. }` → `Sword_Bronze`
/// - `ItemKind::Weapon { school: "blunt" }`       → `Axe_Bronze`
/// - `ItemKind::Weapon { school: "dagger" }`      → `Table_Knife`
/// - anything else                                 → `None`
///
/// Offhand:
/// - `ItemKind::Shield { .. }`                     → `Shield_Wooden`
/// - `ItemKind::Weapon { school: "dagger" }`       → `Table_Knife` (dual-wield)
/// - anything else                                 → `None`
pub fn weapon_props_from_equipped(
    slots: &HashMap<EquipSlot, ItemInstance>,
    registry: &ContentRegistry,
) -> EquippedProps {
    let mainhand = slots
        .get(&EquipSlot::MainHand)
        .and_then(|inst| registry.resolve(inst).ok())
        .and_then(|r| weapon_prop_from_kind(&r.kind, /*offhand=*/ false));
    let offhand = slots
        .get(&EquipSlot::OffHand)
        .and_then(|inst| registry.resolve(inst).ok())
        .and_then(|r| weapon_prop_from_kind(&r.kind, /*offhand=*/ true));
    EquippedProps { mainhand, offhand }
}

fn weapon_prop_from_kind(kind: &ItemKind, offhand: bool) -> Option<String> {
    match kind {
        ItemKind::Weapon { school, .. } => Some(match school.as_str() {
            "dagger" => "Table_Knife",
            "blunt" => "Axe_Bronze",
            // Everything edged / one-handed / unrecognized reads as a
            // sword in MEGAKIT (the pack ships no bow/staff/wand
            // meshes). Catches blade, spear, claw, fang, etc.
            _ => "Sword_Bronze",
        })
        .map(str::to_string),
        ItemKind::Shield { .. } if offhand => Some("Shield_Wooden".to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vaern_equipment::Equipped;
    use vaern_items::{ArmorLayer, BaseKind, ItemBase, Material, Quality, Rarity, SizeClass};

    fn test_registry() -> ContentRegistry {
        let mut r = ContentRegistry::new();
        // One base per armor type × "chest" slot — enough to drive
        // the resolver's body-slot branches.
        for (id, at) in [
            ("cloth_chest", ArmorType::Cloth),
            ("gambeson_chest", ArmorType::Gambeson),
            ("leather_chest", ArmorType::Leather),
            ("mail_chest", ArmorType::Mail),
            ("plate_chest", ArmorType::Plate),
        ] {
            r.insert_base(ItemBase {
                id: id.into(),
                piece_name: id.into(),
                size: SizeClass::Medium,
                base_weight_kg: 1.0,
                volume_l: None,
                stackable: false,
                stack_max: 1,
                no_vendor: false,
                soulbound: false,
                vendor_base_price: None,
                kind: BaseKind::Armor {
                    slot: "chest".into(),
                    armor_type: at,
                    layer: ArmorLayer::Under,
                    coverage: vec![],
                    base_armor_class: 1.0,
                },
            });
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
            weapon_eligible: false,
            shield_eligible: false,
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

    fn inst(base: &str) -> ItemInstance {
        ItemInstance::new(base, "testmetal", "regular")
    }

    #[test]
    fn naked_renders_all_peasant_base_color() {
        let reg = test_registry();
        let eq = Equipped::new();
        let outfit = outfit_from_equipped(eq.slots(), &reg);
        assert_eq!(outfit.body, Some(NAKED));
        assert_eq!(outfit.legs, Some(NAKED));
        assert_eq!(outfit.arms, Some(NAKED));
        assert_eq!(outfit.feet, Some(NAKED));
        assert_eq!(outfit.head_piece, None);
    }

    #[test]
    fn plate_chest_swaps_body_only() {
        let reg = test_registry();
        let mut eq = Equipped::new();
        eq.equip(EquipSlot::Chest, inst("plate_chest"), &reg).unwrap();
        let outfit = outfit_from_equipped(eq.slots(), &reg);
        assert_eq!(
            outfit.body,
            Some(OutfitSlot::new(Outfit::Knight, ColorVariant::V2))
        );
        // Other slots stay naked — each renders independently.
        assert_eq!(outfit.legs, Some(NAKED));
        assert_eq!(outfit.arms, Some(NAKED));
        assert_eq!(outfit.feet, Some(NAKED));
    }

    #[test]
    fn mail_and_plate_bodies_differ() {
        let reg = test_registry();
        let mut eq_mail = Equipped::new();
        eq_mail.equip(EquipSlot::Chest, inst("mail_chest"), &reg).unwrap();
        let mail = outfit_from_equipped(eq_mail.slots(), &reg);

        let mut eq_plate = Equipped::new();
        eq_plate.equip(EquipSlot::Chest, inst("plate_chest"), &reg).unwrap();
        let plate = outfit_from_equipped(eq_plate.slots(), &reg);

        // Different body meshes (KnightCloth vs Knight) AND different
        // colors (V3 vs V2) — so they read as distinct armor tiers.
        assert_eq!(
            mail.body,
            Some(OutfitSlot::new(Outfit::KnightCloth, ColorVariant::V3))
        );
        assert_eq!(
            plate.body,
            Some(OutfitSlot::new(Outfit::Knight, ColorVariant::V2))
        );
    }

    #[test]
    fn cloth_chest_swaps_to_wizard() {
        let reg = test_registry();
        let mut eq = Equipped::new();
        eq.equip(EquipSlot::Chest, inst("cloth_chest"), &reg).unwrap();
        let outfit = outfit_from_equipped(eq.slots(), &reg);
        assert_eq!(
            outfit.body,
            Some(OutfitSlot::new(Outfit::Wizard, ColorVariant::V2))
        );
    }

    #[test]
    fn gambeson_chest_swaps_to_peasant_v2_not_naked() {
        // Gambeson renders on the Peasant family but must NOT use
        // BaseColor — naked identity would be ambiguous otherwise.
        let reg = test_registry();
        let mut eq = Equipped::new();
        eq.equip(EquipSlot::Chest, inst("gambeson_chest"), &reg).unwrap();
        let outfit = outfit_from_equipped(eq.slots(), &reg);
        assert_eq!(
            outfit.body,
            Some(OutfitSlot::new(Outfit::Peasant, ColorVariant::V2))
        );
        assert_ne!(outfit.body, Some(NAKED));
    }

    #[test]
    fn leather_chest_swaps_to_ranger() {
        let reg = test_registry();
        let mut eq = Equipped::new();
        eq.equip(EquipSlot::Chest, inst("leather_chest"), &reg).unwrap();
        let outfit = outfit_from_equipped(eq.slots(), &reg);
        assert_eq!(
            outfit.body,
            Some(OutfitSlot::new(Outfit::Ranger, ColorVariant::V2))
        );
    }

    #[test]
    fn no_equipped_armor_family_shares_the_naked_palette() {
        // Invariant: Peasant BaseColor is reserved. Every equipped
        // armor-type must map to a distinct (outfit, color) tuple.
        for at in [
            ArmorType::Cloth,
            ArmorType::Gambeson,
            ArmorType::Leather,
            ArmorType::Mail,
            ArmorType::Plate,
        ] {
            assert_ne!(body_slot(Some(at)), NAKED);
            assert_ne!(legs_slot(Some(at)), NAKED);
            assert_ne!(arms_slot(Some(at)), NAKED);
            assert_ne!(feet_slot(Some(at)), NAKED);
        }
    }
}
