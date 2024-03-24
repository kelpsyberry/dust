#[derive(Default)]
pub struct ToonCode {
    pub toon_uniforms: String,

    pub toon_get_color: &'static str,
}

impl ToonCode {
    pub fn new(bg_index: u32) -> ToonCode {
        ToonCode {
            toon_uniforms: format!(
                "@group({bg_index}) @binding(0) var<uniform> toon_colors: array<vec4<u32>, 0x20>;"
            ),

            toon_get_color: "let toon_color = vec4<f32>(toon_colors[u32(v_color.r * 31.0)]) * \
                             (1.0 / 31.0);",
        }
    }
}
