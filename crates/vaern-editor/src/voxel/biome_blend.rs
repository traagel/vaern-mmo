//! Multi-texture biome blend material — single shared
//! `ExtendedMaterial<StandardMaterial, BiomeBlendExt>` bound on every
//! voxel chunk entity. Per-vertex blend weights drive a 4-biome
//! splat in the fragment shader so painted biome boundaries read as
//! soft transitions instead of hard chunk-aligned grid lines.
//!
//! The fragment shader at `assets/shaders/biome_blend.wgsl` exposes a
//! single `biome_blend_at` helper as the swap point for Phase 6
//! (splatmap upgrade) — that function's body is replaced with a
//! splatmap texture sample without touching the texture array
//! bindings, sampler, or PBR pipeline.

use std::path::Path;

use bevy::asset::RenderAssetUsages;
use bevy::image::{
    Image, ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor,
};
use bevy::math::Vec2;
use bevy::mesh::{MeshVertexAttribute, MeshVertexBufferLayoutRef, VertexAttributeValues};
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, Extent3d, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
    TextureDimension, TextureFormat, VertexFormat,
};
use bevy::shader::ShaderRef;

use super::biomes::BiomeKey;
use super::overrides::BiomeOverrideMap;
use super::stream::DEFAULT_BIOME;

/// World units per repeat of one biome texture tile. Mirrors the value
/// used by the legacy `build_biome_material` so painted ground reads at
/// the same physical scale as the single-material code path.
pub const TILE_SIZE_M: f32 = 24.0;

/// Side length (texels) of every biome texture layer in the texture
/// arrays. 1024² = ~2.3cm/texel at the 24m tile size — well below
/// per-pixel detail at any normal viewing distance, and 4× less VRAM
/// + bandwidth than the source 2K JPGs. The build pass downsamples
/// each ambientCG layer with `image::imageops::Triangle` before
/// composing the array.
pub const ARRAY_LAYER_RES: u32 = 1024;

/// Anisotropic-filter clamp on the shared sampler. Lowered from the
/// legacy 16 to 4 — at 24m tile size + per-fragment 4-biome blend (8
/// texture samples per fragment), 16× anisotropy was burning serious
/// fragment-shader time without much visible quality difference.
pub const SAMPLER_ANISOTROPY: u16 = 4;

/// Custom mesh vertex attribute: weights for biomes 0..3 (Grass,
/// GrassLush, Mossy, Dirt). Linearly interpolated. Slot i in the
/// vec4 maps directly to texture-array layer i — no ID indirection.
pub const ATTRIBUTE_BIOME_WEIGHTS_LO: MeshVertexAttribute = MeshVertexAttribute::new(
    "Vertex_BiomeWeightsLo",
    988_477_790,
    VertexFormat::Float32x4,
);

/// Custom mesh vertex attribute: weights for biomes 4..7 (Snow,
/// Stone, Scorched, Marsh). Linearly interpolated.
pub const ATTRIBUTE_BIOME_WEIGHTS_HI: MeshVertexAttribute = MeshVertexAttribute::new(
    "Vertex_BiomeWeightsHi",
    988_477_791,
    VertexFormat::Float32x4,
);

/// Custom mesh vertex attribute: weight for biome 8 (Rocky) in `.x`,
/// the other three components are reserved padding. Linearly
/// interpolated.
///
/// We split the 9 weights across three vec4 attributes (rather than
/// one per-biome float each) because Bevy/wgpu vertex layouts are
/// vec4-aligned and three vec4s pack the 9 useful floats tightly.
///
/// **Why per-biome weights instead of (4 IDs + 4 weights) with a
/// sort step?** Adjacent chunks' triangles use *different* provoking
/// vertices via WGSL's `@interpolate(flat)` — different vertices have
/// different "candidate sub-cell" sets, so their ID slot orderings
/// diverge, producing visible color jumps at chunk boundaries. With
/// fixed-slot per-biome weights the whole chain is linear-interpolated,
/// no flat-interp variance, no boundary artifacts.
pub const ATTRIBUTE_BIOME_WEIGHTS_8: MeshVertexAttribute = MeshVertexAttribute::new(
    "Vertex_BiomeWeights8",
    988_477_792,
    VertexFormat::Float32x4,
);

