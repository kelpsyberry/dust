pub struct CommonCode {
    pub common_vert_inputs: &'static str,
    pub common_vert_outputs: &'static str,
    pub common_set_vert_outputs: &'static str,

    pub common_frag_inputs: &'static str,
    pub common_frag_outputs: &'static str,
}

impl CommonCode {
    pub const fn new() -> Self {
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
                let depth_f32 = f32(depth) * (1.0 / 0x1000000);
                output.position = vec4<f32>(
                    vec2<f32>(position) * vec2<f32>(0.125 / 256.0, -0.125 / 192.0) + \
                                      vec2<f32>(-1.0, 1.0),
                    depth_f32,
                    1.0,
                ) * (f32(w) * (1.0 / 0x10000));
                output.v_color = vec4<f32>(vec3<f32>(v_color.rgb) * vec3<f32>(1.0 / 511.0), 1.0);",

            common_frag_inputs: "@location(0) v_color: vec4<f32>,",
            common_frag_outputs: "@location(0) color: vec4<f32>,",
        }
    }
}

pub const COMMON_VERT_ATTRIBS: [wgpu::VertexAttribute; 4] = [
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
