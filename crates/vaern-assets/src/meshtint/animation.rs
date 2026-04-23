//! Animation pipeline for skinned characters.
//!
//! Two rig conventions are supported in parallel:
//! - **Meshtint** — `Rig::Meshtint`. Local Meshtint clips (see content
//!   notes below). Meshtint-authored content only; UE5 clips cannot
//!   drive these bones (different bone names + bind poses).
//! - **Quaternius modular** — `Rig::QuaterniusModular`. The Unreal
//!   Mannequin skeleton shared by Quaternius's Universal Base
//!   Characters, modular-outfit parts, and the Universal Animation
//!   Library (UAL1 + UAL2, 86 clips). UAL plays natively — no
//!   retargeting needed.
//!
//! Pipeline:
//!
//! 1. Scan filesystem + declared multi-anim GLBs into [`MeshtintAnimationCatalog`].
//! 2. Kick off async loads of every referenced GLB at startup.
//! 3. Once all Gltf assets are resolved, build **one shared
//!    [`AnimationGraph`]** with one node per clip and stash it in
//!    [`MeshtintAnimations`].
//! 4. For each character entity marked [`AnimatedRig`], install
//!    `AnimationPlayer` + graph handle + transitions on the scene's
//!    top-level glTF node, then walk descendants inserting
//!    `AnimationTargetId` + `AnimatedBy` using the same Name-path
//!    convention `bevy_gltf` uses at load time.
//!
//! # Known content gaps
//!
//! - **Meshtint**: 7 of 11 shipped clip FBXs have zero bone keyframes
//!   (fbx2gltf produces 0 samplers); Female Walking's FBX take range is
//!   `[12-14]`, yielding a 0.067s twitch. Only Female Talking, Male Arm
//!   Waist, Male Working 02 animate properly.
//! - **UAL + Quaternius**: 86 production clips across UAL1 + UAL2.
//!   Locomotion / combat / death / jump / roll / farming / ... All
//!   drive the Quaternius modular character natively.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use bevy::animation::graph::{AnimationGraph, AnimationGraphHandle, AnimationNodeIndex};
use bevy::animation::transition::AnimationTransitions;
use bevy::animation::{AnimatedBy, AnimationPlayer, AnimationTargetId};
use bevy::gltf::Gltf;
use bevy::prelude::*;

use super::Gender;

/// Meshtint animation folder relative to the asset server root.
pub const MESHTINT_ANIM_FOLDER_REL: &str = "extracted/meshtint/animations";

/// Back-compat re-export of the old name.
pub const ANIM_FOLDER_REL: &str = MESHTINT_ANIM_FOLDER_REL;

/// Universal Animation Library GLB paths (UE Mannequin skeleton).
/// Drive the Quaternius modular character natively.
pub const UAL_GLB_PATHS: &[&str] = &[
    "extracted/animations/UAL1_Standard.glb",
    "extracted/animations/UAL2_Standard.glb",
];

/// Which skeleton a clip (or character) uses. Clips from one rig will
/// not drive bones spawned under a different rig.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Rig {
    /// Meshtint's `Rig*` bone convention (RigPelvis, RigLThigh, …).
    Meshtint,
    /// Unreal Mannequin convention (pelvis, thigh_l, spine_01, …) used
    /// by the Quaternius Universal Base Characters + modular outfit
    /// pack + the Universal Animation Library.
    QuaterniusModular,
}

impl Rig {
    pub const ALL: &'static [Rig] = &[Rig::Meshtint, Rig::QuaterniusModular];

    pub fn label(self) -> &'static str {
        match self {
            Rig::Meshtint => "Meshtint",
            Rig::QuaterniusModular => "Quaternius",
        }
    }
}

/// One playable clip.
#[derive(Clone, Debug)]
pub struct AnimationClipSrc {
    /// Unique key across the whole catalog — what
    /// [`MeshtintAnimations::node_for`] keys on.
    pub key: String,
    /// Human-friendly label for UI.
    pub pretty: String,
    /// Which rig this clip is authored against.
    pub rig: Rig,
    /// For Meshtint clips only. `None` for rig-agnostic sources.
    pub gender: Option<Gender>,
    /// Asset-server-relative GLB path the clip lives in.
    pub glb_path: String,
    /// If `Some`, look up the clip by this name in the GLB's
    /// `named_animations`. If `None`, take the first positional
    /// animation (used for Meshtint clip GLBs where every clip is
    /// named `Take 001`).
    pub anim_name: Option<String>,
}

