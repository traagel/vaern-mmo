//! Per-NPC vendor stock. Drives what a vendor sells + the runtime
//! stock tracking if a listing is `Limited`. Sell tab (player →
//! vendor) doesn't need any authored data — the vendor pays
//! `vendor_sell_price` on any non-soulbound non-quest item the
//! player presents.
//!
//! Design choice: one material + quality pair per listing. If a
//! vendor stocks "iron longsword" and "steel longsword" as distinct
//! listings, each is authored as its own entry. Keeps the resolve
//! path simple; pretty enough for pre-alpha.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// How many copies of a listing the vendor is willing to sell. `Infinite`
/// is the pre-alpha default — no restock bookkeeping. `Limited(u32)`
/// ticks down on each buy and removes the listing on exhaust. No
/// restock cycle yet; a listing at 0 stays gone until the NPC
/// respawns (which re-reads the YAML).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VendorSupply {
    Infinite,
    Limited(u32),
}

impl Default for VendorSupply {
    fn default() -> Self {
        Self::Infinite
    }
}

/// One item a vendor sells. Resolves against the content registry at
/// display time (client needs name + icon) and at transaction time
/// (server prices via `vendor_buy_price`, transfers via
/// `ContentRegistry::resolve`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VendorListing {
    pub base_id: String,
    #[serde(default)]
    pub material_id: Option<String>,
    #[serde(default)]
    pub quality_id: Option<String>,
    #[serde(default)]
    pub supply: VendorSupply,
}

impl VendorListing {
    /// Convert to a `vaern_items::ItemInstance`. `quality_id` defaults
    /// to "regular" when unset; the YAML is allowed to omit it for the
    /// common case. `material_id: None` means a material-less base
    /// (e.g. consumables, runes) which composes fine.
    pub fn to_instance(&self) -> vaern_items::ItemInstance {
        vaern_items::ItemInstance {
            base_id: self.base_id.clone(),
            material_id: self.material_id.clone(),
            quality_id: self
                .quality_id
                .clone()
                .unwrap_or_else(|| "regular".to_string()),
            affixes: Vec::new(),
        }
    }
}

/// Vendor's full inventory — one component per vendor NPC. Server-only.
/// Client sees the derived `VendorWindowSnapshot` (listings + prices)
/// when opening the window, not this struct directly.
#[derive(Component, Debug, Clone, Default, Serialize, Deserialize)]
pub struct VendorStock {
    pub listings: Vec<VendorListing>,
}

impl VendorStock {
    pub fn new(listings: Vec<VendorListing>) -> Self {
        Self { listings }
    }

    /// Decrement one unit of stock on `idx`. Returns `false` if the
    /// index is out of range or the listing had zero stock (buy
    /// should be rejected). `Infinite` always returns `true`.
    pub fn consume(&mut self, idx: usize) -> bool {
        let Some(listing) = self.listings.get_mut(idx) else {
            return false;
        };
        match &mut listing.supply {
            VendorSupply::Infinite => true,
            VendorSupply::Limited(n) => {
                if *n == 0 {
                    return false;
                }
                *n -= 1;
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infinite_supply_never_depletes() {
        let mut stock = VendorStock::new(vec![VendorListing {
            base_id: "cloth_shirt".into(),
            material_id: Some("linen".into()),
            quality_id: Some("regular".into()),
            supply: VendorSupply::Infinite,
        }]);
        for _ in 0..1000 {
            assert!(stock.consume(0));
        }
    }

    #[test]
    fn limited_supply_depletes_and_rejects() {
        let mut stock = VendorStock::new(vec![VendorListing {
            base_id: "potion_healing_minor".into(),
            material_id: None,
            quality_id: None,
            supply: VendorSupply::Limited(2),
        }]);
        assert!(stock.consume(0));
        assert!(stock.consume(0));
        assert!(!stock.consume(0), "depleted stock must reject");
    }

    #[test]
    fn out_of_bounds_consume_is_false() {
        let mut stock = VendorStock::default();
        assert!(!stock.consume(0));
    }

    #[test]
    fn listing_defaults_quality_to_regular() {
        let l = VendorListing {
            base_id: "cloth_shirt".into(),
            material_id: Some("linen".into()),
            quality_id: None,
            supply: VendorSupply::Infinite,
        };
        let inst = l.to_instance();
        assert_eq!(inst.quality_id, "regular");
    }
}
