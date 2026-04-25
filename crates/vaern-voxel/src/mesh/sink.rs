//! Output sinks for mesh extractors.
//!
//! The extractor writes positions, normals, and indices through a
//! trait object, so callers choose whether to collect into a reusable
//! buffer ([`BufferedMesh`]) or stream straight into an engine-specific
//! mesh builder. Separating the sink from the extractor keeps the
//! algorithm engine-agnostic and lets us swap to a packed or striped
//! mesh format later without changing the extractor code.

/// Output target for an [`crate::IsoSurfaceExtractor`] pass.
///
/// Implementors decide how positions and normals are stored; the
/// extractor guarantees it writes positions first (via
/// [`push_vertex`]) so [`positions`] returns a non-empty slice by the
/// time it needs to ask for positions during the quad pass.
pub trait MeshSink {
    /// Clear any previously buffered output. Called once at the start
    /// of an extraction pass so the sink's buffers can be reused.
    fn reset(&mut self);

    /// Append one vertex. Returns its index (monotonically increasing
    /// from 0) so downstream callers can reference it in triangles.
    fn push_vertex(&mut self, position: [f32; 3], normal: [f32; 3]) -> u32;

    /// Append `count` indices (a multiple of 3 — each triple is a
    /// triangle).
    fn push_indices(&mut self, indices: &[u32]);

    /// Read-only view of positions written so far. The extractor needs
    /// this during the quad pass to pick a diagonal split.
    fn positions(&self) -> &[[f32; 3]];
}

/// Default sink: three parallel `Vec`s. Reusable across frames by
/// calling [`reset`] before each extraction.
#[derive(Default, Clone, Debug)]
pub struct BufferedMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

impl BufferedMesh {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}

impl MeshSink for BufferedMesh {
    fn reset(&mut self) {
        self.positions.clear();
        self.normals.clear();
        self.indices.clear();
    }

    fn push_vertex(&mut self, position: [f32; 3], normal: [f32; 3]) -> u32 {
        let index = self.positions.len() as u32;
        self.positions.push(position);
        self.normals.push(normal);
        index
    }

    fn push_indices(&mut self, indices: &[u32]) {
        self.indices.extend_from_slice(indices);
    }

    fn positions(&self) -> &[[f32; 3]] {
        &self.positions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffered_mesh_tracks_indices() {
        let mut m = BufferedMesh::new();
        let a = m.push_vertex([0.0; 3], [0.0, 1.0, 0.0]);
        let b = m.push_vertex([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        let c = m.push_vertex([0.0, 0.0, 1.0], [0.0, 1.0, 0.0]);
        m.push_indices(&[a, b, c]);
        assert_eq!(m.triangle_count(), 1);
        m.reset();
        assert!(m.is_empty());
    }
}
