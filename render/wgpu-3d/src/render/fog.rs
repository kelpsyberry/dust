use crate::BgLayouts;

#[derive(Default)]
pub struct FogCode {
    pub fog_uniforms: String,
    pub fog_set_frag_outputs: &'static str,
}

impl FogCode {
    pub fn new(enabled_bg_index: u32) -> Self {
        FogCode {
            fog_uniforms: format!(
                "struct FogEnabledUniform {{
                    enabled: u32,
                }};
                
                @group({enabled_bg_index}) @binding(0) var<uniform> fog_enabled: FogEnabledUniform;",
            ),

            fog_set_frag_outputs: "output.attrs.a = f32(fog_enabled.enabled);",
        }
    }
}

fn shader_module_src(only_alpha: bool) -> String {
    let blend = if only_alpha {
        "color.a = mix(color.a, fog_color.a, density);"
    } else {
        "color = mix(color, fog_color, density);"
    };

    format!(
        "
struct WrappedDensity {{
    @align(16) value: u32
}}

struct FogDataUniform {{
    densities: array<WrappedDensity, 34>,
    color: vec4<u32>,
    offset: u32,
    depth_shift: u32,
}};

@group(0) @binding(0) var<uniform> fog_data: FogDataUniform;
@group(1) @binding(0) var color_texture: texture_2d<f32>;
@group(2) @binding(0) var depth_texture: texture_depth_2d;
@group(2) @binding(1) var attrs_texture: texture_2d<f32>;

struct VertOutput {{
    @builtin(position) pos: vec4<f32>,
}}

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
) -> VertOutput {{
    var vert_positions: array<vec2<f32>, 4> = array<vec2<f32>, 4>(
        vec2(-1.0, 1.0),
        vec2(1.0, 1.0),
        vec2(-1.0, -1.0),
        vec2(1.0, -1.0),
    );

    var output: VertOutput;
    output.pos = vec4<f32>((*(&vert_positions))[vertex_index], 0.0, 1.0);
    return output;
}}

@fragment
fn fs_main(
    @builtin(position) position: vec4<f32>,
) -> @location(0) vec4<f32> {{
    var coords = vec2<u32>(position.xy);
    var color = textureLoad(color_texture, coords, 0);
    var depth = textureLoad(depth_texture, coords, 0);
    var attrs = textureLoad(attrs_texture, coords, 0);
    var fog_color = vec4<f32>(fog_data.color) * vec4<f32>(1.0 / 31.0);
    if attrs.a > 0.5 {{
        var z = u32(depth * 0x1000000);
        var offset: u32;
        if z < fog_data.offset {{
            offset = 0u;
        }} else {{
            offset = min(((z - fog_data.offset) >> 2) << fog_data.depth_shift, 32u << 17);
        }};
        var index = offset >> 17;
        var fract = offset & 0x1FFFFu;
        var density = f32((fog_data.densities[index].value * (0x20000u - fract)
            + fog_data.densities[index + 1].value * fract) >> 17) * (1.0 / 0x80.0);
        {blend}
    }}
    return color;
}}"
    )
}

pub(crate) fn create_pipeline(
    only_alpha: bool,
    device: &wgpu::Device,
    bg_layouts: &BgLayouts,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("3D renderer fog pipeline layout"),
        bind_group_layouts: &[
            &bg_layouts.fog_data,
            &bg_layouts.color,
            &bg_layouts.depth_attrs,
        ],
        push_constant_ranges: &[],
    });

    let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("3D renderer fog shader module"),
        source: wgpu::ShaderSource::Wgsl(shader_module_src(only_alpha).into()),
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("3D renderer fog pipeline"),
        layout: Some(&layout),

        vertex: wgpu::VertexState {
            module: &shader_module,
            entry_point: "vs_main",
            buffers: &[],
        },

        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },

        depth_stencil: None,

        multisample: wgpu::MultisampleState::default(),

        fragment: Some(wgpu::FragmentState {
            module: &shader_module,
            entry_point: "fs_main",
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8Unorm,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),

        multiview: None,
    })
}
