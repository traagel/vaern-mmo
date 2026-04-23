//! Museum composer state + UI-picks-to-entities sync.
//!
//! The [`Composer`] resource is the single source of UI truth: which
//! gender, outfit, body overlays, weapon, palette, and live-tunable
//! grip the user has picked. Three systems reconcile it with the ECS:
//!
//! - [`sync_character`] — spawns / respawns the [`MeshtintCharacter`]
//!   entity on gender change, pushes [`OutfitPieces`] into the
//!   character's component so `vaern-assets` can toggle piece-node
//!   visibility.
//!
//! - [`sync_overlays`] — diffs picked body/weapon variants against
//!   currently-spawned overlay entities; despawns + respawns on change.
//!   Loads the new weapon's registered [`GripSpec`] into [`Composer::grip`]
//!   on weapon-pick change so UI sliders show calibrated values.
//!
//! - [`sync_palette`] + [`push_weapon_grip`] — ambient updates: palette
//!   material on the character, grip transform on the currently-live
//!   weapon overlay (driven by the live-editable `Composer::grip`).

use std::collections::BTreeMap;

use bevy::animation::transition::AnimationTransitions;
use bevy::animation::AnimationPlayer;
use bevy::prelude::*;
use std::time::Duration;
use vaern_assets::{
    spawn_quaternius_character, Beard as QuaterniusBeard, BodyOverlay, BodySlot, Gender, GripSpec,
    HeadPiece as QuaterniusHeadPiece, MegakitCatalog, MeshtintAnimations, MeshtintCharacter,
    MeshtintCharacterBundle, Outfit as QOutfit, OutfitPieces, PaletteOverride, QuaterniusColor,
    QuaterniusGripSpec, QuaterniusGrips, QuaterniusHair, QuaterniusOutfit, QuaterniusWeaponOverlay,
    Rig, WeaponGrips, WeaponOverlay,
};

#[derive(Resource)]
pub struct Composer {
    // --- user picks ---
    /// Which rig we're composing on. Switching this respawns the
    /// character and clears overlays.
    pub rig: Rig,
    pub gender: Gender,
    /// Quaternius per-slot picks — only consulted when `rig` is
    /// `Rig::QuaterniusModular`. Each slot is a slider; `None` = empty.
    pub q_body: Option<QOutfit>,
    pub q_legs: Option<QOutfit>,
    pub q_arms: Option<QOutfit>,
    pub q_feet: Option<QOutfit>,
    pub q_head_piece: Option<QuaterniusHeadPiece>,
    pub q_hair: Option<QuaterniusHair>,
    pub q_beard: Option<QuaterniusBeard>,
    /// Shared color palette across every outfit slot (`1` / `2` / `3`).
    pub q_color: QuaterniusColor,
    /// Which `{Gender}_NN.glb` base variant to spawn. `1` for
    /// `Male_01.glb` — Polygonal Fantasy Pack 1.4 ships only `_01`.
    pub base_variant: u32,
    pub outfit: OutfitPieces,
    /// `body_picks[slot] = variant`, or `0` = no overlay for this slot.
    pub body_picks: BTreeMap<BodySlot, u32>,
    /// Index into the flattened weapon list built by
    /// `Composer::weapon_list`. `0` = no weapon.
    pub weapon_idx: usize,
    /// Index into [`PaletteCache::mats`]. `None` → keep glTF default.
    pub palette_pick: Option<usize>,
    /// Optional sRGB override for the character's skin region. `None` →
    /// palette's native skin swatch. Brow + beard overlays sample the
    /// same palette region as skin, so they follow this color too.
    pub skin_color: Option<[u8; 3]>,
    /// Optional sRGB override for Hair / Brow / Beard overlays.
    /// `apply_overlay_colors` gives those three slots a flat material
    /// with this RGB (bypassing the palette entirely).
    pub hair_color: Option<[u8; 3]>,
    /// Optional sRGB override for the Eyes overlay. Flat material, same
    /// pattern as [`Self::hair_color`].
    pub eye_color: Option<[u8; 3]>,
    /// Live grip edited by the sliders; pushed onto the weapon overlay
    /// each frame by [`push_weapon_grip`]. Re-seeded from the registry
    /// when `weapon_idx` changes.
    pub grip: GripSpec,
    /// MEGAKIT prop basename picked in the Quaternius panel. `None` =
    /// empty hands.
    pub q_prop_id: Option<String>,
    /// Live Quaternius grip edited by the sliders; pushed onto the
    /// Quaternius weapon overlay each frame by
    /// [`push_quaternius_grip`]. Re-seeded from [`QuaterniusGrips`] on
    /// prop-pick change.
    pub q_grip: QuaterniusGripSpec,
    /// Which Meshtint animation clip is currently selected. `None` = no
    /// animation playing (bind pose / T-pose). Keyed by
    /// [`vaern_assets::AnimationClipSrc::key`].
    pub selected_clip: Option<String>,

