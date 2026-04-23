//! Quaternius modular character assembly.
//!
//! A full character is spawned as a **parent entity** (the
//! [`QuaterniusCharacter`] root) with up to three scene children:
//!
//! 1. **Outfit** — `{Gender}_{Outfit}.gltf` (body + legs + arms + feet).
//! 2. **Head** — `Superhero_{Gender}_FullBody_Split.gltf` with every
//!    non-head mesh-node (Region_Torso / Region_LeftArm / … / the
//!    `SuperHero_*` body mesh) hidden via [`HideNonHeadRegions`]. Keeps
//!    `Region_Head` + `Eyes` + `Eyebrows` visible.
//! 3. **Hair** — `Hair_{style}.gltf`, optional.
//!
//! Each child scene has its own UE-Mannequin armature. We place
//! [`AnimatedRig`] on **each child** (not the parent) so the animation
//! installer wires an independent `AnimationPlayer` onto each scene's
//! armature. All three players receive the same clip from the shared
//! `AnimationGraph` and pose identically — the character reads as one
//! cohesive figure.

use bevy::prelude::*;

use crate::meshtint::{AnimatedRig, Gender, Rig};
use crate::regions::NamedRegions;

use super::bones::{BONE_MAINHAND, BONE_OFFHAND};

// --- picks --------------------------------------------------------------

/// Which Quaternius outfit set to spawn. Combined-outfit glTFs package
/// body + legs + arms + feet + head piece + accessories all under one
/// shared armature — we just spawn the right file and everything comes
/// through. See `assets/extracted/characters/outfits/{Gender}_*.gltf`.
///
/// `Knight` and `KnightCloth` share the Knight palette and all shared
/// meshes (arms / legs / feet). They diverge only at the chest — Knight
/// picks `Knight_Body_Armor` (plate plates), KnightCloth picks
/// `Knight_Body_Cloth` (chain-mail-ish torso). Used by the armor-type
/// resolver so `ArmorType::Mail` and `ArmorType::Plate` can render
/// distinct torsos while everything below the waist stays shared.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Outfit {
    Peasant,
    Ranger,
    Noble,
    /// Knight — uses the plate-armor body mesh (`Knight_Body_Armor`).
    /// Paired with `HeadPiece::KnightArmet` + the shared Knight
    /// arms/legs/feet. Palette = `T_Knight_*`.
    Knight,
    /// Knight cloth-body variant (`Knight_Body_Cloth`). Same shared
    /// arms/legs/feet + `T_Knight_*` palette as [`Outfit::Knight`], but
    /// the chest mesh is the cloth-surcoat sculpt. Used by the resolver
    /// for `ArmorType::Mail` chests.
    KnightCloth,
    Wizard,
}

