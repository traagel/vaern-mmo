//! Atomic file write helper.
//!
//! Write the new content to `<path>.tmp` first, then `rename` over the
//! original. POSIX guarantees the rename is atomic on the same
//! filesystem, so a power-loss mid-write leaves either the old file
//! intact or the new one in place — never a half-written file.
//!
//! Used by `zone_io::save_zone` when (V2) the editor learns to write
//! YAML in-place over `src/generated/world/zones/...`.

use std::fs;
use std::io::Write;
use std::path::Path;

/// Write `bytes` to `path` atomically. Returns the io error from any
/// step (creating the temp file, writing, or renaming).
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = tmp_path(path);
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

/// Construct the temp filename — same dir, same stem, `.tmp` extension
/// appended. `foo/bar/baz.yaml` → `foo/bar/baz.yaml.tmp`.
fn tmp_path(path: &Path) -> std::path::PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".tmp");
    s.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tmp_path_appends_extension() {
        let p = Path::new("/foo/bar/baz.yaml");
        let t = tmp_path(p);
        assert_eq!(t.as_os_str(), "/foo/bar/baz.yaml.tmp");
    }

    #[test]
    fn write_atomic_creates_file() {
        let dir = std::env::temp_dir().join(format!("vaern_editor_test_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("hello.yaml");
        write_atomic(&path, b"hello: world\n").unwrap();
        let read = fs::read_to_string(&path).unwrap();
        assert_eq!(read, "hello: world\n");
        // Cleanup is best-effort.
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(&dir);
    }
}
