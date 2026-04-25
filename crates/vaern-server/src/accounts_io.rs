//! Auth message handling on the server. Drains `ClientLogin`,
//! `ClientRegister`, and `ClientCreateCharacter` from each link and
//! ships the corresponding `LoginResult` / `RegisterResult` /
//! `CreateCharacterResult` back. On a successful login or register,
//! attaches an `AuthedAccount` component on the link so
//! `process_pending_spawns` can gate the eventual `ClientHello` spawn.

use bevy::prelude::*;
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use uuid::Uuid;
use vaern_protocol::{
    Channel1, CharacterSummary, ClientCreateCharacter, ClientLogin, ClientRegister,
    CreateCharacterResult, LoginResult, RegisterResult,
};

use crate::accounts::{AccountError, AccountId, AccountStore};
use crate::persistence::CharacterStore;

/// Component attached to a `ClientOf` link entity once that client has
/// successfully logged in or registered. `process_pending_spawns`
/// consults this before honoring `ClientHello`.
#[derive(Component, Debug, Clone)]
pub struct AuthedAccount {
    pub account_id: AccountId,
    pub username: String,
}

/// Server-side auth gating policy. Loaded from `VAERN_REQUIRE_AUTH` at
/// startup: `=1` enforces the AuthedAccount gate on `ClientHello`;
/// anything else (including unset) preserves today's behavior — a fresh
/// link can ClientHello → spawn without first sending Login.
///
/// Phase 1 of Slice 8e ships with the default OFF so the existing dev
/// loop and run-multiplayer.sh keep working. Phase 2 (login UI client-
/// side) flips the dev script to `VAERN_REQUIRE_AUTH=1` and shipping
/// builds enforce auth.
#[derive(Resource, Debug, Clone, Copy)]
pub struct ServerAuthConfig {
    pub require_auth: bool,
}

