// Biome-blend extension for ExtendedMaterial<StandardMaterial, BiomeBlendExt>.
//
// Vertex stage: passes through standard PBR attributes plus three custom
// vertex attributes — `biome_weights_lo` (vec4<f32>, weights for biomes
// 0..3), `biome_weights_hi` (vec4<f32>, weights for biomes 4..7), and
// `biome_weights_8` (vec4<f32> with biome 8 in `.x`, padding in the
// rest). All three interpolate LINEARLY to the fragment.
//
// Fragment stage: reads the 9 per-biome weights, samples each layer
// of the texture array (with non-trivial weight) and weighted-sums.
// Hands the result to Bevy's standard apply_pbr_lighting.
//
// Why per-biome weights instead of (4 IDs + 4 weights) with flat-interp?
// Adjacent chunks' triangles use DIFFERENT provoking vertices via
// WGSL's `@interpolate(flat)`. Different vertices have different
// "candidate sub-cell" sets → their ID slot orderings diverge →
// visible color jumps at chunk boundaries (the artifact the user
// reported). With fixed-slot per-biome weights the whole chain is
// linearly interpolated; nothing is flat-interp; no boundary artifacts.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_vertex_output,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types,
    forward_io::{Vertex, VertexOutput, FragmentOutput},
    mesh_functions,
    view_transformations::position_world_to_clip,
}

// Extension bind group lives at @group(MATERIAL_BIND_GROUP) — base
// StandardMaterial uses 0..=99, extension occupies 100+ per Bevy's
// documented convention. Slots match `BiomeBlendExt`'s
// `#[texture(...)]` / `#[sampler(...)]` attributes.
@group(#{MATERIAL_BIND_GROUP}) @binding(100) var color_array: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var ground_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var normal_array: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var ao_array: texture_2d_array<f32>;

struct BiomeBlendUniform {
    /// World units per repeat of one biome texture tile.
    /// World_xz / tile_size_m → texture UV (then frac-wrapped by
    /// hardware when sampler is Repeat).
    tile_size_m: f32,
    /// Number of layers in the texture arrays.
    layer_count: u32,
    /// Debug viz mode (see `BlendDebugMode` in biome_blend.rs):
    ///   0 = normal PBR
    ///   1 = dominant biome flat color
    ///   2 = first 3 weights as RGB (biomes 0=R, 1=G, 2=B)
    ///   3 = biome-7 (Marsh) weight as grayscale heatmap
    debug_mode: u32,
    /// Pad to 16 bytes for std140 / WebGL2 compatibility.
    _pad0: u32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(104) var<uniform> blend_params: BiomeBlendUniform;

// ---- Vertex ---------------------------------------------------------

// Tangents are intentionally absent. The fragment uses
// `pbr_input_from_vertex_output` (not `pbr_input_from_standard_material`)
// which does not read tangents, and we don't sample normal maps.
struct ChunkVertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(8) biome_weights_lo: vec4<f32>,
    @location(9) biome_weights_hi: vec4<f32>,
    @location(10) biome_weights_8: vec4<f32>,
}

struct ChunkVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(4) @interpolate(flat) instance_index: u32,
    @location(8) biome_weights_lo: vec4<f32>,
    @location(9) biome_weights_hi: vec4<f32>,
    @location(10) biome_weights_8: vec4<f32>,
}

@vertex
fn vertex(in: ChunkVertex) -> ChunkVertexOutput {
    var out: ChunkVertexOutput;

    let world_from_local = mesh_functions::get_world_from_local(in.instance_index);
    let world_position = mesh_functions::mesh_position_local_to_world(
        world_from_local,
        vec4<f32>(in.position, 1.0),
    );
    out.world_position = world_position;
    out.position = position_world_to_clip(world_position.xyz);

    out.world_normal = mesh_functions::mesh_normal_local_to_world(
        in.normal,
        in.instance_index,
    );
    out.uv = in.uv;
    out.instance_index = in.instance_index;
    out.biome_weights_lo = in.biome_weights_lo;
    out.biome_weights_hi = in.biome_weights_hi;
    out.biome_weights_8 = in.biome_weights_8;
    return out;
}

// ---- Fragment -------------------------------------------------------

/// Pull the 9 per-biome weights out of the three vec4 vertex
/// attributes. Slot i ↔ biome with id() == i, no indirection.
fn read_weights(in: ChunkVertexOutput) -> array<f32, 9> {
    return array<f32, 9>(
        in.biome_weights_lo.x, in.biome_weights_lo.y,
        in.biome_weights_lo.z, in.biome_weights_lo.w,
        in.biome_weights_hi.x, in.biome_weights_hi.y,
        in.biome_weights_hi.z, in.biome_weights_hi.w,
        in.biome_weights_8.x,
    );
}