    // --- state mirror (managed by sync systems) ---
    pub character: Option<Entity>,
    /// One or two overlay entities per body slot — slots in
    /// `BodySlot::has_mirrored_pair` spawn a primary plus an
    /// X-mirrored copy for symmetric placement.
    pub body_overlays: BTreeMap<BodySlot, Vec<Entity>>,
    pub weapon_overlay: Option<Entity>,
    pub q_weapon_overlay: Option<Entity>,
    applied_q_prop_id: Option<Option<String>>,
    applied_rig: Option<Rig>,
    applied_gender: Option<Gender>,
    applied_q_body: Option<Option<QOutfit>>,
    applied_q_legs: Option<Option<QOutfit>>,
    applied_q_arms: Option<Option<QOutfit>>,
    applied_q_feet: Option<Option<QOutfit>>,
    applied_q_head_piece: Option<Option<QuaterniusHeadPiece>>,
    applied_q_hair: Option<Option<QuaterniusHair>>,
    applied_q_beard: Option<Option<QuaterniusBeard>>,
    applied_q_color: Option<QuaterniusColor>,
    applied_base: Option<u32>,
    applied_body: BTreeMap<BodySlot, u32>,
    applied_weapon: Option<(String, u32)>,
    applied_palette_skin: Option<(Option<usize>, Option<[u8; 3]>)>,
    applied_clip: Option<String>,

    // --- diagnostics (read by the debug UI) ---
    pub stats_active_overlays: usize,
}

impl Composer {
    /// Set a body-slot pick and clear any slots that conflict with it.
    /// Use this from the UI instead of mutating `body_picks` directly so
    /// the mutex rules in [`BodySlot::conflicts_with`] are enforced.
    pub fn set_body_pick(&mut self, slot: BodySlot, variant: u32) {
        if variant > 0 {
            for &conflict in slot.conflicts_with() {
                self.body_picks.insert(conflict, 0);
            }
        }
        self.body_picks.insert(slot, variant);
    }
}

impl Default for Composer {
    fn default() -> Self {
        let mut body_picks = BTreeMap::new();
        for &slot in BodySlot::ALL {
            body_picks.insert(slot, 0);
        }
        Self {
            rig: Rig::Meshtint,
            gender: Gender::Male,
            q_body: Some(QOutfit::Peasant),
            q_legs: Some(QOutfit::Peasant),
            q_arms: Some(QOutfit::Peasant),
            q_feet: Some(QOutfit::Peasant),
            q_head_piece: None,
            q_hair: Some(QuaterniusHair::SimpleParted),
            q_beard: None,
            q_color: QuaterniusColor::Default,
            base_variant: 1,
            outfit: OutfitPieces::default(),
            body_picks,
            weapon_idx: 0,
            palette_pick: None,
            skin_color: None,
            hair_color: None,
            eye_color: None,
            grip: GripSpec::default(),
            q_prop_id: None,
            q_grip: QuaterniusGripSpec::default(),
            selected_clip: None,
            character: None,
            body_overlays: BTreeMap::new(),
            weapon_overlay: None,
            q_weapon_overlay: None,
            applied_q_prop_id: None,
            applied_rig: None,
            applied_gender: None,
            applied_q_body: None,
            applied_q_legs: None,
            applied_q_arms: None,
            applied_q_feet: None,
            applied_q_head_piece: None,
            applied_q_hair: None,
            applied_q_beard: None,
            applied_q_color: None,
            applied_base: None,
            applied_body: BTreeMap::new(),
            applied_weapon: None,
            applied_palette_skin: None,
            applied_clip: None,
            stats_active_overlays: 0,
        }
    }
}

