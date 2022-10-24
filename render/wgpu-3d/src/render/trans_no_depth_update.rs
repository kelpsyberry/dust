use super::{
    trans::shader_module_src, COMMON_VERT_ATTRIBS, PRIMITIVE_STATE, TEXTURE_VERT_ATTRIBS,
    TRANS_BLENDING,
};
use crate::{PipelineKey, Vertex};
use core::mem;

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
        label: Some("3D renderer translucent no depth update pipeline layout"),
        bind_group_layouts: &bg_layouts,
        push_constant_ranges: &[],
    });

    let (opaque_shader_module, trans_shader_module) = {
        let [opaque_src, trans_src] = shader_module_src(pipeline, texture_bg_index);
        (
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("3D renderer translucent no depth update opaque pass shader module"),
                source: wgpu::ShaderSource::Wgsl(opaque_src.into()),
            }),
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(
                    "3D renderer translucent no depth update translucent pass shader module",
                ),
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
        label: Some("3D renderer translucent no depth update pipeline opaque pass"),
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
            label: Some("3D renderer translucent no depth update pipeline translucent pass"),

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
                depth_write_enabled: false,
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
