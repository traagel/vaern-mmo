//! Compile-time chunk dimensions + world voxel size.
//!
//! Kept in a single module so any module that wants to allocate an
//! aligned sample buffer or do integer coord math pulls the same
//! constants. Changing `CHUNK_DIM` or `PADDING` here cascades to every
//! consumer without touching individual files.

/// Number of content voxel-cubes along each chunk axis. The meshing
/// pass iterates `CHUNK_DIM³` cubes; each cube reads its 8 sample
/// corners from the padded sample array.
pub const CHUNK_DIM: u32 = 32;

/// Width of the padding skirt on each side of the chunk, in voxels.
/// 1 voxel of padding on the negative side lets edit passes read a
/// neighbor without a cross-chunk lookup, and 1 voxel on the positive
/// side is the Surface-Nets seamless-boundary convention.
pub const PADDING: u32 = 1;

/// Samples per chunk axis. `PADDING + CHUNK_DIM + PADDING` when content
/// cubes span `[PADDING, PADDING+CHUNK_DIM]` and consume their 8 corners
/// at `[PADDING, PADDING+CHUNK_DIM+1)`. That +1 is folded into the
/// positive-side pad.
pub const CHUNK_SAMPLES_PER_AXIS: u32 = CHUNK_DIM + 2 * PADDING;

/// Total f32 samples per chunk = `CHUNK_SAMPLES_PER_AXIS³`.
pub const CHUNK_TOTAL_SAMPLES: usize = (CHUNK_SAMPLES_PER_AXIS
    * CHUNK_SAMPLES_PER_AXIS
    * CHUNK_SAMPLES_PER_AXIS) as usize;

/// World-space size of one voxel edge in Bevy units. Smaller = higher
/// destruction resolution but O(1/size³) memory & meshing cost.
pub const VOXEL_SIZE: f32 = 1.0;

/// World-space size of one chunk edge in Bevy units.
pub const CHUNK_WORLD_SIZE: f32 = CHUNK_DIM as f32 * VOXEL_SIZE;

/// SDF value stored in a sample that has never been touched by either a
/// generator or an edit. Positive, large — reads as "air, very far from
/// any surface." Any inside-solid region must have been explicitly
/// written by a generator or brush.
pub const UNINITIALIZED_SDF: f32 = 1.0e6;
