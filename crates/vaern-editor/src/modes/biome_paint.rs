//! Biome-paint mode — sub-cell granularity brush with shape, size,
//! falloff, drag-paint, and eraser. Writes into `BiomeOverrideMap`
//! (sub-cell keyed); marks affected chunks dirty so the mesher re-runs
//! and per-vertex blend weights recompute against the new overrides.
//!
//! Storage: `src/generated/world/biome_overrides.bin` —
//! `OverridesFileV2`, sub-cell keyed, written on Ctrl+S, loaded on
//! Startup.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::EguiContexts;
use vaern_voxel::chunk::{ChunkStore, DirtyChunks};
use vaern_voxel::query::raycast;

use super::{active_mode_is, EditorMode};
use crate::camera::FreeFlyCamera;
use crate::state::EditorAppState;
use crate::ui::console::ConsoleLog;
use crate::voxel::biomes::BiomeKey;
use crate::voxel::overrides::{BiomeOverrideMap, SUB_CELLS_PER_CHUNK, SUB_CELL_SIZE_M};
use crate::voxel::ChunkBiomeMap;

const PAINT_RAY_MAX_DIST: f32 = 500.0;

/// Brush footprint shape. Square selects all sub-cells whose centers
/// lie inside an axis-aligned square; Circle uses a Euclidean disc.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrushShape {
    Circle,
    Square,
}

/// Brush mode. Paint writes the selected biome; Erase removes the
/// override (cell falls back to the default biome).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrushMode {
    Paint,
    Erase,
}

/// Falloff is reserved for V2 — applies a per-cell weight when the
/// brush footprint partially overlaps a sub-cell. V1 uses Hard
/// (binary in-or-out) — a sub-cell is painted iff its center lies in
/// the footprint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrushFalloff {
    Hard,
    Linear,
    Smooth,
}

#[derive(Resource, Debug, Clone)]
pub struct BrushState {
    pub selected: BiomeKey,
    /// Brush radius in world units. Slider range 1..=64.
    pub radius_world_u: f32,
    pub shape: BrushShape,
    pub falloff: BrushFalloff,
    pub mode: BrushMode,
    /// Eyedropper: when true, the next LMB click samples the dominant
    /// biome at cursor instead of painting. One-shot — flips back to
    /// false after the click.
    pub eyedropper_armed: bool,
}

impl Default for BrushState {
    fn default() -> Self {
        Self {
            selected: BiomeKey::Grass,
            radius_world_u: 8.0,
            shape: BrushShape::Circle,
            falloff: BrushFalloff::Hard,
            mode: BrushMode::Paint,
            eyedropper_armed: false,
        }
    }
}

/// Drag-stroke state. Active during LMB-hold; reset on release. Holds
/// the original sub-cell snapshot so undo can revert the entire
/// stroke as one entry.
#[derive(Resource, Default, Debug)]
pub struct BrushStrokeState {
    pub active: bool,
    pub last_stamp_world_xz: Option<Vec2>,
    /// Sub-cells touched by this stroke + their pre-stroke biome (or
    /// `None` if no override existed before).
    pub stroke_snapshot: std::collections::HashMap<(i32, i32), Option<BiomeKey>>,
}

/// World-units distance between successive stamps along a drag. At
/// 0.25 × radius, a radius=8 brush re-stamps every 2 world units —
/// dense enough that consecutive footprints overlap by ~75%, giving
/// a continuous painted line.
const DRAG_SPACING_FACTOR: f32 = 0.25;

pub struct BiomePaintModePlugin;

impl Plugin for BiomePaintModePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BrushState>()
            .init_resource::<BrushStrokeState>()
            .add_systems(
                Update,
                (
                    apply_biome_paint_drag,
                    finish_biome_paint_stroke,
                    apply_brush_hotkeys,
                    draw_brush_cursor,
                )
                    .run_if(in_state(EditorAppState::Editing))
                    .run_if(active_mode_is(EditorMode::BiomePaint)),
            );
    }
}