/// Filesystem + declared-source scan.
#[derive(Resource, Default, Debug)]
pub struct MeshtintAnimationCatalog {
    pub clips: Vec<AnimationClipSrc>,
}

impl MeshtintAnimationCatalog {
    /// Scan Meshtint's animation folder and discover UAL multi-anim GLBs.
    ///
    /// `assets_root` is the on-disk path to the asset folder — not the
    /// Bevy asset-server prefix.
    pub fn scan(assets_root: impl AsRef<Path>) -> Self {
        let assets_root = assets_root.as_ref();
        let mut clips = Vec::new();
        // UAL multi-anim GLBs — drive Quaternius characters natively.
        scan_universal_glbs(assets_root, &mut clips);
        clips.sort_by(|a, b| a.rig.cmp(&b.rig).then_with(|| a.pretty.cmp(&b.pretty)));
        let quaternius = clips
            .iter()
            .filter(|c| c.rig == Rig::QuaterniusModular)
            .count();
        info!(
            "AnimationCatalog built: {} Quaternius clips (UAL)",
            quaternius
        );
        Self { clips }
    }

    pub fn get(&self, key: &str) -> Option<&AnimationClipSrc> {
        self.clips.iter().find(|c| c.key == key)
    }

    pub fn iter_rig(&self, rig: Rig) -> impl Iterator<Item = &AnimationClipSrc> {
        self.clips.iter().filter(move |c| c.rig == rig)
    }
}

/// Peek into UAL GLBs on disk to enumerate their named animations. Reads
/// the JSON chunk directly — same technique `scripts/` uses for bulk
/// clip inspection; avoids paying for the full Bevy asset pipeline just
/// to know the names.
fn scan_universal_glbs(assets_root: &Path, clips: &mut Vec<AnimationClipSrc>) {
    for &rel in UAL_GLB_PATHS {
        let abs = assets_root.join(rel);
        match read_glb_animation_names(&abs) {
            Ok(names) => {
                for name in names {
                    // Skip the T-pose reference clip; it's included in
                    // every UAL for retargeting purposes but is not a
                    // playable animation.
                    if name == "A_TPose" {
                        continue;
                    }
                    let key = format!("UAL_{name}");
                    let pretty = name.replace('_', " ");
                    clips.push(AnimationClipSrc {
                        key,
                        pretty,
                        rig: Rig::QuaterniusModular,
                        gender: None,
                        glb_path: rel.to_string(),
                        anim_name: Some(name),
                    });
                }
            }
            Err(e) => {
                warn!("failed to scan {} for animations: {}", abs.display(), e);
            }
        }
    }
}

fn read_glb_animation_names(path: &Path) -> Result<Vec<String>, String> {
    use std::io::Read;
    let mut f = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hdr = [0u8; 12];
    f.read_exact(&mut hdr).map_err(|e| e.to_string())?;
    if &hdr[0..4] != b"glTF" {
        return Err("not a glTF binary".into());
    }
    let mut chunk_hdr = [0u8; 8];
    f.read_exact(&mut chunk_hdr).map_err(|e| e.to_string())?;
    let json_len = u32::from_le_bytes(chunk_hdr[0..4].try_into().unwrap()) as usize;
    let mut json_bytes = vec![0u8; json_len];
    f.read_exact(&mut json_bytes).map_err(|e| e.to_string())?;

    // Minimal grep-style extraction of top-level animation `"name"` fields.
    // Avoids pulling serde_json as a crate dep for this one-shot scan.
    let s = std::str::from_utf8(&json_bytes).map_err(|e| e.to_string())?;
    let Some(anims_start) = s.find("\"animations\"") else {
        return Ok(Vec::new());
    };
    // Find the balanced `[ ... ]` following "animations":
    let mut depth = 0isize;
    let mut start = None;
    let mut end = None;
    for (i, ch) in s[anims_start..].char_indices() {
        match ch {
            '[' => {
                if depth == 0 {
                    start = Some(anims_start + i + 1);
                }
                depth += 1;
            }
            ']' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(anims_start + i);
                    break;
                }
            }
            _ => {}
        }
    }
    let (Some(s_start), Some(s_end)) = (start, end) else {
        return Err("unterminated animations array".into());
    };
    let slice = &s[s_start..s_end];
    // Pull every `"name":"..."` — simplistic, assumes well-formed glTF.
    let mut out = Vec::new();
    let mut idx = 0;
    while let Some(name_at) = slice[idx..].find("\"name\"") {
        let abs = idx + name_at + "\"name\"".len();
        let Some(colon) = slice[abs..].find(':') else { break };
        let rest = &slice[abs + colon + 1..];
        let Some(q1) = rest.find('"') else { break };
        let after_q1 = &rest[q1 + 1..];
        let Some(q2) = after_q1.find('"') else { break };
        let name = &after_q1[..q2];
        out.push(name.to_string());
        idx = abs + colon + 1 + q1 + 1 + q2 + 1;
    }
    Ok(out)
}

