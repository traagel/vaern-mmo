//! Main-menu / character-create / logout UI. Uses bevy_egui.
//!
//! States:
//!   MainMenu      — connect form + character create. No networking yet.
//!   Connecting    — lightyear Client entity spawned; waiting for Connected.
//!   InGame        — scene up, combat running. Top-right Menu button → Logout.
//!
//! Character persistence is intentionally simple: an in-memory list seeded
//! from `~/.config/vaern/characters.json` on startup, re-saved on create/play.

use std::{
    fs,
    path::{Path, PathBuf},
};

use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};
use lightyear::prelude::client::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use vaern_assets::quaternius::{
    Beard as QBeard, ColorVariant as QColor, Hair as QHair, HeadPiece as QHeadPiece,
    Outfit as QOutfit,
};
use vaern_assets::Gender;
use vaern_core::pillar::Pillar;
use vaern_persistence::PersistedCosmetics;

/// Fixed menu order — matches the enum variant order so `pillar_index`
/// maps to `PILLARS[index]` deterministically.
const PILLARS: [Pillar; 3] = [Pillar::Might, Pillar::Finesse, Pillar::Arcana];

#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    #[default]
    MainMenu,
    Connecting,
    /// Connected to the server, awaiting a `LoginResult` /
    /// `RegisterResult` after sending `ClientLogin` /
    /// `ClientRegister`. Only entered when account-gated login is
    /// active (VAERN_USE_ACCOUNTS=1). On success, we transition to
    /// `CharacterSelect`; on failure, we tear down the connection and
    /// return to `MainMenu` with an inline error.
    Authenticating,
    /// Authed; the client knows the server-driven character roster and
    /// shows the pick-or-create UI. Selecting a character sends
    /// `ClientHello` and transitions to `InGame`. "New character" sends
    /// `ClientCreateCharacter`; the result appends to the roster.
    CharacterSelect,
    InGame,
    /// Server connection lost mid-game. The retry loop in
    /// `net::reconnect_tick` re-spawns the lightyear client with
    /// exponential backoff. On success returns to `InGame`; on
    /// max-attempts-exhausted falls back to `MainMenu`.
    Reconnecting,
}

// ─── character data ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedCharacter {
    pub name: String,
    pub race_id: String,
    pub core_pillar: Pillar,
    /// Cosmetic body type for the Quaternius character mesh.
    /// Defaults to Male on saves written before the gender picker was added.
    #[serde(default = "default_gender")]
    pub gender: Gender,
    /// Stable per-character UUID. Minted on Save Character click; shared
    /// with the server in `ClientHello.character_id`. Old save files
    /// without this field get a fresh UUID on next save.
    #[serde(default)]
    pub character_id: String,
    /// Cosmetic picks from char-create (hair / beard / outfit slots /
    /// color). Server persists verbatim on first login; re-logins ignore
    /// this field (server's on-disk copy wins).
    #[serde(default)]
    pub cosmetics: PersistedCosmetics,
}

fn default_gender() -> Gender {
    Gender::Male
}

#[derive(Resource, Debug, Clone, Default, Serialize, Deserialize)]
pub struct CharacterStore {
    pub characters: Vec<SavedCharacter>,
}

impl CharacterStore {
    fn path() -> PathBuf {
        dirs_like_home()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".config")
            .join("vaern")
            .join("characters.json")
    }

    fn load() -> Self {
        let p = Self::path();
        match fs::read_to_string(&p) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    fn save(&self) {
        let p = Self::path();
        if let Some(dir) = p.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = fs::write(p, s);
        }
    }
}

fn dirs_like_home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Populated at startup; read by `send_hello_on_connect` (in net.rs) to
/// choose the pillar for the ClientHello message, and by the scene
/// render path to drive the Quaternius mesh's gender.
#[derive(Resource, Debug, Clone)]
pub struct SelectedCharacter {
    pub name: String,
    pub race_id: String,
    pub core_pillar: Pillar,
    pub gender: Gender,
    /// UUID the server uses as the persisted save-file stem. Empty if
    /// the character came from a legacy file that predates PR3 and
    /// wasn't re-saved yet — server falls back to anonymous in that
    /// case (no save).
    pub character_id: String,
    pub cosmetics: PersistedCosmetics,
}