/// LMB-hold continuous painting. On press: open a new stroke. On
/// hold: re-stamp every `radius * DRAG_SPACING_FACTOR` world units
/// along the drag path. On release: handled by
/// `finish_biome_paint_stroke`.
#[allow(clippy::too_many_arguments)]
pub fn apply_biome_paint_drag(
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<FreeFlyCamera>>,
    store: Res<ChunkStore>,
    mut overrides: ResMut<BiomeOverrideMap>,
    chunk_biomes: Res<ChunkBiomeMap>,
    mut dirty: ResMut<DirtyChunks>,
    mut brush: ResMut<BrushState>,
    mut stroke: ResMut<BrushStrokeState>,
    mut log: ResMut<ConsoleLog>,
    mut egui: EguiContexts,
) {
    if !mouse_buttons.pressed(MouseButton::Left) {
        return;
    }
    let pressed_now = mouse_buttons.just_pressed(MouseButton::Left);
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
        return;
    };
    let hit_xz = Vec2::new(hit.position.x, hit.position.z);

    // Eyedropper one-shot path: don't paint, sample dominant biome at
    // the cursor's hit position.
    if pressed_now && brush.eyedropper_armed {
        let (sx, sz) = BiomeOverrideMap::world_to_sub(hit_xz.x, hit_xz.y);
        if let Some(biome) = overrides.get(sx, sz) {
            brush.selected = biome;
            log.push(format!("eyedropper → {}", biome.label()));
        } else {
            log.push("eyedropper → (default Marsh)");
            brush.selected = BiomeKey::Marsh;
        }
        brush.eyedropper_armed = false;
        return;
    }

    // Stamp-spacing gate: if mid-drag and we haven't moved far enough
    // since the last stamp, skip this frame.
    if !pressed_now {
        if let Some(last) = stroke.last_stamp_world_xz {
            if (hit_xz - last).length() < brush.radius_world_u * DRAG_SPACING_FACTOR {
                return;
            }
        }
    }

    if pressed_now {
        stroke.active = true;
        stroke.stroke_snapshot.clear();
    }

    let painted = stamp_brush(
        &mut overrides,
        &mut stroke,
        hit_xz,
        brush.radius_world_u,
        brush.shape,
        brush.mode,
        brush.selected,
    );
    stroke.last_stamp_world_xz = Some(hit_xz);

    if painted == 0 {
        return;
    }

    // Mark chunks dirty for the sub-cells we just touched + 1-chunk
    // halo so the per-vertex blend math at chunk boundaries picks up
    // the new overrides.
    let n = SUB_CELLS_PER_CHUNK as i32;
    let mut dirty_columns: std::collections::HashSet<(i32, i32)> =
        std::collections::HashSet::new();
    let r = brush.radius_world_u;
    let center_sub = BiomeOverrideMap::world_to_sub(hit_xz.x, hit_xz.y);
    let r_sub = (r / SUB_CELL_SIZE_M).ceil() as i32;
    for dz in -r_sub..=r_sub {
        for dx in -r_sub..=r_sub {
            let sx = center_sub.0 + dx;
            let sz = center_sub.1 + dz;
            let cx = sx.div_euclid(n);
            let cz = sz.div_euclid(n);
            for hdz in -1..=1 {
                for hdx in -1..=1 {
                    dirty_columns.insert((cx + hdx, cz + hdz));
                }
            }
        }
    }
    let mut marked = 0usize;
    for (coord, _) in chunk_biomes.by_coord.iter() {
        if dirty_columns.contains(&(coord.0.x, coord.0.z)) {
            dirty.mark(*coord);
            marked += 1;
        }
    }
    let _ = &store; // keep system param

    if pressed_now {
        log.push(format!(
            "biome paint stroke begin: {painted} sub-cells, {marked} chunks dirty"
        ));
    }
}

/// On LMB release, log the stroke summary. The stroke snapshot stays
/// in the resource until the next stroke begins (so undo, when wired,
/// can revert it).
pub fn finish_biome_paint_stroke(
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mut stroke: ResMut<BrushStrokeState>,
    mut log: ResMut<ConsoleLog>,
) {
    if !mouse_buttons.just_released(MouseButton::Left) {
        return;
    }
    if !stroke.active {
        return;
    }
    let n = stroke.stroke_snapshot.len();
    log.push(format!("biome paint stroke end: {n} sub-cells modified"));
    stroke.active = false;
    stroke.last_stamp_world_xz = None;
}