impl Outfit {
    pub const ALL: &'static [Outfit] = &[
        Outfit::Peasant,
        Outfit::Ranger,
        Outfit::Noble,
        Outfit::Knight,
        Outfit::KnightCloth,
        Outfit::Wizard,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Outfit::Peasant => "Peasant",
            Outfit::Ranger => "Ranger",
            Outfit::Noble => "Noble",
            Outfit::Knight => "Knight",
            Outfit::KnightCloth => "Knight Cloth",
            Outfit::Wizard => "Wizard",
        }
    }

    /// Filename stem (before the `.gltf`) under `outfits/`. Used by the
    /// combined-outfit [`Self::asset_path`] entry point; shared-mesh
    /// variants (`KnightCloth`) fall back to the master `Knight` file
    /// since no combined `_Cloth` glTF ships.
    fn file_stem(self) -> &'static str {
        match self {
            Outfit::Peasant => "Peasant",
            Outfit::Ranger => "Ranger",
            Outfit::Noble => "Noble",
            Outfit::Knight | Outfit::KnightCloth => "Knight",
            Outfit::Wizard => "Wizard",
        }
    }

    /// Texture palette stem — drives `T_{stem}_{variant}BaseColor.png`.
    /// `KnightCloth` shares the Knight palette; mail vs plate differ by
    /// [`ColorVariant`], not by palette family.
    pub(crate) fn texture_stem(self) -> &'static str {
        match self {
            Outfit::Peasant => "Peasant",
            Outfit::Ranger => "Ranger",
            Outfit::Noble => "Noble",
            Outfit::Knight | Outfit::KnightCloth => "Knight",
            Outfit::Wizard => "Wizard",
        }
    }

    pub fn asset_path(self, gender: Gender) -> String {
        let g = Self::gender_prefix(gender);
        format!(
            "extracted/characters/outfits/{g}_{}.gltf#Scene0",
            self.file_stem()
        )
    }

    fn gender_prefix(gender: Gender) -> &'static str {
        match gender {
            Gender::Male => "Male",
            Gender::Female => "Female",
        }
    }

    // --- per-slot modular part paths ------------------------------------

    /// `{Gender}_{Outfit}_Body[_Armor|_Cloth].gltf`. Knight ships with
    /// two body meshes — plate-body (`_Armor`) and cloth-body (`_Cloth`)
    /// — exposed via the `Knight` / `KnightCloth` variants.
    pub fn body_part_path(self, gender: Gender) -> String {
        let g = Self::gender_prefix(gender);
        let stem = match self {
            Outfit::Peasant => "Peasant_Body",
            Outfit::Ranger => "Ranger_Body",
            Outfit::Noble => "Noble_Body",
            Outfit::Knight => "Knight_Body_Armor",
            Outfit::KnightCloth => "Knight_Body_Cloth",
            Outfit::Wizard => "Wizard_Body",
        };
        format!("extracted/characters/outfits/{g}_{stem}.gltf#Scene0")
    }

    /// `{Gender}_{Outfit}_Legs[_Armor].gltf`. Male Knight legs ship as
    /// `_Legs_Armor`; female Knight legs ship as plain `_Legs`.
    /// `KnightCloth` shares the same mesh as `Knight` — only chest
    /// diverges.
    pub fn legs_part_path(self, gender: Gender) -> String {
        let g = Self::gender_prefix(gender);
        let stem = match self {
            Outfit::Peasant => "Peasant_Legs",
            Outfit::Ranger => "Ranger_Legs",
            Outfit::Noble => "Noble_Legs",
            Outfit::Knight | Outfit::KnightCloth => match gender {
                Gender::Male => "Knight_Legs_Armor",
                Gender::Female => "Knight_Legs",
            },
            Outfit::Wizard => "Wizard_Legs",
        };
        format!("extracted/characters/outfits/{g}_{stem}.gltf#Scene0")
    }

    pub fn arms_part_path(self, gender: Gender) -> String {
        let g = Self::gender_prefix(gender);
        let stem = match self {
            Outfit::Peasant => "Peasant_Arms",
            Outfit::Ranger => "Ranger_Arms",
            Outfit::Noble => "Noble_Arms",
            Outfit::Knight | Outfit::KnightCloth => "Knight_Arms",
            Outfit::Wizard => "Wizard_Arms",
        };
        format!("extracted/characters/outfits/{g}_{stem}.gltf#Scene0")
    }

    /// `{Gender}_{Outfit}_Feet[_Boots|_Armor].gltf`. Suffix varies per
    /// outfit **and** per gender: Ranger male = `_Feet_Boots` /
    /// female = `_Feet`; Knight male = `_Feet_Armor` / female = `_Feet`.
    /// `KnightCloth` shares the Knight feet mesh.
    pub fn feet_part_path(self, gender: Gender) -> String {
        let g = Self::gender_prefix(gender);
        let stem = match self {
            Outfit::Peasant => "Peasant_Feet",
            Outfit::Ranger => match gender {
                Gender::Male => "Ranger_Feet_Boots",
                Gender::Female => "Ranger_Feet",
            },
            Outfit::Noble => "Noble_Feet",
            Outfit::Knight | Outfit::KnightCloth => match gender {
                Gender::Male => "Knight_Feet_Armor",
                Gender::Female => "Knight_Feet",
            },
            Outfit::Wizard => "Wizard_Feet",
        };
        format!("extracted/characters/outfits/{g}_{stem}.gltf#Scene0")
    }
}

/// Optional helmet / head accessory layered on top of the Superhero
/// head. Each tied to a specific outfit texture for color variants.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HeadPiece {
    KnightArmet,
    KnightHorns,
    NobleCrown,
    RangerHood,
}