// ─── menu form state ────────────────────────────────────────────────────────

#[derive(Resource)]
pub struct MenuState {
    pub server_addr: String,
    pub character_name: String,
    pub race_index: usize,
    /// Index into `PILLARS` (0 = Might, 1 = Finesse, 2 = Arcana).
    pub pillar_index: usize,
    /// Cosmetic gender for the new character being authored in the form.
    pub gender: Gender,
    /// `(race_id, display_name, faction)` pulled from src/generated/races
    pub races: Vec<RaceOption>,
    /// In-game menu modal open?
    pub in_game_modal: bool,
    pub selected_existing: Option<usize>,
    /// Slice 8e auth form fields. Only used when the user is opting into
    /// the server-account flow; the legacy local-JSON flow ignores them.
    pub auth_username: String,
    pub auth_password: String,
    pub auth_error: String,
    // Quaternius cosmetic sliders (mirror museum's composer picks).
    pub q_body: Option<QOutfit>,
    pub q_legs: Option<QOutfit>,
    pub q_arms: Option<QOutfit>,
    pub q_feet: Option<QOutfit>,
    pub q_head_piece: Option<QHeadPiece>,
    pub q_hair: Option<QHair>,
    pub q_beard: Option<QBeard>,
    pub q_color: QColor,
}

impl MenuState {
    /// Snapshot the form's cosmetic sliders into a `PersistedCosmetics`.
    /// Used on Save Character + Enter World to freeze the current picks
    /// into the saved / transmitted payload.
    fn cosmetics_from_form(&self) -> PersistedCosmetics {
        let mk = |o: Option<QOutfit>| o.map(|outfit| vaern_assets::OutfitSlot::new(outfit, self.q_color));
        PersistedCosmetics::from_parts(
            self.gender,
            mk(self.q_body),
            mk(self.q_legs),
            mk(self.q_arms),
            mk(self.q_feet),
            self.q_head_piece.map(|p| vaern_assets::HeadSlot::new(p, self.q_color)),
            self.q_hair,
            self.q_beard,
        )
    }
}

#[derive(Clone)]
pub struct RaceOption {
    pub id: String,
    pub display: String,
    pub faction: String,
    pub archetype: String,
}

fn generated_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../src/generated")
}

fn load_menu_options() -> Vec<RaceOption> {
    let root = generated_root();
    vaern_data::load_races(root.join("races"))
        .map(|vs| {
            vs.into_iter()
                .map(|r| RaceOption {
                    display: prettify(&r.id),
                    id: r.id,
                    faction: r.faction,
                    archetype: r.archetype,
                })
                .collect()
        })
        .unwrap_or_default()
}

fn prettify(id: &str) -> String {
    id.split('_')
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(c) => c.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ─── plugin ─────────────────────────────────────────────────────────────────

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        let races = load_menu_options();
        let store = CharacterStore::load();

        app.add_plugins(EguiPlugin::default())
            .init_state::<AppState>()
            .insert_resource(MenuState {
                server_addr: "127.0.0.1:5000".to_string(),
                character_name: String::new(),
                race_index: 0,
                pillar_index: 0,
                gender: Gender::Male,
                races,
                in_game_modal: false,
                selected_existing: None,
                auth_username: String::new(),
                auth_password: String::new(),
                auth_error: String::new(),
                q_body: None,
                q_legs: None,
                q_arms: None,
                q_feet: None,
                q_head_piece: None,
                q_hair: None,
                q_beard: None,
                q_color: QColor::Default,
            })
            .insert_resource(store)
            .add_systems(
                EguiPrimaryContextPass,
                (
                    main_menu_ui.run_if(in_state(AppState::MainMenu)),
                    connecting_ui.run_if(in_state(AppState::Connecting)),
                    authenticating_ui.run_if(in_state(AppState::Authenticating)),
                    character_select_ui.run_if(in_state(AppState::CharacterSelect)),
                    reconnecting_ui.run_if(in_state(AppState::Reconnecting)),
                    in_game_menu_ui.run_if(in_state(AppState::InGame)),
                ),
            )
            .add_systems(
                Update,
                detect_connected.run_if(in_state(AppState::Connecting)),
            );
    }
}

// ─── UI systems ─────────────────────────────────────────────────────────────