/// Flattened (category, variant) list built from the catalog — a single
/// slider can scrub across every weapon in pack order.
#[derive(Resource, Default, Debug)]
pub struct WeaponList {
    pub entries: Vec<(String, u32, String)>, // (category, variant, display_label)
}

impl WeaponList {
    pub fn build(catalog: &vaern_assets::MeshtintCatalog) -> Self {
        let mut entries = Vec::new();
        for category in catalog.weapon_categories() {
            for v in catalog.weapon(category) {
                entries.push((category.to_string(), v.number, v.label.clone()));
            }
        }
        Self { entries }
    }
}

#[derive(Resource, Default)]
pub struct PaletteCache {
    /// Pre-built materials for each stock DS palette, indexed by
    /// `MESHTINT_DS_PALETTES`. Reused when no skin override is active.
    pub stock_mats: Vec<Handle<StandardMaterial>>,
    /// Raw 1024×1024 RGBA pixel bytes of each stock palette. Used as the
    /// canvas when painting a skin-color override into the skin-1 region.
    pub raw: Vec<Vec<u8>>,
}

/// Skin swatch #1 region on every DS palette — the mesh samples this
/// swatch for character skin (and the brow / beard overlay meshes sample
/// the same pixel range, so without further intervention they'd follow
/// skin color automatically). [`apply_hair_color`] overrides the brow /
/// beard meshes back to the hair color separately.
const SKIN_SWATCH_W: u32 = 200;
const SKIN_SWATCH_H: u32 = 80;
const PALETTE_SIZE: u32 = 1024;
/// Default base palette when the user picks a skin/hair color but has
/// no DS palette selected — matches the glTF's embedded `DS Blue Gold.png`.
const DEFAULT_PALETTE_IDX: usize = 0;

/// Ten skin-tone presets, pale → deep. Values are sRGB with alpha 255.
pub const SKIN_PRESETS: &[(&str, [u8; 3])] = &[
    ("Porcelain", [250, 228, 210]),
    ("Fair", [240, 200, 170]),
    ("Light", [225, 180, 145]),
    ("Tan", [210, 155, 110]),
    ("Olive", [180, 130, 90]),
    ("Medium", [160, 110, 75]),
    ("Warm", [130, 85, 55]),
    ("Dark", [100, 65, 40]),
    ("Very Dark", [75, 45, 25]),
    ("Deep", [50, 30, 15]),
];

/// Ten hair-color presets, dark → light + a few accent shades.
pub const HAIR_PRESETS: &[(&str, [u8; 3])] = &[
    ("Black", [25, 20, 20]),
    ("Dark Brown", [60, 40, 25]),
    ("Brown", [95, 60, 35]),
    ("Light Brown", [140, 95, 55]),
    ("Dirty Blond", [180, 150, 100]),
    ("Blond", [230, 200, 130]),
    ("Platinum", [245, 235, 200]),
    ("Red", [140, 60, 30]),
    ("Auburn", [110, 45, 25]),
    ("Gray", [155, 150, 145]),
];