impl HeadPiece {
    pub const ALL: &'static [HeadPiece] = &[
        HeadPiece::KnightArmet,
        HeadPiece::KnightHorns,
        HeadPiece::NobleCrown,
        HeadPiece::RangerHood,
    ];

    pub fn label(self) -> &'static str {
        match self {
            HeadPiece::KnightArmet => "Knight Armet",
            HeadPiece::KnightHorns => "Knight Horns",
            HeadPiece::NobleCrown => "Noble Crown",
            HeadPiece::RangerHood => "Ranger Hood",
        }
    }

    fn file_stem(self) -> &'static str {
        match self {
            HeadPiece::KnightArmet => "Knight_Head_Armet",
            HeadPiece::KnightHorns => "Knight_Head_Horns",
            HeadPiece::NobleCrown => "Noble_Head_Crown",
            HeadPiece::RangerHood => "Ranger_Head_Hood",
        }
    }

    /// Which outfit's color palette applies to this head piece.
    pub fn outfit(self) -> Outfit {
        match self {
            HeadPiece::KnightArmet | HeadPiece::KnightHorns => Outfit::Knight,
            HeadPiece::NobleCrown => Outfit::Noble,
            HeadPiece::RangerHood => Outfit::Ranger,
        }
    }

    pub fn asset_path(self, gender: Gender) -> String {
        let g = Outfit::gender_prefix(gender);
        format!("extracted/characters/outfits/{g}_{}.gltf#Scene0", self.file_stem())
    }
}

/// Facial hair — male only. Only one style ships (`Hair_Beard.gltf`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Beard {
    Full,
}

impl Beard {
    pub const ALL: &'static [Beard] = &[Beard::Full];

    pub fn label(self) -> &'static str {
        "Beard"
    }

    pub fn asset_path(self) -> &'static str {
        "extracted/characters/hair/Hair_Beard.gltf#Scene0"
    }
}

/// Color variant — Quaternius ships three base-color texture palettes
/// per outfit (default, `_2_`, `_3_`). The variant swap only touches
/// the `base_color_texture`; normals + ORM (occlusion / roughness /
/// metallic) are shared across colors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ColorVariant {
    Default,
    V2,
    V3,
}

impl ColorVariant {
    pub const ALL: &'static [ColorVariant] = &[
        ColorVariant::Default,
        ColorVariant::V2,
        ColorVariant::V3,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ColorVariant::Default => "1",
            ColorVariant::V2 => "2",
            ColorVariant::V3 => "3",
        }
    }

    /// `""` for the default, `"2_"` / `"3_"` for the variants.
    fn filename_infix(self) -> &'static str {
        match self {
            ColorVariant::Default => "",
            ColorVariant::V2 => "2_",
            ColorVariant::V3 => "3_",
        }
    }

    pub fn base_color_path(self, outfit: Outfit) -> String {
        format!(
            "extracted/characters/outfits/T_{}_{}BaseColor.png",
            outfit.texture_stem(),
            self.filename_infix()
        )
    }
}

/// Hair style. `None` option lives at the call site (Composer).
/// Gender-filtered — see [`Hair::available_for`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Hair {
    SimpleParted,
    Long,
    Buzzed,
    Buns,
}

impl Hair {
    pub const ALL: &'static [Hair] = &[
        Hair::SimpleParted,
        Hair::Long,
        Hair::Buzzed,
        Hair::Buns,
    ];

    /// Styles that actually look right on `gender`. `SimpleParted`
    /// ships male-only; `Long` and `Buns` ship female-only; `Buzzed`
    /// has separate male (`Hair_Buzzed`) / female (`Hair_BuzzedFemale`)
    /// meshes and works for both.
    pub fn available_for(gender: Gender) -> &'static [Hair] {
        match gender {
            Gender::Male => &[Hair::SimpleParted, Hair::Buzzed],
            Gender::Female => &[Hair::Long, Hair::Buns, Hair::Buzzed],
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Hair::SimpleParted => "Simple Parted",
            Hair::Long => "Long",
            Hair::Buzzed => "Buzzed",
            Hair::Buns => "Buns",
        }
    }

    pub fn asset_path(self, gender: Gender) -> String {
        let stem = match (self, gender) {
            (Hair::Buzzed, Gender::Female) => "Hair_BuzzedFemale",
            (Hair::Buzzed, Gender::Male) => "Hair_Buzzed",
            (Hair::SimpleParted, _) => "Hair_SimpleParted",
            (Hair::Long, _) => "Hair_Long",
            (Hair::Buns, _) => "Hair_Buns",
        };
        format!("extracted/characters/hair/{stem}.gltf#Scene0")
    }
}

