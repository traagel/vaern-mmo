//! Command-line argument parsing for the editor binary.
//!
//! Kept minimal in V1: a zone id (defaulting to dalewatch_marches, the
//! only fully hand-curated zone) and an optional window size override.

use clap::Parser;

/// `vaern-editor` CLI.
#[derive(Parser, Debug, Clone)]
#[command(
    name = "vaern-editor",
    version,
    about = "Standalone map editor for Vaern zones."
)]
pub struct EditorCli {
    /// Zone id to load on startup. Must match a directory under
    /// `src/generated/world/zones/<zone>/`.
    #[arg(long, default_value = "dalewatch_marches")]
    pub zone: String,

    /// Window size as `WIDTHxHEIGHT` (e.g. `1920x1080`). Defaults to
    /// 1600x900.
    #[arg(long = "window-size", default_value = "1600x900")]
    pub window_size: String,
}

impl EditorCli {
    /// Parse `--window-size` into `(width, height)`. Falls back to the
    /// default (1600, 900) on malformed input rather than failing hard
    /// — the editor is a dev tool, not a release-quality CLI.
    pub fn window_size(&self) -> (u32, u32) {
        parse_window_size(&self.window_size).unwrap_or((1600, 900))
    }
}

fn parse_window_size(s: &str) -> Option<(u32, u32)> {
    let (w, h) = s.split_once('x')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_canonical_size() {
        assert_eq!(parse_window_size("1920x1080"), Some((1920, 1080)));
        assert_eq!(parse_window_size("800x600"), Some((800, 600)));
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(parse_window_size("garbage"), None);
        assert_eq!(parse_window_size("1920"), None);
        assert_eq!(parse_window_size("axb"), None);
    }
}
