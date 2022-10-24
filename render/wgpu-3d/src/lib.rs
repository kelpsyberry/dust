#![feature(
    const_mut_refs,
    const_trait_impl,
    portable_simd,
    array_windows,
    inline_const,
    maybe_uninit_uninit_array,
    new_uninit
)]

mod data;
pub use data::{FrameData, GxData, RenderingData};
mod render;
#[cfg(feature = "threaded")]
pub mod threaded;
mod utils;

use ahash::AHashMap as HashMap;
use core::{
    mem::{self, MaybeUninit},
    num::NonZeroU32,
    // simd::u16x2,
    slice,
};
use dust_core::gpu::engine_3d::{Color, Polygon, RenderingControl, ScreenVertex, TextureParams};
use std::sync::Arc;
use utils::{color_to_wgpu_f64, decode_rgb5, round_up_to_alignment};
use wgpu::util::DeviceExt;

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq, Hash)]
    const struct ControlFlags(pub u8): Debug {
        pub texture_mapping_enabled: bool @ 0,
        pub highlight_shading_enabled: bool @ 1,
        pub alpha_blending_enabled: bool @ 3,
        // pub antialiasing_enabled: bool @ 4,

        // TODO: These may require other special handling
        // pub edge_marking_enabled: bool @ 5,
        // pub fog_only_alpha: bool @ 6,
        // pub fog_enabled: bool @ 7,
    }
}

impl From<RenderingControl> for ControlFlags {
    fn from(other: RenderingControl) -> Self {
        ControlFlags(other.0 as u8 & 0xB)
    }
}

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq, Hash)]
    const struct TextureKey(pub u64): Debug {
        pub vram_offset: u16 @ 0..=15,
        pub width_shift: u8 @ 16..=18,
        pub height_shift: u8 @ 19..=21,
        pub format: u8 @ 22..=24,
        pub color_0_is_transparent: bool @ 25,
        pub palette_base: u16 @ 26..=38,
    }
}

impl TextureKey {
    pub fn new(params: TextureParams, tex_palette_base: u16) -> Self {
        TextureKey(
            (params.0 as u64 & 0xFFFF)
                | (params.0 as u64 >> 4 & 0x3FF_0000)
                | (tex_palette_base as u64) << 26,
        )
    }
}

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq, Hash)]
    const struct SamplerKey(pub u8): Debug {
        pub repeat_s: bool @ 0,
        pub repeat_t: bool @ 1,
        pub flip_s: bool @ 2,
        pub flip_t: bool @ 3,
    }
}

