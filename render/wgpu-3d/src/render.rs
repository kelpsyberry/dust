macro_rules! ifdef {
    ($flag: expr, $str: expr) => {
        if $flag {
            $str
        } else {
            Default::default()
        }
    };
}

pub mod opaque;
pub mod trans;
pub mod trans_no_depth_update;

struct CommonCode {
    common_vert_inputs: &'static str,
    common_vert_outputs: &'static str,
    common_set_vert_outputs: &'static str,

    common_frag_inputs: &'static str,
    common_frag_outputs: &'static str,
}

impl CommonCode {
    const fn new() -> Self {
        CommonCode {
            common_vert_inputs: "
                @location(0) position: vec2<u32>,
                @location(1) depth: u32,
                @location(2) w: u32,
                @location(4) v_color: vec4<u32>,",

            common_vert_outputs: "
                @builtin(position) position: vec4<f32>,
                @location(0) v_color: vec4<f32>,",

            // TODO: Use `(vec2<f32>(position) - 7.99)`?
            common_set_vert_outputs: "
                let depth_f32 = f32(depth) * (1.0 / 0x1000000.0);
                output.position = vec4<f32>(
                    vec2<f32>(position) * vec2<f32>(0.125 / 256.0, -0.125 / 192.0) + \
                                      vec2<f32>(-1.0, 1.0),
                    depth_f32,
                    1.0,
                ) * (f32(w) * (1.0 / 0x1000.));
                output.v_color = vec4<f32>(vec3<f32>(v_color.xyz) * vec3<f32>(1.0 / 511.0), 1.0);",

            common_frag_inputs: "@location(0) v_color: vec4<f32>,",
            common_frag_outputs: "@location(0) color: vec4<f32>,",
        }
    }
}

const COMMON_VERT_ATTRIBS: [wgpu::VertexAttribute; 4] = [
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Uint16x2,
        offset: 0,
        shader_location: 0,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Uint32,
        offset: 4,
        shader_location: 1,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Uint32,
        offset: 8,
        shader_location: 2,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Uint16x4,
        offset: 16,
        shader_location: 4,
    },
];

#[derive(Default)]
struct WBufferCode {
    w_buffer_vert_outputs: &'static str,
    w_buffer_set_vert_outputs: &'static str,

    w_buffer_frag_inputs: &'static str,
    w_buffer_frag_outputs: &'static str,
    w_buffer_set_frag_outputs: &'static str,
}

impl WBufferCode {
    const fn new() -> Self {
        WBufferCode {
            w_buffer_vert_outputs: "@location(2) w: f32,",
            w_buffer_set_vert_outputs: "output.w = f32(depth) * (1.0 / 0x1000000.0);",
            w_buffer_frag_inputs: "@location(2) w: f32,",
            w_buffer_frag_outputs: "@builtin(frag_depth) frag_depth: f32,",
            w_buffer_set_frag_outputs: "output.frag_depth = w;",
        }
    }
}

#[derive(Default)]
struct TextureCode {
    texture_uniforms: String,

    texture_vert_inputs: &'static str,
    texture_vert_outputs: &'static str,
    texture_set_vert_outputs: &'static str,

    texture_frag_inputs: &'static str,
    texture_get_color: &'static str,
}

impl TextureCode {
    fn new(bg_index: u32) -> Self {
        TextureCode {
            texture_uniforms: format!(
                "@group({bg_index}) @binding(0) var t_texture: texture_2d<f32>;
                @group({bg_index}) @binding(1) var s_texture: sampler;",
            ),

            texture_vert_inputs: "@location(3) uv: vec2<i32>,",
            texture_vert_outputs: "@location(1) uv: vec2<f32>,",
            texture_set_vert_outputs: "output.uv = vec2<f32>(uv) * vec2<f32>(1.0 / 16.0);",

            texture_frag_inputs: "@location(1) uv: vec2<f32>,",
            texture_get_color: "let t_color = textureSample(t_texture, s_texture, uv / \
                                vec2<f32>(textureDimensions(t_texture))) * vec4<f32>(255.0 / \
                                31.0);",
        }
    }
}

const TEXTURE_VERT_ATTRIBS: [wgpu::VertexAttribute; 1] = [wgpu::VertexAttribute {
    format: wgpu::VertexFormat::Sint16x2,
    offset: 12,
    shader_location: 3,
}];

#[derive(Default)]
struct ToonCode {
    toon_uniforms: String,

    toon_get_color: &'static str,
}

impl ToonCode {
    fn new(bg_index: u32) -> ToonCode {
        ToonCode {
            toon_uniforms: format!(
                "struct ToonUniform {{
                    colors: array<vec4<u32>, 0x20>,
                }};

                @group({bg_index}) @binding(0) var<uniform> toon: ToonUniform;"
            ),
            toon_get_color: "let toon_color = vec4<f32>(toon.colors[u32(v_color.r * 31.0)]) * \
                             (1.0 / 31.0);",
        }
    }
}

fn get_output_color(mode: u8, texture_mapping_enabled: bool) -> &'static str {
    if texture_mapping_enabled {
        match mode {
            0 => {
                "output.color = ((t_color * 63.0 + 1.0) * (v_color * 63.0 + 1.0) - 1.0) * (1.0 / \
                 4095.0);"
            }
            1 => {
                "output.color = vec4<f32>((t_color * t_color.a + v_color * (1.0 - t_color.a)).rgb, \
                 v_color.a);"
            }
            2 => {
                "output.color = ((t_color * 63.0 + 1.0) * (toon_color * 63.0 + 1.0) - 1.0) * (1.0 \
                 / 4095.0);"
            }
            _ => {
                "let blended_color = ((t_color * 63.0 + 1.0) * vec4<f32>(vec3<f32>(v_color.r * \
                 63.0 + 1.0), 64.0) - 1.0) * (1.0 / 4095.0);
                output.color = min(blended_color + vec4<f32>(toon_color.rgb, 0.0), vec4<f32>(1.0));"
            }
        }
    } else {
        match mode {
            0 | 1 => "output.color = v_color;",
            2 => "output.color = toon_color;",
            _ => "output.color = min(v_color.r + vec4<f32>(toon_color.rgb, 0.0), vec4<f32>(1.0));",
        }
    }
}

const PRIMITIVE_STATE: wgpu::PrimitiveState = wgpu::PrimitiveState {
    topology: wgpu::PrimitiveTopology::TriangleList,
    strip_index_format: None,
    front_face: wgpu::FrontFace::Ccw,
    cull_mode: None,
    unclipped_depth: false,
    polygon_mode: wgpu::PolygonMode::Fill,
    conservative: false,
};

const TRANS_BLENDING: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::SrcAlpha,
        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        operation: wgpu::BlendOperation::Add,
    },
    alpha: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::One,
        operation: wgpu::BlendOperation::Max,
    },
};