/// Material extension carrying the three texture-array handles plus
/// blend params. Bevy's `ExtendedMaterial` combines this with the
/// `StandardMaterial` base bind group.
///
/// Sampler binding is paired with `color_array` at slot 101; the
/// fragment shader reuses that one sampler for all three texture
/// arrays (samplers are independent of texture bindings in WGSL).
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct BiomeBlendExt {
    #[texture(100, dimension = "2d_array")]
    #[sampler(101)]
    pub color_array: Handle<Image>,
    #[texture(102, dimension = "2d_array")]
    pub normal_array: Handle<Image>,
    #[texture(103, dimension = "2d_array")]
    pub ao_array: Handle<Image>,
    #[uniform(104)]
    pub params: BlendUniform,
}

#[derive(Clone, Copy, Debug, Default, Reflect, ShaderType)]
pub struct BlendUniform {
    pub tile_size_m: f32,
    pub layer_count: u32,
    /// Debug viz mode. Driven by `BlendDebugMode` resource. Values:
    ///   0 = normal PBR rendering (default, ships product look)
    ///   1 = dominant-biome flat color (no texture, no lighting) —
    ///       confirms which biome each fragment thinks it is
    ///   2 = first-3 weights as RGB channels (slot 0 = red,
    ///       slot 1 = green, slot 2 = blue) — reveals smooth vs
    ///       sharp boundary jumps
    ///   3 = slot-0 weight as grayscale heatmap — confirms whether
    ///       chunk centers really get weight=1 like the math says
    pub debug_mode: u32,
    pub _pad0: u32,
}

/// Debug visualization mode for the biome blend shader. Driven by the
/// inspector dropdown; copied into the live material's uniform on
/// change. Inspector default is `Normal` so end-users never see the
/// debug colors.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlendDebugMode {
    Normal = 0,
    DominantBiome = 1,
    SlotWeightsRgb = 2,
    Slot0Heatmap = 3,
}

impl Default for BlendDebugMode {
    fn default() -> Self {
        Self::Normal
    }
}

impl BlendDebugMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "Normal (PBR)",
            Self::DominantBiome => "Dominant biome (flat color)",
            Self::SlotWeightsRgb => "Slot weights (R/G/B)",
            Self::Slot0Heatmap => "Slot-0 weight (heatmap)",
        }
    }
    pub const ALL: [BlendDebugMode; 4] = [
        Self::Normal,
        Self::DominantBiome,
        Self::SlotWeightsRgb,
        Self::Slot0Heatmap,
    ];
}

impl MaterialExtension for BiomeBlendExt {
    fn vertex_shader() -> ShaderRef {
        "shaders/biome_blend.wgsl".into()
    }
    fn fragment_shader() -> ShaderRef {
        "shaders/biome_blend.wgsl".into()
    }

    // Disable prepass — without our extension shader on the prepass
    // pipeline, the standard prepass would still try to use the
    // standard PBR fragment which doesn't know about our bind group
    // contributions. Easier to skip prepass entirely than to provide a
    // custom prepass shader (we don't need motion vectors / depth
    // prepass for the editor scene).
    fn enable_prepass() -> bool {
        false
    }

    fn specialize(
        _pipeline: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _key: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // Tangents intentionally absent — see WGSL comment. Skipping
        // them avoids the MikkTSpace generation pass and removes the
        // "Vertex_Tangent missing" pipeline-specializer error window
        // that fires when chunks have a fresh mesh but tangents
        // haven't been generated yet.
        let vertex_layout = layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_NORMAL.at_shader_location(1),
            Mesh::ATTRIBUTE_UV_0.at_shader_location(2),
            // Per-biome weights split across 3 vec4s. Slots 0..3 in
            // `_lo`, 4..7 in `_hi`, biome 8 in `_8.x`. Linearly
            // interpolated — no flat-interp variance, no chunk-edge
            // color jumps.
            ATTRIBUTE_BIOME_WEIGHTS_LO.at_shader_location(8),
            ATTRIBUTE_BIOME_WEIGHTS_HI.at_shader_location(9),
            ATTRIBUTE_BIOME_WEIGHTS_8.at_shader_location(10),
        ])?;
        descriptor.vertex.buffers = vec![vertex_layout];
        Ok(())
    }
}

