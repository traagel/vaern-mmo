//! The Bevy-0.18 `Mesh3d` visibility-chain fix.
//!
//! Bevy's glTF loader spawns mesh-primitive entities with
//! `Mesh3d + MeshMaterial3d + Transform` only — **no `Visibility`**. The
//! renderer's extract query requires `&ViewVisibility`, which only
//! exists when `Visibility` is present (via the `#[require]` chain on
//! `Visibility`). Without an explicit `Visibility::Inherited` the mesh
//! never renders, regardless of parent visibility.
//!
//! `apply_visibility_fix` runs each frame and inserts
//! `Visibility::Inherited` on every `Mesh3d` that lacks a `Visibility`
//! component. The `Without<Visibility>` filter self-gates: as soon as
//! the fix runs on a mesh, that mesh falls out of the query. No marker
//! component is needed, and meshes streamed in on later frames by the
//! scene spawner are covered automatically.
//!
//! If you intentionally spawn a mesh with an explicit `Visibility`
//! (including `Visibility::Hidden`), the filter skips it — the fix
//! never overrides caller intent.

use bevy::prelude::*;

pub(super) fn apply_visibility_fix(
    mut commands: Commands,
    orphans: Query<Entity, (With<Mesh3d>, Without<Visibility>)>,
) {
    for e in &orphans {
        commands.entity(e).insert(Visibility::Inherited);
    }
}
