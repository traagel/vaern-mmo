//! Naive Surface Nets extractor.
//!
//! Takes a chunk's padded SDF sample array, runs the two-pass Surface
//! Nets algorithm, and streams positions / normals / indices into a
//! [`MeshSink`]. Generic over the three customizable layers:
//! [`VertexPlacement`], [`NormalStrategy`], [`QuadSplitter`]. Default
//! choice for each is shipped with the crate.
//!
//! The two passes:
//!
//! 1. **Estimate pass** — walk every content-range cube, check its 8
//!    corner signs. If mixed, place a vertex via `VertexPlacement` +
//!    derive a normal via `NormalStrategy`. Remember the (cube → vertex
//!    index) mapping so the quad pass can look up incident vertices.
//! 2. **Quad pass** — for every edge that crosses the iso-surface,
//!    emit a quad connecting the 4 incident cube vertices. Split the
//!    quad into two triangles via `QuadSplitter`. Winding follows the
//!    SDF-gradient direction so front faces point outward from solid.
//!
//! The reference crate we modeled this on handled the chunk-tileable
//! boundary by iterating cubes only up to `max - 1` (so the positive
//! boundary comes from the neighbor chunk's first vertex). We keep that
//! convention: content cubes span [`ChunkShape::MESH_MIN`,
//! [`ChunkShape::MESH_MAX`]`), and the +side boundary samples must match
//! the neighbor's first content row. Generators write consistent values
//! everywhere so this holds by construction at load time; edit passes
//! must halo-sync the affected faces (see `edit/mod.rs`).

use super::placement::VertexPlacement;
use super::normals::NormalStrategy;
use super::sink::MeshSink;
use super::splitter::QuadSplitter;
use super::tables::CUBE_CORNERS;
use crate::chunk::ChunkShape;
use crate::chunk::VoxelChunk;
use bevy::math::Vec3A;

/// Top-level extractor: chunk SDF → mesh.
///
/// Implementors pick how vertices are placed, how normals are derived,
/// and how quads split into triangles. The [`SurfaceNetsExtractor`]
/// default wires the three strategies together with the two-pass Surface
/// Nets scheme; alternative extractors (e.g. a future Dual Contouring
/// pass) can implement this trait with their own pipeline.
pub trait IsoSurfaceExtractor: Send + Sync {
    fn extract(&self, chunk: &VoxelChunk, sink: &mut dyn MeshSink);
}

/// Configurable Surface Nets extractor. Parameterize on trait
/// placeholders so default impls live in one place and users can swap
/// one layer without touching the others.
#[derive(Clone, Copy, Debug, Default)]
pub struct SurfaceNetsExtractor<P, N, Q> {
    pub placement: P,
    pub normals: N,
    pub splitter: Q,
}

impl<P: VertexPlacement, N: NormalStrategy, Q: QuadSplitter> SurfaceNetsExtractor<P, N, Q> {
    pub const fn new(placement: P, normals: N, splitter: Q) -> Self {
        Self {
            placement,
            normals,
            splitter,
        }
    }
}

