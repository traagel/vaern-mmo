//! Entity → visual mesh mapping.
//!
//! Three lanes:
//!
//! * **NPCs** (`With<Replicated> + With<Health> + Without<PlayerTag>`) —
//!   blue cuboid child; trivially cheap, no per-frame sync.
//! * **Own predicted player** (`With<Predicted> + With<PlayerTag>`) —
//!   tagged with [`Player`] + [`PlayerVisual`]. The actual Quaternius
//!   mesh is spawned by [`sync_own_player_visual`] once `OwnEquipped` +
//!   `SelectedCharacter` resources exist.
//! * **Remote interpolated players** (`With<Interpolated> +
//!   With<PlayerTag>`) — Quaternius mesh built from the replicated
//!   [`PlayerAppearance`]. Server folds gear into appearance on every
//!   `Changed<Equipped>`, so armor swaps re-trigger the respawn path
//!   through [`sync_remote_player_visual`].

use bevy::prelude::*;
use lightyear::input::native::prelude::InputMarker;
use lightyear::prelude::*;
use vaern_assets::{
    outfit_from_equipped, spawn_quaternius_character, weapon_props_for_archetype,
    weapon_props_from_equipped, AnimalCatalog, QuaterniusOutfit, QuaterniusWeaponOverlay,
};
use vaern_combat::Health;
use vaern_protocol::{Inputs, NpcAppearance, NpcMesh, PlayerAppearance, PlayerTag, PlayerWeapons};

use crate::ArchetypeTableRes;

use crate::inventory_ui::{ClientContent, OwnEquipped};
use crate::menu::{AppState, SelectedCharacter};
use crate::shared::{attach_mesh_child, ModelAttached, Npc, Player, RemotePlayer};

// --- components -------------------------------------------------------------

/// Marker on the own-player entity that tracks the currently-rendered
/// Quaternius character + which outfit it was last spawned for, so the
/// sync system can skip respawning when `OwnEquipped` ticks but the
/// resolved outfit hasn't actually changed.
///
/// The `mainhand` / `offhand` fields track the weapon-overlay entity
/// parented to the currently-spawned character. On character respawn
/// they auto-despawn with the tree; the weapon sync system sees the
/// cleared `applied_mainhand` / `applied_offhand` and re-spawns.
#[derive(Component, Debug, Default)]
pub(super) struct PlayerVisual {
    pub child: Option<Entity>,
    pub applied: Option<QuaterniusOutfit>,
    pub mainhand: Option<Entity>,
    pub offhand: Option<Entity>,
    pub applied_mainhand: Option<String>,
    pub applied_offhand: Option<String>,
}

/// Same pattern as [`PlayerVisual`] but for remote players. The outfit
/// + weapon source is the server-replicated [`PlayerAppearance`] +
/// [`PlayerWeapons`] components rather than `OwnEquipped` +
/// `SelectedCharacter.cosmetics`, since the own player is the only
/// client that has its own gear snapshot resource.
#[derive(Component, Debug, Default)]
pub(super) struct RemoteVisual {
    pub child: Option<Entity>,
    pub applied: Option<QuaterniusOutfit>,
    pub mainhand: Option<Entity>,
    pub offhand: Option<Entity>,
    pub applied_mainhand: Option<String>,
    pub applied_offhand: Option<String>,
}

/// Marker for a humanoid NPC's spawned Quaternius character root.
/// Unlike [`PlayerVisual`] / [`RemoteVisual`], NPCs don't change
/// outfit mid-life, so we only need the child entity id — no
/// applied-state tracking. The animation driver walks down from
/// `child` to find every `AnimationPlayer` in the Quaternius
/// subtree and retargets them to the clip keyed by the replicated
/// `AnimState`.
#[derive(Component, Debug)]
pub(super) struct NpcHumanoidVisual {
    pub child: Entity,
}

// --- plugin -----------------------------------------------------------------

pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                render_replicated_npcs,
                render_predicted_player,
                // Weapon overlays must run AFTER the outfit sync on
                // the same frame — the outfit sync clears our weapon
                // tracking on character respawn, and we want the new
                // weapons parented against the newly-spawned character
                // without a one-frame gap.
                (sync_own_player_visual, sync_own_player_weapons).chain(),
                (sync_remote_player_visual, sync_remote_player_weapons).chain(),
            )
                .run_if(in_state(AppState::InGame)),
        );
    }
}

// --- NPCs -------------------------------------------------------------------