/// Ten eye-color presets — natural range plus a couple of fantasy hues.
pub const EYE_PRESETS: &[(&str, [u8; 3])] = &[
    ("Black", [15, 15, 20]),
    ("Dark Brown", [55, 35, 20]),
    ("Brown", [90, 60, 30]),
    ("Hazel", [135, 95, 50]),
    ("Amber", [190, 130, 50]),
    ("Green", [80, 130, 80]),
    ("Blue", [70, 130, 180]),
    ("Light Blue", [130, 180, 210]),
    ("Gray", [130, 135, 140]),
    ("Violet", [120, 80, 160]),
];

// --- Systems --------------------------------------------------------------

/// Respawn the character on rig / gender / base-variant change; push
/// outfit picks into the Meshtint character's [`OutfitPieces`] otherwise.
/// UBC Mannequins have no outfit system — they're a single rigged mesh.
pub fn sync_character(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut composer: ResMut<Composer>,
    mut outfits: Query<&mut OutfitPieces, With<MeshtintCharacter>>,
    mut outfit_colors: Query<&mut vaern_assets::OutfitColor>,
) {
    let needs_respawn = composer.applied_rig != Some(composer.rig)
        || composer.applied_gender != Some(composer.gender)
        || composer.applied_q_body != Some(composer.q_body)
        || composer.applied_q_legs != Some(composer.q_legs)
        || composer.applied_q_arms != Some(composer.q_arms)
        || composer.applied_q_feet != Some(composer.q_feet)
        || composer.applied_q_head_piece != Some(composer.q_head_piece)
        || composer.applied_q_hair != Some(composer.q_hair)
        || composer.applied_q_beard != Some(composer.q_beard)
        || composer.applied_base != Some(composer.base_variant);
    // Color changes don't need a respawn — apply_outfit_color picks up
    // mutations to OutfitColor.desired each frame. Still push the new
    // value into the existing component below.
    if needs_respawn {
        if let Some(e) = composer.character.take() {
            if let Ok(mut ec) = commands.get_entity(e) {
                ec.despawn();
            }
        }
        let new = match composer.rig {
            Rig::Meshtint => commands
                .spawn(MeshtintCharacterBundle::new(
                    &assets,
                    composer.gender,
                    composer.base_variant,
                ))
                .id(),
            Rig::QuaterniusModular => spawn_quaternius_character(
                &mut commands,
                &assets,
                composer.gender,
                QuaterniusOutfit::uniform(
                    composer.q_body,
                    composer.q_legs,
                    composer.q_arms,
                    composer.q_feet,
                    composer.q_head_piece,
                    composer.q_hair,
                    composer.q_beard,
                    composer.q_color,
                ),
            ),
        };
        composer.character = Some(new);
        composer.applied_rig = Some(composer.rig);
        composer.applied_gender = Some(composer.gender);
        composer.applied_q_body = Some(composer.q_body);
        composer.applied_q_legs = Some(composer.q_legs);
        composer.applied_q_arms = Some(composer.q_arms);
        composer.applied_q_feet = Some(composer.q_feet);
        composer.applied_q_head_piece = Some(composer.q_head_piece);
        composer.applied_q_hair = Some(composer.q_hair);
        composer.applied_q_beard = Some(composer.q_beard);
        composer.applied_q_color = Some(composer.q_color);
        composer.applied_base = Some(composer.base_variant);

        composer.body_overlays.clear();
        composer.applied_body.clear();
        composer.weapon_overlay = None;
        composer.applied_weapon = None;
        composer.q_weapon_overlay = None;
        composer.applied_q_prop_id = None;
        composer.applied_palette_skin = None;
        composer.applied_clip = None;
        return;
    }

    let Some(character) = composer.character else {
        return;
    };
    if composer.rig == Rig::Meshtint {
        if let Ok(mut outfit) = outfits.get_mut(character) {
            if *outfit != composer.outfit {
                *outfit = composer.outfit;
            }
        }
    } else if composer.rig == Rig::QuaterniusModular
        && composer.applied_q_color != Some(composer.q_color)
    {
        // Live color swap — update the OutfitColor on the outfit
        // child (a descendant of `character`). `apply_outfit_color`
        // picks up the new desired value and re-writes the textures.
        for mut oc in outfit_colors.iter_mut() {
            if oc.desired != composer.q_color {
                oc.desired = composer.q_color;
            }
        }
        composer.applied_q_color = Some(composer.q_color);
    }
}