impl<P: VertexPlacement, N: NormalStrategy, Q: QuadSplitter> IsoSurfaceExtractor
    for SurfaceNetsExtractor<P, N, Q>
{
    fn extract(&self, chunk: &VoxelChunk, sink: &mut dyn MeshSink) {
        sink.reset();

        // Uniform fast path: every sample equals one scalar, so no
        // cube can have mixed signs and there's nothing to mesh. Skip
        // the 32,768-cube scan in O(1). This is the dominant win for
        // air/solid stack chunks at large draw distance.
        if chunk.uniform_value().is_some() {
            return;
        }

        // Allocate `stride_to_index` at the size of the padded sample
        // array so every cube's minimum-corner stride maps back to its
        // emitted vertex (if any). `NULL_VERTEX` = no vertex here.
        let mut stride_to_index = vec![NULL_VERTEX; ChunkShape::TOTAL];

        // --- Pass 1: estimate surface vertex per sign-change cube ---
        let mut surface_strides: Vec<u32> = Vec::new();

        for z in ChunkShape::MESH_MIN..ChunkShape::MESH_MAX {
            for y in ChunkShape::MESH_MIN..ChunkShape::MESH_MAX {
                for x in ChunkShape::MESH_MIN..ChunkShape::MESH_MAX {
                    let min_stride = ChunkShape::linearize([x, y, z]);

                    let mut corner_dists = [0.0_f32; 8];
                    let mut num_negative = 0;
                    for (i, dist) in corner_dists.iter_mut().enumerate() {
                        let [cx, cy, cz] = CUBE_CORNERS[i];
                        let corner_stride = ChunkShape::linearize([x + cx, y + cy, z + cz]);
                        let d = chunk.sample_at_stride(corner_stride as usize);
                        *dist = d;
                        if d < 0.0 {
                            num_negative += 1;
                        }
                    }
                    if num_negative == 0 || num_negative == 8 {
                        continue; // No surface crossing.
                    }

                    let Some(local) = self.placement.place(&corner_dists) else {
                        continue;
                    };
                    let cube_origin = Vec3A::new(x as f32, y as f32, z as f32);
                    let position = cube_origin + local;
                    let normal = self.normals.normal(&corner_dists, local);

                    let index = sink.push_vertex(position.into(), normal.into());
                    stride_to_index[min_stride as usize] = index;
                    surface_strides.push(min_stride);
                }
            }
        }

        // --- Pass 2: emit one quad per iso-crossing edge ---
        let sx = ChunkShape::STRIDE_X as usize;
        let sy = ChunkShape::STRIDE_Y as usize;
        let sz = ChunkShape::STRIDE_Z as usize;

        for &p_stride in &surface_strides {
            let p = p_stride as usize;
            let [x, y, z] = ChunkShape::delinearize(p_stride);

            // Only emit quads on the -side of each face; the +side is
            // handled by the neighbor cube. Skip the first row on each
            // axis — those edges are emitted by the chunk on the -side
            // of that row.
            if y != ChunkShape::MESH_MIN && z != ChunkShape::MESH_MIN {
                maybe_emit_quad_for_edge::<Q>(
                    chunk,
                    &stride_to_index,
                    sink,
                    self.splitter,
                    p,
                    p + sx,
                    sy,
                    sz,
                );
            }
            if x != ChunkShape::MESH_MIN && z != ChunkShape::MESH_MIN {
                maybe_emit_quad_for_edge::<Q>(
                    chunk,
                    &stride_to_index,
                    sink,
                    self.splitter,
                    p,
                    p + sy,
                    sz,
                    sx,
                );
            }
            if x != ChunkShape::MESH_MIN && y != ChunkShape::MESH_MIN {
                maybe_emit_quad_for_edge::<Q>(
                    chunk,
                    &stride_to_index,
                    sink,
                    self.splitter,
                    p,
                    p + sz,
                    sx,
                    sy,
                );
            }
        }
    }
}

/// "No vertex was emitted for this cube." Match against [`u32::MAX`];
/// kept as a named constant so the meaning is obvious at call sites.
pub const NULL_VERTEX: u32 = u32::MAX;

/// Emit (at most) one quad for the edge between `p1` and `p2`.
///
/// `p1`/`p2` are linear strides of adjacent cubes; `axis_b_stride` and
/// `axis_c_stride` are strides along the two axes orthogonal to the
/// p1→p2 edge. The four vertices of the quad are the ones at:
///
/// * v1 = p1
/// * v2 = p1 - axis_b
/// * v3 = p1 - axis_c
/// * v4 = p1 - axis_b - axis_c
///
/// Face winding flips if the SDF sign crossing goes the other way
/// along the edge.
#[allow(clippy::too_many_arguments)]
fn maybe_emit_quad_for_edge<Q: QuadSplitter>(
    chunk: &VoxelChunk,
    stride_to_index: &[u32],
    sink: &mut dyn MeshSink,
    splitter: Q,
    p1: usize,
    p2: usize,
    axis_b_stride: usize,
    axis_c_stride: usize,
) {
    let d1 = chunk.sample_at_stride(p1);
    let d2 = chunk.sample_at_stride(p2);
    let negative_face = match (d1 < 0.0, d2 < 0.0) {
        (true, false) => false,
        (false, true) => true,
        _ => return, // No face.
    };

    let v1 = stride_to_index[p1];
    let v2 = stride_to_index[p1 - axis_b_stride];
    let v3 = stride_to_index[p1 - axis_c_stride];
    let v4 = stride_to_index[p1 - axis_b_stride - axis_c_stride];
    // Guard against stale NULL_VERTEX — can happen if a neighbor cube
    // was empty (e.g. on an unloaded-chunk boundary during partial
    // loads). Skip rather than emit bogus indices.
    if v1 == NULL_VERTEX || v2 == NULL_VERTEX || v3 == NULL_VERTEX || v4 == NULL_VERTEX {
        return;
    }

    let positions = sink.positions();
    let pos1 = Vec3A::from(positions[v1 as usize]);
    let pos2 = Vec3A::from(positions[v2 as usize]);
    let pos3 = Vec3A::from(positions[v3 as usize]);
    let pos4 = Vec3A::from(positions[v4 as usize]);
    let tri = splitter.split(v1, v2, v3, v4, pos1, pos2, pos3, pos4, negative_face);
    sink.push_indices(&tri);
}