/// Server-replicated NPCs (Health but no PlayerTag). Three render
/// paths fall out of the replicated-component mix:
///
/// - `NpcMesh` present → EverythingLibrary GLB (beast path).
/// - `NpcAppearance` present → Quaternius modular character
///   (humanoid path — bandits, cultists, guards, …).
/// - neither → blue cuboid fallback.
///
/// Timing note: both replicate alongside `Health` on the spawn
/// bundle; lightyear 0.26 delivers bundled components on the same
/// tick so by the time we see the entity here, whichever of the two
/// components the server set is already present. `ModelAttached`
/// gates re-entry so outfit changes mid-life would need a different
/// mechanism (NPCs don't change appearance today).
fn render_replicated_npcs(
    new_npcs: Query<
        (Entity, Option<&NpcMesh>, Option<&NpcAppearance>),
        (
            With<Replicated>,
            With<Health>,
            Without<PlayerTag>,
            Without<ModelAttached>,
        ),
    >,
    catalog: Res<AnimalCatalog>,
    archetypes: Res<ArchetypeTableRes>,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    for (entity, npc_mesh, npc_appearance) in &new_npcs {
        // Path 1: beast mesh.
        if let Some(npc_mesh) = npc_mesh {
            if let Some(entry) = catalog.get(&npc_mesh.species) {
                // Quaternius / EverythingLibrary meshes both face +Z
                // in bind pose; Bevy's forward is -Z, so rotate 180°
                // around Y so the mesh faces the same way the parent
                // entity's Transform does.
                let child = commands
                    .spawn((
                        SceneRoot(assets.load(format!("{}#Scene0", entry.path))),
                        Transform::from_scale(Vec3::splat(npc_mesh.scale))
                            .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
                    ))
                    .id();
                commands.entity(entity).add_child(child);
                if let Ok(mut ec) = commands.get_entity(entity) {
                    ec.insert((Npc, ModelAttached));
                }
                continue;
            }
            warn!(
                "NPC {entity:?} has NpcMesh species={:?} but it's not in the catalog — \
                 falling back to cuboid",
                npc_mesh.species
            );
        }

        // Path 2: humanoid Quaternius character. Expand the small
        // replicated archetype key into a full cosmetics bundle
        // using the client-loaded archetype table, then spawn the
        // modular Quaternius character. Unknown keys log and fall
        // through to the cuboid path below.
        if let Some(appearance) = npc_appearance {
            if let Some(cosmetics) = archetypes.resolve(&appearance.archetype) {
                let outfit = cosmetics.to_outfit();
                let gender = cosmetics.gender;
                let child =
                    spawn_quaternius_character(&mut commands, &assets, gender, outfit);
                commands.entity(child).insert(
                    Transform::from_scale(Vec3::splat(appearance.scale()))
                        .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
                );
                commands.entity(entity).add_child(child);
                if let Ok(mut ec) = commands.get_entity(entity) {
                    // Track the Quaternius root so
                    // `drive_npc_humanoid_animation` can find the
                    // subtree's `AnimationPlayer`s without walking
                    // the whole world.
                    ec.insert((Npc, ModelAttached, NpcHumanoidVisual { child }));
                }
                // Archetype-driven weapon overlay — knights carry sword
                // + shield, nobles sword, rangers knife, peasants axe,
                // wizards empty-handed. NPCs don't have replicated
                // equipment today so the prop follows the visual role.
                // One-shot spawn — NPC armament never changes mid-life.
                let props = weapon_props_for_archetype(&appearance.archetype);
                if let Some(prop_id) = props.mainhand {
                    commands.spawn(QuaterniusWeaponOverlay {
                        target: child,
                        prop_id,
                    });
                }
                if let Some(prop_id) = props.offhand {
                    commands.spawn(QuaterniusWeaponOverlay {
                        target: child,
                        prop_id,
                    });
                }
                continue;
            }
            warn!(
                "NPC {entity:?} NpcAppearance.archetype={:?} not in client table — \
                 falling back to cuboid",
                appearance.archetype
            );
        }

        // Path 3: cuboid fallback.
        let mesh = meshes.add(Cuboid::new(1.2, 1.8, 1.2));
        let material = materials.add(StandardMaterial {
            base_color: Color::srgb(0.25, 0.45, 0.85),
            ..default()
        });
        attach_mesh_child(entity, mesh, material, &mut commands);
        if let Ok(mut ec) = commands.get_entity(entity) {
            ec.insert((Npc, ModelAttached));
        }
    }
}

// --- own player -------------------------------------------------------------

