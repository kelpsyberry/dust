use core::num::NonZeroU32;
use imgui::internal::RawWrapper;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct TextureRange {
    pub mip_level: u8,
    pub x: u32,
    pub y: u32,
    pub z: u32,
    pub width: Option<NonZeroU32>,
    pub height: Option<NonZeroU32>,
    pub depth_or_array_layers: Option<NonZeroU32>,
    pub offset: u64,
}

pub struct TextureDescriptor {
    pub label: Option<String>,
    pub size: wgpu::Extent3d,
    pub mip_level_count: u32,
    pub sample_count: u32,
    pub dimension: wgpu::TextureDimension,
    pub format: Option<wgpu::TextureFormat>,
    pub usage: wgpu::TextureUsages,
}

impl TextureDescriptor {
    fn raw_desc(&self, output_format: wgpu::TextureFormat) -> wgpu::TextureDescriptor {
        wgpu::TextureDescriptor {
            label: self.label.as_deref(),
            size: self.size,
            mip_level_count: self.mip_level_count,
            sample_count: self.sample_count,
            dimension: self.dimension,
            format: self.format.unwrap_or(output_format),
            usage: self.usage,
        }
    }
}

impl Default for TextureDescriptor {
    fn default() -> Self {
        TextureDescriptor {
            label: None,
            size: Default::default(),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: None,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        }
    }
}

pub struct Texture {
    sampler: wgpu::Sampler,
    texture_desc: TextureDescriptor,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    bytes_per_row: Option<NonZeroU32>,
    rows_per_image: Option<NonZeroU32>,
}

impl Texture {
    fn rebuild_texture(
        device: &wgpu::Device,
        raw_desc: &wgpu::TextureDescriptor,
    ) -> (
        wgpu::Texture,
        wgpu::TextureView,
        Option<NonZeroU32>,
        Option<NonZeroU32>,
    ) {
        let raw = device.create_texture(raw_desc);
        let view = raw.create_view(&Default::default());
        let format_info = raw_desc.format.describe();
        let bytes_per_row = NonZeroU32::new(
            raw_desc.size.width / format_info.block_dimensions.0 as u32
                * format_info.block_size as u32,
        );
        let rows_per_image = if raw_desc.dimension != wgpu::TextureDimension::D3
            && raw_desc.size.depth_or_array_layers > 1
        {
            NonZeroU32::new(raw_desc.size.height / format_info.block_dimensions.1 as u32)
        } else {
            None
        };
        (raw, view, bytes_per_row, rows_per_image)
    }

    fn rebuild_sampler(device: &wgpu::Device, desc: &wgpu::SamplerDescriptor) -> wgpu::Sampler {
        device.create_sampler(desc)
    }

