macro_rules! ifdef {
    ($flag: expr, $str: expr) => {
        if $flag {
            $str
        } else {
            Default::default()
        }
    };
}

mod common;
pub use common::*;
mod w_buffer;
pub use w_buffer::*;
mod attrs;
pub use attrs::*;
mod toon;
pub use toon::*;
mod texture;
pub use texture::*;
pub mod fog;
pub use fog::FogCode;

pub mod opaque;
pub mod trans;

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