/// Diff body-overlay + weapon picks against spawned entities; despawn +
/// respawn as needed. On weapon change, seed [`Composer::grip`] from the
/// registry so UI sliders reflect calibrated values.
pub fn sync_overlays(
    mut commands: Commands,
    mut composer: ResMut<Composer>,
    grips: Res<WeaponGrips>,
    weapon_list: Res<WeaponList>,
) {
    // Overlays are Meshtint-only: body pieces + weapons anchor to
    // Meshtint's `RigRPalm` / `RigLPalm` bones. Skip entirely on other
    // rigs so the UBC Mannequin doesn't accumulate stale overlay state.
    if composer.rig != Rig::Meshtint {
        return;
    }
    let Some(character) = composer.character else {
        return;
    };

    // Body overlays: diff per slot. Slots with a mirrored pair get two
    // overlay entities (primary + X-mirror) tracked as a `Vec<Entity>`.
    let picks: Vec<(BodySlot, u32)> = composer
        .body_picks
        .iter()
        .map(|(s, v)| (*s, *v))
        .collect();
    for (slot, wanted) in picks {
        let applied = composer.applied_body.get(&slot).copied().unwrap_or(0);
        if applied == wanted {
            continue;
        }

        // Despawn any existing entities for this slot.
        if let Some(entities) = composer.body_overlays.remove(&slot) {
            for e in entities {
                if let Ok(mut ec) = commands.get_entity(e) {
                    ec.despawn();
                }
            }
        }

        if wanted > 0 {
            let primary = commands
                .spawn(BodyOverlay {
                    target: character,
                    gender: composer.gender,
                    slot,
                    variant: wanted,
                    mirror_x: false,
                })
                .id();
            let mut spawned = vec![primary];
            if slot.has_mirrored_pair() {
                let mirror = commands
                    .spawn(BodyOverlay {
                        target: character,
                        gender: composer.gender,
                        slot,
                        variant: wanted,
                        mirror_x: true,
                    })
                    .id();
                spawned.push(mirror);
            }
            composer.body_overlays.insert(slot, spawned);
        }
        composer.applied_body.insert(slot, wanted);
    }

    // Weapon: resolve the flat slider index to a (category, variant).
    // `weapon_idx == 0` means "no weapon".
    let wanted_weapon = if composer.weapon_idx == 0 {
        None
    } else {
        weapon_list
            .entries
            .get(composer.weapon_idx - 1)
            .map(|(c, v, _)| (c.clone(), *v))
    };
    if composer.applied_weapon != wanted_weapon {
        if let Some(e) = composer.weapon_overlay.take() {
            if let Ok(mut ec) = commands.get_entity(e) {
                ec.despawn();
            }
        }
        if let Some((cat, var)) = wanted_weapon.clone() {
            // Seed live grip from the registry so sliders reflect the
            // calibrated value for the new weapon.
            if let Some((_, spec)) = grips.lookup(&cat, var) {
                composer.grip = spec;
            }
            let e = commands
                .spawn(WeaponOverlay {
                    target: character,
                    category: cat,
                    variant: var,
                })
                .id();
            composer.weapon_overlay = Some(e);
        }
        composer.applied_weapon = wanted_weapon;
    }

    // Active-overlay count for debug readout (sums mirror copies).
    composer.stats_active_overlays = composer.body_overlays.values().map(Vec::len).sum::<usize>()
        + usize::from(composer.weapon_overlay.is_some());
}