impl From<TextureParams> for SamplerKey {
    fn from(other: TextureParams) -> Self {
        SamplerKey((other.0 >> 16 & 0xF) as u8)
    }
}

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq, Hash)]
    const struct PipelineKey(pub u16): Debug {
        pub texture_mapping_enabled: bool @ 0,
        pub alpha_blending_enabled: bool @ 1,
        pub depth_test_equal: bool @ 2,
        pub mode: u8 @ 3..=4,
        pub is_shadow: bool @ 5,
        pub w_buffering: bool @ 6,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum BatchKind {
    ShadowMask {
        depth_test_equal: bool,
    },
    Opaque {
        pipeline: PipelineKey,
        texture: Option<(TextureKey, SamplerKey)>,
    },
    Translucent {
        pipeline: PipelineKey,
        texture: Option<(TextureKey, SamplerKey)>,
        id: u8,
        alpha_and_ref: (u8, u8),
    },
    TranslucentNoDepthUpdate {
        pipeline: PipelineKey,
        texture: Option<(TextureKey, SamplerKey)>,
        id: u8,
        alpha_and_ref: (u8, u8),
    },
    Wireframe {
        pipeline: PipelineKey,
        texture: Option<(TextureKey, SamplerKey)>,
    },
}

impl BatchKind {
    pub fn new(control: ControlFlags, w_buffering: bool, alpha_ref: u8, poly: &Polygon) -> Self {
        let mode = poly.attrs.mode();
        let id = poly.attrs.id();
        let depth_test_equal = poly.attrs.depth_test_equal();
        let is_shadow = mode == 3;
        if is_shadow && id == 0 {
            BatchKind::ShadowMask { depth_test_equal }
        } else {
            let texture_mapping_enabled =
                control.texture_mapping_enabled() && poly.tex_params.format() != 0;
            let pipeline = PipelineKey(0)
                .with_texture_mapping_enabled(texture_mapping_enabled)
                .with_alpha_blending_enabled(
                    poly.is_translucent && control.alpha_blending_enabled(),
                )
                .with_depth_test_equal(depth_test_equal)
                .with_mode(if is_shadow {
                    1
                } else {
                    match mode {
                        1 => texture_mapping_enabled as u8,
                        2 => 2 + control.highlight_shading_enabled() as u8,
                        _ => mode,
                    }
                })
                .with_is_shadow(is_shadow)
                .with_w_buffering(w_buffering);
            let texture = texture_mapping_enabled.then(|| {
                (
                    TextureKey::new(poly.tex_params, poly.tex_palette_base),
                    poly.tex_params.into(),
                )
            });

            let alpha = poly.attrs.alpha();

            if poly.is_translucent {
                if poly.attrs.update_depth_for_translucent() {
                    BatchKind::Translucent {
                        pipeline,
                        texture,
                        id,
                        alpha_and_ref: (alpha, alpha_ref),
                    }
                } else {
                    BatchKind::TranslucentNoDepthUpdate {
                        pipeline,
                        texture,
                        id,
                        alpha_and_ref: (alpha, alpha_ref),
                    }
                }
            } else if alpha == 0 {
                BatchKind::Wireframe { pipeline, texture }
            } else {
                BatchKind::Opaque { pipeline, texture }
            }
        }
    }
}

struct Texture {
    view: wgpu::TextureView,
    texture_region_mask: u8,
    tex_pal_region_mask: u8,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum PreparedBatchKind {
    ShadowMask {
        depth_test_equal: bool,
    },
    Opaque {
        pipeline: Option<PipelineKey>,
        texture: Option<Option<((TextureKey, SamplerKey), u8)>>,
    },
    Translucent {
        pipeline_changed: bool,
        pipeline: PipelineKey,
        texture: Option<Option<((TextureKey, SamplerKey), u8)>>,
        id: Option<u8>,
        alpha_and_ref: Option<(u8, u8)>,
    },
    TranslucentNoDepthUpdate {
        pipeline_changed: bool,
        pipeline: PipelineKey,
        texture: Option<Option<((TextureKey, SamplerKey), u8)>>,
        id: Option<u8>,
        alpha_and_ref: Option<(u8, u8)>,
    },
    Wireframe {
        pipeline: Option<PipelineKey>,
        texture: Option<Option<((TextureKey, SamplerKey), u8)>>,
    },
}

#[derive(Debug)]
struct PreparedBatch {
    kind: PreparedBatchKind,
    idxs: u16,
}

#[repr(C)]
struct Vertex {
    pub coords: [u16; 2],
    pub depth: u32,
    pub w: u32,
    pub uv: [i16; 2],
    pub color: [u16; 4],
    pub id: u32,
}

impl Vertex {
    pub fn new(
        raw: &ScreenVertex,
        // hi_res_coords_mask: u16x2,
        depth: u32,
        w: u16,
        id: u8,
    ) -> Self {
        Vertex {
            coords: raw.hi_res_coords.to_array(),
            // coords: (raw.hi_res_coords & hi_res_coords_mask).to_array(),
            depth,
            w: w as u32,
            uv: raw.uv.to_array(),
            color: raw.color.to_array(),
            id: id as u32,
        }
    }
}

fn create_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture_key: TextureKey,
    frame: &FrameData,
    decode_buffer: &mut Vec<u32>,
) -> Texture {
    let width = 8 << texture_key.width_shift();
    let height = 8 << texture_key.height_shift();
    let total_shift = texture_key.width_shift() + texture_key.height_shift();

    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };

    let raw = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("3D renderer texture"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
    });

    decode_buffer.clear();

    let tex_base = (texture_key.vram_offset() as usize) << 3;
    let pal_base = (texture_key.palette_base() as usize) << 3 << (texture_key.format() != 2) as u8;

    let mut texture_region_mask = 0;
    let mut tex_pal_region_mask = 0;

    macro_rules! calc_range {
        ($range: ident, $bits_per_pixel: expr) => {
            let $range = (
                tex_base,
                (tex_base + ((8 * $bits_per_pixel) << total_shift)) & 0x7_FFFF,
            );
            let texture_region_range = ($range.0 >> 17, $range.1.wrapping_sub(1) >> 17 & 3);
            let mut i = texture_region_range.0;
            loop {
                texture_region_mask |= 1 << i;
                if i == texture_region_range.1 {
                    break;
                }
                i = (i + 1) & 3;
            }
        };
    }

    macro_rules! read_palette {
        ($color_index: expr, $alpha: expr) => {{
            let addr = (pal_base + ($color_index << 1)) & 0x1_FFFF;
            tex_pal_region_mask |= 1 << (addr >> 14);
            decode_rgb5(frame.rendering.tex_pal.read_le::<u16>(addr), $alpha)
        }};
    }

    match texture_key.format() {
        1 => {
            calc_range!(range, 8);

            let mut i = range.0;
            while i != range.1 {
                let pixel = unsafe { *frame.rendering.texture.get_unchecked(i) };
                let color_index = pixel as usize & 0x1F;
                let raw_alpha = pixel >> 5;
                decode_buffer.push(read_palette!(color_index, raw_alpha << 2 | raw_alpha >> 1));
                i = (i + 1) & 0x7_FFFF;
            }
        }

        2 => {
            calc_range!(range, 2);

            let mut i = range.0;
            while i != range.1 {
                let mut pixels = unsafe { *frame.rendering.texture.get_unchecked(i) };
                for _ in 0..4 {
                    let color_index = pixels as usize & 3;
                    decode_buffer.push(read_palette!(
                        color_index,
                        if texture_key.color_0_is_transparent() && color_index == 0 {
                            0
                        } else {
                            0x1F
                        }
                    ));
                    pixels >>= 2;
                }
                i = (i + 1) & 0x7_FFFF;
            }
        }

        3 => {
            calc_range!(range, 4);

            let mut i = range.0;
            while i != range.1 {
                let mut pixels = unsafe { *frame.rendering.texture.get_unchecked(i) };
                for _ in 0..2 {
                    let color_index = pixels as usize & 0xF;
                    decode_buffer.push(read_palette!(
                        color_index,
                        if texture_key.color_0_is_transparent() && color_index == 0 {
                            0
                        } else {
                            0x1F
                        }
                    ));
                    pixels >>= 4;
                }
                i = (i + 1) & 0x7_FFFF;
            }
        }

        4 => {
            calc_range!(range, 8);

            let mut i = range.0;
            while i != range.1 {
                let color_index = unsafe { *frame.rendering.texture.get_unchecked(i) } as usize;
                decode_buffer.push(read_palette!(
                    color_index,
                    if texture_key.color_0_is_transparent() && color_index == 0 {
                        0
                    } else {
                        0x1F
                    }
                ));
                i = (i + 1) & 0x7_FFFF;
            }
        }

        5 => {
            let slot_0_2_range = (
                tex_base & 0x5_FFFF,
                (tex_base & 0x4_0000) | ((tex_base + ((8 * 2) << total_shift)) & 0x1_FFFF),
            );
            texture_region_mask = 1 << (tex_base >> 17 & 2) | 2;

            let len = 64 << total_shift;
            decode_buffer.reserve(len);

            let mut dst_pos = 0;
            let width = width as usize;
            let in_block_line_increment = width - 4;
            let width_mask = width - 1;
            let block_line_increment = width * 3;

            let mut i = slot_0_2_range.0;
            while i != slot_0_2_range.1 {
                unsafe {
                    let mut pixels = frame.rendering.texture.read_le_aligned_unchecked::<u32>(i);
                    let pal_data_addr = 0x2_0000 | (i >> 1 & 0xFFFE) | (i >> 2 & 0x1_0000);
                    let pal_data = frame
                        .rendering
                        .texture
                        .read_le_aligned_unchecked::<u16>(pal_data_addr);
                    let pal_base = pal_base + (pal_data << 2) as usize;
                    let mode = pal_data >> 14;

                    let mut dst = decode_buffer.as_mut_ptr().add(dst_pos);

                    macro_rules! process {
                        (|$texel: ident| $process: expr) => {
                            for _ in 0..4 {
                                for _ in 0..4 {
                                    let $texel = pixels & 3;
                                    dst.write($process);
                                    pixels >>= 2;
                                    dst = dst.add(1);
                                }
                                dst = dst.add(in_block_line_increment);
                            }
                        };
                    }

                    macro_rules! color {
                        ($i: expr) => {
                            decode_rgb5(
                                {
                                    let addr = (pal_base + ($i << 1)) & 0x1_FFFE;
                                    tex_pal_region_mask |= 1 << (addr >> 14);
                                    frame
                                        .rendering
                                        .tex_pal
                                        .read_le_aligned_unchecked::<u16>(addr)
                                },
                                0x1F,
                            )
                        };
                    }

                    let color_0 = color!(0);
                    let color_1 = color!(1);

                    match mode {
                        0 => process!(|texel| {
                            match texel {
                                0 => color_0,
                                1 => color_1,
                                2 => color!(2),
                                _ => 0,
                            }
                        }),
                        1 => process!(|texel| {
                            match texel {
                                0 => color_0,
                                1 => color_1,
                                2 => (color_0 + color_1) >> 1 & 0x1F1F_1F1F,
                                _ => 0,
                            }
                        }),
                        2 => process!(|texel| {
                            match texel {
                                0 => color_0,
                                1 => color_1,
                                2 => color!(2),
                                _ => color!(3),
                            }
                        }),
                        _ => process!(|texel| {
                            match texel {
                                0 => color_0,
                                1 => color_1,
                                2 => (color_0 * 5 + color_1 * 3) >> 3 & 0x1F1F_1F1F,
                                _ => (color_0 * 3 + color_1 * 5) >> 3 & 0x1F1F_1F1F,
                            }
                        }),
                    };
                }

                dst_pos += 4;
                if dst_pos & width_mask == 0 {
                    dst_pos += block_line_increment;
                }

                i = (i + 4) & 0x7_FFFF;
            }

            unsafe {
                decode_buffer.set_len(len);
            }
        }

        6 => {
            calc_range!(range, 8);

            let mut i = range.0;
            while i != range.1 {
                let pixel = unsafe { *frame.rendering.texture.get_unchecked(i) };
                let color_index = pixel as usize & 7;
                let raw_alpha = pixel >> 3;
                decode_buffer.push(read_palette!(color_index, raw_alpha));
                i = (i + 1) & 0x7_FFFF;
            }
        }

        _ => {
            calc_range!(range, 16);

            let mut i = range.0;
            while i != range.1 {
                let color = unsafe { frame.rendering.texture.read_le_aligned_unchecked::<u16>(i) };
                decode_buffer.push(decode_rgb5(
                    color,
                    if color & 0x8000 != 0 { 0x1F } else { 0 },
                ));
                i = (i + 2) & 0x7_FFFF;
            }
        }
    }

    unsafe {
        queue.write_texture(
            raw.as_image_copy(),
            slice::from_raw_parts(decode_buffer.as_ptr() as *const u8, decode_buffer.len() * 4),
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(NonZeroU32::new_unchecked(width << 2)),
                rows_per_image: None,
            },
            size,
        );
    }

    let view = raw.create_view(&wgpu::TextureViewDescriptor::default());

    Texture {
        view,
        texture_region_mask,
        tex_pal_region_mask: tex_pal_region_mask & 0x3F,
    }
}

