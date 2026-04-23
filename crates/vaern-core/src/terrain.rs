//! Shared terrain height field.
//!
//! Both client (mesh generation) and server (entity Y-snapping) sample
//! `height` so the two agree on where the ground is at any `(x, z)`.
//! Deterministic, no RNG state, no external crate — amplitude is kept
//! small so differences between the server's Transform Y=0 baseline
//! and an entity snapped onto this surface remain subtle (≤ ~2u).
//!
//! When swapping to a real heightmap or tiled noise, replace `height`
//! here and the rest of the workspace picks it up automatically.

/// Terrain surface Y at world-space `(x, z)`.
#[inline]
pub fn height(x: f32, z: f32) -> f32 {
    let low = (x * 0.03).sin() * (z * 0.03).cos() * 1.2;
    let mid = (x * 0.07).sin() * (z * 0.09).cos() * 0.6;
    low + mid
}
