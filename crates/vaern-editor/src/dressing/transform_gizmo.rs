//! Transform gizmo — translate / rotate-Y / scale handles for the
//! selected dressing prop.
//!
//! V1: stub. Building this requires either pulling in a gizmo crate
//! (e.g. `bevy_transform_gizmo`) or hand-rolling a 3-axis arrow + ring
//! widget. The latter is ~400 LoC; deferred.
//!
//! When this lands, it should write the new `Transform` back to the
//! editor's mirror of the `AuthoredProp` so save-on-exit can serialize
//! the change.

use bevy::prelude::*;

pub struct TransformGizmoPlugin;

impl Plugin for TransformGizmoPlugin {
    fn build(&self, _app: &mut App) {
        // TODO(editor): translate / rotate-Y / scale handles.
    }
}
