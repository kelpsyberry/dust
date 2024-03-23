#[derive(Default)]
pub struct AttrsCode {
    pub attrs_frag_outputs: &'static str,
    pub attrs_init_frag_outputs: &'static str,
}

impl AttrsCode {
    pub const fn new() -> Self {
        AttrsCode {
            attrs_frag_outputs: "@location(1) attrs: vec4<f32>,",
            attrs_init_frag_outputs: "output.attrs = vec4<f32>(0);",
        }
    }
}
