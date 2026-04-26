//! Top-level editor state — the resources that survive across mode
//! changes and zone reloads.
//!
//! [`EditorContext`] holds the active zone id + a coarse "what's the
//! editor doing right now" status. Mode-specific state lives in the
//! `modes::*` modules so this stays small.

use bevy::prelude::*;

/// One-shot config injected by the binary entrypoint before plugin
/// build. Read once during `Startup` to seed `EditorContext`.
#[derive(Resource, Debug, Clone)]
pub struct EditorBootConfig {
    pub zone_id: String,
}

impl Default for EditorBootConfig {
    fn default() -> Self {
        Self {
            zone_id: "dalewatch_marches".to_string(),
        }
    }
}

/// Active editor state. The single source of truth for "which zone are
/// we editing right now" + a status string the toolbar prints.
#[derive(Resource, Debug, Clone)]
pub struct EditorContext {
    pub active_zone: String,
    pub status: String,
}

impl Default for EditorContext {
    fn default() -> Self {
        Self {
            active_zone: "dalewatch_marches".to_string(),
            status: "boot".to_string(),
        }
    }
}

impl EditorContext {
    /// Set the status line. Logs at `info` so the console captures the
    /// transition. UI panels surface this as the bottom-bar status.
    pub fn set_status(&mut self, s: impl Into<String>) {
        let s = s.into();
        info!(status = %s, "editor status");
        self.status = s;
    }
}

/// Editor bootstrap state. Gates the world-load + voxel-stream + UI
/// systems so they run only after the boot config has been read.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum EditorAppState {
    /// Pre-boot: assets loading, no zone yet selected.
    #[default]
    Booting,
    /// Active zone loaded; free-fly + UI live.
    Editing,
}

/// Plugin: registers the `EditorAppState` machine + `EditorContext`
/// resource and seeds it from `EditorBootConfig` on Startup.
pub struct EditorStatePlugin;

impl Plugin for EditorStatePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<EditorAppState>()
            .init_resource::<EditorBootConfig>()
            .init_resource::<EditorContext>()
            .add_systems(Startup, seed_context_from_boot)
            .add_systems(Startup, advance_to_editing.after(seed_context_from_boot));
    }
}

fn seed_context_from_boot(boot: Res<EditorBootConfig>, mut ctx: ResMut<EditorContext>) {
    ctx.active_zone = boot.zone_id.clone();
    ctx.set_status(format!("loading zone: {}", boot.zone_id));
}

fn advance_to_editing(mut next: ResMut<NextState<EditorAppState>>) {
    // V1: jump straight from Booting to Editing in the same frame. Once
    // there's an actual async asset preload phase, gate this on the
    // preload finishing.
    next.set(EditorAppState::Editing);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boot_config_default_is_dalewatch() {
        let cfg = EditorBootConfig::default();
        assert_eq!(cfg.zone_id, "dalewatch_marches");
    }

    #[test]
    fn editor_context_set_status_updates() {
        let mut ctx = EditorContext::default();
        ctx.set_status("hello");
        assert_eq!(ctx.status, "hello");
    }
}
