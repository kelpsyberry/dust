use crate::BgLayouts;

// TODO: No polygon edge detection actually breaks a lot of things...

#[derive(Default)]
pub struct EdgeMarkingCode {
    pub edge_marking_uniforms: String,
    pub edge_marking_set_frag_outputs: &'static str,
}

impl EdgeMarkingCode {
    pub fn new(id_bg_index: u32) -> Self {
        EdgeMarkingCode {
            edge_marking_uniforms: format!(
                "@group({id_bg_index}) @binding(0) var<uniform> id: u32;",
            ),

            edge_marking_set_frag_outputs: "output.attrs.r = f32(id) / 0x3F;",
        }
    }
}

fn shader_module_src(antialiasing_enabled: bool) -> String {
    let edge_alpha = if antialiasing_enabled { "0.5" } else { "1.0" };
    format!(
        "
@group(0) @binding(0) var<uniform> edge_colors: array<vec4<u32>, 8>;
@group(1) @binding(0) var depth_texture: texture_depth_2d;
@group(1) @binding(1) var attrs_texture: texture_2d<f32>;

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

fn loadOrDepth(texture: texture_depth_2d, coords: vec2<i32>, default_: f32) -> f32 {{
    if (any(coords.xy < vec2(0i)) || any(coords.xy >= vec2<i32>(textureDimensions(texture)))) {{
        return default_;
    }} else {{
        return textureLoad(texture, coords, 0);
    }}
}}

fn loadOrAttrs(texture: texture_2d<f32>, coords: vec2<i32>, default_: f32) -> f32 {{
    if (any(coords.xy < vec2(0i)) || any(coords.xy >= vec2<i32>(textureDimensions(texture)))) {{
        return default_;
    }} else {{
        return textureLoad(texture, coords, 0).r;
    }}
}}

@fragment
fn fs_main(
    @builtin(position) position: vec4<f32>,
) -> @location(0) vec4<f32> {{
    var coords = vec2<i32>(position.xy);
    var depth = textureLoad(depth_texture, coords, 0);
    var attrs = textureLoad(attrs_texture, coords, 0);
    var id = u32(attrs.r * 0x3F);
    var u_attrs = loadOrAttrs(attrs_texture, coords + vec2<i32>( 0i, -1i), 0.0);
    var u_depth = loadOrDepth(depth_texture, coords + vec2<i32>( 0i, -1i), 1.0);
    var d_attrs = loadOrAttrs(attrs_texture, coords + vec2<i32>( 0i,  1i), 0.0);
    var d_depth = loadOrDepth(depth_texture, coords + vec2<i32>( 0i,  1i), 1.0);
    var l_attrs = loadOrAttrs(attrs_texture, coords + vec2<i32>(-1i,  0i), 0.0);
    var l_depth = loadOrDepth(depth_texture, coords + vec2<i32>(-1i,  0i), 1.0);
    var r_attrs = loadOrAttrs(attrs_texture, coords + vec2<i32>( 1i,  0i), 0.0);
    var r_depth = loadOrDepth(depth_texture, coords + vec2<i32>( 1i,  0i), 1.0);
    if (
        (u_attrs != attrs.r && depth < u_depth) ||
        (d_attrs != attrs.r && depth < d_depth) ||
        (l_attrs != attrs.r && depth < l_depth) ||
        (r_attrs != attrs.r && depth < r_depth)
    ) {{
        var edge_color = vec4<f32>(edge_colors[id >> 3]) * vec4<f32>(1.0 / 31);
        return vec4(edge_color.rgb, {edge_alpha});
    }}
    return vec4(0.0);
}}"
    )
}

pub(crate) fn create_pipeline(
    antialiasing_enabled: bool,
    device: &wgpu::Device,
    bg_layouts: &BgLayouts,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("3D renderer edge marking pipeline layout"),
        bind_group_layouts: &[&bg_layouts.edge_colors, &bg_layouts.depth_attrs],
        push_constant_ranges: &[],
    });

    let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("3D renderer edge marking shader module"),
        source: wgpu::ShaderSource::Wgsl(shader_module_src(antialiasing_enabled).into()),
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("3D renderer edge marking pipeline"),
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
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),

        multiview: None,
    })
}
