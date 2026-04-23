//! Vendor pricing, player-market floors, and gold-sink bookkeeping.
//!
//! Vaern runs a hybrid economy (see design notes): NPC vendors anchor
//! currency value with formula-priced commodities, and players list
//! crafted / named / rare gear on a market above the vendor floor. This
//! crate owns the math; systems that mutate wallets or spawn listings
//! live in the server.
//!
//! The faucet/drain loop needs a fixed buy/sell spread on every NPC
//! transaction — that spread IS the primary per-transaction gold sink.
//! Do not set `buy_spread` to 0; pin it, tune it, record it.
//!
//! Session 5 note: pricing operates on `ResolvedItem` (the composed view
//! of an `ItemInstance`), not the retired flat `Item` struct. Callers
//! compose the instance, resolve via `ContentRegistry`, then price.

use serde::{Deserialize, Serialize};
use vaern_items::ResolvedItem;

/// Global tuning knobs for NPC vendor pricing. Loaded once at server
/// start (defaults are production-safe) and exposed as a Bevy `Resource`
/// once vaern-server wires it in. Not `Resource`-derived here so the
/// crate stays ECS-agnostic for sim use.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct VendorPricing {
    /// Fraction of buy price kept by the vendor on resale. 0.6 = player
    /// sells back for 40% of sticker price (classic MMO ratio, also the
    /// primary per-transaction gold sink).
    pub buy_spread: f32,
    /// Multiplier applied to the base price when the item has
    /// `no_vendor: true` and we still need a reference number (e.g. for
    /// insurance, destruction reimbursement). NPC vendors themselves
    /// reject `no_vendor` items.
    pub no_vendor_reference_mult: f32,
    /// Floor markup over vendor buy price for player-market listings.
    /// 1.1 = market minimum is 110% of vendor buy. Prevents arbitrage
    /// where a player relists a vendor item for less than the vendor.
    pub market_floor_mult: f32,
}

impl Default for VendorPricing {
    fn default() -> Self {
        Self {
            buy_spread: 0.6,
            no_vendor_reference_mult: 1.0,
            market_floor_mult: 1.1,
        }
    }
}

/// Condition / live-instance modifier applied on top of `ResolvedItem.base_price`.
/// Condition degrades with use (repaired → pristine → worn → broken),
/// enhancement captures runtime-applied temporary buffs / enchant tiers
/// not baked into the base price.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct QualityMod {
    /// 1.0 = pristine; 0.25 = nearly broken. Linear multiplier over price.
    pub condition: f32,
    /// Flat multiplier for enchant / runtime enhancement. 1.0 default.
    pub enhancement: f32,
}

impl Default for QualityMod {
    fn default() -> Self {
        Self {
            condition: 1.0,
            enhancement: 1.0,
        }
    }
}

impl QualityMod {
    fn total(self) -> f32 {
        self.condition.max(0.0) * self.enhancement.max(0.0)
    }
}

/// Reason an NPC will refuse a transaction. Server maps these to player-
/// facing toasts; the economy math never panics on a bad request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VendorError {
    /// Item has `no_vendor: true` (quest item, unique curio).
    ItemNotVendorable,
    /// Item is soulbound — NPC vendor rejects it outright too.
    Soulbound,
}

/// Price a player pays an NPC to buy `item` at the given quality.
/// Rounds up so small fractional tiers don't collapse to 0.
pub fn vendor_buy_price(item: &ResolvedItem, quality: QualityMod) -> Result<u32, VendorError> {
    if item.no_vendor {
        return Err(VendorError::ItemNotVendorable);
    }
    let base = item.base_price as f32;
    let priced = (base * quality.total()).max(1.0);
    Ok(priced.ceil() as u32)
}

/// Price an NPC pays a player to take `item` off their hands.
/// Always strictly less than `vendor_buy_price` by `buy_spread`.
pub fn vendor_sell_price(
    item: &ResolvedItem,
    pricing: &VendorPricing,
    quality: QualityMod,
) -> Result<u32, VendorError> {
    if item.soulbound {
        return Err(VendorError::Soulbound);
    }
    if item.no_vendor {
        return Err(VendorError::ItemNotVendorable);
    }
    let buy = vendor_buy_price(item, quality)? as f32;
    let spread = pricing.buy_spread.clamp(0.0, 0.99);
    let sell = buy * (1.0 - spread);
    Ok(sell.max(1.0).round() as u32)
}

/// Minimum listing price on the player market. Listings below this are
/// rejected server-side so no one can undercut the vendor floor and
/// drain currency into the NPC loop. Soulbound items can't be listed.
pub fn market_floor(
    item: &ResolvedItem,
    pricing: &VendorPricing,
    quality: QualityMod,
) -> Result<u32, VendorError> {
    if item.soulbound {
        return Err(VendorError::Soulbound);
    }
    // no_vendor items are still market-listable — they just have no NPC
    // price to anchor on. Use the resolved base price times the reference mult.
    let buy = if item.no_vendor {
        (item.base_price as f32) * pricing.no_vendor_reference_mult * quality.total()
    } else {
        vendor_buy_price(item, quality)? as f32
    };
    let floor = buy * pricing.market_floor_mult.max(1.0);
    Ok(floor.ceil() as u32)
}