fn main_menu_ui(
    mut contexts: EguiContexts,
    mut menu: ResMut<MenuState>,
    mut store: ResMut<CharacterStore>,
    mut next_state: ResMut<NextState<AppState>>,
    mut commands: Commands,
) {
    let ctx = match contexts.ctx_mut() {
        Ok(c) => c,
        Err(_) => return,
    };
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);
            ui.heading(egui::RichText::new("Vaern").size(42.0));
            ui.label(
                egui::RichText::new("hardcore coop-MMO — scaffold build")
                    .italics()
                    .color(egui::Color32::from_gray(160)),
            );
            ui.add_space(24.0);
        });

        // ── Server account (Slice 8e) ─────────────────────────────────
        // Optional. Leave blank to use the legacy local-JSON flow.
        ui.group(|ui| {
            ui.heading("Server account (optional)");
            ui.add_space(4.0);
            if !menu.auth_error.is_empty() {
                ui.colored_label(egui::Color32::from_rgb(220, 90, 90), &menu.auth_error);
                ui.add_space(4.0);
            }
            ui.horizontal(|ui| {
                ui.label("Username:");
                ui.add(
                    egui::TextEdit::singleline(&mut menu.auth_username).desired_width(140.0),
                );
                ui.label("Password:");
                ui.add(
                    egui::TextEdit::singleline(&mut menu.auth_password)
                        .password(true)
                        .desired_width(140.0),
                );
                let can_auth = !menu.auth_username.trim().is_empty()
                    && !menu.auth_password.is_empty();
                if ui
                    .add_enabled(can_auth, egui::Button::new("Login"))
                    .clicked()
                {
                    commands.insert_resource(crate::net::ClientCredentials {
                        username: menu.auth_username.trim().to_string(),
                        password: menu.auth_password.clone(),
                        register_instead: false,
                    });
                    menu.auth_error.clear();
                    next_state.set(AppState::Connecting);
                }
                if ui
                    .add_enabled(can_auth, egui::Button::new("Register"))
                    .clicked()
                {
                    commands.insert_resource(crate::net::ClientCredentials {
                        username: menu.auth_username.trim().to_string(),
                        password: menu.auth_password.clone(),
                        register_instead: true,
                    });
                    menu.auth_error.clear();
                    next_state.set(AppState::Connecting);
                }
            });
            ui.label(
                egui::RichText::new(
                    "Server-side accounts (Slice 8e). Leave blank + use legacy flow below for the dev loop.",
                )
                .small()
                .color(egui::Color32::from_gray(140)),
            );
        });
        ui.add_space(12.0);

        ui.columns(2, |cols| {
            // ── Left: existing characters ──────────────────────────────────
            cols[0].group(|ui| {
                ui.heading("Characters");
                ui.add_space(4.0);
                if store.characters.is_empty() {
                    ui.label(egui::RichText::new("(none yet — create one →)").italics());
                }
                let mut to_delete: Option<usize> = None;
                for (i, ch) in store.characters.iter().enumerate() {
                    let selected = menu.selected_existing == Some(i);
                    ui.horizontal(|ui| {
                        if ui
                            .selectable_label(
                                selected,
                                format!(
                                    "{} · {} · {}",
                                    ch.name,
                                    prettify(&ch.race_id),
                                    pillar_display(ch.core_pillar),
                                ),
                            )
                            .clicked()
                        {
                            menu.selected_existing = Some(i);
                            menu.character_name = ch.name.clone();
                        }
                        if ui.small_button("✕").clicked() {
                            to_delete = Some(i);
                        }
                    });
                }
                if let Some(i) = to_delete {
                    store.characters.remove(i);
                    store.save();
                    if menu.selected_existing == Some(i) {
                        menu.selected_existing = None;
                    }
                }
            });

            // ── Right: create / connect ────────────────────────────────────
            cols[1].group(|ui| {
                ui.heading("Create character");
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.text_edit_singleline(&mut menu.character_name);
                });
                ui.add_space(4.0);
                race_picker(ui, &mut menu);
                ui.add_space(4.0);
                gender_picker(ui, &mut menu);
                ui.add_space(4.0);
                pillar_picker(ui, &mut menu);
                ui.add_space(4.0);
                appearance_picker(ui, &mut menu);
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(
                        "Note: you commit to a pillar only. Archetype / Order\n\
                         unlocks emerge through play.",
                    )
                    .italics()
                    .color(egui::Color32::from_gray(150)),
                );
            });
        });

        ui.add_space(16.0);
        ui.separator();
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.label("Server:");
            ui.add(
                egui::TextEdit::singleline(&mut menu.server_addr)
                    .desired_width(200.0),
            );
            ui.add_space(24.0);

            // Save + play
            let can_save = !menu.character_name.trim().is_empty() && !menu.races.is_empty();
            if ui
                .add_enabled(can_save, egui::Button::new("Save Character"))
                .clicked()
            {
                let name = menu.character_name.trim().to_string();
                // Preserve a pre-existing UUID when upserting by name —
                // we don't want to orphan the server-side save file just
                // because the player clicked Save again.
                let existing_uuid = store
                    .characters
                    .iter()
                    .find(|c| c.name == name)
                    .and_then(|c| (!c.character_id.is_empty()).then(|| c.character_id.clone()));
                let character_id = existing_uuid.unwrap_or_else(|| Uuid::new_v4().to_string());
                let race = &menu.races[menu.race_index];
                let ch = SavedCharacter {
                    name,
                    race_id: race.id.clone(),
                    core_pillar: PILLARS[menu.pillar_index],
                    gender: menu.gender,
                    character_id,
                    cosmetics: menu.cosmetics_from_form(),
                };
                if let Some(existing) = store.characters.iter_mut().find(|c| c.name == ch.name) {
                    *existing = ch;
                } else {
                    store.characters.push(ch);
                }
                store.save();
            }

            // Enter world
            let target = selected_to_play(&menu, &store);
            if ui
                .add_enabled(target.is_some(), egui::Button::new("Enter World →"))
                .clicked()
            {
                if let Some(ch) = target {
                    commands.insert_resource(SelectedCharacter {
                        name: ch.name.clone(),
                        race_id: ch.race_id.clone(),
                        core_pillar: ch.core_pillar,
                        gender: ch.gender,
                        character_id: ch.character_id.clone(),
                        cosmetics: ch.cosmetics.clone(),
                    });
                    next_state.set(AppState::Connecting);
                }
            }
        });
    });
}