// --- components ---------------------------------------------------------

/// An outfit-bearing slot pick: which outfit family to spawn + which
/// color variant to paint it in. Used for body / legs / arms / feet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OutfitSlot {
    pub outfit: Outfit,
    pub color: ColorVariant,
}

impl OutfitSlot {
    pub const fn new(outfit: Outfit, color: ColorVariant) -> Self {
        Self { outfit, color }
    }
}

/// A head-piece pick + its color variant. `HeadPiece` carries the
/// outfit-palette link internally ([`HeadPiece::outfit`]); this just
/// picks which of the three texture variants applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HeadSlot {
    pub piece: HeadPiece,
    pub color: ColorVariant,
}

impl HeadSlot {
    pub const fn new(piece: HeadPiece, color: ColorVariant) -> Self {
        Self { piece, color }
    }
}

/// Character root marker on the parent entity. Tracks the full slot
/// loadout so queries can read the composed character's picks.
#[derive(Component, Clone, Copy, Debug)]
pub struct QuaterniusCharacter {
    pub gender: Gender,
    pub body: Option<OutfitSlot>,
    pub legs: Option<OutfitSlot>,
    pub arms: Option<OutfitSlot>,
    pub feet: Option<OutfitSlot>,
    pub head_piece: Option<HeadSlot>,
    pub hair: Option<Hair>,
    pub beard: Option<Beard>,
}

/// Bundle of per-slot picks passed to [`spawn_quaternius_character`].
/// Each slot is independent — its own outfit family + color. Leave a
/// slot `None` to skip spawning that piece.
///
/// For the common "one color for the whole character" case (museum
/// slider, char-create preview), use [`QuaterniusOutfit::uniform`].
///
/// `PartialEq`/`Eq` let the client render loop detect actual visual
/// changes and skip respawning when the underlying equipment snapshot
/// ticked but produced the same outfit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuaterniusOutfit {
    pub body: Option<OutfitSlot>,
    pub legs: Option<OutfitSlot>,
    pub arms: Option<OutfitSlot>,
    pub feet: Option<OutfitSlot>,
    pub head_piece: Option<HeadSlot>,
    pub hair: Option<Hair>,
    pub beard: Option<Beard>,
}

impl QuaterniusOutfit {
    /// Convenience constructor: paint every outfit-bearing slot with the
    /// same color. Matches the pre-per-slot-color API for callers that
    /// have a single palette pick (e.g. the museum composer's palette
    /// slider).
    pub fn uniform(
        body: Option<Outfit>,
        legs: Option<Outfit>,
        arms: Option<Outfit>,
        feet: Option<Outfit>,
        head_piece: Option<HeadPiece>,
        hair: Option<Hair>,
        beard: Option<Beard>,
        color: ColorVariant,
    ) -> Self {
        Self {
            body: body.map(|o| OutfitSlot::new(o, color)),
            legs: legs.map(|o| OutfitSlot::new(o, color)),
            arms: arms.map(|o| OutfitSlot::new(o, color)),
            feet: feet.map(|o| OutfitSlot::new(o, color)),
            head_piece: head_piece.map(|p| HeadSlot::new(p, color)),
            hair,
            beard,
        }
    }

    /// Empty character — no outfit pieces, no head piece, no hair.
    /// Starting point for resolvers that fill slots conditionally.
    pub const fn empty() -> Self {
        Self {
            body: None,
            legs: None,
            arms: None,
            feet: None,
            head_piece: None,
            hair: None,
            beard: None,
        }
    }
}