/// Tag the predicted player with [`Player`] + [`InputMarker`] + an empty
/// [`PlayerVisual`]. The actual Quaternius character mesh is spawned by
/// [`sync_own_player_visual`] once `ClientContent` + `SelectedCharacter`
/// are available, so this system doesn't block on asset or resource
/// readiness.
fn render_predicted_player(
    new: Query<Entity, (With<Predicted>, With<PlayerTag>, Without<ModelAttached>)>,
    mut commands: Commands,
) {
    for entity in &new {
        if let Ok(mut ec) = commands.get_entity(entity) {
            ec.insert((
                Player,
                InputMarker::<Inputs>::default(),
                ModelAttached,
                PlayerVisual::default(),
            ));
        }
    }
}

/// Keep the own-player Quaternius mesh in sync with the server's
/// equipped state. Runs every frame; compares the freshly-resolved
/// [`QuaterniusOutfit`] against the last-applied one and skips work if
/// nothing changed visually (the resolver only consumes five primary
/// slots, so equipping a ring or shield produces the same outfit and
/// leaves the mesh alone).
fn sync_own_player_visual(
    mut q: Query<(Entity, &mut PlayerVisual), With<Player>>,
    equipped: Res<OwnEquipped>,
    content: Option<Res<ClientContent>>,
    selected: Option<Res<SelectedCharacter>>,
    assets: Res<AssetServer>,
    mut commands: Commands,
) {
    let Some(content) = content.as_deref() else { return };
    let Some(selected) = selected.as_deref() else { return };
    let Ok((player_entity, mut visual)) = q.single_mut() else { return };

    let mut desired = outfit_from_equipped(&equipped.slots, &content.0);
    // Overlay cosmetic picks (hair, beard, head piece) from char-create.
    // Armor head slot wins over cosmetic head piece — if a helmet is
    // equipped, render the armor-derived head_piece; otherwise fall back
    // to the user's cosmetic pick. Hair and beard layer additively.
    desired.hair = selected.cosmetics.hair_enum();
    desired.beard = selected.cosmetics.beard_enum();
    if desired.head_piece.is_none() {
        if let Some(ph) = selected.cosmetics.head_piece.as_ref() {
            if let Ok(slot) = ph.to_slot() {
                desired.head_piece = Some(slot);
            }
        }
    }
    if visual.applied == Some(desired) {
        return;
    }

    // Despawn the previous character (scene + all its part children go
    // with it — `despawn` is recursive in Bevy 0.18). Weapon overlays
    // were parented to the hand bones and despawn with the tree; drop
    // our tracking so `sync_own_player_weapons` re-spawns them against
    // the new character.
    if let Some(old) = visual.child.take() {
        if let Ok(mut ec) = commands.get_entity(old) {
            ec.despawn();
        }
    }
    visual.mainhand = None;
    visual.offhand = None;
    visual.applied_mainhand = None;
    visual.applied_offhand = None;

    let child = spawn_quaternius_character(&mut commands, &assets, selected.gender, desired);
    // Quaternius meshes face +Z in bind pose; Bevy's "forward" is -Z.
    // Without this, the character always faces the camera when the
    // player walks forward. Rotate the mesh root 180° so its local
    // forward lines up with the parent player's movement direction.
    commands
        .entity(child)
        .insert(Transform::from_rotation(Quat::from_rotation_y(
            std::f32::consts::PI,
        )));
    commands.entity(player_entity).add_child(child);
    visual.child = Some(child);
    visual.applied = Some(desired);
}

/// Spawn (or replace) the own-player weapon overlays against the
/// currently-spawned Quaternius character. Reads the mainhand/offhand
/// prop ids from `OwnEquipped` + the item registry and diffs against
/// `PlayerVisual.applied_mainhand` / `applied_offhand` to avoid
/// thrashing.
///
/// Runs every frame (cheap: one query, short-circuits on no-op) and
/// is explicitly chained after [`sync_own_player_visual`] so a
/// character respawn + re-arm happen in the same frame.
fn sync_own_player_weapons(
    mut q: Query<&mut PlayerVisual, With<Player>>,
    equipped: Res<OwnEquipped>,
    content: Option<Res<ClientContent>>,
    mut commands: Commands,
) {
    let Some(content) = content.as_deref() else { return };
    let Ok(mut visual) = q.single_mut() else { return };
    let Some(character) = visual.child else { return };

    let props = weapon_props_from_equipped(&equipped.slots, &content.0);
    // `Mut<PlayerVisual>` doesn't split-borrow via DerefMut — reach
    // through to a raw `&mut PlayerVisual` so we can pass two
    // sibling field references in the same call.
    let v = &mut *visual;
    apply_weapon_overlay(
        &mut v.mainhand,
        &mut v.applied_mainhand,
        &props.mainhand,
        character,
        &mut commands,
    );
    apply_weapon_overlay(
        &mut v.offhand,
        &mut v.applied_offhand,
        &props.offhand,
        character,
        &mut commands,
    );
}