/// egui section for the Quaternius cosmetic picks. Mirrors the museum
/// composer's `ui_rig` block — 4 outfit slots + head piece + hair +
/// optional beard + color strip. Hair is gender-filtered.
fn appearance_picker(ui: &mut egui::Ui, menu: &mut MenuState) {
    egui::CollapsingHeader::new("Appearance")
        .default_open(false)
        .show(ui, |ui| {
            ui_slot_slider(ui, "Body", &mut menu.q_body, QOutfit::ALL, |o| o.label());
            ui_slot_slider(ui, "Legs", &mut menu.q_legs, QOutfit::ALL, |o| o.label());
            ui_slot_slider(ui, "Arms", &mut menu.q_arms, QOutfit::ALL, |o| o.label());
            ui_slot_slider(ui, "Feet", &mut menu.q_feet, QOutfit::ALL, |o| o.label());
            ui_slot_slider(
                ui,
                "Head Piece",
                &mut menu.q_head_piece,
                QHeadPiece::ALL,
                |hp| hp.label(),
            );

            let hair_options = QHair::available_for(menu.gender);
            // If gender changed since last frame, a previously-picked
            // hair style may no longer be in the gender's option list —
            // drop it so the slider doesn't render a stale label.
            if let Some(h) = menu.q_hair {
                if !hair_options.contains(&h) {
                    menu.q_hair = None;
                }
            }
            ui_slot_slider(ui, "Hair", &mut menu.q_hair, hair_options, |h| h.label());

            if menu.gender == Gender::Male {
                ui_slot_slider(ui, "Beard", &mut menu.q_beard, QBeard::ALL, |b| b.label());
            } else if menu.q_beard.is_some() {
                menu.q_beard = None;
            }

            ui.horizontal(|ui| {
                ui.label("Color:");
                for &c in QColor::ALL {
                    let selected = menu.q_color == c;
                    if ui.selectable_label(selected, c.label()).clicked() && !selected {
                        menu.q_color = c;
                    }
                }
            });
        });
}