impl ServerAuthConfig {
    pub fn from_env() -> Self {
        let require_auth = std::env::var("VAERN_REQUIRE_AUTH")
            .map(|s| s == "1" || s.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        Self { require_auth }
    }
}

/// Drain login / register / create-character messages from every
/// `ClientOf` link and respond. On success, attach `AuthedAccount` to
/// the link entity.
pub fn handle_auth_messages(
    store: Res<AccountStore>,
    character_store: Res<CharacterStore>,
    mut commands: Commands,
    mut login_rx: Query<(Entity, &mut MessageReceiver<ClientLogin>), With<ClientOf>>,
    mut register_rx: Query<(Entity, &mut MessageReceiver<ClientRegister>), With<ClientOf>>,
    mut create_rx: Query<(Entity, &mut MessageReceiver<ClientCreateCharacter>), With<ClientOf>>,
    mut login_tx: Query<&mut MessageSender<LoginResult>, With<ClientOf>>,
    mut register_tx: Query<&mut MessageSender<RegisterResult>, With<ClientOf>>,
    mut create_tx: Query<&mut MessageSender<CreateCharacterResult>, With<ClientOf>>,
    authed: Query<&AuthedAccount>,
) {
    // ── login ────────────────────────────────────────────────────────
    for (link, mut rx) in &mut login_rx {
        for msg in rx.receive() {
            let result = process_login(&store, &character_store, &msg, link, &authed);
            apply_login_result(&mut commands, &mut login_tx, link, result);
        }
    }

    // ── register ─────────────────────────────────────────────────────
    for (link, mut rx) in &mut register_rx {
        for msg in rx.receive() {
            let result = process_register(&store, &msg, link, &authed);
            apply_register_result(&mut commands, &mut register_tx, link, result);
        }
    }

    // ── create character ─────────────────────────────────────────────
    for (link, mut rx) in &mut create_rx {
        for msg in rx.receive() {
            let result = process_create_character(&store, &msg, link, &authed);
            if let Ok(mut sender) = create_tx.get_mut(link) {
                let _ = sender.send::<Channel1>(result);
            }
        }
    }
}

/// Result of a login attempt that the caller will translate into a
/// `LoginResult` message + side effects.
struct LoginOutcome {
    ok: bool,
    error_msg: String,
    account_id: Option<AccountId>,
    username: String,
    characters: Vec<CharacterSummary>,
}

fn process_login(
    store: &AccountStore,
    character_store: &CharacterStore,
    msg: &ClientLogin,
    link: Entity,
    authed: &Query<&AuthedAccount>,
) -> LoginOutcome {
    if authed.get(link).is_ok() {
        return LoginOutcome {
            ok: false,
            error_msg: "already logged in".to_string(),
            account_id: None,
            username: msg.username.clone(),
            characters: vec![],
        };
    }
    match store.authenticate(&msg.username, &msg.password) {
        Ok(account_id) => {
            let chars = store
                .list_characters(&account_id)
                .unwrap_or_default()
                .into_iter()
                .map(|c| build_character_summary(character_store, c))
                .collect();
            LoginOutcome {
                ok: true,
                error_msg: String::new(),
                account_id: Some(account_id),
                username: msg.username.clone(),
                characters: chars,
            }
        }
        Err(e) => LoginOutcome {
            ok: false,
            error_msg: client_facing_error(&e),
            account_id: None,
            username: msg.username.clone(),
            characters: vec![],
        },
    }
}

/// Resolve a `CharacterRow` (from accounts.db) into a wire-side
/// `CharacterSummary` by joining with the on-disk `PersistedCharacter`
/// JSON. Falls back to placeholder fields when the JSON is missing or
/// can't parse — the row still appears in the roster (so the user can
/// see their stranded character) just with `?` race and L0.
fn build_character_summary(
    character_store: &CharacterStore,
    row: crate::accounts::CharacterRow,
) -> CharacterSummary {
    match Uuid::parse_str(&row.character_id)
        .ok()
        .and_then(|uuid| character_store.load(uuid).ok())
    {
        Some(persisted) => CharacterSummary {
            character_id: row.character_id,
            name: row.character_name,
            race_id: persisted.race_id,
            core_pillar: persisted.core_pillar,
            level: persisted.experience.level,
        },
        None => CharacterSummary {
            character_id: row.character_id,
            name: row.character_name,
            race_id: String::new(),
            core_pillar: vaern_core::Pillar::Might,
            level: 0,
        },
    }
}

fn apply_login_result(
    commands: &mut Commands,
    senders: &mut Query<&mut MessageSender<LoginResult>, With<ClientOf>>,
    link: Entity,
    outcome: LoginOutcome,
) {
    if outcome.ok
        && let Some(ref account_id) = outcome.account_id
    {
        commands.entity(link).insert(AuthedAccount {
            account_id: account_id.clone(),
            username: outcome.username.clone(),
        });
        info!(
            "[auth] login ok: link={link:?} username={} account={}",
            outcome.username, account_id
        );
    } else {
        warn!(
            "[auth] login refused: link={link:?} username={} reason={}",
            outcome.username, outcome.error_msg
        );
    }
    if let Ok(mut sender) = senders.get_mut(link) {
        let _ = sender.send::<Channel1>(LoginResult {
            ok: outcome.ok,
            error_msg: outcome.error_msg,
            characters: outcome.characters,
        });
    }
}

struct RegisterOutcome {
    ok: bool,
    error_msg: String,
    account_id: Option<AccountId>,
    username: String,
}

fn process_register(
    store: &AccountStore,
    msg: &ClientRegister,
    link: Entity,
    authed: &Query<&AuthedAccount>,
) -> RegisterOutcome {
    if authed.get(link).is_ok() {
        return RegisterOutcome {
            ok: false,
            error_msg: "already logged in".to_string(),
            account_id: None,
            username: msg.username.clone(),
        };
    }
    match store.register(&msg.username, &msg.password) {
        Ok(account_id) => RegisterOutcome {
            ok: true,
            error_msg: String::new(),
            account_id: Some(account_id),
            username: msg.username.clone(),
        },
        Err(e) => RegisterOutcome {
            ok: false,
            error_msg: client_facing_error(&e),
            account_id: None,
            username: msg.username.clone(),
        },
    }
}

fn apply_register_result(
    commands: &mut Commands,
    senders: &mut Query<&mut MessageSender<RegisterResult>, With<ClientOf>>,
    link: Entity,
    outcome: RegisterOutcome,
) {
    if outcome.ok
        && let Some(ref account_id) = outcome.account_id
    {
        commands.entity(link).insert(AuthedAccount {
            account_id: account_id.clone(),
            username: outcome.username.clone(),
        });
        info!(
            "[auth] register ok: link={link:?} username={} account={}",
            outcome.username, account_id
        );
    } else {
        warn!(
            "[auth] register refused: link={link:?} username={} reason={}",
            outcome.username, outcome.error_msg
        );
    }
    if let Ok(mut sender) = senders.get_mut(link) {
        let _ = sender.send::<Channel1>(RegisterResult {
            ok: outcome.ok,
            error_msg: outcome.error_msg,
        });
    }
}

fn process_create_character(
    store: &AccountStore,
    msg: &ClientCreateCharacter,
    link: Entity,
    authed: &Query<&AuthedAccount>,
) -> CreateCharacterResult {
    let Ok(account) = authed.get(link) else {
        return CreateCharacterResult {
            ok: false,
            error_msg: "must be logged in to create a character".to_string(),
            character_id: String::new(),
        };
    };
    let character_id = Uuid::new_v4().to_string();
    match store.create_character(&account.account_id, &character_id, &msg.name) {
        Ok(()) => {
            info!(
                "[auth] create_character ok: account={} name={:?} character_id={}",
                account.account_id, msg.name, character_id
            );
            CreateCharacterResult {
                ok: true,
                error_msg: String::new(),
                character_id,
            }
        }
        Err(e) => {
            warn!(
                "[auth] create_character refused: account={} name={:?} reason={}",
                account.account_id, msg.name, e
            );
            CreateCharacterResult {
                ok: false,
                error_msg: client_facing_error(&e),
                character_id: String::new(),
            }
        }
    }
}

/// Map server-side `AccountError` to a short string the client can show
/// in-line. We deliberately collapse `NotFound` and `WrongPassword` into
/// the same "wrong username or password" message so the login form
/// doesn't leak which usernames exist on the server.
fn client_facing_error(e: &AccountError) -> String {
    match e {
        AccountError::UsernameTaken(_) => "username already taken".to_string(),
        AccountError::NameTaken(_) => "character name already taken".to_string(),
        AccountError::WrongPassword | AccountError::NotFound => {
            "wrong username or password".to_string()
        }
        AccountError::InvalidUsername(why) => format!("invalid username: {why}"),
        AccountError::InvalidPassword(why) => format!("invalid password: {why}"),
        AccountError::InvalidCharacterName(why) => format!("invalid character name: {why}"),
        AccountError::Sql(_) | AccountError::Hash(_) | AccountError::Io(_) => {
            "server error — try again".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::CharacterRow;
    use tempfile::TempDir;
    use vaern_character::Experience;
    use vaern_core::pillar::Pillar;
    use vaern_equipment::Equipped;
    use vaern_inventory::{ConsumableBelt, PlayerInventory};
    use vaern_persistence::{
        PersistedCharacter, PersistedCosmetics, PersistedQuestLog, SCHEMA_VERSION,
        ServerCharacterStore,
    };
    use vaern_professions::ProfessionSkills;
    use vaern_stats::{PillarCaps, PillarScores, PillarXp};

    fn temp_character_store() -> (CharacterStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let inner = ServerCharacterStore::open(dir.path().into()).unwrap();
        (CharacterStore(inner), dir)
    }

    fn sample_character(
        uuid: Uuid,
        name: &str,
        race_id: &str,
        pillar: Pillar,
        level: u32,
    ) -> PersistedCharacter {
        PersistedCharacter {
            schema_version: SCHEMA_VERSION,
            character_id: uuid.to_string(),
            name: name.into(),
            race_id: race_id.into(),
            core_pillar: pillar,
            cosmetics: PersistedCosmetics::default(),
            experience: Experience { current: 0, level },
            pillar_scores: PillarScores::default(),
            pillar_caps: PillarCaps::default(),
            pillar_xp: PillarXp::default(),
            inventory: PlayerInventory::default(),
            equipped: Equipped::default(),
            belt: ConsumableBelt::default(),
            professions: ProfessionSkills::default(),
            wallet_copper: 0,
            quest_log: PersistedQuestLog::default(),
            position: None,
            yaw_rad: None,
            created_at: 1_714_000_000,
            updated_at: 1_714_000_000,
        }
    }

    #[test]
    fn build_character_summary_populates_from_persisted() {
        let (store, _dir) = temp_character_store();
        let uuid = Uuid::new_v4();
        let chr = sample_character(uuid, "Telyn", "hearthkin", Pillar::Finesse, 7);
        store.save(uuid, &chr).expect("save");

        let row = CharacterRow {
            character_id: uuid.to_string(),
            character_name: "Telyn".into(),
            created_at: 0,
        };
        let summary = build_character_summary(&store, row);

        assert_eq!(summary.character_id, uuid.to_string());
        assert_eq!(summary.name, "Telyn");
        assert_eq!(summary.race_id, "hearthkin");
        assert_eq!(summary.core_pillar, Pillar::Finesse);
        assert_eq!(summary.level, 7);
    }

    #[test]
    fn build_character_summary_falls_back_when_persisted_file_missing() {
        // Orphan row — DB has the row but no on-disk JSON. The summary
        // should still appear with placeholder fields rather than dropping
        // the row or panicking.
        let (store, _dir) = temp_character_store();
        let uuid = Uuid::new_v4();

        let row = CharacterRow {
            character_id: uuid.to_string(),
            character_name: "Stranded".into(),
            created_at: 0,
        };
        let summary = build_character_summary(&store, row);

        assert_eq!(summary.character_id, uuid.to_string());
        assert_eq!(summary.name, "Stranded");
        assert_eq!(summary.race_id, "");
        assert_eq!(summary.core_pillar, Pillar::Might);
        assert_eq!(summary.level, 0);
    }

    #[test]
    fn build_character_summary_falls_back_when_character_id_unparsable() {
        // Defensive: row exists but its `character_id` isn't a valid UUID
        // (shouldn't happen via accounts.rs but the build_character_summary
        // helper must not panic on bad input).
        let (store, _dir) = temp_character_store();

        let row = CharacterRow {
            character_id: "not-a-uuid".into(),
            character_name: "Garbage".into(),
            created_at: 0,
        };
        let summary = build_character_summary(&store, row);

        assert_eq!(summary.character_id, "not-a-uuid");
        assert_eq!(summary.race_id, "");
        assert_eq!(summary.core_pillar, Pillar::Might);
        assert_eq!(summary.level, 0);
    }
}
