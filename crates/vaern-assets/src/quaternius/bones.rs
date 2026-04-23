//! Quaternius skeleton bone-name constants.
//!
//! The UE Mannequin skeleton uses `hand_r` / `hand_l` for the palm
//! bones — direct children of the forearm, essentially identity-
//! transformed in bind pose so a weapon parented there inherits the
//! hand's world transform directly without extra correction.
//!
//! Quaternius characters are multiple scene children under a single
//! parent entity (body / legs / arms / feet / head, each with its
//! own armature). The [`crate::regions::NamedRegions`] walker does a
//! depth-first traversal in spawn order, so the body scene — spawned
//! first in `spawn_quaternius_character` — is the one whose hand
//! bones get cached. All armatures animate in lock-step through the
//! shared `AnimationGraph`, so anchoring to body's hand is visually
//! identical to any other armature's hand.

/// Right palm bone name on the UE Mannequin. Dominant / mainhand.
pub const BONE_MAINHAND: &str = "hand_r";

/// Left palm bone name on the UE Mannequin. Offhand — shields,
/// torches, secondary weapons.
pub const BONE_OFFHAND: &str = "hand_l";