/// Per-transaction gold-sink telemetry. Server logs these so a balance
/// pass can compare faucet (XP-tagged mob drops, quest rewards) against
/// drain without scraping systems individually.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoldSinkKind {
    /// Vendor buy/sell spread (ambient, every transaction).
    VendorSpread,
    /// Repair bills.
    Repair,
    /// Reagent purchases (e.g. Rite of Return components).
    Reagent,
    /// Travel tolls, corpse-retrieval fees, ritual gold costs.
    Ritual,
    /// Player-market listing/deposit fee (non-refundable slice).
    MarketFee,
}

/// Single ledger entry emitted by a gold-sink event. Intentionally
/// small and `Copy` so it's cheap to stream into a channel.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct GoldSinkEntry {
    pub kind: GoldSinkKind,
    pub amount: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use vaern_items::{
        ArmorType, BaseKind, ContentRegistry, ItemBase, ItemInstance, Material, Quality, Rarity,
        SizeClass, WeaponGrip,
    };

    // Build a tiny registry containing one sword base, a `testmetal`
    // material pinned at 100g base price, and a regular quality.
    fn test_registry(no_vendor: bool, soulbound: bool) -> ContentRegistry {
        let mut r = ContentRegistry::new();
        r.insert_base(ItemBase {
            id: "sword".into(),
            piece_name: "Test Sword".into(),
            size: SizeClass::Medium,
            base_weight_kg: 1.0,
            volume_l: Some(2.0),
            stackable: false,
            stack_max: 1,
            no_vendor,
            soulbound,
            // Pin price so assertions don't drift with the formula.
            vendor_base_price: Some(100),
            kind: BaseKind::Weapon {
                grip: WeaponGrip::OneHanded,
                school: "blade".into(),
                base_min_dmg: 4.0,
                base_max_dmg: 8.0,
            },
        });
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

    fn resolved(reg: &ContentRegistry) -> ResolvedItem {
        reg.resolve(&ItemInstance::new("sword", "testmetal", "regular"))
            .expect("resolves")
    }

    #[test]
    fn buy_price_respects_base_and_quality() {
        let reg = test_registry(false, false);
        let item = resolved(&reg);
        assert_eq!(vendor_buy_price(&item, QualityMod::default()), Ok(100));
        let half = QualityMod {
            condition: 0.5,
            enhancement: 1.0,
        };
        assert_eq!(vendor_buy_price(&item, half), Ok(50));
    }

    #[test]
    fn sell_price_is_strictly_below_buy() {
        let p = VendorPricing::default();
        let reg = test_registry(false, false);
        let item = resolved(&reg);
        let buy = vendor_buy_price(&item, QualityMod::default()).unwrap();
        let sell = vendor_sell_price(&item, &p, QualityMod::default()).unwrap();
        assert!(sell < buy);
        // 60% spread → 40% of 100 = 40.
        assert_eq!(sell, 40);
    }

    #[test]
    fn vendor_rejects_no_vendor_items() {
        let p = VendorPricing::default();
        let reg = test_registry(true, false);
        let item = resolved(&reg);
        assert_eq!(
            vendor_buy_price(&item, QualityMod::default()),
            Err(VendorError::ItemNotVendorable)
        );
        assert_eq!(
            vendor_sell_price(&item, &p, QualityMod::default()),
            Err(VendorError::ItemNotVendorable)
        );
    }

    #[test]
    fn vendor_rejects_soulbound_on_sell() {
        let p = VendorPricing::default();
        let reg = test_registry(false, true);
        let item = resolved(&reg);
        // Buy works (vendor can still sell soulbound items like hearthstones).
        assert!(vendor_buy_price(&item, QualityMod::default()).is_ok());
        // Sell fails — player can't offload soulbound to vendor.
        assert_eq!(
            vendor_sell_price(&item, &p, QualityMod::default()),
            Err(VendorError::Soulbound)
        );
    }

    #[test]
    fn market_floor_sits_above_vendor_buy() {
        let p = VendorPricing::default();
        let reg = test_registry(false, false);
        let item = resolved(&reg);
        let buy = vendor_buy_price(&item, QualityMod::default()).unwrap();
        let floor = market_floor(&item, &p, QualityMod::default()).unwrap();
        assert!(floor > buy);
        // 110% of 100 = 110.
        assert_eq!(floor, 110);
    }

    #[test]
    fn market_floor_rejects_soulbound() {
        let p = VendorPricing::default();
        let reg = test_registry(false, true);
        let item = resolved(&reg);
        assert_eq!(
            market_floor(&item, &p, QualityMod::default()),
            Err(VendorError::Soulbound)
        );
    }
}