fn create_sampler(device: &wgpu::Device, sampler_key: SamplerKey) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("3D renderer texture descriptor"),
        address_mode_u: if sampler_key.repeat_s() {
            if sampler_key.flip_s() {
                wgpu::AddressMode::MirrorRepeat
            } else {
                wgpu::AddressMode::Repeat
            }
        } else {
            wgpu::AddressMode::ClampToEdge
        },
        address_mode_v: if sampler_key.repeat_t() {
            if sampler_key.flip_t() {
                wgpu::AddressMode::MirrorRepeat
            } else {
                wgpu::AddressMode::Repeat
            }
        } else {
            wgpu::AddressMode::ClampToEdge
        },
        ..Default::default()
    })
}

struct OutputAttachments {
    color: wgpu::Texture,
    color_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
}

impl OutputAttachments {
    fn new(device: &wgpu::Device, resolution_scale_shift: u8) -> Self {
        let resolution_scale = 1 << resolution_scale_shift;

        let color = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("3D renderer color"),
            size: wgpu::Extent3d {
                width: 256 * resolution_scale,
                height: 192 * resolution_scale,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        });
        let color_view = color.create_view(&wgpu::TextureViewDescriptor {
            label: Some("3D renderer color view"),
            ..wgpu::TextureViewDescriptor::default()
        });

        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("3D renderer depth"),
            size: wgpu::Extent3d {
                width: 256 * resolution_scale,
                height: 192 * resolution_scale,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32FloatStencil8,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        });
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor {
            label: Some("3D renderer depth view"),
            ..wgpu::TextureViewDescriptor::default()
        });

        OutputAttachments {
            color,
            color_view,
            depth_view,
        }
    }
}