/// Stamp the brush footprint at `world_center` into the override map,
/// recording each sub-cell's pre-stroke biome in the stroke snapshot
/// (only the FIRST visit per cell — later stamps in the same stroke
/// don't overwrite the snapshot, so undo always reverts to pre-stroke
/// state).
fn stamp_brush(
    overrides: &mut BiomeOverrideMap,
    stroke: &mut BrushStrokeState,
    world_center: Vec2,
    radius: f32,
    shape: BrushShape,
    mode: BrushMode,
    biome: BiomeKey,
) -> usize {
    let r2 = radius * radius;
    let r_sub = (radius / SUB_CELL_SIZE_M).ceil() as i32;
    let center_sub = BiomeOverrideMap::world_to_sub(world_center.x, world_center.y);
    let mut count = 0usize;
    for dz in -r_sub..=r_sub {
        for dx in -r_sub..=r_sub {
            let sx = center_sub.0 + dx;
            let sz = center_sub.1 + dz;
            let (cx, cz) = BiomeOverrideMap::sub_cell_center(sx, sz);
            let in_footprint = match shape {
                BrushShape::Circle => {
                    let v = Vec2::new(cx, cz) - world_center;
                    v.length_squared() <= r2
                }
                BrushShape::Square => {
                    (cx - world_center.x).abs() <= radius
                        && (cz - world_center.y).abs() <= radius
                }
            };
            if !in_footprint {
                continue;
            }

            // Snapshot the sub-cell's pre-stroke biome (only first
            // visit per cell in this stroke).
            stroke
                .stroke_snapshot
                .entry((sx, sz))
                .or_insert_with(|| overrides.get(sx, sz));

            match mode {
                BrushMode::Paint => overrides.set(sx, sz, biome),
                BrushMode::Erase => overrides.clear(sx, sz),
            }
            count += 1;
        }
    }
    count
}

/// Hotkey bindings: `[` shrink, `]` grow brush; `B` paint mode,
/// `E` erase mode, `I` arm eyedropper.
pub fn apply_brush_hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    mut brush: ResMut<BrushState>,
    mut log: ResMut<ConsoleLog>,
) {
    if keys.just_pressed(KeyCode::BracketLeft) {
        brush.radius_world_u = (brush.radius_world_u * 0.85).max(1.0);
        log.push(format!("brush radius {:.1}u", brush.radius_world_u));
    }
    if keys.just_pressed(KeyCode::BracketRight) {
        brush.radius_world_u = (brush.radius_world_u * 1.15).min(64.0);
        log.push(format!("brush radius {:.1}u", brush.radius_world_u));
    }
    if keys.just_pressed(KeyCode::KeyB) {
        brush.mode = BrushMode::Paint;
        log.push("brush mode: Paint");
    }
    if keys.just_pressed(KeyCode::KeyE) {
        brush.mode = BrushMode::Erase;
        log.push("brush mode: Erase");
    }
    if keys.just_pressed(KeyCode::KeyI) {
        brush.eyedropper_armed = !brush.eyedropper_armed;
        log.push(if brush.eyedropper_armed {
            "eyedropper armed (next click samples)"
        } else {
            "eyedropper disarmed"
        });
    }
}

