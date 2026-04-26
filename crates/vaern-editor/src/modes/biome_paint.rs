//! Biome-paint mode — chunk-aligned biome stamping.
//!
//! Granularity: a click writes one biome to every chunk-XZ column
//! within `radius_chunks`. The override map (`BiomeOverrideMap`) stores
//! the assignment; the streamer + a per-stroke material-swap helper
//! make the change visible immediately on already-meshed chunks and on
//! any future chunks that stream in.
//!
//! Storage: `src/generated/world/biome_overrides.bin` — bincode-
//! serialized `Vec<((i32, i32), u8)>`. Saved alongside voxel edits when
//! the toolbar Save fires; loaded on Startup.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::EguiContexts;
use vaern_voxel::chunk::{ChunkCoord, ChunkStore};
use vaern_voxel::plugin::ChunkRenderTag;
use vaern_voxel::query::raycast;

use super::{active_mode_is, EditorMode};
use crate::camera::FreeFlyCamera;
use crate::state::EditorAppState;
use crate::ui::console::ConsoleLog;
use crate::voxel::biomes::BiomeKey;
use crate::voxel::overrides::BiomeOverrideMap;
use crate::voxel::stream::build_biome_material;
use crate::voxel::{BiomeMaterials, ChunkBiomeMap};

const PAINT_RAY_MAX_DIST: f32 = 500.0;

#[derive(Resource, Debug, Clone)]
pub struct BiomePaintState {
    pub selected: BiomeKey,
    pub radius_chunks: u32,
}

impl Default for BiomePaintState {
    fn default() -> Self {
        Self {
            selected: BiomeKey::Grass,
            radius_chunks: 0,
        }
    }
}

pub const MAX_PAINT_RADIUS_CHUNKS: u32 = 4;

pub struct BiomePaintModePlugin;

impl Plugin for BiomePaintModePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BiomePaintState>().add_systems(
            Update,
            apply_biome_paint_on_click
                .run_if(in_state(EditorAppState::Editing))
                .run_if(active_mode_is(EditorMode::BiomePaint)),
        );
    }
}

/// Cursor + LMB → resolve chunk under cursor → set override entries
/// for the brush footprint → swap `MeshMaterial3d` directly on each
/// rendered chunk entity at the painted (cx, cz) columns.
#[allow(clippy::too_many_arguments)]
pub fn apply_biome_paint_on_click(
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<FreeFlyCamera>>,
    store: Res<ChunkStore>,
    mut overrides: ResMut<BiomeOverrideMap>,
    mut chunk_biomes: ResMut<ChunkBiomeMap>,
    mut biome_mats: ResMut<BiomeMaterials>,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    paint: Res<BiomePaintState>,
    chunks_q: Query<(Entity, &ChunkRenderTag)>,
    mut commands: Commands,
    mut log: ResMut<ConsoleLog>,
    mut egui: EguiContexts,
) {
    if !mouse_buttons.just_pressed(MouseButton::Left) {
        return;
    }
    let egui_owns = egui
        .ctx_mut()
        .map(|c| c.is_pointer_over_area() || c.wants_pointer_input())
        .unwrap_or(false);
    if egui_owns {
        return;
    }
    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let Ok((cam, cam_xform)) = cameras.single() else {
        return;
    };
    let ray = match cam.viewport_to_world(cam_xform, cursor) {
        Ok(r) => r,
        Err(_) => return,
    };
    let Some(hit) = raycast(&store, ray.origin, *ray.direction, PAINT_RAY_MAX_DIST) else {
        log.push("biome paint: no surface under cursor");
        return;
    };

    let center = ChunkCoord::containing(hit.position);
    let r = paint.radius_chunks as i32;
    let new_biome = paint.selected;

    // Ensure the new biome's material exists in the cache before we
    // start swapping handles on chunk entities.
    let new_handle = biome_mats
        .0
        .entry(new_biome)
        .or_insert_with(|| build_biome_material(&asset_server, &mut materials, new_biome))
        .clone();

    let mut painted_columns: std::collections::HashSet<(i32, i32)> =
        std::collections::HashSet::new();
    for dz in -r..=r {
        for dx in -r..=r {
            let cx = center.0.x + dx;
            let cz = center.0.z + dz;
            overrides.set(cx, cz, new_biome);
            painted_columns.insert((cx, cz));
        }
    }

    let affected_coords: Vec<ChunkCoord> = chunk_biomes
        .by_coord
        .keys()
        .filter(|c| painted_columns.contains(&(c.0.x, c.0.z)))
        .copied()
        .collect();
    for coord in &affected_coords {
        chunk_biomes.by_coord.insert(*coord, new_biome);
    }

    let mut swapped = 0usize;
    for (entity, tag) in chunks_q.iter() {
        if painted_columns.contains(&(tag.coord.0.x, tag.coord.0.z)) {
            commands
                .entity(entity)
                .insert(MeshMaterial3d(new_handle.clone()));
            swapped += 1;
        }
    }

    log.push(format!(
        "biome paint: {} columns ({} chunk entities swapped) → {}",
        painted_columns.len(),
        swapped,
        new_biome.label(),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paint_state_default_is_grass_radius_zero() {
        let s = BiomePaintState::default();
        assert_eq!(s.selected, BiomeKey::Grass);
        assert_eq!(s.radius_chunks, 0);
    }
}
