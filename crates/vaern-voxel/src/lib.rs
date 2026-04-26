//! Chunked signed-distance-field voxel world for Vaern.
//!
//! The world is represented as a sparse grid of cubic chunks. Each chunk
//! stores a regular 3D grid of signed-distance values (f32 world units;
//! negative = solid, positive = air, zero = surface). Meshes are
//! extracted per chunk via a Surface Nets extractor and re-built when
//! the chunk's `version` advances. The same crate is shared by
//! `vaern-server` (authoritative store + collision queries) and
//! `vaern-client` (meshing + rendering).
//!
//! # Editable API layers
//!
//! Every step of the pipeline is behind a trait so a single layer can be
//! swapped without touching call sites. Concrete default impls live next
//! to the trait definitions:
//!
//! | layer                     | trait                 | default impl                               |
//! |---------------------------|-----------------------|--------------------------------------------|
//! | scalar source / composite | [`SdfField`]          | [`ChunkField`], [`primitive::*`], CSG ops  |
//! | cube-center placement     | [`VertexPlacement`]   | [`CentroidPlacement`]                      |
//! | per-vertex normals        | [`NormalStrategy`]    | [`SdfGradientNormals`]                     |
//! | quad → triangles          | [`QuadSplitter`]      | [`ShortDiagonalSplitter`]                  |
//! | full extraction pipeline  | [`IsoSurfaceExtractor`] | [`SurfaceNetsExtractor`]                 |
//! | world seeding             | [`WorldGenerator`]    | [`HeightfieldGenerator`]                   |
//! | edit shapes               | [`Brush`]             | [`brush::SphereBrush`], [`brush::BoxBrush`]|
//! | mesh output               | [`MeshSink`]          | [`BufferedMesh`]                           |
//!
//! # Coordinate conventions
//!
//! * **World coords** (`Vec3`) — Bevy world space, +Y up.
//! * **Voxel coords** (`IVec3`) — integer sample indices, in world voxels
//!   of size [`config::VOXEL_SIZE`].
//! * **Chunk coords** (`IVec3`) — integer chunk indices. Chunk at
//!   `(cx, cy, cz)` covers voxels `[cx*DIM, (cx+1)*DIM)` on each axis.
//!
//! See [`chunk::coord`] for conversion helpers.

pub mod chunk;
pub mod config;
pub mod edit;
pub mod generator;
pub mod mesh;
pub mod persistence;
pub mod plugin;
pub mod query;
pub mod replication;
pub mod sdf;

pub use chunk::{ChunkCoord, ChunkShape, ChunkStore, DirtyChunks, VoxelChunk, coord};
pub use config::{CHUNK_DIM, CHUNK_SAMPLES_PER_AXIS, CHUNK_TOTAL_SAMPLES, PADDING, VOXEL_SIZE};
pub use edit::{Brush, EditStroke, brush};
pub use generator::{HeightfieldGenerator, WorldGenerator};
pub use mesh::{
    BufferedMesh, CentroidPlacement, IsoSurfaceExtractor, MeshSink, NormalStrategy, QuadSplitter,
    SdfGradientNormals, ShortDiagonalSplitter, SurfaceNetsExtractor, VertexPlacement,
};
pub use plugin::VaernVoxelPlugin;
pub use sdf::{ChunkField, SdfField, SdfValue, primitive};

/// Re-export Bevy's Vec3A for downstream consumers.
pub use bevy::math::Vec3A;
