#[derive(Default)]
pub struct WBufferCode {
    pub w_buffer_vert_outputs: &'static str,
    pub w_buffer_set_vert_outputs: &'static str,

    pub w_buffer_frag_inputs: &'static str,
    pub w_buffer_frag_outputs: &'static str,
    pub w_buffer_set_frag_outputs: &'static str,
}

impl WBufferCode {
    pub const fn new() -> Self {
        WBufferCode {
            w_buffer_vert_outputs: "@location(2) w: f32,",
            w_buffer_set_vert_outputs: "output.w = f32(depth) * (1.0 / 0x1000000);",
            w_buffer_frag_inputs: "@location(2) w: f32,",
            w_buffer_frag_outputs: "@builtin(frag_depth) frag_depth: f32,",
            w_buffer_set_frag_outputs: "output.frag_depth = w;",
        }
    }
}
