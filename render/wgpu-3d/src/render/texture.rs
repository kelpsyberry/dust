#[derive(Default)]
pub struct TextureCode {
    pub texture_uniforms: String,

    pub texture_vert_inputs: &'static str,
    pub texture_vert_outputs: &'static str,
    pub texture_set_vert_outputs: &'static str,

    pub texture_frag_inputs: &'static str,
    pub texture_get_color: &'static str,
}

impl TextureCode {
    pub fn new(bg_index: u32) -> Self {
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

pub const TEXTURE_VERT_ATTRIBS: [wgpu::VertexAttribute; 1] = [wgpu::VertexAttribute {
    format: wgpu::VertexFormat::Sint16x2,
    offset: 12,
    shader_location: 3,
}];