/// Shared animation graph + per-clip node lookup. Empty until
/// [`build_animation_graph`] finishes (every referenced GLB must finish
/// loading first).
#[derive(Resource, Default)]
pub struct MeshtintAnimations {
    pending: Vec<(AnimationClipSrc, Handle<Gltf>)>,
    /// `clip.key` → node index inside the shared graph.
    pub nodes: HashMap<String, AnimationNodeIndex>,
    /// Handle to the shared graph asset; `None` until built.
    pub graph: Option<Handle<AnimationGraph>>,
}

impl MeshtintAnimations {
    pub fn is_ready(&self) -> bool {
        self.graph.is_some()
    }

    pub fn node_for(&self, key: &str) -> Option<AnimationNodeIndex> {
        self.nodes.get(key).copied()
    }
}

/// Marker: put on any character entity whose scene you want animated.
/// Every [`MeshtintCharacterBundle`] includes it automatically; other
/// character sources (e.g. a raw UBC Mannequin spawn) must insert it
/// alongside their `SceneRoot`.
#[derive(Component, Clone, Copy, Debug)]
pub struct AnimatedRig(pub Rig);

/// Marker: the installer has already wired this character's animation
/// player + targets. Prevents re-running every frame.
#[derive(Component)]
pub struct AnimationPlayerInstalled;

// --- Systems --------------------------------------------------------------

/// Startup: kick off async load of every catalogued clip's GLB.
pub fn load_animation_sources(
    assets: Res<AssetServer>,
    catalog: Option<Res<MeshtintAnimationCatalog>>,
    mut state: ResMut<MeshtintAnimations>,
) {
    let Some(catalog) = catalog else { return };
    if !state.pending.is_empty() || state.is_ready() {
        return;
    }
    for src in &catalog.clips {
        let h: Handle<Gltf> = assets.load(&src.glb_path);
        state.pending.push((src.clone(), h));
    }
    info!(
        "MeshtintAnimations: queued {} clip entries for load",
        state.pending.len()
    );
}

/// Update: once every pending Gltf has resolved, fold every clip into a
/// shared graph and publish the handle.
pub fn build_animation_graph(
    mut state: ResMut<MeshtintAnimations>,
    gltfs: Res<Assets<Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
) {
    if state.is_ready() || state.pending.is_empty() {
        return;
    }
    if !state.pending.iter().all(|(_, h)| gltfs.get(h).is_some()) {
        return;
    }

    let mut graph = AnimationGraph::new();
    let root = graph.root;
    let mut nodes = HashMap::new();
    let pending = std::mem::take(&mut state.pending);
    for (src, handle) in &pending {
        let Some(gltf) = gltfs.get(handle) else {
            warn!("Gltf disappeared mid-build for {}", src.key);
            continue;
        };
        let clip_handle = match &src.anim_name {
            Some(name) => gltf.named_animations.get(name.as_str()).cloned(),
            None => gltf.animations.first().cloned(),
        };
        let Some(clip) = clip_handle else {
            warn!(
                "no AnimationClip for {} (anim_name={:?}) in {}",
                src.key, src.anim_name, src.glb_path,
            );
            continue;
        };
        let node = graph.add_clip(clip, 1.0, root);
        nodes.insert(src.key.clone(), node);
    }
    let handle = graphs.add(graph);
    info!("MeshtintAnimations graph built with {} clips", nodes.len());
    state.nodes = nodes;
    state.graph = Some(handle);
}