/// 9 distinct primary-ish colors, indexed by BiomeKey id (0..8).
/// Used by debug_mode=1 (DominantBiome flat color).
fn biome_debug_color(id: u32) -> vec3<f32> {
    switch (id) {
        case 0u: { return vec3<f32>(0.30, 0.70, 0.30); } // Grass
        case 1u: { return vec3<f32>(0.10, 0.85, 0.20); } // GrassLush
        case 2u: { return vec3<f32>(0.20, 0.50, 0.30); } // Mossy
        case 3u: { return vec3<f32>(0.55, 0.40, 0.20); } // Dirt
        case 4u: { return vec3<f32>(0.95, 0.95, 1.00); } // Snow
        case 5u: { return vec3<f32>(0.55, 0.55, 0.60); } // Stone
        case 6u: { return vec3<f32>(0.30, 0.20, 0.20); } // Scorched
        case 7u: { return vec3<f32>(0.40, 0.55, 0.50); } // Marsh
        case 8u: { return vec3<f32>(0.50, 0.45, 0.45); } // Rocky
        default: { return vec3<f32>(1.00, 0.00, 1.00); } // unknown → magenta
    }
}

@fragment
fn fragment(in: ChunkVertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    let weights = read_weights(in);

    // Debug mode early-return paths: bypass PBR entirely so the
    // displayed color is unambiguously the underlying value.
    if (blend_params.debug_mode != 0u) {
        var debug_rgb: vec3<f32> = vec3<f32>(0.0);
        switch (blend_params.debug_mode) {
            case 1u: {
                // Dominant biome (highest weight) → flat color.
                var best_w = weights[0];
                var best_id = 0u;
                for (var i = 1u; i < 9u; i = i + 1u) {
                    if (weights[i] > best_w) {
                        best_w = weights[i];
                        best_id = i;
                    }
                }
                debug_rgb = biome_debug_color(best_id);
            }
            case 2u: {
                // First 3 weights as RGB (biomes 0=Grass, 1=GrassLush, 2=Mossy).
                debug_rgb = vec3<f32>(weights[0], weights[1], weights[2]);
            }
            case 3u: {
                // Biome 7 (Marsh — the default) weight as grayscale.
                debug_rgb = vec3<f32>(weights[7]);
            }
            default: {
                debug_rgb = vec3<f32>(1.0, 0.0, 1.0); // shouldn't reach
            }
        }
        var dbg: FragmentOutput;
        dbg.color = vec4<f32>(debug_rgb, 1.0);
        return dbg;
    }

    // Build the VertexOutput shape that Bevy's pbr_input_from_vertex_output expects.
    var pbr_in: VertexOutput;
    pbr_in.position = in.position;
    pbr_in.world_position = in.world_position;
    pbr_in.world_normal = in.world_normal;
    pbr_in.uv = in.uv;
    pbr_in.instance_index = in.instance_index;

    var pbr_input: pbr_types::PbrInput = pbr_input_from_vertex_output(pbr_in, is_front, false);

    let tile_uv = in.world_position.xz / blend_params.tile_size_m;

    // Sum over all 9 biome layers, weighted by per-biome weights.
    // Skip layers with sub-threshold weight to avoid useless texture
    // fetches. ~3-9 sample calls per fragment depending on how many
    // biomes contribute at this point (typically 1-2 for interior
    // areas, 2-4 along boundaries).
    var color_acc = vec3<f32>(0.0);
    var ao_acc = 0.0;
    var w_total = 0.0;
    let layer_cap = max(blend_params.layer_count, 1u) - 1u;
    for (var i = 0u; i < 9u; i = i + 1u) {
        let w = weights[i];
        if (w <= 0.0001) {
            continue;
        }
        let layer = i32(min(i, layer_cap));
        color_acc = color_acc + textureSample(color_array, ground_sampler, tile_uv, layer).rgb * w;
        ao_acc = ao_acc + textureSample(ao_array, ground_sampler, tile_uv, layer).r * w;
        w_total = w_total + w;
    }
    if (w_total > 0.0001) {
        color_acc = color_acc / w_total;
        ao_acc = ao_acc / w_total;
    } else {
        // Degenerate (zero) weights — fall back to Marsh (default biome, id=7).
        color_acc = textureSample(color_array, ground_sampler, tile_uv, 7).rgb;
        ao_acc = textureSample(ao_array, ground_sampler, tile_uv, 7).r;
    }

    pbr_input.material.base_color = vec4<f32>(color_acc, 1.0);
    pbr_input.material.perceptual_roughness = 1.0;
    pbr_input.material.metallic = 0.0;
    pbr_input.material.reflectance = vec3<f32>(0.5);
    pbr_input.diffuse_occlusion = vec3<f32>(ao_acc);
    pbr_input.material.flags = pbr_types::STANDARD_MATERIAL_FLAGS_FOG_ENABLED_BIT;

    let lit = apply_pbr_lighting(pbr_input);
    var out: FragmentOutput;
    out.color = main_pass_post_lighting_processing(pbr_input, lit);
    return out;
}
