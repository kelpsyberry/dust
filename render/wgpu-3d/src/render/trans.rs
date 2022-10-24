use super::{
    get_output_color, CommonCode, TextureCode, ToonCode, WBufferCode, COMMON_VERT_ATTRIBS,
    PRIMITIVE_STATE, TEXTURE_VERT_ATTRIBS, TRANS_BLENDING,
};
use crate::{PipelineKey, Vertex};
use core::mem;

pub(super) fn shader_module_src(pipeline: PipelineKey, texture_bg_index: u32) -> [String; 2] {
    let CommonCode {
        common_vert_inputs,
        common_vert_outputs,
        common_set_vert_outputs,
        common_frag_inputs,
        common_frag_outputs,
    } = CommonCode::new();

    let ToonCode {
        toon_uniforms,
        toon_get_color,
    } = ifdef!(pipeline.mode() >= 2, ToonCode::new(2));

    let WBufferCode {
        w_buffer_vert_outputs,
        w_buffer_set_vert_outputs,
        w_buffer_frag_inputs,
        w_buffer_frag_outputs,
        w_buffer_set_frag_outputs,
    } = ifdef!(pipeline.w_buffering(), WBufferCode::new());

    let TextureCode {
        texture_uniforms,
        texture_vert_inputs,
        texture_vert_outputs,
        texture_set_vert_outputs,
        texture_frag_inputs,
        texture_get_color,
    } = ifdef!(
        pipeline.texture_mapping_enabled(),
        TextureCode::new(texture_bg_index)
    );

    let get_output_color = get_output_color(pipeline.mode(), pipeline.texture_mapping_enabled());

    [
        "if output.color.a < 1.0 { discard; }",
        "if (output.color.a < alpha_and_ref.alpha_ref) || (output.color.a >= 1.0) { discard; }",
    ]
    .map(|alpha_test| {
        format!(
            "
struct AlphaAndRefUniform {{
    alpha: f32,
    alpha_ref: f32,
}};

@group(1) @binding(0) var<uniform> alpha_and_ref: AlphaAndRefUniform;

{texture_uniforms}
{toon_uniforms}

struct VertOutput {{
    {common_vert_outputs}
    {w_buffer_vert_outputs}
    {texture_vert_outputs}
}}

@vertex
fn vs_main(
    {common_vert_inputs}
    {texture_vert_inputs}
) -> VertOutput {{
    var output: VertOutput;
    {common_set_vert_outputs}
    {w_buffer_set_vert_outputs}
    {texture_set_vert_outputs}
    output.v_color.a = alpha_and_ref.alpha;
    return output;
}}

struct FragOutput {{
    {common_frag_outputs}
    {w_buffer_frag_outputs}
}}

@fragment
fn fs_main(
    {common_frag_inputs}
    {w_buffer_frag_inputs}
    {texture_frag_inputs}
) -> FragOutput {{
    var output: FragOutput;
    {w_buffer_set_frag_outputs}
    {texture_get_color}
    {toon_get_color}
    {get_output_color}
    {alpha_test}
    return output;
}}"
        )
    })
}

pub(crate) fn create_pipeline(
    pipeline: PipelineKey,
    device: &wgpu::Device,
    id_bg_layout: &wgpu::BindGroupLayout,
    alpha_and_ref_bg_layout: &wgpu::BindGroupLayout,
    toon_bg_layout: &wgpu::BindGroupLayout,
    texture_bg_layout: &wgpu::BindGroupLayout,
) -> [wgpu::RenderPipeline; 2] {
    let mut bg_layouts = vec![id_bg_layout, alpha_and_ref_bg_layout];

    if pipeline.mode() >= 2 {
        bg_layouts.push(toon_bg_layout);
    }

    let texture_bg_index = bg_layouts.len() as u32;
    if pipeline.texture_mapping_enabled() {
        bg_layouts.push(texture_bg_layout);
    }

    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("3D renderer translucent pipeline layout"),
        bind_group_layouts: &bg_layouts,
        push_constant_ranges: &[],
    });

    let (opaque_shader_module, trans_shader_module) = {
        let [opaque_src, trans_src] = shader_module_src(pipeline, texture_bg_index);
        (
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("3D renderer translucent opaque pass shader module"),
                source: wgpu::ShaderSource::Wgsl(opaque_src.into()),
            }),
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("3D renderer translucent translucent pass shader module"),
                source: wgpu::ShaderSource::Wgsl(trans_src.into()),
            }),
        )
    };

    let mut attribs = COMMON_VERT_ATTRIBS.to_vec();

    if pipeline.texture_mapping_enabled() {
        attribs.extend_from_slice(&TEXTURE_VERT_ATTRIBS);
    }

    let stencil_face_state = wgpu::StencilFaceState {
        compare: wgpu::CompareFunction::NotEqual,
        fail_op: wgpu::StencilOperation::Keep,
        depth_fail_op: wgpu::StencilOperation::Keep,
        pass_op: wgpu::StencilOperation::Replace,
    };

    let opaque_desc = wgpu::RenderPipelineDescriptor {
        label: Some("3D renderer translucent pipeline opaque pass"),
        layout: Some(&layout),

        vertex: wgpu::VertexState {
            module: &opaque_shader_module,
            entry_point: "vs_main",
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: mem::size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &attribs,
            }],
        },

        primitive: PRIMITIVE_STATE,

        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32FloatStencil8,
            depth_write_enabled: true,
            depth_compare: if pipeline.depth_test_equal() {
                wgpu::CompareFunction::Equal
            } else {
                wgpu::CompareFunction::Less
            },
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),

        multisample: wgpu::MultisampleState::default(),

        fragment: Some(wgpu::FragmentState {
            module: &opaque_shader_module,
            entry_point: "fs_main",
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8Unorm,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),

        multiview: None,
    };

    [
        device.create_render_pipeline(&opaque_desc),
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("3D renderer translucent pipeline translucent pass"),

            vertex: wgpu::VertexState {
                module: &trans_shader_module,
                entry_point: "vs_main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attribs,
                }],
            },

            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32FloatStencil8,
                depth_write_enabled: true,
                depth_compare: if pipeline.depth_test_equal() {
                    wgpu::CompareFunction::Equal
                } else {
                    wgpu::CompareFunction::Less
                },
                stencil: wgpu::StencilState {
                    front: stencil_face_state,
                    back: stencil_face_state,
                    read_mask: 0x7F,
                    write_mask: 0x7F,
                },
                bias: wgpu::DepthBiasState::default(),
            }),

            fragment: Some(wgpu::FragmentState {
                module: &trans_shader_module,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: pipeline.alpha_blending_enabled().then_some(TRANS_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),

            ..opaque_desc
        }),
    ]
}
