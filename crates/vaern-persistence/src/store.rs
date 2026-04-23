//! `ServerCharacterStore` — on-disk character persistence.
//!
//! One JSON file per character at `<root>/<uuid>.json`. Atomic save:
//! write to `<uuid>.json.tmp`, `sync_all()`, then `rename` over the
//! target. On ext4 the rename is atomic inside a filesystem, so a crash
//! mid-write leaves the previous good file intact (or nothing at all
//! for a first save).
//!
//! Unknown schema versions are quarantined with a `.corrupt-<unix_ts>`
//! suffix rather than auto-upgraded — we'd rather fail loud than load
//! a stale format that drops a field.
//!
//! This module is pure I/O + path plumbing. It has no Bevy deps.
//! Systems that drive saves live in `vaern-server::persistence`.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use uuid::Uuid;

use crate::schema::{PersistedCharacter, SCHEMA_VERSION};

/// Default on-disk root: `$HOME/.config/vaern/server/characters`.
/// Mirrors the client-local path layout so a single vaern install keeps
/// all state under one `.config/vaern` tree.
pub fn default_root() -> PathBuf {
    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("vaern")
        .join("server")
        .join("characters")
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Load / save errors.
#[derive(Debug)]
pub enum StoreError {
    Io(io::Error),
    /// File existed but either failed to parse as JSON, had the wrong
    /// schema version, or had a `character_id` field that didn't match
    /// the filename UUID. All three are handled identically by the
    /// caller: log + treat as missing + let a fresh CreateNew path
    /// seed a new file at the same UUID. The offending file is
    /// renamed to `<path>.corrupt-<unix_ts>` before returning.
    Corrupt {
        reason: CorruptReason,
        quarantined_to: PathBuf,
    },
}

#[derive(Debug)]
pub enum CorruptReason {
    ParseFailed(serde_json::Error),
    UnsupportedVersion { found: u32, expected: u32 },
    IdMismatch { file_uuid: Uuid, field_id: String },
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::Corrupt { reason, quarantined_to } => {
                write!(f, "corrupt save file (")?;
                match reason {
                    CorruptReason::ParseFailed(e) => write!(f, "parse failed: {e}")?,
                    CorruptReason::UnsupportedVersion { found, expected } => {
                        write!(f, "schema version {found} != {expected}")?
                    }
                    CorruptReason::IdMismatch { file_uuid, field_id } => {
                        write!(f, "uuid {file_uuid} != character_id {field_id:?}")?
                    }
                }
                write!(f, "); quarantined to {}", quarantined_to.display())
            }
        }
    }
}

impl std::error::Error for StoreError {}

impl From<io::Error> for StoreError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// Server-side character store. Lightweight — owns a root path and
/// nothing else. Bevy wraps it as a `Resource` in vaern-server.
#[derive(Debug, Clone)]
pub struct ServerCharacterStore {
    root: PathBuf,
}

impl ServerCharacterStore {
    /// Open a store at `root`. Creates the directory if missing.
    /// Fails only if the path exists and isn't a directory, or if we
    /// can't create it.
    pub fn open(root: PathBuf) -> io::Result<Self> {
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    /// Open a store at `default_root()`.
    pub fn open_default() -> io::Result<Self> {
        Self::open(default_root())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn file_for(&self, uuid: Uuid) -> PathBuf {
        self.root.join(format!("{uuid}.json"))
    }

    pub fn exists(&self, uuid: Uuid) -> bool {
        self.file_for(uuid).is_file()
    }

    /// List every character UUID with a well-formed file in the store.
    /// Skips stray files (e.g. quarantined `.corrupt-*` siblings).
    pub fn list(&self) -> Vec<Uuid> {
        let Ok(dir) = fs::read_dir(&self.root) else { return Vec::new() };
        let mut out = Vec::new();
        for entry in dir.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else { continue };
            let Some(ext) = path.extension().and_then(|s| s.to_str()) else { continue };
            if ext != "json" {
                continue;
            }
            if let Ok(uuid) = Uuid::parse_str(stem) {
                out.push(uuid);
            }
        }
        out
    }

    /// Load a character by UUID. On any recoverable format problem
    /// (bad JSON, stale schema_version, id mismatch) the file is
    /// quarantined and `StoreError::Corrupt` is returned — caller
    /// should fall back to a fresh CreateNew path at the same UUID.
    pub fn load(&self, uuid: Uuid) -> Result<PersistedCharacter, StoreError> {
        let path = self.file_for(uuid);
        let mut file = File::open(&path)?;
        let mut buf = String::new();
        file.read_to_string(&mut buf)?;

        let value: serde_json::Value = match serde_json::from_str(&buf) {
            Ok(v) => v,
            Err(e) => {
                return Err(StoreError::Corrupt {
                    reason: CorruptReason::ParseFailed(e),
                    quarantined_to: quarantine(&path)?,
                });
            }
        };

        let found_version = value
            .get("schema_version")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32)
            .unwrap_or(0);
        if found_version != SCHEMA_VERSION {
            return Err(StoreError::Corrupt {
                reason: CorruptReason::UnsupportedVersion {
                    found: found_version,
                    expected: SCHEMA_VERSION,
                },
                quarantined_to: quarantine(&path)?,
            });
        }

        let ch: PersistedCharacter = match serde_json::from_value(value) {
            Ok(ch) => ch,
            Err(e) => {
                return Err(StoreError::Corrupt {
                    reason: CorruptReason::ParseFailed(e),
                    quarantined_to: quarantine(&path)?,
                });
            }
        };

        if Uuid::parse_str(&ch.character_id).ok() != Some(uuid) {
            let field_id = ch.character_id.clone();
            return Err(StoreError::Corrupt {
                reason: CorruptReason::IdMismatch {
                    file_uuid: uuid,
                    field_id,
                },
                quarantined_to: quarantine(&path)?,
            });
        }

        Ok(ch)
    }

    /// Atomically save a character. Writes to `<uuid>.json.tmp`, fsyncs,
    /// then renames over the target. Caller's responsibility to ensure
    /// `ch.character_id == uuid.to_string()`.
    pub fn save(&self, uuid: Uuid, ch: &PersistedCharacter) -> io::Result<()> {
        let final_path = self.file_for(uuid);
        let tmp_path = self.root.join(format!("{uuid}.json.tmp"));

        let json = serde_json::to_vec_pretty(ch).map_err(io::Error::other)?;

        {
            let mut file = File::create(&tmp_path)?;
            file.write_all(&json)?;
            file.sync_all()?;
        }
        fs::rename(&tmp_path, &final_path)?;
        Ok(())
    }
}

/// Rename `path` to `<path>.corrupt-<unix_ts>`. Returns the new path.
fn quarantine(path: &Path) -> io::Result<PathBuf> {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut corrupted = path.as_os_str().to_owned();
    corrupted.push(format!(".corrupt-{ts}"));
    let corrupted_path = PathBuf::from(corrupted);
    fs::rename(path, &corrupted_path)?;
    Ok(corrupted_path)
}