pub struct Renderer {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,

    resolution_scale_shift: u8,
    // hi_res_coords_mask: u16x2,
    output_attachments: OutputAttachments,

    vtx_buffer: wgpu::Buffer,
    vtx_buffer_contents: Vec<Vertex>,
    idx_buffer: wgpu::Buffer,
    idx_buffer_contents: Vec<u16>,

    toon_colors: [Color; 0x20],
    toon_buffer: wgpu::Buffer,
    toon_bg_layout: wgpu::BindGroupLayout,
    toon_bg: wgpu::BindGroup,

    // rear_plane_texture: wgpu::Texture,
    // render_rear_plane_bitmap_pipeline: Pipeline,
    pipelines: HashMap<PipelineKey, wgpu::RenderPipeline>,
    trans_pipelines: HashMap<PipelineKey, [wgpu::RenderPipeline; 2]>,
    trans_no_depth_update_pipelines: HashMap<PipelineKey, [wgpu::RenderPipeline; 2]>,
    textures: HashMap<TextureKey, Texture>,
    samplers: [Option<wgpu::Sampler>; 0x10],
    texture_bg_layout: wgpu::BindGroupLayout,
    texture_bgs: HashMap<(TextureKey, SamplerKey), wgpu::BindGroup>,

    id_bg_layout: wgpu::BindGroupLayout,
    id_bg: wgpu::BindGroup,
    id_bg_elem_size: usize,

    alpha_and_ref_bg_layout: wgpu::BindGroupLayout,
    alpha_and_ref_bg: wgpu::BindGroup,
    alpha_and_ref_bg_elem_size: usize,

    texture_decode_buffer: Vec<u32>,
    batches: Vec<PreparedBatch>,
}