pub type BiomeBlendMaterial = ExtendedMaterial<StandardMaterial, BiomeBlendExt>;

/// Single shared material instance. All chunk render entities use this
/// handle — the per-chunk `BiomeMaterials` cache is gone.
///
/// `fallback_material` is a plain `StandardMaterial` with no texture
/// bindings — used for the "Disable biome blend" perf-isolation
/// toggle in the streaming panel. Lets the user swap chunks back to
/// a vanilla PBR pipeline to measure whether the custom shader is the
/// frame-time bottleneck.
#[derive(Resource)]
pub struct BiomeBlendAssets {
    pub material: Handle<BiomeBlendMaterial>,
    pub fallback_material: Handle<StandardMaterial>,
}

/// Resource: when true (default), chunks render through the shared
/// `BiomeBlendMaterial`. When false, chunks render through a plain
/// `StandardMaterial` so the user can A/B the per-fragment cost.
#[derive(Resource)]
pub struct BiomeBlendEnabled(pub bool);

impl Default for BiomeBlendEnabled {
    fn default() -> Self {
        Self(true)
    }
}

/// Startup system: synchronously load all 9 biome jpgs from the
/// workspace assets folder, build three texture-array `Image`s, and
/// create the shared `BiomeBlendMaterial` instance.
///
/// Sync load is ~1–2s of jpeg decode at startup; runs once. Going
/// async via `AssetServer.load_with_settings` would require waiting on
/// 27 separate asset events before we could compose the arrays, plus
/// asset-state machinery. Not worth it for a startup cost.
pub fn init_biome_blend_assets(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<BiomeBlendMaterial>>,
    mut std_materials: ResMut<Assets<StandardMaterial>>,
    mut log: ResMut<crate::ui::console::ConsoleLog>,
) {
    let assets_root = workspace_assets_root();

    let color_array = match build_layered_image(
        &assets_root,
        BiomeKey::ALL.iter().map(|b| b.textures().color),
        TextureFormat::Rgba8UnormSrgb,
    ) {
        Ok(img) => img,
        Err(e) => {
            warn!("biome_blend: color array build failed: {e}");
            log.push(format!("biome_blend: color array FAILED: {e}"));
            fallback_layered_image(BiomeKey::ALL.len() as u32, TextureFormat::Rgba8UnormSrgb)
        }
    };
    let normal_array = match build_layered_image(
        &assets_root,
        BiomeKey::ALL.iter().map(|b| b.textures().normal),
        TextureFormat::Rgba8Unorm,
    ) {
        Ok(img) => img,
        Err(e) => {
            warn!("biome_blend: normal array build failed: {e}");
            log.push(format!("biome_blend: normal array FAILED: {e}"));
            fallback_layered_image(BiomeKey::ALL.len() as u32, TextureFormat::Rgba8Unorm)
        }
    };
    // AO is optional per biome; pass white when missing so the layer
    // contributes neutrally to the blend.
    let ao_array = match build_layered_image_optional(
        &assets_root,
        BiomeKey::ALL.iter().map(|b| b.textures().ao),
        TextureFormat::Rgba8Unorm,
    ) {
        Ok(img) => img,
        Err(e) => {
            warn!("biome_blend: ao array build failed: {e}");
            log.push(format!("biome_blend: ao array FAILED: {e}"));
            fallback_layered_image(BiomeKey::ALL.len() as u32, TextureFormat::Rgba8Unorm)
        }
    };

    // The sampler at binding 101 comes from the color array Image's
    // own sampler descriptor (paired via `#[sampler(101)]` on the
    // color_array field). Configure it here; the WGSL fragment uses
    // this same sampler with all three texture arrays.
    let mut color_array = color_array;
    color_array.sampler = ground_sampler();
    let mut normal_array = normal_array;
    normal_array.sampler = ground_sampler();
    let mut ao_array = ao_array;
    ao_array.sampler = ground_sampler();

    let color_handle = images.add(color_array);
    let normal_handle = images.add(normal_array);
    let ao_handle = images.add(ao_array);

    let material = BiomeBlendMaterial {
        base: StandardMaterial {
            perceptual_roughness: 1.0,
            metallic: 0.0,
            ..default()
        },
        extension: BiomeBlendExt {
            color_array: color_handle,
            normal_array: normal_handle,
            ao_array: ao_handle,
            params: BlendUniform {
                tile_size_m: TILE_SIZE_M,
                layer_count: BiomeKey::ALL.len() as u32,
                debug_mode: BlendDebugMode::Normal as u32,
                _pad0: 0,
            },
        },
    };
    let material_handle = materials.add(material);

    // Plain-vanilla StandardMaterial as the perf-isolation fallback.
    // Mid-grey so chunks are still readable when the user toggles the
    // biome blend off; no textures, no shader extension cost.
    let fallback = std_materials.add(StandardMaterial {
        base_color: Color::srgb(0.45, 0.45, 0.45),
        perceptual_roughness: 1.0,
        metallic: 0.0,
        ..default()
    });

    commands.insert_resource(BiomeBlendAssets {
        material: material_handle,
        fallback_material: fallback,
    });
}

