//! Glue that wires the procedural [`WorldTerrain`] into the
//! `vaern-core::terrain::height` global resolver.
//!
//! Server + client call [`install_terrain_resolver`] once at startup
//! (before `App::new()` or during the first Startup system, but
//! before any voxel chunk generator samples `terrain::height`). After
//! that, every call into `vaern_core::terrain::height(x, z)` routes
//! through the procedural heightfield + per-zone paint deltas.
//!
//! The closure registered owns an `Arc<WorldTerrain>` so the heightfield
//! state lives for the rest of the process. `OnceLock` semantics in
//! `vaern_core::terrain` mean only the first registration sticks —
//! subsequent calls are silent no-ops.

use std::path::Path;
use std::sync::Arc;

use crate::WorldTerrain;

/// Build a [`WorldTerrain`] from `world_root` and register it as the
/// process-global terrain resolver. Returns the loaded `Arc<WorldTerrain>`
/// so the caller can keep a reference for diagnostics / direct
/// queries; the registry also holds an `Arc` so dropping the returned
/// value doesn't unregister.
///
/// Idempotent: the underlying registry is a `OnceLock`; calling twice
/// in the same process logs a warning and keeps the first registration.
pub fn install_terrain_resolver(
    world_root: &Path,
) -> Result<Arc<WorldTerrain>, vaern_data::LoadError> {
    let world_terrain = Arc::new(WorldTerrain::build(world_root)?);
    if vaern_core::terrain::resolver_is_registered() {
        eprintln!(
            "vaern-cartography: terrain resolver already registered — second install ignored"
        );
        return Ok(world_terrain);
    }
    let resolver_arc = Arc::clone(&world_terrain);
    vaern_core::terrain::register_resolver(move |x, z| resolver_arc.final_height(x, z));
    Ok(world_terrain)
}