/// Phase 2D: immediate-mode gizmo drawing the brush footprint at the
/// cursor's hit position. Runs every frame in BiomePaint mode.
pub fn draw_brush_cursor(
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<FreeFlyCamera>>,
    store: Res<ChunkStore>,
    brush: Res<BrushState>,
    mut gizmos: Gizmos,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let Ok((cam, cam_xform)) = cameras.single() else {
        return;
    };
    let Ok(ray) = cam.viewport_to_world(cam_xform, cursor) else {
        return;
    };
    let Some(hit) = raycast(&store, ray.origin, *ray.direction, PAINT_RAY_MAX_DIST) else {
        return;
    };

    // Color: bright green for Paint, red for Erase, yellow if
    // eyedropper armed.
    let color = if brush.eyedropper_armed {
        Color::srgb(1.0, 0.9, 0.2)
    } else {
        match brush.mode {
            BrushMode::Paint => Color::srgb(0.3, 1.0, 0.3),
            BrushMode::Erase => Color::srgb(1.0, 0.3, 0.3),
        }
    };
    let center = hit.position + Vec3::Y * 0.05; // tiny lift to avoid z-fighting

    match brush.shape {
        BrushShape::Circle => {
            gizmos.circle(
                Isometry3d::new(center, Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                brush.radius_world_u,
                color,
            );
        }
        BrushShape::Square => {
            // Draw a square via 4 line segments at +Y up.
            let r = brush.radius_world_u;
            let corners = [
                center + Vec3::new(-r, 0.0, -r),
                center + Vec3::new(r, 0.0, -r),
                center + Vec3::new(r, 0.0, r),
                center + Vec3::new(-r, 0.0, r),
            ];
            for i in 0..4 {
                gizmos.line(corners[i], corners[(i + 1) % 4], color);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brush_state_default_is_grass_paint_radius_8() {
        let s = BrushState::default();
        assert_eq!(s.selected, BiomeKey::Grass);
        assert_eq!(s.radius_world_u, 8.0);
        assert_eq!(s.shape, BrushShape::Circle);
        assert_eq!(s.mode, BrushMode::Paint);
        assert!(!s.eyedropper_armed);
    }

    #[test]
    fn stamp_circle_paints_only_cells_inside_disc() {
        let mut overrides = BiomeOverrideMap::default();
        let mut stroke = BrushStrokeState::default();
        let center = Vec2::new(0.0, 0.0);
        let radius = SUB_CELL_SIZE_M; // 8m → covers 1-cell radius around center

        let count = stamp_brush(
            &mut overrides,
            &mut stroke,
            center,
            radius,
            BrushShape::Circle,
            BrushMode::Paint,
            BiomeKey::Snow,
        );

        // Cells whose centers are at distance ≤ radius from (0,0).
        // Center cells are at (0,0)→(±0.5)*8u = (4,4), (-4,4) etc.
        // Distance from (0,0) to (4,4) = ~5.66 < 8 → inside.
        // Distance to (-4, -12) = ~12.6 > 8 → outside.
        assert!(count > 0);
        // (0, 0) sub-cell center is at (4, 4) → distance ~5.66 → in.
        assert_eq!(overrides.get(0, 0), Some(BiomeKey::Snow));
        // (-1, 0) sub-cell center is at (-4, 4) → distance ~5.66 → in.
        assert_eq!(overrides.get(-1, 0), Some(BiomeKey::Snow));
        // (-2, 0) sub-cell center is at (-12, 4) → distance ~12.6 → out.
        assert_eq!(overrides.get(-2, 0), None);
    }

    #[test]
    fn stamp_square_paints_full_aabb() {
        let mut overrides = BiomeOverrideMap::default();
        let mut stroke = BrushStrokeState::default();
        let center = Vec2::new(0.0, 0.0);
        let radius = SUB_CELL_SIZE_M; // 8m square → covers cells with centers in ±8m AABB

        stamp_brush(
            &mut overrides,
            &mut stroke,
            center,
            radius,
            BrushShape::Square,
            BrushMode::Paint,
            BiomeKey::Stone,
        );
        // (0, 0) center (4, 4), (-1, 0) center (-4, 4) — both in AABB.
        assert_eq!(overrides.get(0, 0), Some(BiomeKey::Stone));
        assert_eq!(overrides.get(-1, 0), Some(BiomeKey::Stone));
        // (-1, -1) center (-4, -4) — in.
        assert_eq!(overrides.get(-1, -1), Some(BiomeKey::Stone));
    }

    #[test]
    fn erase_mode_removes_existing_override() {
        let mut overrides = BiomeOverrideMap::default();
        overrides.set(0, 0, BiomeKey::Snow);
        let mut stroke = BrushStrokeState::default();

        stamp_brush(
            &mut overrides,
            &mut stroke,
            Vec2::new(0.0, 0.0),
            SUB_CELL_SIZE_M * 0.5, // small radius, just sub-cell (0,0)
            BrushShape::Square,
            BrushMode::Erase,
            BiomeKey::Grass, // ignored in Erase mode
        );
        assert_eq!(overrides.get(0, 0), None);
    }

    #[test]
    fn stroke_snapshot_records_only_first_visit_per_cell() {
        let mut overrides = BiomeOverrideMap::default();
        overrides.set(0, 0, BiomeKey::Snow);
        let mut stroke = BrushStrokeState::default();

        // First stamp: snapshot records pre-stroke Snow.
        stamp_brush(
            &mut overrides,
            &mut stroke,
            Vec2::new(0.0, 0.0),
            SUB_CELL_SIZE_M * 0.5,
            BrushShape::Square,
            BrushMode::Paint,
            BiomeKey::Marsh,
        );
        assert_eq!(stroke.stroke_snapshot.get(&(0, 0)), Some(&Some(BiomeKey::Snow)));

        // Second stamp on same cell: snapshot must NOT change.
        stamp_brush(
            &mut overrides,
            &mut stroke,
            Vec2::new(0.0, 0.0),
            SUB_CELL_SIZE_M * 0.5,
            BrushShape::Square,
            BrushMode::Paint,
            BiomeKey::Stone,
        );
        assert_eq!(stroke.stroke_snapshot.get(&(0, 0)), Some(&Some(BiomeKey::Snow)));
    }
}