fn workspace_assets_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets")
        .canonicalize()
        .expect("workspace assets/ folder must exist for biome blend init")
}

fn ground_sampler() -> ImageSampler {
    ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        address_mode_w: ImageAddressMode::Repeat,
        anisotropy_clamp: SAMPLER_ANISOTROPY,
        // Trilinear: linearly filter within each mip level, then
        // linearly blend between mips. Without `mipmap_filter` set
        // the GPU uses nearest-mip sampling which causes visible LOD
        // popping at distance.
        mipmap_filter: ImageFilterMode::Linear,
        ..ImageSamplerDescriptor::linear()
    })
}

/// Number of mip levels for a square texture of side `dim`. Levels
/// halve until 1×1, so log2(dim) + 1.
fn mip_level_count(dim: u32) -> u32 {
    32 - dim.leading_zeros()
}

/// Decode `path`, resize to `target_size`² with high-quality
/// downsampling, then build a full mipmap chain. Returns one
/// `Vec<u8>` per mip level (largest first), each holding RGBA8 pixels
/// for that level.
///
/// Mipmaps are essential for the biome-blend material: without them
/// the fragment shader does up to anisotropy_clamp full-resolution
/// texel fetches per sample × 8 samples per fragment (4 color + 4
/// AO), saturating texture bandwidth and crushing FPS even with a
/// handful of visible chunks.
fn build_mip_chain(img: image::DynamicImage, target_size: u32) -> Vec<Vec<u8>> {
    let base = if img.width() != target_size || img.height() != target_size {
        img.resize_exact(
            target_size,
            target_size,
            image::imageops::FilterType::Triangle,
        )
    } else {
        img
    };
    let level_count = mip_level_count(target_size) as usize;
    let mut mips: Vec<Vec<u8>> = Vec::with_capacity(level_count);
    mips.push(base.to_rgba8().into_raw());
    let mut current = base;
    let mut size = target_size;
    while size > 1 {
        size /= 2;
        current = current.resize_exact(size, size, image::imageops::FilterType::Triangle);
        mips.push(current.to_rgba8().into_raw());
    }
    mips
}

/// Read each path under `assets_root`, decode to RGBA8, downsample
/// to `ARRAY_LAYER_RES`², generate a full mip chain, append all mip
/// levels to a single 2D-array `Image`.
///
/// Bevy's `Image` uses `TextureDataOrder::LayerMajor` by default —
/// the `data` buffer is laid out `Layer0Mip0 Layer0Mip1 ...
/// Layer0MipN Layer1Mip0 ...`. We append mips per-layer in that
/// order.
fn build_layered_image<'a, I>(
    assets_root: &Path,
    paths: I,
    format: TextureFormat,
) -> anyhow::Result<Image>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut data: Vec<u8> = Vec::new();
    let mut layer_count = 0u32;
    let mip_count = mip_level_count(ARRAY_LAYER_RES);
    for rel in paths {
        let abs = assets_root.join(rel);
        let raw = image::open(&abs)
            .map_err(|e| anyhow::anyhow!("decode {}: {}", abs.display(), e))?;
        if raw.width() != ARRAY_LAYER_RES || raw.height() != ARRAY_LAYER_RES {
            debug!(
                "biome_blend: resizing {} from {}x{} to {res}x{res} (Triangle)",
                abs.display(),
                raw.width(),
                raw.height(),
                res = ARRAY_LAYER_RES
            );
        }
        let mips = build_mip_chain(raw, ARRAY_LAYER_RES);
        debug_assert_eq!(mips.len() as u32, mip_count);
        for mip in mips {
            data.extend_from_slice(&mip);
        }
        layer_count += 1;
    }
    // Use `new_uninit` + manual `data` assign so the data length
    // (full mip chain × N layers) doesn't trip `Image::new`'s
    // single-mip-level size assertion. The texture upload pipeline
    // reads `texture_descriptor.mip_level_count` to know how to
    // partition the buffer.
    let mut image = Image::new_uninit(
        Extent3d {
            width: ARRAY_LAYER_RES,
            height: ARRAY_LAYER_RES,
            depth_or_array_layers: layer_count,
        },
        TextureDimension::D2,
        format,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.data = Some(data);
    image.texture_descriptor.mip_level_count = mip_count;
    Ok(image)
}

