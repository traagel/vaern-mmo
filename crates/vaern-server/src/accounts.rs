//! Server-side account store. Backed by SQLite at
//! `~/.config/vaern/server/accounts.db`. Two tables:
//!
//! ```text
//! accounts (account_id TEXT PK, username TEXT UNIQUE NOCASE, password_hash TEXT, created_at INTEGER)
//! characters (character_id TEXT PK, account_id TEXT, character_name TEXT UNIQUE NOCASE, created_at INTEGER)
//! ```
//!
//! Passwords are bcrypt hashes (default cost 12 in release, cost 4 in tests
//! so the test suite stays fast). Account creation enforces username
//! uniqueness; character creation enforces name uniqueness across the whole
//! server (case-insensitive). The `character_id` column matches the file
//! stem of `~/.config/vaern/server/characters/<character_id>.json` so the
//! existing `vaern_persistence::ServerCharacterStore` keeps working
//! verbatim.
//!
//! ## Error model
//!
//! `AccountError` distinguishes "expected" failure paths (`UsernameTaken`,
//! `NameTaken`, `WrongPassword`, `NotFound`) from "infrastructure" failures
//! (`Sql`, `Hash`). Callers should surface the expected variants to the
//! client in `LoginResult.error_msg`/`RegisterResult.error_msg` so the user
//! sees something actionable.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use bevy::prelude::Resource;
use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

/// Bcrypt cost. 12 in release (~250ms hash on a modern CPU), 4 in tests
/// (~1ms) so the unit-test suite stays under a second.
#[cfg(not(test))]
const BCRYPT_COST: u32 = 12;
#[cfg(test)]
const BCRYPT_COST: u32 = 4;

/// Maximum username length in characters. Plenty for the pre-alpha tester
/// pool; rejects pathological inputs. Matches the SQLite TEXT column
/// (no length limit on storage; this is a validation gate).
pub const MAX_USERNAME_LEN: usize = 32;
/// Minimum username length so an empty `username` field doesn't sneak past.
pub const MIN_USERNAME_LEN: usize = 2;
/// Minimum password length. Enforced at register-time only; existing
/// accounts with shorter passwords still authenticate.
pub const MIN_PASSWORD_LEN: usize = 4;
/// Maximum character name length.
pub const MAX_CHARACTER_NAME_LEN: usize = 24;
/// Minimum character name length.
pub const MIN_CHARACTER_NAME_LEN: usize = 2;

/// UUID-shaped identifier for one account row. The `String` form is what
/// gets stored in SQLite + sent over the wire.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AccountId(pub String);

impl std::fmt::Display for AccountId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Surfaced to the client in LoginResult.characters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterRow {
    pub character_id: String,
    pub character_name: String,
    pub created_at: i64,
}

#[derive(Debug)]
pub enum AccountError {
    UsernameTaken(String),
    NameTaken(String),
    WrongPassword,
    NotFound,
    InvalidUsername(&'static str),
    InvalidPassword(&'static str),
    InvalidCharacterName(&'static str),
    Sql(rusqlite::Error),
    Hash(bcrypt::BcryptError),
    Io(std::io::Error),
}

impl std::fmt::Display for AccountError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AccountError::UsernameTaken(u) => write!(f, "username '{u}' is already taken"),
            AccountError::NameTaken(n) => write!(f, "character name '{n}' is already taken"),
            AccountError::WrongPassword => write!(f, "wrong password"),
            AccountError::NotFound => write!(f, "account not found"),
            AccountError::InvalidUsername(why) => write!(f, "invalid username: {why}"),
            AccountError::InvalidPassword(why) => write!(f, "invalid password: {why}"),
            AccountError::InvalidCharacterName(why) => {
                write!(f, "invalid character name: {why}")
            }
            AccountError::Sql(e) => write!(f, "database error: {e}"),
            AccountError::Hash(e) => write!(f, "hash error: {e}"),
            AccountError::Io(e) => write!(f, "io error: {e}"),
        }
    }
}

impl std::error::Error for AccountError {}

impl From<rusqlite::Error> for AccountError {
    fn from(e: rusqlite::Error) -> Self {
        AccountError::Sql(e)
    }
}
impl From<bcrypt::BcryptError> for AccountError {
    fn from(e: bcrypt::BcryptError) -> Self {
        AccountError::Hash(e)
    }
}
impl From<std::io::Error> for AccountError {
    fn from(e: std::io::Error) -> Self {
        AccountError::Io(e)
    }
}