/// Slider over `options` where `0 = None`. Lifted verbatim from the
/// museum's `ui_slot_slider`; kept local here (rather than promoted to
/// vaern-assets) to avoid forcing bevy_egui into the assets crate.
fn ui_slot_slider<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut Option<T>,
    options: &[T],
    item_label: impl Fn(T) -> &'static str,
) {
    let max = options.len();
    let current = value
        .and_then(|v| options.iter().position(|o| *o == v).map(|i| i + 1))
        .unwrap_or(0);
    let mut idx = current;
    let item_text = if idx == 0 {
        "None".to_string()
    } else {
        options
            .get(idx - 1)
            .map(|v| item_label(*v).to_string())
            .unwrap_or_else(|| format!("#{idx}"))
    };
    ui.add(
        egui::Slider::new(&mut idx, 0..=max)
            .text(format!("{label}: {item_text}"))
            .integer(),
    );
    if idx != current {
        *value = if idx == 0 { None } else { options.get(idx - 1).copied() };
    }
}

/// If the user has an existing character selected, play that one.
/// Otherwise, if the form is filled, play from the form fields directly
/// (no UUID — the server will treat this session as anonymous until the
/// player clicks Save Character).
fn selected_to_play<'a>(menu: &'a MenuState, store: &'a CharacterStore) -> Option<SavedCharacter> {
    if let Some(i) = menu.selected_existing {
        if let Some(ch) = store.characters.get(i) {
            return Some(ch.clone());
        }
    }
    if menu.character_name.trim().is_empty() || menu.races.is_empty() {
        return None;
    }
    let race = &menu.races[menu.race_index];
    Some(SavedCharacter {
        name: menu.character_name.trim().to_string(),
        race_id: race.id.clone(),
        core_pillar: PILLARS[menu.pillar_index],
        gender: menu.gender,
        character_id: String::new(),
        cosmetics: menu.cosmetics_from_form(),
    })
}

fn race_picker(ui: &mut egui::Ui, menu: &mut MenuState) {
    if menu.races.is_empty() {
        ui.label(egui::RichText::new("races/ not loaded").color(egui::Color32::RED));
        return;
    }
    ui.horizontal(|ui| {
        ui.label("Race:");
        egui::ComboBox::from_id_salt("race_combo")
            .selected_text(&menu.races[menu.race_index].display)
            .show_ui(ui, |ui| {
                for i in 0..menu.races.len() {
                    let r = menu.races[i].clone();
                    ui.selectable_value(
                        &mut menu.race_index,
                        i,
                        format!("{} · {} ({})", r.display, r.archetype, short_faction(&r.faction)),
                    );
                }
            });
    });
}

fn gender_picker(ui: &mut egui::Ui, menu: &mut MenuState) {
    ui.horizontal(|ui| {
        ui.label("Body:");
        ui.selectable_value(&mut menu.gender, Gender::Male, "Male");
        ui.selectable_value(&mut menu.gender, Gender::Female, "Female");
    });
}

fn pillar_picker(ui: &mut egui::Ui, menu: &mut MenuState) {
    ui.horizontal(|ui| {
        ui.label("Pillar:");
        egui::ComboBox::from_id_salt("pillar_combo")
            .selected_text(pillar_display(PILLARS[menu.pillar_index]))
            .show_ui(ui, |ui| {
                for (i, pillar) in PILLARS.iter().enumerate() {
                    ui.selectable_value(
                        &mut menu.pillar_index,
                        i,
                        format!("{} — {}", pillar_display(*pillar), pillar_blurb(*pillar)),
                    );
                }
            });
    });
}

/// Short player-facing label for a Pillar. Kept separate from the
/// lowercase serde form so the menu can title-case it.
pub fn pillar_display(pillar: Pillar) -> &'static str {
    match pillar {
        Pillar::Might => "Might",
        Pillar::Finesse => "Finesse",
        Pillar::Arcana => "Arcana",
    }
}

fn pillar_blurb(pillar: Pillar) -> &'static str {
    match pillar {
        Pillar::Might => "armor, weapons, endurance",
        Pillar::Finesse => "stealth, precision, evasion",
        Pillar::Arcana => "spells, rituals, wards",
    }
}

fn short_faction(f: &str) -> &str {
    match f {
        "faction_a" => "Concord",
        "faction_b" => "Rend",
        _ => f,
    }
}

