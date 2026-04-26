//! Component tags for editor-owned entities.
//!
//! Use these for queries that should not touch user-side ECS world
//! contents (there are none in V1, but the marker is in place for
//! future zone-switch teardown).

use bevy::prelude::*;

/// Marker on every editor-spawned entity (lights, gizmos, debug
/// overlays). Dressing entities also carry their own
/// `crate::dressing::EditorDressingEntity` marker; both can coexist.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct EditorWorld;
