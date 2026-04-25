//! Copper → gold/silver/copper display. Server never needs this — all
//! math runs in the atomic copper unit. UIs format on read.
//!
//! Conversion is the classic-MMO 100/100 schedule: 1 silver = 100
//! copper, 1 gold = 100 silver = 10_000 copper. No platinum tier
//! planned; when mid-game mob drops cross 100 gold (~1M copper) the
//! display format stays readable because Bevy's egui font renders the
//! digit count fine.

pub const COPPER_PER_SILVER: u64 = 100;
pub const COPPER_PER_GOLD: u64 = COPPER_PER_SILVER * 100;

/// Split `copper` into `(gold, silver, copper)`. Used for display only.
pub fn split_copper(amount: u64) -> (u64, u64, u64) {
    let gold = amount / COPPER_PER_GOLD;
    let rem = amount % COPPER_PER_GOLD;
    let silver = rem / COPPER_PER_SILVER;
    let copper = rem % COPPER_PER_SILVER;
    (gold, silver, copper)
}

/// Render a copper amount as a compact `"12g 34s 56c"` string. Omits
/// zero-valued higher denominations (e.g. `"34s 56c"` or `"56c"`) so
/// small balances stay readable.
pub fn format_copper_as_gsc(amount: u64) -> String {
    let (g, s, c) = split_copper(amount);
    if g > 0 {
        format!("{g}g {s}s {c}c")
    } else if s > 0 {
        format!("{s}s {c}c")
    } else {
        format!("{c}c")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_renders_zero_copper() {
        assert_eq!(format_copper_as_gsc(0), "0c");
    }

    #[test]
    fn under_hundred_is_copper_only() {
        assert_eq!(format_copper_as_gsc(47), "47c");
    }

    #[test]
    fn silver_band_omits_gold() {
        assert_eq!(format_copper_as_gsc(347), "3s 47c");
    }

    #[test]
    fn gold_band_includes_all_three() {
        // 2g 34s 56c = 23456 copper
        assert_eq!(format_copper_as_gsc(23_456), "2g 34s 56c");
    }

    #[test]
    fn exact_gold_boundary() {
        assert_eq!(format_copper_as_gsc(10_000), "1g 0s 0c");
    }

    #[test]
    fn split_round_trip() {
        let (g, s, c) = split_copper(98_765);
        assert_eq!(g * COPPER_PER_GOLD + s * COPPER_PER_SILVER + c, 98_765);
    }
}
