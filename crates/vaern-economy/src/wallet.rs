//! Player wallet — atomic copper balance. The server owns mutation; the
//! client receives snapshots and never writes. One currency unit on the
//! wire; display formatting (`copper.format_gsc()`) is a UI-layer
//! concern (see `currency`).
//!
//! `u64` so boss-named hoards can't arithmetic-overflow in a long
//! session. Typical mob drop is single-digit copper; a rare raid boss
//! might pay 10k copper. `u64` covers 184 quintillion copper — not a
//! real ceiling.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Per-player currency pool. Server-authoritative; broadcast to the
/// owning client via `WalletSnapshot` on change.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PlayerWallet {
    pub copper: u64,
}

impl PlayerWallet {
    pub fn new(copper: u64) -> Self {
        Self { copper }
    }

    /// Add `amount` copper. Saturates at `u64::MAX` instead of
    /// panicking on overflow.
    pub fn credit(&mut self, amount: u64) {
        self.copper = self.copper.saturating_add(amount);
    }

    /// Try to spend `amount` copper. Returns `true` if the wallet had
    /// enough and the debit happened; `false` otherwise (wallet
    /// unchanged). Never goes negative.
    pub fn try_debit(&mut self, amount: u64) -> bool {
        if self.copper < amount {
            return false;
        }
        self.copper -= amount;
        true
    }

    /// Non-mutating affordability check. Prefer `try_debit` when you're
    /// about to spend — avoids check-then-act races.
    pub fn can_afford(&self, amount: u64) -> bool {
        self.copper >= amount
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credit_accumulates() {
        let mut w = PlayerWallet::default();
        w.credit(100);
        w.credit(50);
        assert_eq!(w.copper, 150);
    }

    #[test]
    fn credit_saturates_on_overflow() {
        let mut w = PlayerWallet::new(u64::MAX - 5);
        w.credit(100);
        assert_eq!(w.copper, u64::MAX);
    }

    #[test]
    fn try_debit_succeeds_when_affordable() {
        let mut w = PlayerWallet::new(100);
        assert!(w.try_debit(40));
        assert_eq!(w.copper, 60);
    }

    #[test]
    fn try_debit_refuses_when_short() {
        let mut w = PlayerWallet::new(30);
        assert!(!w.try_debit(40));
        assert_eq!(w.copper, 30);
    }

    #[test]
    fn try_debit_exact_balance_drains_to_zero() {
        let mut w = PlayerWallet::new(100);
        assert!(w.try_debit(100));
        assert_eq!(w.copper, 0);
    }

    #[test]
    fn can_afford_is_non_mutating() {
        let w = PlayerWallet::new(50);
        assert!(w.can_afford(40));
        assert!(w.can_afford(50));
        assert!(!w.can_afford(51));
        assert_eq!(w.copper, 50);
    }
}