    fn rebuild_bind_group(
        device: &wgpu::Device,
        bind_group_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        view: &wgpu::TextureView,
        label: wgpu::Label,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label,
            layout: bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(view),
                },
            ],
        })
    }

    #[must_use]
    fn new(
        device: &wgpu::Device,
        bind_group_layout: &wgpu::BindGroupLayout,
        sampler_desc: &wgpu::SamplerDescriptor,
        texture_desc: TextureDescriptor,
        output_format: wgpu::TextureFormat,
    ) -> Self {
        let raw_texture_desc = texture_desc.raw_desc(output_format);
        let sampler = Self::rebuild_sampler(device, sampler_desc);
        let (texture, view, bytes_per_row, rows_per_image) =
            Self::rebuild_texture(device, &raw_texture_desc);
        let bind_group = Self::rebuild_bind_group(
            device,
            bind_group_layout,
            &sampler,
            &view,
            raw_texture_desc.label,
        );
        Texture {
            sampler,
            texture_desc,
            texture,
            view,
            bind_group,
            bytes_per_row,
            rows_per_image,
        }
    }

    fn rebuild_texture_from_current_desc(
        &mut self,
        device: &wgpu::Device,
        bind_group_layout: &wgpu::BindGroupLayout,
        output_format: wgpu::TextureFormat,
    ) {
        let raw_texture_desc = self.texture_desc.raw_desc(output_format);
        let (texture, view, bytes_per_row, rows_per_image) =
            Self::rebuild_texture(device, &raw_texture_desc);
        self.bind_group = Self::rebuild_bind_group(
            device,
            bind_group_layout,
            &self.sampler,
            &view,
            raw_texture_desc.label,
        );
        self.texture = texture;
        self.view = view;
        self.bytes_per_row = bytes_per_row;
        self.rows_per_image = rows_per_image;
    }

    pub fn rebuild_with_texture_desc(
        &mut self,
        device: &wgpu::Device,
        bind_group_layout: &wgpu::BindGroupLayout,
        desc: TextureDescriptor,
        output_format: wgpu::TextureFormat,
    ) {
        self.texture_desc = desc;
        self.rebuild_texture_from_current_desc(device, bind_group_layout, output_format);
    }

    pub fn change_swapchain_format(
        &mut self,
        device: &wgpu::Device,
        bind_group_layout: &wgpu::BindGroupLayout,
        output_format: wgpu::TextureFormat,
    ) {
        if self.texture_desc.format.is_none() {
            self.rebuild_texture_from_current_desc(device, bind_group_layout, output_format);
        }
    }

    #[inline]
    pub fn rebuild_with_sampler_desc(
        &mut self,
        device: &wgpu::Device,
        bind_group_layout: &wgpu::BindGroupLayout,
        desc: &wgpu::SamplerDescriptor,
    ) {
        self.sampler = Self::rebuild_sampler(device, desc);
        self.bind_group = Self::rebuild_bind_group(
            device,
            bind_group_layout,
            &self.sampler,
            &self.view,
            self.texture_desc.label.as_deref(),
        );
    }

    #[inline]
    pub fn sampler(&self) -> &wgpu::Sampler {
        &self.sampler
    }

    #[inline]
    pub fn texture_desc(&self) -> &TextureDescriptor {
        &self.texture_desc
    }

    #[inline]
    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    #[inline]
    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    #[inline]
    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    pub fn set_data(&mut self, queue: &wgpu::Queue, data: &[u8], range: TextureRange) {
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: range.x,
                    y: range.y,
                    z: range.z,
                },
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::ImageDataLayout {
                offset: range.offset,
                bytes_per_row: self.bytes_per_row,
                rows_per_image: self.rows_per_image,
            },
            wgpu::Extent3d {
                width: match range.width {
                    Some(value) => value.get(),
                    None => self.texture_desc.size.width,
                },
                height: match range.height {
                    Some(value) => value.get(),
                    None => self.texture_desc.size.height,
                },
                depth_or_array_layers: match range.depth_or_array_layers {
                    Some(value) => value.get(),
                    None => self.texture_desc.size.depth_or_array_layers,
                },
            },
        );
    }
}

pub struct Renderer {
    pub view_bind_group_layout: wgpu::BindGroupLayout,
    pub view_buffer: wgpu::Buffer,
    pub view_bind_group: wgpu::BindGroup,
    pub texture_bind_group_layout: wgpu::BindGroupLayout,
    pub vtx_buffer: Option<wgpu::Buffer>,
    pub vtx_buffer_capacity: u64,
    pub idx_buffer: Option<wgpu::Buffer>,
    pub idx_buffer_capacity: u64,
    pub pipeline_layout: wgpu::PipelineLayout,
    pub vs: wgpu::ShaderModule,
    pub fs: wgpu::ShaderModule,
    pub output_format: wgpu::TextureFormat,
    pub pipeline: wgpu::RenderPipeline,
    pub textures: HashMap<imgui::TextureId, Texture>,
    pub next_texture_id: usize,
}

impl Renderer {
    fn rebuild_fs(device: &wgpu::Device, srgb: bool) -> wgpu::ShaderModule {
        device.create_shader_module(&if srgb {
            wgpu::include_spirv!("../../shaders/out/imgui-srgb.frag.spv")
        } else {
            wgpu::include_spirv!("../../shaders/out/imgui-linear.frag.spv")
        })
    }