impl Renderer {
    pub fn new(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        resolution_scale_shift: u8,
    ) -> Self {
        let device_limits = device.limits();
        let min_uniform_buffer_offset_alignment = device_limits.min_uniform_buffer_offset_alignment;

        // let hi_res_coords_mask = u16x2::splat(!(0x10 >> resolution_scale_shift.min(4)) - 1);

        let output_attachments = OutputAttachments::new(&device, resolution_scale_shift);

        let vert_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("3D renderer vertices"),
            size: mem::size_of::<Vertex>() as u64 * 6144,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::VERTEX,
            mapped_at_creation: false,
        });

        let idx_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("3D renderer vertex indices"),
            size: 2 * 2048 * (10 - 2) * 3,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::INDEX,
            mapped_at_creation: false,
        });

        let toon_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("3D renderer toon table"),
            size: 0x200,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
            mapped_at_creation: false,
        });
        let toon_bg_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("3D renderer toon table bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(0x200),
                },
                count: None,
            }],
        });
        let toon_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("3D renderer toon table bind group"),
            layout: &toon_bg_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &toon_buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(0x200),
                }),
            }],
        });

        let texture_bg_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("3D renderer texture bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        macro_rules! uniform_bind_group {
            ($label: literal, $shader_stages: expr, $binding_size: expr, $contents: expr) => {{
                let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some(concat!("3D renderer ", $label, " bind group layout")),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: $shader_stages,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: true,
                            min_binding_size: wgpu::BufferSize::new($binding_size),
                        },
                        count: None,
                    }],
                });

                let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(concat!("3D renderer ", $label, " buffer")),
                    contents: &$contents,
                    usage: wgpu::BufferUsages::UNIFORM,
                });

                let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some(concat!("3D renderer ", $label, " bind group")),
                    layout: &layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &buffer,
                            offset: 0,
                            size: wgpu::BufferSize::new($binding_size),
                        }),
                    }],
                });

                (layout, bind_group)
            }};
        }

        let id_bg_elem_size =
            round_up_to_alignment(4, min_uniform_buffer_offset_alignment as usize);
        let (id_bg_layout, id_bg) = uniform_bind_group!("ID", wgpu::ShaderStages::FRAGMENT, 4, {
            let mut buffer_contents = Vec::new();
            buffer_contents.resize(id_bg_elem_size * 0x40, 0);
            let mut addr = 0;
            for i in 0..0x40 {
                buffer_contents[addr..addr + 4].copy_from_slice(&(i as u32).to_ne_bytes());
                addr += id_bg_elem_size;
            }
            buffer_contents
        });

        let alpha_and_ref_bg_elem_size =
            round_up_to_alignment(8, min_uniform_buffer_offset_alignment as usize);
        let (alpha_and_ref_bg_layout, alpha_and_ref_bg) = uniform_bind_group!(
            "alpha",
            wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
            8,
            {
                let mut buffer_contents = Vec::new();
                buffer_contents.resize(alpha_and_ref_bg_elem_size * (31 * 0x20), 0);
                let mut addr = 0;
                for alpha in 1_u32..0x20 {
                    for alpha_ref in 0_u32..0x20 {
                        buffer_contents[addr..addr + 4]
                            .copy_from_slice(&(alpha as f32 * (1.0 / 31.0)).to_ne_bytes());
                        buffer_contents[addr + 4..addr + 8].copy_from_slice(
                            &((alpha_ref as f32 + 0.5) * (1.0 / 31.0)).to_ne_bytes(),
                        );
                        addr += alpha_and_ref_bg_elem_size;
                    }
                }
                buffer_contents
            }
        );

        Renderer {
            device,
            queue,

            resolution_scale_shift,
            // hi_res_coords_mask,
            output_attachments,

            vtx_buffer: vert_buffer,
            vtx_buffer_contents: Vec::new(),
            idx_buffer,
            idx_buffer_contents: Vec::new(),

            toon_colors: [Color::splat(0xFF); 0x20],
            toon_buffer,
            toon_bg_layout,
            toon_bg,

            pipelines: HashMap::default(),
            trans_pipelines: HashMap::default(),
            trans_no_depth_update_pipelines: HashMap::default(),

            textures: HashMap::default(),
            samplers: [const { None }; 0x10],
            texture_bg_layout,
            texture_bgs: HashMap::default(),

            id_bg_layout,
            id_bg,
            id_bg_elem_size,

            alpha_and_ref_bg,
            alpha_and_ref_bg_layout,
            alpha_and_ref_bg_elem_size,

            texture_decode_buffer: Vec::new(),
            batches: Vec::new(),
        }
    }

    #[inline]
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    #[inline]
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    #[inline]
    pub fn resolution_scale_shift(&self) -> u8 {
        self.resolution_scale_shift
    }

    pub fn set_resolution_scale_shift(&mut self, value: u8) {
        if value == self.resolution_scale_shift {
            return;
        }
        self.resolution_scale_shift = value;
        self.output_attachments = OutputAttachments::new(&self.device, value);
    }

    pub fn create_output_view(&self) -> wgpu::TextureView {
        self.output_attachments
            .color
            .create_view(&Default::default())
    }

    pub fn render_frame(&mut self, frame: &FrameData) -> wgpu::CommandBuffer {
        self.textures.retain(|_, texture| {
            (texture.texture_region_mask & frame.rendering.texture_dirty)
                | (texture.tex_pal_region_mask & frame.rendering.tex_pal_dirty)
                == 0
        });
        self.texture_bgs
            .retain(|(texture, _), _| self.textures.contains_key(texture));

        let mut command_encoder =
            self.device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("3D renderer command encoder"),
                });

        let mut render_pass = command_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("3D renderer render pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.output_attachments.color_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(
                        if frame.rendering.control.rear_plane_bitmap_enabled() {
                            wgpu::Color::BLACK
                        } else {
                            color_to_wgpu_f64(frame.rendering.clear_color)
                        },
                    ),
                    store: true,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.output_attachments.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(
                        if frame.rendering.control.rear_plane_bitmap_enabled() {
                            0.0
                        } else {
                            frame.rendering.clear_depth as f32 / (1 << 24) as f32
                        },
                    ),
                    store: false,
                }),
                stencil_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(0),
                    store: false,
                }),
            }),
        });

        if frame.rendering.control.rear_plane_bitmap_enabled() {
            // TODO
        }

        let polys = &frame.gx.poly_ram[..frame.gx.poly_ram_level as usize];
        if !polys.is_empty() && frame.rendering.alpha_test_ref < 0x1F {
            let control_flags = ControlFlags::from(frame.rendering.control);

            let mut toon_used = false;

            self.vtx_buffer_contents.clear();
            self.idx_buffer_contents.clear();

            self.batches.clear();

            let mut cur_batch = None;
            let mut cur_batch_indices_start = 0;

            let mut prepare_texture = |(texture_key, sampler_key): (TextureKey, SamplerKey)| {
                self.texture_bgs
                    .entry((texture_key, sampler_key))
                    .or_insert_with(|| {
                        let texture = self.textures.entry(texture_key).or_insert_with(|| {
                            create_texture(
                                &self.device,
                                &self.queue,
                                texture_key,
                                frame,
                                &mut self.texture_decode_buffer,
                            )
                        });
                        let sampler = self.samplers[sampler_key.0 as usize]
                            .get_or_insert_with(|| create_sampler(&self.device, sampler_key));
                        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                            label: Some("3D renderer texture bind group"),
                            layout: &self.texture_bg_layout,
                            entries: &[
                                wgpu::BindGroupEntry {
                                    binding: 0,
                                    resource: wgpu::BindingResource::TextureView(&texture.view),
                                },
                                wgpu::BindGroupEntry {
                                    binding: 1,
                                    resource: wgpu::BindingResource::Sampler(sampler),
                                },
                            ],
                        })
                    });
            };

            let mut prepare_batch = |batch_kind: BatchKind, cur_batch_kind: Option<BatchKind>| {
                macro_rules! field_updates {
                    (
                        $batch_kind: ident;
                        $($field: ident, $cur_field: ident, $field_changed: ident);*
                    ) => {
                        let ($($field_changed),*) = if let Some(BatchKind::$batch_kind {
                            $($field: $cur_field),*
                        }) = cur_batch_kind
                        {
                            ($($field != $cur_field),*)
                        } else {
                            ($({let _ = $field; true}),*)
                        };
                    }
                }

                macro_rules! bg_indices {
                    ($start: expr, ($($ident: ident$(* $cond: expr)?),*), $process: expr) => {{
                        let mut bg_index = $start;
                        $(
                            #[allow(unused_assignments)]
                            let $ident = if true$(&& $cond)* {
                                let res = bg_index;
                                bg_index += 1;
                                res
                            } else {
                                0
                            };
                        )*
                        $process
                    }}
                }

                match batch_kind {
                    BatchKind::ShadowMask { depth_test_equal } => {
                        // TODO: Prepare pipeline (the only possible change is depth_test_equal, so
                        // it's not checked)

                        PreparedBatchKind::ShadowMask { depth_test_equal }
                    }

                    BatchKind::Opaque { pipeline, texture } => {
                        field_updates!(
                            Opaque;
                            pipeline, cur_pipeline, pipeline_changed;
                            texture, cur_texture, texture_changed
                        );

                        if pipeline_changed {
                            self.pipelines.entry(pipeline).or_insert_with(|| {
                                render::opaque::create_pipeline(
                                    pipeline,
                                    &self.device,
                                    &self.toon_bg_layout,
                                    &self.texture_bg_layout,
                                )
                            });
                        }

                        if texture_changed {
                            if let Some(texture) = texture {
                                prepare_texture(texture);
                            }
                        }

                        bg_indices!(
                            0,
                            (
                                _toon_bg_index * pipeline.mode() >= 2,
                                texture_bg_index * texture.is_some()
                            ),
                            PreparedBatchKind::Opaque {
                                pipeline: pipeline_changed.then_some(pipeline),
                                texture: texture_changed
                                    .then(|| texture.map(|t| (t, texture_bg_index))),
                            }
                        )
                    }

                    BatchKind::Translucent {
                        pipeline,
                        texture,
                        id,
                        alpha_and_ref,
                    } => {
                        field_updates!(
                            Translucent;
                            pipeline, cur_pipeline, pipeline_changed;
                            texture, cur_texture, texture_changed;
                            id, cur_id, id_changed;
                            alpha_and_ref, cur_alpha_and_ref, alpha_and_ref_changed
                        );

                        if pipeline_changed {
                            self.trans_pipelines.entry(pipeline).or_insert_with(|| {
                                render::trans::create_pipeline(
                                    pipeline,
                                    &self.device,
                                    &self.id_bg_layout,
                                    &self.alpha_and_ref_bg_layout,
                                    &self.toon_bg_layout,
                                    &self.texture_bg_layout,
                                )
                            });
                        }

                        if texture_changed {
                            if let Some(texture) = texture {
                                prepare_texture(texture);
                            }
                        }

                        bg_indices!(
                            2,
                            (
                                _toon_bg_index * pipeline.mode() >= 2,
                                texture_bg_index * texture.is_some()
                            ),
                            PreparedBatchKind::Translucent {
                                pipeline_changed,
                                pipeline,
                                texture: texture_changed
                                    .then(|| texture.map(|t| (t, texture_bg_index))),
                                id: id_changed.then_some(id),
                                alpha_and_ref: alpha_and_ref_changed.then_some(alpha_and_ref)
                            }
                        )
                    }

                    BatchKind::TranslucentNoDepthUpdate {
                        pipeline,
                        texture,
                        id,
                        alpha_and_ref,
                    } => {
                        field_updates!(
                            TranslucentNoDepthUpdate;
                            pipeline, cur_pipeline, pipeline_changed;
                            texture, cur_texture, texture_changed;
                            id, cur_id, id_changed;
                            alpha_and_ref, cur_alpha_and_ref, alpha_and_ref_changed
                        );

                        if pipeline_changed {
                            self.trans_no_depth_update_pipelines
                                .entry(pipeline)
                                .or_insert_with(|| {
                                    render::trans_no_depth_update::create_pipeline(
                                        pipeline,
                                        &self.device,
                                        &self.id_bg_layout,
                                        &self.alpha_and_ref_bg_layout,
                                        &self.toon_bg_layout,
                                        &self.texture_bg_layout,
                                    )
                                });
                        }

                        if texture_changed {
                            if let Some(texture) = texture {
                                prepare_texture(texture);
                            }
                        }

                        bg_indices!(
                            2,
                            (
                                _toon_bg_index * pipeline.mode() >= 2,
                                texture_bg_index * texture.is_some()
                            ),
                            PreparedBatchKind::TranslucentNoDepthUpdate {
                                pipeline_changed,
                                pipeline,
                                texture: texture_changed
                                    .then(|| texture.map(|t| (t, texture_bg_index))),
                                id: id_changed.then_some(id),
                                alpha_and_ref: alpha_and_ref_changed.then_some(alpha_and_ref)
                            }
                        )
                    }

                    BatchKind::Wireframe { pipeline, texture } => {
                        field_updates!(
                            Wireframe;
                            pipeline, cur_pipeline, pipeline_changed;
                            texture, cur_texture, texture_changed
                        );

                        if pipeline_changed {
                            // TODO
                        }

                        if texture_changed {
                            if let Some(texture) = texture {
                                prepare_texture(texture);
                            }
                        }

                        bg_indices!(
                            0,
                            (
                                _toon_bg_index * pipeline.mode() >= 2,
                                texture_bg_index * texture.is_some()
                            ),
                            PreparedBatchKind::Wireframe {
                                pipeline: pipeline_changed.then_some(pipeline),
                                texture: texture_changed
                                    .then(|| texture.map(|t| (t, texture_bg_index))),
                            }
                        )
                    }
                }
            };

            macro_rules! finish_batch {
                () => {
                    if let Some((_, prepared_batch_kind)) = &cur_batch {
                        self.batches.push(PreparedBatch {
                            kind: *prepared_batch_kind,
                            idxs: (self.idx_buffer_contents.len() - cur_batch_indices_start) as u16,
                        });
                    }
                };
            }

            for poly in polys {
                let batch_kind = BatchKind::new(
                    control_flags,
                    frame.gx.w_buffering,
                    frame.rendering.alpha_test_ref,
                    poly,
                );
                if match cur_batch {
                    None => true,
                    Some((cur_batch_kind, _)) => cur_batch_kind != batch_kind,
                } {
                    finish_batch!();
                    cur_batch = Some((
                        batch_kind,
                        prepare_batch(batch_kind, cur_batch.as_ref().map(|v| v.0)),
                    ));
                    cur_batch_indices_start = self.idx_buffer_contents.len();
                }

                toon_used |= poly.attrs.mode() == 2;

                if poly.vertices_len.get() < 3 || poly.attrs.mode() == 3 {
                    // TODO: Do process shadow/shadow mask polygons
                    continue;
                }

                let id = poly.attrs.id();
                let base_idx = self.vtx_buffer_contents.len() as u16;
                self.vtx_buffer_contents.extend(
                    poly.vertices[..poly.vertices_len.get() as usize]
                        .iter()
                        .enumerate()
                        .map(|(i, vert_addr)| {
                            Vertex::new(
                                &frame.gx.vert_ram[vert_addr.get() as usize],
                                // self.hi_res_coords_mask,
                                poly.depth_values[i],
                                poly.w_values[i],
                                id,
                            )
                        }),
                );

                for i in base_idx..base_idx + (poly.vertices_len.get() - 1) as u16 {
                    self.idx_buffer_contents
                        .extend_from_slice(&[base_idx, i, i + 1]);
                }
            }
            finish_batch!();

            unsafe {
                self.queue.write_buffer(
                    &self.vtx_buffer,
                    0,
                    slice::from_raw_parts(
                        self.vtx_buffer_contents.as_ptr() as *const u8,
                        round_up_to_alignment(
                            self.vtx_buffer_contents.len() * mem::size_of::<Vertex>(),
                            wgpu::COPY_BUFFER_ALIGNMENT as usize,
                        ),
                    ),
                );
                self.queue.write_buffer(
                    &self.idx_buffer,
                    0,
                    slice::from_raw_parts(
                        self.idx_buffer_contents.as_ptr() as *const u8,
                        round_up_to_alignment(
                            self.idx_buffer_contents.len() * 2,
                            wgpu::COPY_BUFFER_ALIGNMENT as usize,
                        ),
                    ),
                );
                if toon_used && frame.rendering.toon_colors != self.toon_colors {
                    self.toon_colors = frame.rendering.toon_colors;
                    let mut toon_colors = MaybeUninit::uninit_array::<0x20>();
                    for (dst, src) in toon_colors.iter_mut().zip(&self.toon_colors) {
                        *dst = MaybeUninit::new(src.cast::<u32>().to_array());
                    }
                    self.queue.write_buffer(
                        &self.toon_buffer,
                        0,
                        slice::from_raw_parts(toon_colors.as_ptr() as *const u8, 0x200),
                    );
                }
            }

            render_pass.set_vertex_buffer(0, self.vtx_buffer.slice(..));
            render_pass.set_index_buffer(self.idx_buffer.slice(..), wgpu::IndexFormat::Uint16);

            let mut cur_idx_base = 0;
            for batch in &self.batches {
                match batch.kind {
                    PreparedBatchKind::ShadowMask { depth_test_equal } => {}

                    PreparedBatchKind::Opaque { pipeline, texture } => {
                        if let Some(pipeline) = pipeline {
                            render_pass.set_pipeline(&self.pipelines[&pipeline]);
                            if pipeline.mode() >= 2 {
                                render_pass.set_bind_group(0, &self.toon_bg, &[])
                            }
                        }

                        if let Some(Some((texture, bg_index))) = texture {
                            render_pass.set_bind_group(
                                bg_index as u32,
                                &self.texture_bgs[&texture],
                                &[],
                            );
                        }

                        if batch.idxs != 0 {
                            render_pass.draw_indexed(
                                cur_idx_base..cur_idx_base + batch.idxs as u32,
                                0,
                                0..1,
                            );
                        }
                    }

                    PreparedBatchKind::Translucent {
                        pipeline_changed,
                        pipeline,
                        texture,
                        id,
                        alpha_and_ref,
                    } => {
                        if pipeline_changed && pipeline.mode() >= 2 {
                            render_pass.set_bind_group(2, &self.toon_bg, &[])
                        }

                        if let Some(id) = id {
                            render_pass.set_stencil_reference((id | 0x40) as u32);
                            render_pass.set_bind_group(
                                0,
                                &self.id_bg,
                                &[(id as usize * self.id_bg_elem_size) as wgpu::DynamicOffset],
                            );
                        }

                        if let Some((alpha, alpha_ref)) = alpha_and_ref {
                            render_pass.set_bind_group(
                                1,
                                &self.alpha_and_ref_bg,
                                &[(((alpha - 1) as usize * 0x20 + alpha_ref as usize)
                                    * self.alpha_and_ref_bg_elem_size)
                                    as wgpu::DynamicOffset],
                            );
                        }

                        if let Some(Some((texture, bg_index))) = texture {
                            render_pass.set_bind_group(
                                bg_index as u32,
                                &self.texture_bgs[&texture],
                                &[],
                            );
                        }

                        if batch.idxs != 0 {
                            for pipeline in &self.trans_pipelines[&pipeline] {
                                render_pass.set_pipeline(pipeline);
                                render_pass.draw_indexed(
                                    cur_idx_base..cur_idx_base + batch.idxs as u32,
                                    0,
                                    0..1,
                                );
                            }
                        }
                    }

                    PreparedBatchKind::TranslucentNoDepthUpdate {
                        pipeline_changed,
                        pipeline,
                        texture,
                        id,
                        alpha_and_ref,
                    } => {
                        if pipeline_changed && pipeline.mode() >= 2 {
                            render_pass.set_bind_group(2, &self.toon_bg, &[])
                        }

                        if let Some(id) = id {
                            render_pass.set_stencil_reference((id | 0x40) as u32);
                            render_pass.set_bind_group(
                                0,
                                &self.id_bg,
                                &[(id as usize * self.id_bg_elem_size) as wgpu::DynamicOffset],
                            );
                        }

                        if let Some((alpha, alpha_ref)) = alpha_and_ref {
                            render_pass.set_bind_group(
                                1,
                                &self.alpha_and_ref_bg,
                                &[(((alpha - 1) as usize * 0x20 + alpha_ref as usize)
                                    * self.alpha_and_ref_bg_elem_size)
                                    as wgpu::DynamicOffset],
                            );
                        }

                        if let Some(Some((texture, bg_index))) = texture {
                            render_pass.set_bind_group(
                                bg_index as u32,
                                &self.texture_bgs[&texture],
                                &[],
                            );
                        }

                        if batch.idxs != 0 {
                            for pipeline in &self.trans_no_depth_update_pipelines[&pipeline] {
                                render_pass.set_pipeline(pipeline);
                                render_pass.draw_indexed(
                                    cur_idx_base..cur_idx_base + batch.idxs as u32,
                                    0,
                                    0..1,
                                );
                            }
                        }
                    }

                    PreparedBatchKind::Wireframe { pipeline, texture } => {}
                }

                cur_idx_base += batch.idxs as u32;
            }
        }

        drop(render_pass);

        command_encoder.finish()
    }
}