/// Same as `build_layered_image` but each path is `Option<&str>`;
/// missing layers are filled with neutral white texels (255 across
/// every channel — fully unoccluded) so the AO blend treats them as
/// "no occlusion". Synthetic layers also get a full mip chain so the
/// texture array's `mip_level_count` stays consistent.
fn build_layered_image_optional<'a, I>(
    assets_root: &Path,
    paths: I,
    format: TextureFormat,
) -> anyhow::Result<Image>
where
    I: IntoIterator<Item = Option<&'a str>>,
{
    let mut data: Vec<u8> = Vec::new();
    let mut layer_count = 0u32;
    let mip_count = mip_level_count(ARRAY_LAYER_RES);
    for rel in paths {
        match rel {
            Some(rel) => {
                let abs = assets_root.join(rel);
                let raw = image::open(&abs)
                    .map_err(|e| anyhow::anyhow!("decode {}: {}", abs.display(), e))?;
                let mips = build_mip_chain(raw, ARRAY_LAYER_RES);
                debug_assert_eq!(mips.len() as u32, mip_count);
                for mip in mips {
                    data.extend_from_slice(&mip);
                }
            }
            None => {
                // Synthesize a fully-white layer + mip chain.
                let mut size = ARRAY_LAYER_RES;
                loop {
                    let bytes = (size * size * 4) as usize;
                    data.extend(std::iter::repeat(255u8).take(bytes));
                    if size == 1 {
                        break;
                    }
                    size /= 2;
                }
            }
        }
        layer_count += 1;
    }
    // Use `new_uninit` + manual `data` assign so the data length
    // (full mip chain × N layers) doesn't trip `Image::new`'s
    // single-mip-level size assertion. The texture upload pipeline
    // reads `texture_descriptor.mip_level_count` to know how to
    // partition the buffer.
    let mut image = Image::new_uninit(
        Extent3d {
            width: ARRAY_LAYER_RES,
            height: ARRAY_LAYER_RES,
            depth_or_array_layers: layer_count,
        },
        TextureDimension::D2,
        format,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.data = Some(data);
    image.texture_descriptor.mip_level_count = mip_count;
    Ok(image)
}

/// Defensive fallback: if the real layered-image build fails, ship a
/// 1x1 magenta-ish array of `n` layers so the material still binds and
/// the editor can launch (rather than crashing on missing textures).
fn fallback_layered_image(n: u32, format: TextureFormat) -> Image {
    let bytes_per_pixel = 4usize;
    let layers = (0..n)
        .flat_map(|_| [255u8, 0, 255, 255].into_iter().take(bytes_per_pixel))
        .collect::<Vec<u8>>();
    Image::new(
        Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: n.max(1),
        },
        TextureDimension::D2,
        layers,
        format,
        RenderAssetUsages::RENDER_WORLD,
    )
}

// ---- Per-vertex blend weight computation ---------------------------