/// Mirror `(palette_pick, skin_color)` into a [`PaletteOverride`] on the
/// character.
///
/// - `(None, None)` → no override (stock glTF material).
/// - `(Some(i), None)` → the pre-built `stock_mats[i]` handle.
/// - `(_, Some(rgb))` → clone the chosen (or default) palette's raw bytes,
///   paint `rgb` over the skin-1 swatch region (x=0..200, y=0..80), upload
///   as a fresh `Image`, create a new `StandardMaterial`, apply.
///
/// Palette overrides are sticky — removing the component does NOT revert
/// to the original glTF texture. Respawn the character to reset.
pub fn sync_palette(
    mut commands: Commands,
    mut composer: ResMut<Composer>,
    cache: Res<PaletteCache>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let current = (composer.palette_pick, composer.skin_color);
    if composer.applied_palette_skin == Some(current) {
        return;
    }
    let Some(character) = composer.character else {
        return;
    };

    match current {
        (None, None) => {
            commands.entity(character).remove::<PaletteOverride>();
        }
        (Some(idx), None) => {
            if let Some(mat) = cache.stock_mats.get(idx) {
                commands
                    .entity(character)
                    .insert(PaletteOverride(mat.clone()));
            }
        }
        (palette_opt, Some(skin_rgb)) => {
            let base_idx = palette_opt.unwrap_or(DEFAULT_PALETTE_IDX);
            let Some(base_raw) = cache.raw.get(base_idx) else {
                return;
            };
            let mat_handle = build_palette_override(
                base_raw,
                Some(skin_rgb),
                images.as_mut(),
                materials.as_mut(),
            );
            commands
                .entity(character)
                .insert(PaletteOverride(mat_handle));
        }
    }

    composer.applied_palette_skin = Some(current);
}