/// Update: for each [`AnimatedRig`] character whose scene has spawned:
///
/// 1. Find the scene's top-level glTF node (first named direct child of
///    the character entity — `RootNode` for Meshtint, `Armature` for
///    UBC Mannequins).
/// 2. Install `AnimationPlayer` + graph handle + transitions there.
/// 3. Walk the subtree and insert `AnimationTargetId` + `AnimatedBy`
///    using the same Name-path convention `bevy_gltf` uses at load time
///    (path includes the top-level node's own name, then each
///    descendant's name).
///
/// Step 3 is load-bearing: base character GLBs ship without animations,
/// so `bevy_gltf` never tags their bone entities. Without this manual
/// installation every clip channel falls on the floor and the mannequin
/// stays in bind pose.
///
/// Idempotent — skips characters already marked with
/// [`AnimationPlayerInstalled`].
pub fn install_character_animation_player(
    mut commands: Commands,
    state: Res<MeshtintAnimations>,
    characters: Query<(Entity, &AnimatedRig), Without<AnimationPlayerInstalled>>,
    children: Query<&Children>,
    names: Query<&Name>,
    existing_players: Query<&AnimationPlayer>,
    existing_targets: Query<&AnimatedBy>,
) {
    let Some(graph) = state.graph.clone() else {
        return;
    };
    for (character, rig) in &characters {
        let Some(scene_root) = first_named_child(character, &children, &names) else {
            continue; // scene not spawned yet — try next frame
        };
        // Install each of (AnimationPlayer, AnimationGraphHandle,
        // AnimationTransitions) only if missing. Bevy's glTF loader
        // auto-inserts AnimationPlayer on the armature root whenever
        // the GLB has embedded animations (several Quaternius Female
        // Knight / Peasant / Wizard parts do). Gating the whole insert
        // on "no player yet" misses those cases — the auto-installed
        // player lacks our graph handle + transitions, and sync plays
        // silently fail on it.
        if existing_players.get(scene_root).is_err() {
            commands
                .entity(scene_root)
                .insert(AnimationPlayer::default());
        }
        commands
            .entity(scene_root)
            .insert((AnimationGraphHandle(graph.clone()), AnimationTransitions::new()));
        let mut target_count = 0;
        install_animation_targets(
            scene_root,
            &[],
            scene_root,
            rig.0,
            &children,
            &names,
            &existing_targets,
            &mut commands,
            &mut target_count,
        );
        info!(
            "{:?} character {:?}: animation player + {} targets installed",
            rig.0, character, target_count
        );
        commands
            .entity(character)
            .insert(AnimationPlayerInstalled);
    }
}

/// Recursively tag `entity` + descendants with `AnimationTargetId` +
/// `AnimatedBy(player)`. Path matches hierarchy (same convention
/// `bevy_gltf::gltf_ext::scene::collect_path` uses at load time):
/// start empty, push each node's Name as we descend. Clip GLBs were
/// authored against the same hierarchy convention, so target hashes
/// line up.
fn install_animation_targets(
    entity: Entity,
    parent_path: &[Name],
    player: Entity,
    _rig: Rig,
    children: &Query<&Children>,
    names: &Query<&Name>,
    existing_targets: &Query<&AnimatedBy>,
    commands: &mut Commands,
    count: &mut usize,
) {
    let Ok(name) = names.get(entity) else {
        return;
    };
    let mut path = parent_path.to_vec();
    path.push(name.clone());

    if existing_targets.get(entity).is_err() {
        let id = AnimationTargetId::from_names(path.iter());
        commands.entity(entity).insert((id, AnimatedBy(player)));
        *count += 1;
    }

    if let Ok(kids) = children.get(entity) {
        for &c in kids {
            install_animation_targets(
                c,
                &path,
                player,
                _rig,
                children,
                names,
                existing_targets,
                commands,
                count,
            );
        }
    }
}

/// BFS for the shallowest named descendant of `root`. Bevy's scene
/// loader sometimes wraps scene content in an unnamed intermediate
/// entity, so direct-children-only search misses the actual glTF top
/// node. BFS guarantees we land on whatever the first Name-bearing
/// entity is: `RootNode` for Meshtint, `Armature` for UBC.
fn first_named_child(
    root: Entity,
    children: &Query<&Children>,
    names: &Query<&Name>,
) -> Option<Entity> {
    let mut queue: std::collections::VecDeque<Entity> = std::collections::VecDeque::new();
    if let Ok(kids) = children.get(root) {
        for &c in kids {
            queue.push_back(c);
        }
    }
    while let Some(e) = queue.pop_front() {
        if names.get(e).is_ok() {
            return Some(e);
        }
        if let Ok(kids) = children.get(e) {
            for &c in kids {
                queue.push_back(c);
            }
        }
    }
    None
}