/// Compute the four-nearest-chunk biome IDs + weights for a vertex at
/// world position `(world_x, world_z)`.
///
/// Algorithm: each chunk-XZ owns a Voronoi cell whose center is the
/// chunk's footprint center. The vertex sits inside one cell and within
/// reach of 3 (corner case: 4) neighbor cells via its quadrant. We
/// look up the biome at the current chunk and the three quadrant
/// neighbors, then weight by **distance to each neighbor's footprint
/// center**, normalized via smoothstep so:
///   * vertex at chunk center → weight 1.0 on own biome, 0 elsewhere
///   * vertex on chunk boundary → 50/50 split with neighbor
///   * vertex on chunk corner → ~25/25/25/25 split with three neighbors
///
/// Output IDs are sorted ascending so adjacent triangles whose vertex
/// neighborhoods overlap put the same biome ID in the same slot — a
/// minor reduction in flat-interpolation discontinuity at biome
/// cluster edges. (Doesn't fully solve it; see the splatmap upgrade
/// path in the plan's Phase 6.)
pub fn compute_blend_weights(
    world_x: f32,
    world_z: f32,
    overrides: &BiomeOverrideMap,
) -> [f32; 9] {
    // Sub-cell granularity: each cell is `SUB_CELL_SIZE_M` wide
    // (8m at SUB_CELLS_PER_CHUNK=4 and CHUNK_WORLD_SIZE=32). Blend
    // zones are 8m wide, matching sub-chunk paint resolution.
    let cs = super::overrides::SUB_CELL_SIZE_M;
    let owner_x = (world_x / cs).floor() as i32;
    let owner_z = (world_z / cs).floor() as i32;
    let local_x = world_x - (owner_x as f32) * cs;
    let local_z = world_z - (owner_z as f32) * cs;
    let qx = if local_x < cs * 0.5 { -1 } else { 1 };
    let qz = if local_z < cs * 0.5 { -1 } else { 1 };

    let candidates = [
        (owner_x, owner_z),
        (owner_x + qx, owner_z),
        (owner_x, owner_z + qz),
        (owner_x + qx, owner_z + qz),
    ];

    // Accumulate distance-weighted contributions into per-biome slots.
    // Multiple candidates may map to the same biome (e.g. all 4 sub-
    // cells = Marsh on an unpainted area) → their weights sum into
    // the same slot.
    let mut weights = [0f32; 9];
    for (cx, cz) in candidates {
        let center = Vec2::new(
            (cx as f32) * cs + cs * 0.5,
            (cz as f32) * cs + cs * 0.5,
        );
        let d = (Vec2::new(world_x, world_z) - center).length();
        // Smoothstep falloff over one sub-cell width.
        let t = (d / cs).clamp(0.0, 1.0);
        let w = 1.0 - t * t * (3.0 - 2.0 * t);
        let biome = overrides.get(cx, cz).unwrap_or(DEFAULT_BIOME);
        weights[biome.id() as usize] += w;
    }

    // Renormalize so the sum is 1.0 — saves a fragment-shader divide
    // and ensures per-biome contributions stay in [0,1].
    let sum: f32 = weights.iter().sum();
    if sum > 1e-6 {
        for w in weights.iter_mut() {
            *w /= sum;
        }
    } else {
        weights[DEFAULT_BIOME.id() as usize] = 1.0;
    }
    weights
}

