use super::{
    get_output_color, AttrsCode, CommonCode, EdgeMarkingCode, FogCode, TextureCode, ToonCode,
    WBufferCode, COMMON_VERT_ATTRIBS, PRIMITIVE_STATE, TEXTURE_VERT_ATTRIBS,
};
use crate::{BgLayouts, PipelineKey, Vertex};
use core::mem;

fn shader_module_src(
    pipeline: PipelineKey,
    fog_enabled_bg_index: u32,
    id_bg_index: u32,
    texture_bg_index: u32,
    toon_bg_index: u32,
) -> String {
    let CommonCode {
        common_vert_inputs,
        common_vert_outputs,
        common_set_vert_outputs,
        common_frag_inputs,
        common_frag_outputs,
    } = CommonCode::new();

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

    let ToonCode {
        toon_uniforms,
        toon_get_color,
    } = ifdef!(pipeline.mode() >= 2, ToonCode::new(toon_bg_index));

    let AttrsCode {
        attrs_frag_outputs,
        attrs_init_frag_outputs,
    } = ifdef!(pipeline.attrs_enabled(), AttrsCode::new());

    let FogCode {
        fog_uniforms,
        fog_set_frag_outputs,
    } = ifdef!(pipeline.fog_enabled(), FogCode::new(fog_enabled_bg_index));

    let EdgeMarkingCode {
        edge_marking_uniforms,
        edge_marking_set_frag_outputs,
    } = ifdef!(
        pipeline.edge_marking_enabled(),
        EdgeMarkingCode::new(id_bg_index)
    );

    let get_output_color = get_output_color(pipeline.mode(), pipeline.texture_mapping_enabled());

    format!(
        "
{texture_uniforms}
{toon_uniforms}
{fog_uniforms}
{edge_marking_uniforms}

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
    {attrs_frag_outputs}
}}

@fragment
fn fs_main(
    {common_frag_inputs}
    {w_buffer_frag_inputs}
    {texture_frag_inputs}
) -> FragOutput {{
    var output: FragOutput;
    {texture_get_color}
    {toon_get_color}
    {get_output_color}
    if output.color.a < 0.5 {{
        discard;
    }}
    {w_buffer_set_frag_outputs}
    {attrs_init_frag_outputs}
    {fog_set_frag_outputs}
    {edge_marking_set_frag_outputs}
    return output;
}}"
    )
}

pub(crate) fn create_pipeline(
    pipeline: PipelineKey,
    device: &wgpu::Device,
    bg_layouts: &BgLayouts,
) -> wgpu::RenderPipeline {
    let mut bg_layouts_ = Vec::new();

    let fog_enabled_bg_index = bg_layouts_.len() as u32;
    if pipeline.fog_enabled() {
        bg_layouts_.push(&bg_layouts.fog_enabled);
    }

    let id_bg_index = bg_layouts_.len() as u32;
    if pipeline.edge_marking_enabled() {
        bg_layouts_.push(&bg_layouts.id);
    }

    let texture_bg_index = bg_layouts_.len() as u32;
    if pipeline.texture_mapping_enabled() {
        bg_layouts_.push(&bg_layouts.texture);
    }

    let toon_bg_index = bg_layouts_.len() as u32;
    if pipeline.mode() >= 2 {
        bg_layouts_.push(&bg_layouts.toon);
    }

    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("3D renderer opaque pipeline layout"),
        bind_group_layouts: &bg_layouts_,
        push_constant_ranges: &[],
    });

    let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("3D renderer opaque shader module"),
        source: wgpu::ShaderSource::Wgsl(
            shader_module_src(
                pipeline,
                fog_enabled_bg_index,
                id_bg_index,
                texture_bg_index,
                toon_bg_index,
            )
            .into(),
        ),
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
            entry_point: None,
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: mem::size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &attribs,
            }],
            compilation_options: Default::default(),
        },

        primitive: PRIMITIVE_STATE,

        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth24PlusStencil8,
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
            entry_point: None,
            targets: if pipeline.attrs_enabled() {
                &[
                    Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                    Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                ]
            } else {
                &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })]
            },
            compilation_options: Default::default(),
        }),

        multiview: None,
        cache: None,
    })
}