/// Shared overlay-slot reconciler: despawn the current overlay (if
/// different from desired) and spawn a fresh one targeted at the
/// character entity. No-ops when `applied == desired`.
fn apply_weapon_overlay(
    slot: &mut Option<Entity>,
    applied: &mut Option<String>,
    desired: &Option<String>,
    target: Entity,
    commands: &mut Commands,
) {
    if applied == desired {
        return;
    }
    if let Some(old) = slot.take() {
        if let Ok(mut ec) = commands.get_entity(old) {
            ec.despawn();
        }
    }
    if let Some(prop_id) = desired.clone() {
        let new = commands
            .spawn(QuaterniusWeaponOverlay {
                target,
                prop_id,
            })
            .id();
        *slot = Some(new);
    }
    *applied = desired.clone();
}

// --- remote players ---------------------------------------------------------

/// Spawn (or replace) remote-player weapon overlays against each
/// interpolated character. Reads the replicated `PlayerWeapons`
/// component (filled server-side from `Equipped` on every gear
/// change). Runs every frame — `apply_weapon_overlay` short-circuits
/// when nothing changed.
fn sync_remote_player_weapons(
    mut q: Query<(&mut RemoteVisual, &PlayerWeapons), With<RemotePlayer>>,
    mut commands: Commands,
) {
    for (mut visual, weapons) in &mut q {
        let Some(character) = visual.child else { continue };
        let v = &mut *visual;
        apply_weapon_overlay(
            &mut v.mainhand,
            &mut v.applied_mainhand,
            &weapons.mainhand,
            character,
            &mut commands,
        );
        apply_weapon_overlay(
            &mut v.offhand,
            &mut v.applied_offhand,
            &weapons.offhand,
            character,
            &mut commands,
        );
    }
}

/// Other players (Interpolated): spawn a Quaternius character keyed on
/// the server-replicated `PlayerAppearance`. The server folds equipped
/// gear into `PlayerAppearance` on every `Changed<Equipped>` (see
/// `vaern-server::persistence::sync_player_appearance_from_gear`), so
/// swapping armor re-triggers this system on remote clients and the
/// mesh respawns with the new outfit.
fn sync_remote_player_visual(
    mut q: Query<
        (Entity, Option<&mut RemoteVisual>, &PlayerAppearance),
        (
            With<Interpolated>,
            With<PlayerTag>,
            Or<(Added<PlayerAppearance>, Changed<PlayerAppearance>)>,
        ),
    >,
    assets: Res<AssetServer>,
    mut commands: Commands,
) {
    for (entity, visual, appearance) in &mut q {
        let desired = appearance.0.to_outfit();
        let gender = appearance.0.gender;
        if let Some(mut v) = visual {
            if v.applied.as_ref() == Some(&desired) {
                continue;
            }
            if let Some(old) = v.child.take() {
                if let Ok(mut ec) = commands.get_entity(old) {
                    ec.despawn();
                }
            }
            // Weapons were children of the old tree — drop tracking so
            // `sync_remote_player_weapons` respawns against the new
            // character next frame.
            v.mainhand = None;
            v.offhand = None;
            v.applied_mainhand = None;
            v.applied_offhand = None;
            let child = spawn_quaternius_character(&mut commands, &assets, gender, desired);
            commands
                .entity(child)
                .insert(Transform::from_rotation(Quat::from_rotation_y(
                    std::f32::consts::PI,
                )));
            commands.entity(entity).add_child(child);
            v.child = Some(child);
            v.applied = Some(desired);
        } else {
            let child = spawn_quaternius_character(&mut commands, &assets, gender, desired);
            commands
                .entity(child)
                .insert(Transform::from_rotation(Quat::from_rotation_y(
                    std::f32::consts::PI,
                )));
            commands.entity(entity).add_child(child);
            commands.entity(entity).insert((
                RemotePlayer,
                RemoteVisual {
                    child: Some(child),
                    applied: Some(desired),
                    ..default()
                },
            ));
        }
    }
}