/// Marker on the head-scene child entity. Triggers
/// [`hide_non_head_regions`] to walk the spawned hierarchy and hide
/// every body-region mesh node so only the head remains visible.
#[derive(Component)]
pub struct HideNonHeadRegions;

/// Marker on the outfit-scene child entity. Records which outfit
/// this scene was spawned for + which color variant is desired, so
/// [`apply_outfit_color`] can walk the unique materials under this
/// scene and swap their `base_color_texture` to the requested palette.
#[derive(Component)]
pub struct OutfitColor {
    pub outfit: Outfit,
    pub desired: ColorVariant,
    /// Last-applied variant; `None` means "never applied yet —
    /// system should run even if desired == Default".
    pub applied: Option<ColorVariant>,
}

// --- spawn helper --------------------------------------------------------

/// Spawn a Quaternius character composed slot-by-slot. Every slot is
/// optional — `None` skips that piece. The color variant applies to
/// every outfit-bearing slot (body / legs / arms / feet / head piece).
///
/// Returns the parent entity.
pub fn spawn_quaternius_character(
    commands: &mut Commands,
    assets: &AssetServer,
    gender: Gender,
    outfit: QuaterniusOutfit,
) -> Entity {
    let parent = commands
        .spawn((
            QuaterniusCharacter {
                gender,
                body: outfit.body,
                legs: outfit.legs,
                arms: outfit.arms,
                feet: outfit.feet,
                head_piece: outfit.head_piece,
                hair: outfit.hair,
                beard: outfit.beard,
            },
            Transform::default(),
            Visibility::default(),
            // Resolves `hand_r` / `hand_l` out of the body sub-scene's
            // skeleton (body is spawned first below, so the NamedRegions
            // depth-first walker finds body's bones before any other
            // armature's). Weapon overlays read this to parent their
            // mesh to the correct palm entity.
            NamedRegions::expect(&[BONE_MAINHAND, BONE_OFFHAND]),
        ))
        .id();

    // Helper: spawn an outfit-bearing part child and tag with OutfitColor.
    // Each part carries its own color — `apply_outfit_color` reconciles
    // them independently, so naked-slot + equipped-slot can render with
    // different palettes on the same character.
    let spawn_part = |commands: &mut Commands,
                      path: String,
                      palette: Outfit,
                      color: ColorVariant| {
        let child = commands
            .spawn((
                SceneRoot(assets.load(path)),
                AnimatedRig(Rig::QuaterniusModular),
                OutfitColor {
                    outfit: palette,
                    desired: color,
                    applied: None,
                },
                ChildOf(parent),
            ))
            .id();
        commands.entity(parent).add_child(child);
    };

    if let Some(s) = outfit.body {
        spawn_part(commands, s.outfit.body_part_path(gender), s.outfit, s.color);
    }
    if let Some(s) = outfit.legs {
        spawn_part(commands, s.outfit.legs_part_path(gender), s.outfit, s.color);
    }
    if let Some(s) = outfit.arms {
        spawn_part(commands, s.outfit.arms_part_path(gender), s.outfit, s.color);
    }
    if let Some(s) = outfit.feet {
        spawn_part(commands, s.outfit.feet_part_path(gender), s.outfit, s.color);
    }
    if let Some(hs) = outfit.head_piece {
        spawn_part(commands, hs.piece.asset_path(gender), hs.piece.outfit(), hs.color);
    }

    // Head (Superhero Split with non-head regions hidden).
    let head_gltf = match gender {
        Gender::Male => "extracted/characters/base/Superhero_Male_FullBody_Split.gltf#Scene0",
        Gender::Female => "extracted/characters/base/Superhero_Female_FullBody_Split.gltf#Scene0",
    };
    let head_child = commands
        .spawn((
            SceneRoot(assets.load(head_gltf)),
            AnimatedRig(Rig::QuaterniusModular),
            HideNonHeadRegions,
            ChildOf(parent),
        ))
        .id();
    commands.entity(parent).add_child(head_child);

    // Hair (no color — hair uses its own palette).
    if let Some(h) = outfit.hair {
        let hair_child = commands
            .spawn((
                SceneRoot(assets.load(h.asset_path(gender))),
                AnimatedRig(Rig::QuaterniusModular),
                ChildOf(parent),
            ))
            .id();
        commands.entity(parent).add_child(hair_child);
    }

    // Beard — male-only, separate mesh (Hair_Beard.gltf).
    if let (Gender::Male, Some(b)) = (gender, outfit.beard) {
        let beard_child = commands
            .spawn((
                SceneRoot(assets.load(b.asset_path())),
                AnimatedRig(Rig::QuaterniusModular),
                ChildOf(parent),
            ))
            .id();
        commands.entity(parent).add_child(beard_child);
    }

    parent
}

