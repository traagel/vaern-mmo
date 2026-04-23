//! Server-side character persistence format + converters.
//!
//! PR1 scope: on-disk schema types (`PersistedCharacter`, `PersistedCosmetics`,
//! `PersistedQuestLog`) and cosmetic-enum ⇄ string-tag adapters. No file I/O
//! and no Bevy systems yet — the `ServerCharacterStore` and the dirty-flag
//! flush land in PR2.
//!
//! Cosmetic enums (`Outfit` / `HeadPiece` / `Hair` / `Beard` / `ColorVariant`)
//! are NOT `Serialize`-derived in `vaern-assets`. Instead we convert to/from
//! stable lowercase string tags here. Reordering or renaming a variant
//! breaks compilation in `cosmetic::outfit_tag` (etc.) rather than silently
//! corrupting on-disk files.

pub mod cosmetic;
pub mod humanoid;
pub mod sanitize;
pub mod schema;
pub mod store;

pub use cosmetic::{
    PersistedCosmetics, PersistedHeadSlot, PersistedOutfitSlot, beard_tag, color_tag, hair_tag,
    head_piece_tag, outfit_tag,
};
pub use humanoid::{HumanoidArchetype, HumanoidArchetypeTable};
pub use sanitize::{DroppedItem, sanitize_loadout};
pub use schema::{PersistedCharacter, PersistedQuestEntry, PersistedQuestLog, SCHEMA_VERSION};
pub use store::{CorruptReason, ServerCharacterStore, StoreError, default_root};
