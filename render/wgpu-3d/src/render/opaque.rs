use super::{
    get_output_color, CommonCode, TextureCode, ToonCode, WBufferCode, COMMON_VERT_ATTRIBS,
    PRIMITIVE_STATE, TEXTURE_VERT_ATTRIBS,
};
use crate::{PipelineKey, Vertex};
use core::mem;

fn shader_module_src(pipeline: PipelineKey, texture_bg_index: u32) -> String {
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
    } = ifdef!(pipeline.mode() >= 2, ToonCode::new(0));

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

    format!(
        "
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
    @location(5) id: u32,
) -> VertOutput {{
    var output: VertOutput;
    {common_set_vert_outputs}
    {w_buffer_set_vert_outputs}
    {texture_set_vert_outputs}
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
    if output.color.a < 1.0 {{
        discard;
    }}
    return output;
}}"
    )
}

pub(crate) fn create_pipeline(
    pipeline: PipelineKey,
    device: &wgpu::Device,
    toon_bg_layout: &wgpu::BindGroupLayout,
    texture_bg_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let mut pipeline_bg_layouts = Vec::new();

    if pipeline.mode() >= 2 {
        pipeline_bg_layouts.push(toon_bg_layout);
    }

    let texture_bg_index = pipeline_bg_layouts.len() as u32;
    if pipeline.texture_mapping_enabled() {
        pipeline_bg_layouts.push(texture_bg_layout);
    }

    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("3D renderer opaque pipeline layout"),
        bind_group_layouts: &pipeline_bg_layouts,
        push_constant_ranges: &[],
    });

    let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("3D renderer opaque shader module"),
        source: wgpu::ShaderSource::Wgsl(shader_module_src(pipeline, texture_bg_index).into()),
    });

    let mut attribs = COMMON_VERT_ATTRIBS.to_vec();

    if pipeline.texture_mapping_enabled() {
        attribs.extend_from_slice(&TEXTURE_VERT_ATTRIBS);
    }

    attribs.push(wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Uint32,
        offset: 24,
        shader_location: 5,
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("3D renderer opaque pipeline"),
        layout: Some(&layout),

        vertex: wgpu::VertexState {
            module: &shader_module,
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
            module: &shader_module,
            entry_point: "fs_main",
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8Unorm,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),

        multiview: None,
    })
}