fn connecting_ui(mut contexts: EguiContexts) {
    let ctx = match contexts.ctx_mut() {
        Ok(c) => c,
        Err(_) => return,
    };
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(100.0);
            ui.heading("Connecting…");
            ui.add_space(12.0);
            ui.spinner();
        });
    });
}

/// UI shown while `AppState::Authenticating` is active. Spinner +
/// "Authenticating..." copy. Result-handling lives in
/// `net::drain_auth_results` which transitions us out.
fn authenticating_ui(mut contexts: EguiContexts) {
    let ctx = match contexts.ctx_mut() {
        Ok(c) => c,
        Err(_) => return,
    };
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(100.0);
            ui.heading("Authenticating…");
            ui.add_space(12.0);
            ui.spinner();
        });
    });
}

/// UI shown while `AppState::CharacterSelect` is active. Lists the
/// server-supplied character roster with a Play button per row, plus a
/// minimal "Create Character" form that ships `ClientCreateCharacter`
/// over the wire.
#[allow(clippy::too_many_arguments)]
fn character_select_ui(
    mut contexts: EguiContexts,
    mut menu: ResMut<MenuState>,
    mut roster: ResMut<crate::net::ServerCharacterRoster>,
    mut next_state: ResMut<NextState<AppState>>,
    mut create_tx_q: Query<
        &mut lightyear::prelude::MessageSender<vaern_protocol::ClientCreateCharacter>,
        With<lightyear::prelude::client::Client>,
    >,
    mut hello_tx_q: Query<
        &mut lightyear::prelude::MessageSender<vaern_protocol::ClientHello>,
        With<lightyear::prelude::client::Client>,
    >,
    mut commands: Commands,
) {
    let ctx = match contexts.ctx_mut() {
        Ok(c) => c,
        Err(_) => return,
    };
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(28.0);
            ui.heading(format!(
                "Welcome, {}",
                if roster.account_username.is_empty() {
                    "wanderer"
                } else {
                    roster.account_username.as_str()
                }
            ));
            ui.add_space(16.0);
        });

        if !roster.last_error.is_empty() {
            ui.colored_label(egui::Color32::from_rgb(220, 90, 90), &roster.last_error);
            ui.add_space(8.0);
        }

        ui.columns(2, |cols| {
            // ── Left: server-side character roster ───────────────────────
            cols[0].group(|ui| {
                ui.heading("Your characters");
                ui.add_space(4.0);
                if roster.characters.is_empty() {
                    ui.label(
                        egui::RichText::new("(no characters yet — create one →)")
                            .italics(),
                    );
                }
                let summaries = roster.characters.clone();
                for c in summaries {
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "{}  ·  {}  ·  {}  ·  L{}",
                            if c.name.is_empty() {
                                "(unnamed)".to_string()
                            } else {
                                c.name.clone()
                            },
                            if c.race_id.is_empty() {
                                "?".to_string()
                            } else {
                                prettify(&c.race_id)
                            },
                            pillar_display(c.core_pillar),
                            c.level
                        ));
                        if ui.button("▶ Play").clicked() {
                            // Ship ClientHello with this character_id.
                            // Server resolves via PersistedCharacter file.
                            let cosmetics = if c.race_id.is_empty() {
                                None
                            } else {
                                Some(PersistedCosmetics::default())
                            };
                            commands.insert_resource(SelectedCharacter {
                                name: c.name.clone(),
                                race_id: c.race_id.clone(),
                                core_pillar: c.core_pillar,
                                gender: Gender::Male,
                                character_id: c.character_id.clone(),
                                cosmetics: cosmetics.clone().unwrap_or_default(),
                            });
                            for mut sender in &mut hello_tx_q {
                                let _ = sender
                                    .send::<vaern_protocol::Channel1>(
                                        vaern_protocol::ClientHello {
                                            core_pillar: c.core_pillar,
                                            race_id: c.race_id.clone(),
                                            character_id: c.character_id.clone(),
                                            character_name: c.name.clone(),
                                            cosmetics: cosmetics.clone(),
                                        },
                                    );
                            }
                            next_state.set(AppState::InGame);
                        }
                    });
                }
            });

            // ── Right: create-new-character form ─────────────────────────
            cols[1].group(|ui| {
                ui.heading("Create character");
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.text_edit_singleline(&mut menu.character_name);
                });
                ui.add_space(4.0);
                race_picker(ui, &mut menu);
                ui.add_space(4.0);
                gender_picker(ui, &mut menu);
                ui.add_space(4.0);
                pillar_picker(ui, &mut menu);
                ui.add_space(8.0);
                let can_create = !menu.character_name.trim().is_empty()
                    && !menu.races.is_empty();
                if ui
                    .add_enabled(can_create, egui::Button::new("Create on Server"))
                    .clicked()
                {
                    let name = menu.character_name.trim().to_string();
                    let race = &menu.races[menu.race_index];
                    let cosmetics = menu.cosmetics_from_form();
                    for mut sender in &mut create_tx_q {
                        let _ = sender
                            .send::<vaern_protocol::Channel1>(
                                vaern_protocol::ClientCreateCharacter {
                                    name: name.clone(),
                                    race_id: race.id.clone(),
                                    core_pillar: PILLARS[menu.pillar_index],
                                    cosmetics: Some(cosmetics.clone()),
                                },
                            );
                    }
                    roster.last_error.clear();
                }
            });
        });

        ui.add_space(16.0);
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("← Log out").clicked() {
                commands.remove_resource::<crate::net::CachedCredentials>();
                roster.characters.clear();
                roster.account_username.clear();
                next_state.set(AppState::MainMenu);
            }
        });
    });
}