// --- systems -------------------------------------------------------------

/// Walks every [`HideNonHeadRegions`] scene's descendants looking for
/// named nodes matching `Region_{Torso,LeftArm,RightArm,LeftLeg,RightLeg}`
/// and sets `Visibility::Hidden` on them — mesh primitive children
/// inherit the hidden state, so the outfit covers those regions
/// instead. `Region_Head`, `Eyes`, `Eyebrows` stay visible.
///
/// Bevy's glTF loader spawns the scene tree over multiple frames. We
/// keep the marker until we've actually seen at least one of the
/// named regions in the hierarchy — early passes would otherwise
/// strip the marker before the named nodes materialise.
pub fn hide_non_head_regions(
    mut commands: Commands,
    roots: Query<Entity, With<HideNonHeadRegions>>,
    children: Query<&Children>,
    names: Query<&Name>,
) {
    for root in &roots {
        let mut saw_target = false;
        let mut stack = vec![root];
        while let Some(e) = stack.pop() {
            if let Ok(n) = names.get(e) {
                let name = n.as_str();
                if is_recognised_region(name) {
                    saw_target = true;
                    if should_hide(name) {
                        commands.entity(e).insert(Visibility::Hidden);
                    }
                }
            }
            if let Ok(kids) = children.get(e) {
                for &c in kids {
                    stack.push(c);
                }
            }
        }
        if saw_target {
            commands.entity(root).remove::<HideNonHeadRegions>();
        }
    }
}

/// Walks each [`OutfitColor`] scene and replaces the
/// `base_color_texture` on every outfit material with the variant
/// requested by `OutfitColor::desired`. Unique material handles are
/// only touched once per run; skips when `desired == applied`.
///
/// Runs each frame — scenes stream in across frames, so the first
/// successful apply may need several passes before every material
/// exists in `Assets<StandardMaterial>`. Once any mesh-material is
/// actually written we record the applied variant and go idle.
pub fn apply_outfit_color(
    mut outfits: Query<(Entity, &mut OutfitColor)>,
    children: Query<&Children>,
    mesh_mats: Query<&MeshMaterial3d<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    assets: Res<AssetServer>,
) {
    use bevy::platform::collections::HashSet;

    for (root, mut color) in &mut outfits {
        if color.applied == Some(color.desired) {
            continue;
        }
        let new_tex: Handle<Image> = assets.load(color.desired.base_color_path(color.outfit));

        let mut seen: HashSet<bevy::asset::AssetId<StandardMaterial>> = HashSet::default();
        let mut touched = false;
        let mut stack = vec![root];
        while let Some(e) = stack.pop() {
            if let Ok(handle) = mesh_mats.get(e) {
                let id = handle.0.id();
                if seen.insert(id) {
                    if let Some(mat) = materials.get_mut(&handle.0) {
                        mat.base_color_texture = Some(new_tex.clone());
                        touched = true;
                    }
                }
            }
            if let Ok(kids) = children.get(e) {
                for &c in kids {
                    stack.push(c);
                }
            }
        }
        if touched {
            color.applied = Some(color.desired);
        }
    }
}

fn is_recognised_region(name: &str) -> bool {
    name == "Region_Head"
        || name == "Region_Torso"
        || name == "Region_LeftArm"
        || name == "Region_RightArm"
        || name == "Region_LeftLeg"
        || name == "Region_RightLeg"
        || name == "Eyes"
        || name == "Eyebrows"
}

fn should_hide(name: &str) -> bool {
    // Keep head-relevant parts; hide every body region we're
    // replacing with the outfit.
    !(name == "Region_Head" || name == "Eyes" || name == "Eyebrows")
}
