//! Convert a [`BufferedMesh`] into a Bevy [`Mesh`] asset.
//!
//! Kept separate from the extractor so the extractor stays
//! rendering-engine-agnostic (server can extract + raycast against the
//! output without linking the Bevy mesh type path). The client's
//! mesher calls this after extraction to upload to the renderer.

use super::sink::BufferedMesh;
use bevy::asset::RenderAssetUsages;
use bevy::math::Vec3;
use bevy::mesh::{Indices, Mesh, PrimitiveTopology};

/// Build a Bevy [`Mesh`] from a [`BufferedMesh`].
///
/// * `voxel_to_world` scales cube-local coords (samples) to world
///   units — pass [`crate::VOXEL_SIZE`].
/// * `origin_offset` is a world-space offset subtracted so the mesh's
///   local (0,0,0) sits at `ChunkCoord::world_origin` after the
///   transform system places it. Pass `-PADDING * VOXEL_SIZE` to
///   account for the padding voxels on the -side of the sample array.
pub fn build_bevy_mesh(
    sink: &BufferedMesh,
    voxel_to_world: f32,
    origin_offset: Vec3,
) -> Mesh {
    let positions: Vec<[f32; 3]> = sink
        .positions
        .iter()
        .map(|&[x, y, z]| {
            let p = Vec3::new(x, y, z) * voxel_to_world + origin_offset;
            p.to_array()
        })
        .collect();

    // Normals from the SDF gradient are unnormalized by design; the
    // shader normalizes per-fragment, but we can also pre-normalize
    // CPU-side for fixed-function paths that expect unit-length.
    let normals: Vec<[f32; 3]> = sink
        .normals
        .iter()
        .map(|&[x, y, z]| {
            let n = Vec3::new(x, y, z).normalize_or_zero();
            n.to_array()
        })
        .collect();

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_indices(Indices::U32(sink.indices.clone()));
    mesh
}