/// Insert per-vertex `ATTRIBUTE_BIOME_IDS` + `ATTRIBUTE_BIOME_WEIGHTS`
/// on a chunk mesh, computed from each vertex's world-XZ position
/// against the current `BiomeOverrideMap`.
///
/// Called from `ensure_chunk_mesh_attributes` after the world-XZ UVs
/// + tangents are generated. Re-runs on every re-mesh, so biome paint
/// edits that mark neighbor chunks dirty automatically pick up the
/// new override values.
pub fn insert_blend_attributes(
    mesh: &mut Mesh,
    chunk_origin: Vec3,
    overrides: &BiomeOverrideMap,
) {
    let positions = match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
        Some(VertexAttributeValues::Float32x3(p)) => p.clone(),
        _ => return,
    };
    // Three vec4 buffers: weights[0..4], weights[4..8], weights[8..9]
    // (the third has 3 padding floats — kept zero so the shader can
    // still read its `.x` component).
    let mut w_lo: Vec<[f32; 4]> = Vec::with_capacity(positions.len());
    let mut w_hi: Vec<[f32; 4]> = Vec::with_capacity(positions.len());
    let mut w_8: Vec<[f32; 4]> = Vec::with_capacity(positions.len());
    for p in &positions {
        let world_x = p[0] + chunk_origin.x;
        let world_z = p[2] + chunk_origin.z;
        let w = compute_blend_weights(world_x, world_z, overrides);
        w_lo.push([w[0], w[1], w[2], w[3]]);
        w_hi.push([w[4], w[5], w[6], w[7]]);
        w_8.push([w[8], 0.0, 0.0, 0.0]);
    }
    mesh.insert_attribute(ATTRIBUTE_BIOME_WEIGHTS_LO, VertexAttributeValues::Float32x4(w_lo));
    mesh.insert_attribute(ATTRIBUTE_BIOME_WEIGHTS_HI, VertexAttributeValues::Float32x4(w_hi));
    mesh.insert_attribute(ATTRIBUTE_BIOME_WEIGHTS_8, VertexAttributeValues::Float32x4(w_8));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertex_at_sub_cell_center_weights_own_biome_dominantly() {
        let mut overrides = BiomeOverrideMap::default();
        overrides.set(0, 0, BiomeKey::Snow);
        overrides.set(1, 0, BiomeKey::Marsh);
        overrides.set(0, 1, BiomeKey::Marsh);
        overrides.set(1, 1, BiomeKey::Marsh);

        let cs = super::super::overrides::SUB_CELL_SIZE_M;
        let weights = compute_blend_weights(cs * 0.5, cs * 0.5, &overrides);
        let snow_w = weights[BiomeKey::Snow.id() as usize];
        let marsh_w = weights[BiomeKey::Marsh.id() as usize];
        assert!(
            snow_w > marsh_w,
            "Snow weight {snow_w} should dominate at sub-cell center over Marsh {marsh_w}"
        );
    }

    #[test]
    fn weights_sum_to_one() {
        let mut overrides = BiomeOverrideMap::default();
        overrides.set(0, 0, BiomeKey::Stone);
        overrides.set(1, 0, BiomeKey::Marsh);
        for (wx, wz) in &[(0.0, 0.0), (4.0, 4.0), (6.0, 2.0), (7.5, 7.5)] {
            let weights = compute_blend_weights(*wx, *wz, &overrides);
            let sum: f32 = weights.iter().sum();
            assert!(
                (sum - 1.0).abs() < 1e-4,
                "weights at ({wx}, {wz}) sum to {sum}, weights={weights:?}"
            );
        }
    }

    #[test]
    fn distinct_biomes_each_get_nonzero_weight_at_corner() {
        let mut overrides = BiomeOverrideMap::default();
        overrides.set(0, 0, BiomeKey::Snow);
        overrides.set(1, 0, BiomeKey::Marsh);
        overrides.set(0, 1, BiomeKey::Stone);
        overrides.set(1, 1, BiomeKey::Grass);

        let cs = super::super::overrides::SUB_CELL_SIZE_M;
        // Sample at the corner where all 4 sub-cell biomes meet.
        let weights = compute_blend_weights(cs, cs, &overrides);
        for biome in [
            BiomeKey::Snow,
            BiomeKey::Marsh,
            BiomeKey::Stone,
            BiomeKey::Grass,
        ] {
            let w = weights[biome.id() as usize];
            assert!(
                w > 0.05,
                "{:?} should contribute at the 4-cell corner; got {w}",
                biome
            );
        }
    }

    #[test]
    fn vertex_on_sub_cell_boundary_blends_two_biomes() {
        let mut overrides = BiomeOverrideMap::default();
        overrides.set(0, 0, BiomeKey::Snow);
        overrides.set(1, 0, BiomeKey::Marsh);

        let cs = super::super::overrides::SUB_CELL_SIZE_M;
        let weights = compute_blend_weights(cs, cs * 0.5, &overrides);
        let snow_w = weights[BiomeKey::Snow.id() as usize];
        let marsh_w = weights[BiomeKey::Marsh.id() as usize];

        // Roughly equal at the boundary (within ~10% — exact values
        // depend on the smoothstep falloff curve and quadrant choice).
        assert!(
            (snow_w - marsh_w).abs() < 0.2,
            "boundary blend should be near 50/50: snow={snow_w}, marsh={marsh_w}"
        );
        assert!(snow_w > 0.2);
        assert!(marsh_w > 0.2);
    }
}
