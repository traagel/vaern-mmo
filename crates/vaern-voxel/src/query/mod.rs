//! Read-only queries against a [`crate::ChunkStore`].
//!
//! These are the APIs the rest of the game uses to "ask the world":
//! * [`ground_y`] — the player-Y-snap replacement for
//!   `vaern_core::terrain::height`. Walks the SDF downward to find
//!   where it crosses zero.
//! * [`raycast`] — directional ray march. Useful for projectile-terrain
//!   collision, line-of-sight gates, and mouse-picking.
//!
//! The implementations are deliberately simple (analytic SDF-aware
//! ray march, no BVH). Voxel-scale detail means worst-case step count
//! is bounded by ray length / voxel size; for longer probes a
//! sphere-traced version using the SDF's distance estimation is a
//! one-file drop-in.

pub mod raycast;

pub use raycast::{RayHit, ground_y, raycast};