/// Server-side account store. The Bevy resource wraps the SQLite
/// connection in a `Mutex` because `rusqlite::Connection` is `Send` but
/// `!Sync`, and Bevy's `Res<T>`/`ResMut<T>` requires `Sync` for
/// resources accessed across systems. The mutex is only ever held for
/// the duration of one `register` / `authenticate` / `list_characters`
/// call, so contention is negligible.
#[derive(Resource)]
pub struct AccountStore {
    inner: Mutex<Connection>,
    #[allow(dead_code)]
    path: PathBuf,
}

impl AccountStore {
    /// Open (or create) the default-location database at
    /// `~/.config/vaern/server/accounts.db`. Creates the parent dir if
    /// it doesn't exist; runs the schema migration on every open so a
    /// freshly-cloned repo just works.
    pub fn open_default() -> Result<Self, AccountError> {
        let path = default_db_path()?;
        Self::open_at(&path)
    }

    /// Open at a custom path. Used by tests to point at a temp dir.
    pub fn open_at(path: &Path) -> Result<Self, AccountError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        // Foreign keys are off by default in SQLite; turn them on so
        // the FK on characters.account_id is enforced.
        conn.execute("PRAGMA foreign_keys = ON;", [])?;
        run_schema(&conn)?;
        Ok(Self {
            inner: Mutex::new(conn),
            path: path.to_path_buf(),
        })
    }

    /// Register a fresh account. Validates username + password, hashes
    /// the password with bcrypt, inserts. Returns the new account id on
    /// success, `UsernameTaken` if the username already exists.
    pub fn register(
        &self,
        username: &str,
        password: &str,
    ) -> Result<AccountId, AccountError> {
        validate_username(username)?;
        validate_password(password)?;
        let hash = bcrypt::hash(password, BCRYPT_COST)?;
        let account_id = Uuid::new_v4().to_string();
        let now = unix_now();
        let conn = self.inner.lock().expect("AccountStore mutex poisoned");
        let res = conn.execute(
            "INSERT INTO accounts (account_id, username, password_hash, created_at) VALUES (?, ?, ?, ?)",
            params![account_id, username, hash, now],
        );
        match res {
            Ok(_) => Ok(AccountId(account_id)),
            Err(rusqlite::Error::SqliteFailure(e, _))
                if e.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                Err(AccountError::UsernameTaken(username.to_string()))
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Authenticate an existing account. Looks up the username
    /// (case-insensitive thanks to the `COLLATE NOCASE` column),
    /// verifies the bcrypt hash, returns the account_id on match.
    /// Returns `NotFound` for an unknown username and `WrongPassword`
    /// for a known username with a non-matching password — callers
    /// MAY collapse both into the same client-facing error message
    /// to avoid leaking which usernames exist.
    pub fn authenticate(
        &self,
        username: &str,
        password: &str,
    ) -> Result<AccountId, AccountError> {
        validate_username(username)?;
        let conn = self.inner.lock().expect("AccountStore mutex poisoned");
        let row: Option<(String, String)> = conn
            .query_row(
                "SELECT account_id, password_hash FROM accounts WHERE username = ?",
                params![username],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
            .optional()?;
        let (account_id, hash) = match row {
            Some(r) => r,
            None => return Err(AccountError::NotFound),
        };
        if bcrypt::verify(password, &hash)? {
            Ok(AccountId(account_id))
        } else {
            Err(AccountError::WrongPassword)
        }
    }

    /// List every character attached to the given account, oldest first.
    pub fn list_characters(
        &self,
        account_id: &AccountId,
    ) -> Result<Vec<CharacterRow>, AccountError> {
        let conn = self.inner.lock().expect("AccountStore mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT character_id, character_name, created_at FROM characters \
             WHERE account_id = ? ORDER BY created_at ASC",
        )?;
        let rows = stmt
            .query_map(params![account_id.0], |r| {
                Ok(CharacterRow {
                    character_id: r.get(0)?,
                    character_name: r.get(1)?,
                    created_at: r.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Insert a fresh character row. Caller mints `character_id`
    /// (typically `Uuid::new_v4().to_string()`) so it can match the
    /// JSON file stem for `vaern_persistence::ServerCharacterStore`.
    /// Enforces case-insensitive name uniqueness across the entire
    /// server.
    pub fn create_character(
        &self,
        account_id: &AccountId,
        character_id: &str,
        name: &str,
    ) -> Result<(), AccountError> {
        validate_character_name(name)?;
        let now = unix_now();
        let conn = self.inner.lock().expect("AccountStore mutex poisoned");
        let res = conn.execute(
            "INSERT INTO characters (character_id, account_id, character_name, created_at) VALUES (?, ?, ?, ?)",
            params![character_id, account_id.0, name, now],
        );
        match res {
            Ok(_) => Ok(()),
            Err(rusqlite::Error::SqliteFailure(e, _))
                if e.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                // Could be the PK on character_id OR the unique on
                // character_name. We optimize the common case (name)
                // but the message mentions both possibilities.
                Err(AccountError::NameTaken(name.to_string()))
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Look up the account id that owns a given character id, or
    /// `None` if the character isn't in the table.
    pub fn account_for_character(
        &self,
        character_id: &str,
    ) -> Result<Option<AccountId>, AccountError> {
        let conn = self.inner.lock().expect("AccountStore mutex poisoned");
        let row: Option<String> = conn
            .query_row(
                "SELECT account_id FROM characters WHERE character_id = ?",
                params![character_id],
                |r| r.get::<_, String>(0),
            )
            .optional()?;
        Ok(row.map(AccountId))
    }
}

fn default_db_path() -> Result<PathBuf, AccountError> {
    let home = std::env::var_os("HOME").ok_or_else(|| {
        AccountError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "$HOME not set; cannot pick accounts database path",
        ))
    })?;
    Ok(PathBuf::from(home)
        .join(".config/vaern/server/accounts.db"))
}

fn run_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS accounts (
            account_id    TEXT PRIMARY KEY,
            username      TEXT NOT NULL UNIQUE COLLATE NOCASE,
            password_hash TEXT NOT NULL,
            created_at    INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS characters (
            character_id   TEXT PRIMARY KEY,
            account_id     TEXT NOT NULL REFERENCES accounts(account_id) ON DELETE CASCADE,
            character_name TEXT NOT NULL UNIQUE COLLATE NOCASE,
            created_at     INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_characters_account_id ON characters(account_id);
        ",
    )
}

fn validate_username(username: &str) -> Result<(), AccountError> {
    if username.len() < MIN_USERNAME_LEN {
        return Err(AccountError::InvalidUsername("too short"));
    }
    if username.len() > MAX_USERNAME_LEN {
        return Err(AccountError::InvalidUsername("too long"));
    }
    if !username
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(AccountError::InvalidUsername(
            "only ascii alphanumerics, '_', and '-' allowed",
        ));
    }
    Ok(())
}

fn validate_password(password: &str) -> Result<(), AccountError> {
    if password.len() < MIN_PASSWORD_LEN {
        return Err(AccountError::InvalidPassword("too short"));
    }
    Ok(())
}

fn validate_character_name(name: &str) -> Result<(), AccountError> {
    let trimmed = name.trim();
    if trimmed.len() < MIN_CHARACTER_NAME_LEN {
        return Err(AccountError::InvalidCharacterName("too short"));
    }
    if trimmed.len() > MAX_CHARACTER_NAME_LEN {
        return Err(AccountError::InvalidCharacterName("too long"));
    }
    if trimmed != name {
        return Err(AccountError::InvalidCharacterName(
            "no leading or trailing whitespace",
        ));
    }
    Ok(())
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_store() -> (AccountStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = AccountStore::open_at(&dir.path().join("accounts.db")).unwrap();
        (store, dir)
    }

    #[test]
    fn register_round_trips_and_authenticates_with_correct_password() {
        let (store, _dir) = temp_store();
        let id = store.register("alice", "hunter2").unwrap();
        let id2 = store.authenticate("alice", "hunter2").unwrap();
        assert_eq!(id, id2);
    }

    #[test]
    fn authenticate_is_case_insensitive_on_username() {
        let (store, _dir) = temp_store();
        let id = store.register("Alice", "hunter2").unwrap();
        // Same row regardless of case.
        let id2 = store.authenticate("alice", "hunter2").unwrap();
        assert_eq!(id, id2);
    }

    #[test]
    fn register_rejects_duplicate_username_case_insensitive() {
        let (store, _dir) = temp_store();
        store.register("alice", "hunter2").unwrap();
        let err = store.register("ALICE", "different").unwrap_err();
        assert!(matches!(err, AccountError::UsernameTaken(_)), "got {err:?}");
    }

    #[test]
    fn authenticate_fails_on_wrong_password() {
        let (store, _dir) = temp_store();
        store.register("alice", "hunter2").unwrap();
        let err = store.authenticate("alice", "different").unwrap_err();
        assert!(matches!(err, AccountError::WrongPassword), "got {err:?}");
    }

    #[test]
    fn authenticate_unknown_user_returns_not_found() {
        let (store, _dir) = temp_store();
        let err = store.authenticate("nobody", "anything").unwrap_err();
        assert!(matches!(err, AccountError::NotFound), "got {err:?}");
    }

    #[test]
    fn register_rejects_short_username() {
        let (store, _dir) = temp_store();
        let err = store.register("a", "hunter2").unwrap_err();
        assert!(matches!(err, AccountError::InvalidUsername(_)), "got {err:?}");
    }

    #[test]
    fn register_rejects_short_password() {
        let (store, _dir) = temp_store();
        let err = store.register("alice", "ab").unwrap_err();
        assert!(matches!(err, AccountError::InvalidPassword(_)), "got {err:?}");
    }

    #[test]
    fn register_rejects_funky_username_chars() {
        let (store, _dir) = temp_store();
        let err = store.register("al ice", "hunter2").unwrap_err();
        assert!(matches!(err, AccountError::InvalidUsername(_)), "got {err:?}");
        let err = store.register("al/ice", "hunter2").unwrap_err();
        assert!(matches!(err, AccountError::InvalidUsername(_)), "got {err:?}");
    }

    #[test]
    fn create_character_enforces_name_uniqueness() {
        let (store, _dir) = temp_store();
        let alice = store.register("alice", "hunter2").unwrap();
        let bob = store.register("bob", "hunter2").unwrap();
        let cid_a = Uuid::new_v4().to_string();
        let cid_b = Uuid::new_v4().to_string();
        store.create_character(&alice, &cid_a, "Telyn").unwrap();
        // Different account, different character_id, but SAME name —
        // server-wide uniqueness should reject.
        let err = store
            .create_character(&bob, &cid_b, "telyn")
            .unwrap_err();
        assert!(matches!(err, AccountError::NameTaken(_)), "got {err:?}");
    }

    #[test]
    fn list_characters_returns_only_owned_characters() {
        let (store, _dir) = temp_store();
        let alice = store.register("alice", "hunter2").unwrap();
        let bob = store.register("bob", "hunter2").unwrap();
        let cid_a1 = Uuid::new_v4().to_string();
        let cid_a2 = Uuid::new_v4().to_string();
        let cid_b1 = Uuid::new_v4().to_string();
        store.create_character(&alice, &cid_a1, "Telyn").unwrap();
        store.create_character(&alice, &cid_a2, "Brenn").unwrap();
        store.create_character(&bob, &cid_b1, "Halen").unwrap();

        let alice_chars = store.list_characters(&alice).unwrap();
        assert_eq!(alice_chars.len(), 2);
        let names: Vec<&str> = alice_chars
            .iter()
            .map(|c| c.character_name.as_str())
            .collect();
        assert!(names.contains(&"Telyn"));
        assert!(names.contains(&"Brenn"));
        assert!(!names.contains(&"Halen"));

        let bob_chars = store.list_characters(&bob).unwrap();
        assert_eq!(bob_chars.len(), 1);
        assert_eq!(bob_chars[0].character_name, "Halen");
    }

    #[test]
    fn create_character_rejects_blank_name() {
        let (store, _dir) = temp_store();
        let alice = store.register("alice", "hunter2").unwrap();
        let cid = Uuid::new_v4().to_string();
        let err = store.create_character(&alice, &cid, " ").unwrap_err();
        assert!(matches!(err, AccountError::InvalidCharacterName(_)), "got {err:?}");
    }

    #[test]
    fn account_for_character_lookup() {
        let (store, _dir) = temp_store();
        let alice = store.register("alice", "hunter2").unwrap();
        let cid = Uuid::new_v4().to_string();
        store.create_character(&alice, &cid, "Telyn").unwrap();
        let owner = store.account_for_character(&cid).unwrap();
        assert_eq!(owner, Some(alice));
        let missing = store.account_for_character("nonexistent").unwrap();
        assert_eq!(missing, None);
    }

    #[test]
    fn schema_is_idempotent() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("accounts.db");
        // Open + write something + close + reopen — no errors.
        {
            let s = AccountStore::open_at(&path).unwrap();
            s.register("alice", "hunter2").unwrap();
        }
        let s = AccountStore::open_at(&path).unwrap();
        let id = s.authenticate("alice", "hunter2").unwrap();
        assert!(!id.0.is_empty());
    }
}
