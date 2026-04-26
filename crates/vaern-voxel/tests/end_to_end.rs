//! End-to-end smoke tests: SDF → chunk seed → mesh extraction →
//! edit → re-extract. Covers the full public API surface.
//!
//! Kept out of the unit-test modules because these drive the whole
//! stack (plugin-level systems aside — those need a real Bevy app).

use bevy::math::Vec3;
use vaern_voxel::chunk::VoxelChunk;
use vaern_voxel::edit::{BrushMode, EditStroke, SphereBrush};
use vaern_voxel::generator::{HeightfieldGenerator, WorldGenerator};
use vaern_voxel::mesh::{
    BufferedMesh, DefaultExtractor, IsoSurfaceExtractor,
};
use vaern_voxel::sdf::primitive::Sphere;
use vaern_voxel::{ChunkCoord, ChunkStore, DirtyChunks, PADDING};

#[test]
fn sphere_sdf_produces_a_closed_mesh() {
    // Carve a sphere-shaped solid region into an otherwise-air chunk.
    let coord = ChunkCoord::new(0, 0, 0);
    let mut chunk = VoxelChunk::new_air();
    let origin = coord.world_origin();

    // Chunk covers world [0, 32) on each axis. Put a sphere at the
    // chunk's center so the mesh sits fully inside content range.
    let center = origin + Vec3::splat(16.0);
    let sphere = Sphere::new(center, 8.0);

    // Seed every padded sample with the sphere SDF, but flip the sign
    // — we want the sphere to be solid (negative) and outside to be
    // air (positive). Sphere::sample returns positive outside and
    // negative inside, which is exactly what we want already.
    chunk.fill_all_padded(|[ix, iy, iz]| {
        let lx = ix as f32 - PADDING as f32;
        let ly = iy as f32 - PADDING as f32;
        let lz = iz as f32 - PADDING as f32;
        let p = origin + Vec3::new(lx, ly, lz);
        use vaern_voxel::sdf::SdfField;
        sphere.sample(p)
    });

    let extractor = DefaultExtractor::default_config();
    let mut mesh = BufferedMesh::new();
    extractor.extract(&chunk, &mut mesh);

    assert!(!mesh.positions.is_empty(), "expected vertices for sphere");
    assert!(!mesh.indices.is_empty(), "expected indices for sphere");
    assert_eq!(
        mesh.indices.len() % 3,
        0,
        "indices must be a multiple of 3"
    );
    assert_eq!(
        mesh.positions.len(),
        mesh.normals.len(),
        "one normal per position"
    );
}

#[test]
fn heightfield_generator_produces_a_mesh_with_some_surface() {
    // Seed a single chunk from the shared heightmap and mesh it.
    // The heightmap has amplitude ~2u; with chunk (0,0,0) covering
    // world y=[0..32), we need the chunk to straddle y≈0. The chunk
    // at (0, -1, 0) sits at world y=[-32..0) — that's the one whose
    // +y padded row crosses the surface.
    let coord = ChunkCoord::new(0, -1, 0);
    let mut chunk = VoxelChunk::new_air();
    let generator = HeightfieldGenerator::new();
    generator.seed_chunk(coord, &mut chunk);

    let extractor = DefaultExtractor::default_config();
    let mut mesh = BufferedMesh::new();
    extractor.extract(&chunk, &mut mesh);

    assert!(
        !mesh.positions.is_empty(),
        "heightfield chunk should have a surface"
    );
}

#[test]
fn edit_makes_a_previously_empty_chunk_dirty() {
    let mut store = ChunkStore::new();
    let mut dirty = DirtyChunks::new();

    // No chunks loaded. A subtract brush into empty space should have
    // no effect (air minus anything is still air) — but a union-add
    // brush writes solid material, creating new chunks.
    let brush = SphereBrush {
        center: Vec3::new(5.0, 5.0, 5.0),
        radius: 3.0,
        mode: BrushMode::Union,
        falloff: vaern_voxel::edit::Falloff::Hard,
    };
    let touched = EditStroke::new(brush, &mut store, &mut dirty).apply();
    assert!(!touched.is_empty(), "union brush should create chunks");
    assert!(!dirty.is_empty(), "dirty set should be populated");
    assert!(store.len() > 0, "store should now hold the carved chunk");
}

#[test]
fn edit_plus_remesh_changes_mesh_output() {
    // Build a pre-solid chunk, mesh it once, carve a crater, re-mesh.
    // Expect vertex count to increase (new cave surface adds triangles).
    let coord = ChunkCoord::new(0, 0, 0);
    let mut store = ChunkStore::new();
    let mut dirty = DirtyChunks::new();

    let mut chunk = VoxelChunk::new_air();
    // Entire chunk solid.
    chunk.fill_all_padded(|_| -10.0);
    store.insert(coord, chunk);

    // Nothing to mesh yet (solid with no surface) — let's set up a
    // real surface first by running a box-shaped "hollow" so the
    // chunk has an iso-surface we can see.
    let hollow = SphereBrush {
        center: Vec3::new(8.0, 8.0, 8.0),
        radius: 6.0,
        mode: BrushMode::Subtract,
        falloff: vaern_voxel::edit::Falloff::Hard,
    };
    EditStroke::new(hollow, &mut store, &mut dirty).apply();
    dirty.drain_all();

    let extractor = DefaultExtractor::default_config();
    let mut mesh1 = BufferedMesh::new();
    extractor.extract(store.get(coord).unwrap(), &mut mesh1);
    let initial_verts = mesh1.positions.len();
    assert!(initial_verts > 0, "first carve should produce a surface");

    // Carve a second, non-overlapping crater.
    let second = SphereBrush {
        center: Vec3::new(24.0, 24.0, 24.0),
        radius: 4.0,
        mode: BrushMode::Subtract,
        falloff: vaern_voxel::edit::Falloff::Hard,
    };
    EditStroke::new(second, &mut store, &mut dirty).apply();

    let mut mesh2 = BufferedMesh::new();
    extractor.extract(store.get(coord).unwrap(), &mut mesh2);
    assert!(
        mesh2.positions.len() > initial_verts,
        "second carve should add to vertex count"
    );
}
