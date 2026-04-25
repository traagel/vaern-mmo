//! Iso-surface extraction pipeline.
//!
//! The public top-level is [`IsoSurfaceExtractor`] — one method,
//! chunk-in / mesh-out. Everything else in this module is a *layer* of
//! the pipeline exposed as its own trait so callers can swap one slice
//! without forking the rest:
//!
//! * [`VertexPlacement`] — where a vertex sits inside its sign-change
//!   cube. Default [`CentroidPlacement`] implements Surface Nets; a
//!   future QEF-based impl would implement Dual Contouring.
//! * [`NormalStrategy`] — per-vertex normal estimation. Default
//!   [`SdfGradientNormals`] reads the gradient of the interpolated SDF.
//! * [`QuadSplitter`] — triangulation of each emitted quad. Default
//!   [`ShortDiagonalSplitter`] picks the shorter diagonal to minimize
//!   aspect ratio.
//! * [`MeshSink`] — where the resulting positions / normals / indices
//!   land. Default [`BufferedMesh`] collects into three `Vec`s that can
//!   be reused frame-to-frame.
//!
//! The Bevy-mesh conversion lives in [`bevy_mesh`] and is intentionally
//! isolated from the extractor so non-rendering consumers (server
//! raycasts, dedicated physics) don't pull in the renderer types.

pub mod bevy_mesh;
pub mod normals;
pub mod placement;
pub mod sink;
pub mod splitter;
pub mod surface_nets;
pub mod tables;

pub use bevy_mesh::build_bevy_mesh;
pub use normals::{FlatCornerNormals, NormalStrategy, SdfGradientNormals};
pub use placement::{CentroidPlacement, VertexPlacement};
pub use sink::{BufferedMesh, MeshSink};
pub use splitter::{FixedDiagonalSplitter, QuadSplitter, ShortDiagonalSplitter};
pub use surface_nets::{IsoSurfaceExtractor, NULL_VERTEX, SurfaceNetsExtractor};

/// Default extractor configuration: centroid placement + SDF-gradient
/// normals + short-diagonal splitter. The one every caller wants until
/// they have a reason to customize.
pub type DefaultExtractor =
    SurfaceNetsExtractor<CentroidPlacement, SdfGradientNormals, ShortDiagonalSplitter>;

impl DefaultExtractor {
    /// Construct the default extractor. Zero-sized; can be stored in a
    /// resource without carrying state.
    pub const fn default_config() -> Self {
        Self::new(
            CentroidPlacement,
            SdfGradientNormals,
            ShortDiagonalSplitter,
        )
    }
}