/// UI shown while `AppState::Reconnecting` is active. Shows attempt
/// counter + countdown to the next retry. Cancel button drops the user
/// back to the main menu without waiting out the remaining attempts.
fn reconnecting_ui(
    mut contexts: EguiContexts,
    state: Res<crate::net::ReconnectState>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    let ctx = match contexts.ctx_mut() {
        Ok(c) => c,
        Err(_) => return,
    };
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(100.0);
            ui.heading(
                egui::RichText::new("Reconnecting…")
                    .color(egui::Color32::from_rgb(240, 200, 80)),
            );
            ui.add_space(12.0);
            ui.spinner();
            ui.add_space(16.0);
            let attempt = state.attempts.max(1);
            ui.label(format!(
                "attempt {attempt} of {}",
                crate::net::RECONNECT_MAX_ATTEMPTS
            ));
            let remaining = state.seconds_until_next_retry();
            if remaining > 0 {
                ui.label(format!("next try in {remaining}s"));
            } else {
                ui.label("trying now…");
            }
            ui.add_space(20.0);
            if ui.button("Cancel").clicked() {
                next_state.set(AppState::MainMenu);
            }
        });
    });
}

/// Watch for a lightyear `Connected` marker on the client entity and advance
/// to InGame. Without this, the user stays on the Connecting screen forever.
fn detect_connected(
    clients: Query<Entity, (With<Client>, With<Connected>)>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if clients.iter().next().is_some() {
        next_state.set(AppState::InGame);
    }
}

fn in_game_menu_ui(
    mut contexts: EguiContexts,
    mut menu: ResMut<MenuState>,
    selected: Option<Res<SelectedCharacter>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    let ctx = match contexts.ctx_mut() {
        Ok(c) => c,
        Err(_) => return,
    };

    // Top-right persistent controls.
    egui::Area::new(egui::Id::new("top_right_controls"))
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-12.0, 12.0))
        .show(ctx, |ui| {
            if let Some(ch) = selected.as_deref() {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} · {} · {}",
                            ch.name,
                            prettify(&ch.race_id),
                            pillar_display(ch.core_pillar),
                        ))
                        .color(egui::Color32::from_gray(220)),
                    );
                    if ui.button("☰ Menu").clicked() {
                        menu.in_game_modal = !menu.in_game_modal;
                    }
                });
            } else if ui.button("☰ Menu").clicked() {
                menu.in_game_modal = !menu.in_game_modal;
            }
        });

    // Modal menu.
    if menu.in_game_modal {
        egui::Window::new("Menu")
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                if ui.button("Close").clicked() {
                    menu.in_game_modal = false;
                }
                ui.add_space(4.0);
                if ui.button("Return to Character Select").clicked() {
                    menu.in_game_modal = false;
                    next_state.set(AppState::MainMenu);
                }
                ui.add_space(4.0);
                if ui.button("Quit Game").clicked() {
                    std::process::exit(0);
                }
            });
    }
}