    fn rebuild_pipeline(
        device: &wgpu::Device,
        layout: &wgpu::PipelineLayout,
        vs: &wgpu::ShaderModule,
        fs: &wgpu::ShaderModule,
        output_format: wgpu::TextureFormat,
    ) -> wgpu::RenderPipeline {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("imgui"),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: vs,
                entry_point: "main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 20,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 8,
                            shader_location: 1,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Unorm8x4,
                            offset: 16,
                            shader_location: 2,
                        },
                    ],
                }],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                clamp_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: fs,
                entry_point: "main",
                targets: &[wgpu::ColorTargetState {
                    format: output_format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            operation: wgpu::BlendOperation::Add,
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        },
                        alpha: wgpu::BlendComponent {
                            operation: wgpu::BlendOperation::Add,
                            src_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            dst_factor: wgpu::BlendFactor::Zero,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::all(),
                }],
            }),
        })
    }

    #[must_use]
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        imgui: &mut imgui::Context,
        output_format: wgpu::TextureFormat,
    ) -> Self {
        let view_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("imgui view"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
        let view_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("imgui view"),
            size: 16,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
            mapped_at_creation: false,
        });
        let view_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("imgui view"),
            layout: &view_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &view_buffer,
                    offset: 0,
                    size: Some(core::num::NonZeroU64::new(16).unwrap()),
                }),
            }],
        });
        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("imgui texture"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler {
                            filtering: true,
                            comparison: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                ],
            });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("imgui"),
            bind_group_layouts: &[&view_bind_group_layout, &texture_bind_group_layout],
            push_constant_ranges: &[],
        });
        let vs =
            device.create_shader_module(&wgpu::include_spirv!("../../shaders/out/imgui.vert.spv"));
        let fs = Self::rebuild_fs(device, output_format.describe().srgb);
        let pipeline = Self::rebuild_pipeline(device, &pipeline_layout, &vs, &fs, output_format);

        let mut renderer = Renderer {
            view_bind_group_layout,
            view_buffer,
            view_bind_group,
            texture_bind_group_layout,
            pipeline_layout,
            vs,
            fs,
            pipeline,
            textures: HashMap::with_capacity(1),
            next_texture_id: 1,
            vtx_buffer: None,
            vtx_buffer_capacity: 0,
            idx_buffer: None,
            idx_buffer_capacity: 0,
            output_format,
        };

        renderer.reload_fonts(device, queue, imgui);

        renderer
    }

    #[inline]
    pub fn change_swapchain_format(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat) {
        let prev_srgb = self.output_format.describe().srgb;
        self.output_format = format;
        let srgb = format.describe().srgb;
        if srgb != prev_srgb {
            self.fs = Self::rebuild_fs(device, srgb);
        }
        self.pipeline =
            Self::rebuild_pipeline(device, &self.pipeline_layout, &self.vs, &self.fs, format);
        for texture in self.textures.values_mut() {
            texture.change_swapchain_format(
                device,
                &self.texture_bind_group_layout,
                self.output_format,
            );
        }
    }

    #[inline]
    pub fn create_texture(
        &mut self,
        device: &wgpu::Device,
        sampler_desc: &wgpu::SamplerDescriptor,
        texture_desc: TextureDescriptor,
    ) -> Texture {
        Texture::new(
            device,
            &self.texture_bind_group_layout,
            sampler_desc,
            texture_desc,
            self.output_format,
        )
    }

    #[inline]
    pub fn add_texture(&mut self, texture: Texture) -> imgui::TextureId {
        let id = self.next_texture_id.into();
        self.next_texture_id += 1;
        self.textures.insert(id, texture);
        id
    }

    #[inline]
    pub fn remove_texture(&mut self, id: imgui::TextureId) -> Option<Texture> {
        self.textures.remove(&id)
    }

    #[inline]
    pub fn texture(&self, id: imgui::TextureId) -> &Texture {
        &self.textures[&id]
    }

    #[inline]
    pub fn texture_mut(&mut self, id: imgui::TextureId) -> &mut Texture {
        self.textures.get_mut(&id).unwrap()
    }

    pub fn reload_fonts(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        imgui: &mut imgui::Context,
    ) {
        let font_tex_id = imgui.fonts().tex_id;
        if font_tex_id.id() != 0 {
            self.textures.remove(&font_tex_id);
        }
        let mut fonts = imgui.fonts();
        let font_atlas = fonts.build_rgba32_texture();
        let mut font_texture = self.create_texture(
            device,
            &wgpu::SamplerDescriptor {
                label: Some("imgui font atlas sampler"),
                min_filter: wgpu::FilterMode::Linear,
                mag_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            },
            TextureDescriptor {
                label: Some("imgui font atlas".to_string()),
                size: wgpu::Extent3d {
                    width: font_atlas.width,
                    height: font_atlas.height,
                    depth_or_array_layers: 1,
                },
                format: Some(wgpu::TextureFormat::Rgba8Unorm),
                ..Default::default()
            },
        );
        font_texture.set_data(queue, font_atlas.data, TextureRange::default());
        fonts.clear_tex_data();
        self.textures.insert(font_tex_id, font_texture);
    }

    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        frame: &wgpu::TextureView,
        draw_data: &imgui::DrawData,
    ) {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[wgpu::RenderPassColorAttachment {
                view: frame,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: true,
                },
            }],
            depth_stencil_attachment: None,
        });

        if draw_data.total_vtx_count == 0 || draw_data.total_idx_count == 0 {
            return;
        }

        let fb_width = draw_data.display_size[0] * draw_data.framebuffer_scale[0];
        let fb_height = draw_data.display_size[1] * draw_data.framebuffer_scale[1];
        if fb_width <= 0.0 || fb_height <= 0.0 {
            return;
        }

        let mut vtx_size =
            draw_data.total_vtx_count as u64 * core::mem::size_of::<imgui::DrawVert>() as u64;
        vtx_size += wgpu::COPY_BUFFER_ALIGNMENT - 1;
        vtx_size -= vtx_size % wgpu::COPY_BUFFER_ALIGNMENT;
        let mut idx_size =
            draw_data.total_idx_count as u64 * core::mem::size_of::<imgui::DrawIdx>() as u64;
        idx_size += wgpu::COPY_BUFFER_ALIGNMENT - 1;
        idx_size -= idx_size % wgpu::COPY_BUFFER_ALIGNMENT;

        if self.vtx_buffer.is_none() || vtx_size > self.vtx_buffer_capacity {
            self.vtx_buffer.take();
            self.vtx_buffer_capacity = vtx_size.next_power_of_two();
            self.vtx_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: self.vtx_buffer_capacity,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        let vtx_buffer = self.vtx_buffer.as_ref().unwrap();

        if self.idx_buffer.is_none() || idx_size > self.idx_buffer_capacity {
            self.idx_buffer.take();
            self.idx_buffer_capacity = idx_size.next_power_of_two();
            self.idx_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: self.idx_buffer_capacity,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        let idx_buffer = self.idx_buffer.as_ref().unwrap();

        let mut vtx = Vec::with_capacity(vtx_size as usize);
        let mut idx = Vec::with_capacity(idx_size as usize);
        for draw_list in draw_data.draw_lists() {
            let vtx_buffer = draw_list.vtx_buffer();
            let idx_buffer = draw_list.idx_buffer();
            unsafe {
                vtx.extend_from_slice(core::slice::from_raw_parts(
                    vtx_buffer.as_ptr() as *const u8,
                    vtx_buffer.len() * core::mem::size_of::<imgui::DrawVert>(),
                ));
                idx.extend_from_slice(core::slice::from_raw_parts(
                    idx_buffer.as_ptr() as *const u8,
                    idx_buffer.len() * core::mem::size_of::<imgui::DrawIdx>(),
                ));
            }
        }
        vtx.resize(vtx_size as usize, 0);
        idx.resize(idx_size as usize, 0);
        queue.write_buffer(vtx_buffer, 0, &vtx);
        queue.write_buffer(idx_buffer, 0, &idx);

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_index_buffer(
            idx_buffer.slice(..),
            if core::mem::size_of::<imgui::DrawIdx>() == 2 {
                wgpu::IndexFormat::Uint16
            } else {
                wgpu::IndexFormat::Uint32
            },
        );
        render_pass.set_viewport(0.0, 0.0, fb_width, fb_height, 0.0, 1.0);

        let scale = [
            2.0 / draw_data.display_size[0],
            2.0 / draw_data.display_size[1],
        ];
        let scale_translate = [
            scale[0],
            scale[1],
            -1.0 - draw_data.display_pos[0] * scale[0],
            -1.0 - draw_data.display_pos[1] * scale[1],
        ];
        unsafe {
            queue.write_buffer(
                &self.view_buffer,
                0,
                core::slice::from_raw_parts(scale_translate.as_ptr() as *const u8, 16),
            );
        }
        render_pass.set_bind_group(0, &self.view_bind_group, &[]);

        let mut vertex_pos = 0;
        let mut index_pos = 0;
        for draw_list in draw_data.draw_lists() {
            for cmd in draw_list.commands() {
                match cmd {
                    imgui::DrawCmd::Elements { count, cmd_params } => {
                        let texture = match self.textures.get(&cmd_params.texture_id) {
                            Some(texture) => texture,
                            None => continue,
                        };

                        render_pass
                            .set_vertex_buffer(0, vtx_buffer.slice(cmd_params.vtx_offset as u64..));

                        let clip_rect = [
                            (cmd_params.clip_rect[0] - draw_data.display_pos[0])
                                * draw_data.framebuffer_scale[0],
                            (cmd_params.clip_rect[1] - draw_data.display_pos[1])
                                * draw_data.framebuffer_scale[1],
                            (cmd_params.clip_rect[2] - draw_data.display_pos[0])
                                * draw_data.framebuffer_scale[0],
                            (cmd_params.clip_rect[3] - draw_data.display_pos[1])
                                * draw_data.framebuffer_scale[1],
                        ];
                        if clip_rect[0] >= fb_width
                            || clip_rect[1] >= fb_height
                            || clip_rect[2] <= 0.0
                            || clip_rect[3] <= 0.0
                        {
                            continue;
                        }
                        render_pass.set_scissor_rect(
                            clip_rect[0].max(0.0).floor() as u32,
                            clip_rect[1].max(0.0).floor() as u32,
                            (clip_rect[2] - clip_rect[0]).abs().ceil() as u32,
                            (clip_rect[3] - clip_rect[1]).abs().ceil() as u32,
                        );

                        render_pass.set_bind_group(1, &texture.bind_group, &[]);

                        let start = index_pos + cmd_params.idx_offset;
                        render_pass.draw_indexed(
                            start as u32..(start + count) as u32,
                            (vertex_pos + cmd_params.vtx_offset) as i32,
                            0..1,
                        );
                    }
                    imgui::DrawCmd::ResetRenderState => {
                        render_pass.set_pipeline(&self.pipeline);
                        render_pass.set_index_buffer(
                            idx_buffer.slice(..),
                            if core::mem::size_of::<imgui::DrawIdx>() == 2 {
                                wgpu::IndexFormat::Uint16
                            } else {
                                wgpu::IndexFormat::Uint32
                            },
                        );
                        render_pass.set_viewport(0.0, 0.0, fb_width, fb_height, 0.0, 1.0);
                        render_pass.set_bind_group(0, &self.view_bind_group, &[]);
                    }
                    imgui::DrawCmd::RawCallback { callback, raw_cmd } => unsafe {
                        callback(draw_list.raw(), raw_cmd);
                    },
                }
            }
            vertex_pos += draw_list.vtx_buffer().len();
            index_pos += draw_list.idx_buffer().len();
        }
    }
}
