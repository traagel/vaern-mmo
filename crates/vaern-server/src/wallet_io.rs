//! Broadcast `WalletSnapshot` to the owning client whenever
//! `PlayerWallet` changes. Follows the inventory/equipped pattern:
//! one S→C message per mutation, idle ticks cost nothing.

use bevy::prelude::*;
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use vaern_economy::PlayerWallet;
use vaern_protocol::{Channel1, PlayerTag, WalletSnapshot};

/// Ship a `WalletSnapshot` to each player whose `PlayerWallet` was
/// mutated this tick (credit from loot, quest payout, future vendor
/// buy/sell). Gated on `Changed<PlayerWallet>` so most ticks early-out
/// after a single filtered iterator check.
pub fn broadcast_wallet_on_change(
    players: Query<(&ControlledBy, &PlayerWallet), (With<PlayerTag>, Changed<PlayerWallet>)>,
    mut senders: Query<&mut MessageSender<WalletSnapshot>, With<ClientOf>>,
) {
    for (cb, wallet) in &players {
        let Ok(mut sender) = senders.get_mut(cb.owner) else { continue };
        let _ = sender.send::<Channel1>(WalletSnapshot { copper: wallet.copper });
    }
}