fn build_palette_override(
    base_raw: &[u8],
    skin_rgb: Option<[u8; 3]>,
    images: &mut Assets<Image>,
    materials: &mut Assets<StandardMaterial>,
) -> Handle<StandardMaterial> {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

    // Hair / Eye UVs don't respect palette banding — those are handled
    // by `apply_overlay_colors` with flat per-entity materials instead.
    let mut buf = base_raw.to_vec();
    if let Some(rgb) = skin_rgb {
        paint_rect(&mut buf, 0, 0, SKIN_SWATCH_W, SKIN_SWATCH_H, rgb);
    }

    let image = Image::new(
        Extent3d {
            width: PALETTE_SIZE,
            height: PALETTE_SIZE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        buf,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    let tex = images.add(image);
    materials.add(StandardMaterial {
        base_color_texture: Some(tex),
        base_color: Color::WHITE,
        perceptual_roughness: 0.85,
        metallic: 0.0,
        ..default()
    })
}

fn paint_rect(buf: &mut [u8], x0: u32, y0: u32, w: u32, h: u32, rgb: [u8; 3]) {
    for y in y0..(y0 + h).min(PALETTE_SIZE) {
        let row_start = (y * PALETTE_SIZE) as usize * 4;
        for x in x0..(x0 + w).min(PALETTE_SIZE) {
            let i = row_start + x as usize * 4;
            buf[i] = rgb[0];
            buf[i + 1] = rgb[1];
            buf[i + 2] = rgb[2];
            buf[i + 3] = 255;
        }
    }
}

/// Per-frame: apply flat-color materials to overlay slots whose UVs
/// don't respect the DS palette's swatch banding. Currently handles two
/// groups:
/// - Hair / Brow / Beard → [`Composer::hair_color`]
/// - Eyes → [`Composer::eye_color`]
///
/// A flat `StandardMaterial` (no texture, solid `base_color`) assigned
/// per-entity bypasses the palette entirely for these slots — necessary
/// because e.g. the Hair overlay's UVs span `u,v ∈ 0..1` and tile the
/// whole palette across its surface, so painting any single band only
/// recolours a slice of hair.
///
/// Must run **after**
/// `vaern_assets::meshtint::palette::apply_palette_override` so its
/// `MeshMaterial3d` inserts overwrite any palette-wide material that
/// was just applied. Enforced via `.after(...)` in `main.rs`.
///
/// Each colour group caches its last-built material by RGB so idle
/// frames don't allocate a new asset.
pub fn apply_overlay_colors(
    mut commands: Commands,
    composer: Res<Composer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    overlays: Query<(Entity, &BodyOverlay)>,
    children: Query<&Children>,
    meshes: Query<(), With<Mesh3d>>,
    mut hair_cache: Local<Option<([u8; 3], Handle<StandardMaterial>)>>,
    mut eye_cache: Local<Option<([u8; 3], Handle<StandardMaterial>)>>,
) {
    apply_flat_group(
        &mut commands,
        materials.as_mut(),
        composer.hair_color,
        &mut hair_cache,
        &overlays,
        &children,
        &meshes,
        &[BodySlot::Hair, BodySlot::Brow, BodySlot::Beard],
    );
    apply_flat_group(
        &mut commands,
        materials.as_mut(),
        composer.eye_color,
        &mut eye_cache,
        &overlays,
        &children,
        &meshes,
        &[BodySlot::Eyes],
    );
}

fn apply_flat_group(
    commands: &mut Commands,
    materials: &mut Assets<StandardMaterial>,
    color: Option<[u8; 3]>,
    cached: &mut Local<Option<([u8; 3], Handle<StandardMaterial>)>>,
    overlays: &Query<(Entity, &BodyOverlay)>,
    children: &Query<&Children>,
    meshes: &Query<(), With<Mesh3d>>,
    slots: &[BodySlot],
) {
    let Some(rgb) = color else {
        return;
    };
    if cached.as_ref().map(|(c, _)| *c) != Some(rgb) {
        let mat = materials.add(StandardMaterial {
            base_color: Color::srgb_u8(rgb[0], rgb[1], rgb[2]),
            base_color_texture: None,
            perceptual_roughness: 0.85,
            metallic: 0.0,
            ..default()
        });
        **cached = Some((rgb, mat));
    }
    let Some((_, flat_mat)) = cached.as_ref() else {
        return;
    };

    for (root, overlay) in overlays {
        if !slots.contains(&overlay.slot) {
            continue;
        }
        let mut stack = vec![root];
        while let Some(e) = stack.pop() {
            if meshes.contains(e) {
                commands.entity(e).insert(MeshMaterial3d(flat_mat.clone()));
            }
            if let Ok(kids) = children.get(e) {
                for &c in kids {
                    stack.push(c);
                }
            }
        }
    }
}

/// Push the live-editable `Composer::grip` onto the currently-spawned
/// weapon overlay each frame. Cheap: 0-1 `WeaponOverlay` entities.
pub fn push_weapon_grip(
    composer: Res<Composer>,
    mut q: Query<&mut Transform, With<WeaponOverlay>>,
) {
    let tf = composer.grip.transform();
    for mut t in &mut q {
        *t = tf;
    }
}

/// Diff the Quaternius prop pick against the spawned overlay; despawn
/// + respawn on change. Seeds [`Composer::q_grip`] from the registry
/// on new picks so sliders reflect calibrated values. Skipped entirely
/// when the active rig isn't Quaternius.
pub fn sync_quaternius_weapon(
    mut commands: Commands,
    mut composer: ResMut<Composer>,
    grips: Res<QuaterniusGrips>,
) {
    if composer.rig != Rig::QuaterniusModular {
        return;
    }
    let Some(character) = composer.character else {
        return;
    };
    let wanted = composer.q_prop_id.clone();
    if composer.applied_q_prop_id.as_ref() == Some(&wanted) {
        return;
    }
    if let Some(e) = composer.q_weapon_overlay.take() {
        if let Ok(mut ec) = commands.get_entity(e) {
            ec.despawn();
        }
    }
    if let Some(prop_id) = wanted.clone() {
        let (_, spec) = grips.lookup(&prop_id);
        composer.q_grip = spec;
        let e = commands
            .spawn(QuaterniusWeaponOverlay {
                target: character,
                prop_id,
            })
            .id();
        composer.q_weapon_overlay = Some(e);
    }
    composer.applied_q_prop_id = Some(wanted);
}

/// Push the live-editable `Composer::q_grip` onto the currently-spawned
/// Quaternius weapon overlay each frame. Counterpart of
/// [`push_weapon_grip`] for the Quaternius rig.
pub fn push_quaternius_grip(
    composer: Res<Composer>,
    mut q: Query<&mut Transform, With<QuaterniusWeaponOverlay>>,
) {
    let tf = composer.q_grip.transform();
    for mut t in &mut q {
        *t = tf;
    }
}

/// Flat list of MEGAKIT prop basenames for the museum's Quaternius
/// weapon picker. Rebuilt once at startup from [`MegakitCatalog`].
#[derive(Resource, Default, Debug)]
pub struct MegakitPropList {
    pub basenames: Vec<String>,
}

impl MegakitPropList {
    pub fn build(catalog: &MegakitCatalog) -> Self {
        Self {
            basenames: catalog
                .iter()
                .map(|(k, _e): (&str, _)| k.to_string())
                .collect(),
        }
    }
}

/// Cross-fade duration between clip switches. Short enough that picking
/// another clip feels instant; long enough to hide the bone snap.
const CLIP_CROSSFADE: Duration = Duration::from_millis(150);

/// Per-`AnimationPlayer` marker tracking which clip key is currently
/// active on this specific player. Per-entity (not per-composer) so
/// that newly-spawned scene children pick up the current clip on the
/// next frame even if the selection hasn't changed — the change
/// detection then compares each player's `AppliedClip` against the
/// global `Composer::selected_clip`.
#[derive(Component)]
pub struct AppliedClip(Option<String>);

/// Ensure every `AnimationPlayer` under the character is playing
/// [`Composer::selected_clip`] (or stopped if `None`).
///
/// Quaternius characters spawn as a parent with three scene children
/// (outfit + head + hair), each carrying its own player. Scene loads
/// trickle in across frames; new scenes spawned after a hair/outfit
/// change need the currently-selected clip applied too. Per-entity
/// [`AppliedClip`] marker lets us detect players whose state differs
/// from the desired selection and apply idempotently.
pub fn sync_selected_clip(
    mut commands: Commands,
    composer: Res<Composer>,
    anims: Res<MeshtintAnimations>,
    children: Query<&Children>,
    mut players: Query<(
        Entity,
        &mut AnimationPlayer,
        &mut AnimationTransitions,
        Option<&AppliedClip>,
    )>,
) {
    if !anims.is_ready() {
        return;
    }
    let Some(character) = composer.character else {
        return;
    };

    let player_entities = find_all_animation_players(character, &children, &players);
    if player_entities.is_empty() {
        return;
    }

    let desired_key = composer.selected_clip.as_deref();
    let desired_node = match desired_key {
        None => None,
        Some(k) => anims.node_for(k),
    };

    for entity in player_entities {
        let Ok((_, mut player, mut transitions, applied)) = players.get_mut(entity) else {
            continue;
        };
        let current = applied.and_then(|a| a.0.as_deref());
        if current == desired_key {
            continue;
        }
        match desired_node {
            None => {
                player.stop_all();
            }
            Some(node) => {
                transitions.play(&mut player, node, CLIP_CROSSFADE).repeat();
            }
        }
        commands
            .entity(entity)
            .insert(AppliedClip(desired_key.map(|s| s.to_string())));
    }
}

fn find_all_animation_players(
    root: Entity,
    children: &Query<&Children>,
    players: &Query<(
        Entity,
        &mut AnimationPlayer,
        &mut AnimationTransitions,
        Option<&AppliedClip>,
    )>,
) -> Vec<Entity> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(e) = stack.pop() {
        if players.get(e).is_ok() {
            out.push(e);
        }
        if let Ok(kids) = children.get(e) {
            for &c in kids {
                stack.push(c);
            }
        }
    }
    out
}
