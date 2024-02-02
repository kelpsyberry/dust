#[cfg(target_os = "macos")]
use cocoa::{
    appkit::{NSWindow, NSWindowStyleMask},
    base::id,
    foundation::NSRect,
};
use copypasta::{ClipboardContext, ClipboardProvider};
use emu_utils::resource;
use std::{
    iter,
    mem::ManuallyDrop,
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{
    dpi::{LogicalSize, PhysicalSize},
    event::{Event, StartCause, WindowEvent},
    event_loop::EventLoop,
    window::{Window as WinitWindow, WindowBuilder as WinitWindowBuilder},
};
#[cfg(target_os = "macos")]
use winit::{
    platform::macos::WindowBuilderExtMacOS,
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
};

pub enum AdapterSelection {
    Auto(wgpu::PowerPreference),
    Manual(wgpu::Backends, Box<dyn FnMut(&wgpu::Adapter) -> bool>),
}

pub struct GfxState {
    surface: wgpu::Surface<'static>,
    adapter: wgpu::Adapter,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    sc_needs_rebuild: bool,
    surface_config: wgpu::SurfaceConfiguration,
    surface_format_changed: bool,
}

impl GfxState {
    async fn new(
        window: &WinitWindow,
        features: wgpu::Features,
        adapter: AdapterSelection,
    ) -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let surface = unsafe {
            instance.create_surface_unsafe(
                wgpu::SurfaceTargetUnsafe::from_window(window)
                    .expect("couldn't get surface target from window"),
            )
        }
        .expect("couldn't create surface");

        let adapter = match adapter {
            AdapterSelection::Auto(power_preference) => {
                instance
                    .request_adapter(&wgpu::RequestAdapterOptions {
                        power_preference,
                        force_fallback_adapter: false,
                        compatible_surface: Some(&surface),
                    })
                    .await
            }
            AdapterSelection::Manual(backends, suitable) => instance
                .enumerate_adapters(backends)
                .into_iter()
                .find(suitable),
        }
        .expect("couldn't create graphics adapter");

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: features,
                    required_limits: wgpu::Limits {
                        max_texture_dimension_2d: 4096,
                        ..wgpu::Limits::downlevel_webgl2_defaults()
                    },
                },
                None,
            )
            .await
            .expect("couldn't open connection to graphics device");

        let size = window.inner_size();
        let surf_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: {
                let formats = surface.get_capabilities(&adapter).formats;
                let preferred = formats
                    .first()
                    .expect("couldn't get surface preferred format");
                #[cfg(target_os = "macos")]
                {
                    *formats.iter().find(|f| !f.is_srgb()).unwrap_or(preferred)
                }
                #[cfg(not(target_os = "macos"))]
                *preferred
            },
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: Vec::new(),
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surf_config);

        GfxState {
            surface,
            adapter,
            device: Arc::new(device),
            queue: Arc::new(queue),
            sc_needs_rebuild: false,
            surface_config: surf_config,
            surface_format_changed: false,
        }
    }

    pub fn surface(&self) -> &wgpu::Surface {
        &self.surface
    }

    pub fn adapter(&self) -> &wgpu::Adapter {
        &self.adapter
    }

    pub fn device(&self) -> &Arc<wgpu::Device> {
        &self.device
    }

    pub fn queue(&self) -> &Arc<wgpu::Queue> {
        &self.queue
    }

    pub fn surface_config(&self) -> &wgpu::SurfaceConfiguration {
        &self.surface_config
    }

    pub fn surface_format_changed(&self) -> bool {
        self.surface_format_changed
    }

    fn invalidate_swapchain(&mut self) {
        self.sc_needs_rebuild = true;
    }

    fn rebuild_swapchain(&mut self, size: PhysicalSize<u32>) {
        self.sc_needs_rebuild = false;
        self.surface_config.width = size.width;
        self.surface_config.height = size.height;
        if size.width != 0 && size.height != 0 {
            self.surface.configure(&self.device, &self.surface_config);
        }
    }

    fn update_format_and_rebuild_swapchain(&mut self, size: PhysicalSize<u32>) {
        let new_format = *self
            .surface
            .get_capabilities(&self.adapter)
            .formats
            .first()
            .expect("couldn't get surface preferred format");
        if new_format != self.surface_config.format {
            self.surface_config.format = new_format;
            self.surface_format_changed = true;
        }
        self.rebuild_swapchain(size);
    }

    pub fn start_frame(&mut self, size: PhysicalSize<u32>) -> wgpu::SurfaceTexture {
        self.surface_format_changed = false;
        if self.sc_needs_rebuild {
            self.rebuild_swapchain(size);
        }
        loop {
            match self.surface.get_current_texture() {
                Ok(frame) => {
                    if frame.suboptimal {
                        self.update_format_and_rebuild_swapchain(size);
                    } else {
                        break frame;
                    }
                }
                Err(error) => match error {
                    wgpu::SurfaceError::Timeout | wgpu::SurfaceError::Outdated => {}
                    wgpu::SurfaceError::Lost => {
                        self.update_format_and_rebuild_swapchain(size);
                    }
                    wgpu::SurfaceError::OutOfMemory => panic!("swapchain ran out of memory"),
                },
            }
        }
    }
}

pub struct ImGuiState {
    pub gfx: imgui_wgpu::Renderer,
    pub winit: imgui_winit_support::WinitPlatform,
    pub normal_font: imgui::FontId,
    pub mono_font: imgui::FontId,
    pub large_icon_font: imgui::FontId,
}

impl ImGuiState {
    fn new(
        window: &WinitWindow,
        gfx_state: &GfxState,
        scale_factor: f64,
        imgui: &mut imgui::Context,
    ) -> Self {
        struct ClipboardBackend(ClipboardContext);

        impl imgui::ClipboardBackend for ClipboardBackend {
            fn get(&mut self) -> Option<String> {
                self.0.get_contents().ok()
            }

            fn set(&mut self, value: &str) {
                let _ = self.0.set_contents(value.to_string());
            }
        }

        imgui.set_ini_filename(None);
        if let Ok(ctx) = ClipboardContext::new() {
            imgui.set_clipboard_backend(ClipboardBackend(ctx));
        }
        let imgui_io = imgui.io_mut();
        imgui_io.config_flags |= imgui::ConfigFlags::IS_SRGB | imgui::ConfigFlags::DOCKING_ENABLE;
        imgui_io.config_windows_move_from_title_bar_only = true;
        imgui_io.font_global_scale = (1.0 / scale_factor) as f32;

        let open_sans_data = resource!(
            "../../fonts/OpenSans-Regular.ttf",
            "fonts/OpenSans-Regular.ttf"
        );
        let fira_mono_data = resource!(
            "../../fonts/FiraMono-Regular.ttf",
            "fonts/FiraMono-Regular.ttf"
        );
        let fa_solid_data = resource!(
            "../../fonts/FontAwesome-Solid.ttf",
            "fonts/FontAwesome-Solid.ttf"
        );
        let fa_brands_data = resource!(
            "../../fonts/FontAwesome-Brands.ttf",
            "fonts/FontAwesome-Brands.ttf"
        );
        let fa_solid_glyph_ranges = imgui::FontGlyphRanges::from_slice(&[0xE000, 0xF8FF, 0]);
        let fa_brands_glyph_ranges = imgui::FontGlyphRanges::from_slice(&[0xF392, 0xF392, 0]);

        let normal_font = imgui.fonts().add_font(&[
            imgui::FontSource::TtfData {
                data: open_sans_data,
                size_pixels: (16.0 * scale_factor).round() as f32,
                config: Some(imgui::FontConfig {
                    oversample_h: 2,
                    ..Default::default()
                }),
            },
            imgui::FontSource::TtfData {
                data: fa_solid_data,
                size_pixels: (16.0 * scale_factor).round() as f32,
                config: Some(imgui::FontConfig {
                    glyph_ranges: fa_solid_glyph_ranges,
                    glyph_min_advance_x: (20.0 * scale_factor).round() as f32,
                    glyph_offset: [0.0, 2.0],
                    oversample_h: 2,
                    ..Default::default()
                }),
            },
            imgui::FontSource::TtfData {
                data: fa_brands_data,
                size_pixels: (16.0 * scale_factor).round() as f32,
                config: Some(imgui::FontConfig {
                    glyph_ranges: fa_brands_glyph_ranges,
                    glyph_min_advance_x: (20.0 * scale_factor).round() as f32,
                    glyph_offset: [0.0, 2.0],
                    oversample_h: 2,
                    ..Default::default()
                }),
            },
        ]);
        let mono_font = imgui.fonts().add_font(&[imgui::FontSource::TtfData {
            data: fira_mono_data,
            size_pixels: (13.0 * scale_factor).round() as f32,
            config: Some(imgui::FontConfig {
                oversample_h: 2,
                ..Default::default()
            }),
        }]);
        let large_icon_font = imgui.fonts().add_font(&[imgui::FontSource::TtfData {
            data: fa_solid_data,
            size_pixels: (32.0 * scale_factor).round() as f32,
            config: Some(imgui::FontConfig {
                glyph_ranges: imgui::FontGlyphRanges::from_slice(&[
                    0x002B, 0x002B, 0xE000, 0xF8FF, 0,
                ]),
                glyph_min_advance_x: (40.0 * scale_factor).round() as f32,
                oversample_h: 2,
                ..Default::default()
            }),
        }]);

        if gfx_state.surface_config.format.is_srgb() {
            let style = imgui.style_mut();
            for color in &mut style.colors {
                for component in &mut color[..3] {
                    *component = component.powf(2.2);
                }
            }
        }

        let gfx = imgui_wgpu::Renderer::new(
            &gfx_state.device,
            &gfx_state.queue,
            imgui,
            gfx_state.surface_config.format,
        );

        let mut winit = imgui_winit_support::WinitPlatform::init(imgui);
        winit.attach_window(
            imgui.io_mut(),
            window,
            imgui_winit_support::HiDpiMode::Default,
        );

        ImGuiState {
            gfx,
            winit,
            normal_font,
            mono_font,
            large_icon_font,
        }
    }
}

pub struct Builder {
    pub event_loop: EventLoop<()>,
    pub window: Window,

    pub imgui: imgui::Context,
}

pub struct Window {
    window: WinitWindow,
    is_hidden: bool,
    scale_factor: f64,
    last_frame: Instant,
    gfx: GfxState,

    pub imgui: ImGuiState,

    is_occluded: bool,
    #[cfg(target_os = "macos")]
    macos_title_bar_is_hidden: bool,
    #[cfg(target_os = "macos")]
    pub macos_title_bar_height: f32,
}

impl Window {
    pub fn window(&self) -> &WinitWindow {
        &self.window
    }

    pub fn scale_factor(&self) -> f64 {
        self.scale_factor
    }

    pub fn gfx(&self) -> &GfxState {
        &self.gfx
    }

    #[cfg(target_os = "macos")]
    fn ns_window(&self) -> Option<id> {
        if let RawWindowHandle::AppKit(window) =
            RawWindowHandle::from(self.window.window_handle().ok()?)
        {
            Some(unsafe { msg_send![window.ns_view.as_ptr() as id, window] })
        } else {
            None
        }
    }

    #[cfg(target_os = "macos")]
    fn macos_title_bar_height(&self, ns_window: id) -> f32 {
        let content_layout_rect: NSRect = unsafe { msg_send![ns_window, contentLayoutRect] };
        (self.window.outer_size().height as f64 / self.scale_factor
            - content_layout_rect.size.height) as f32
    }

    #[cfg(target_os = "macos")]
    pub fn set_macos_title_bar_hidden(&mut self, hidden: bool) {
        let Some(ns_window) = self.ns_window() else {
            return;
        };
        self.macos_title_bar_is_hidden = hidden;
        self.macos_title_bar_height = if hidden {
            self.macos_title_bar_height(ns_window)
        } else {
            0.0
        };
        unsafe {
            ns_window.setTitlebarAppearsTransparent_(hidden as cocoa::base::BOOL);
            let prev_style_mask = ns_window.styleMask();
            ns_window.setStyleMask_(if hidden {
                prev_style_mask | NSWindowStyleMask::NSFullSizeContentViewWindowMask
            } else {
                prev_style_mask & !NSWindowStyleMask::NSFullSizeContentViewWindowMask
            });
        }
    }

    pub fn main_menu_bar(&mut self, ui: &imgui::Ui, f: impl FnOnce(&mut Self)) {
        #[cfg(target_os = "macos")]
        let frame_padding = if self.macos_title_bar_is_hidden {
            Some(ui.push_style_var(imgui::StyleVar::FramePadding([
                0.0,
                0.5 * (self.macos_title_bar_height - ui.text_line_height()),
            ])))
        } else {
            None
        };

        ui.main_menu_bar(|| {
            #[cfg(target_os = "macos")]
            {
                drop(frame_padding);
                if self.macos_title_bar_is_hidden && self.window.fullscreen().is_none() {
                    // TODO: There has to be some way to compute this width instead of
                    //       hardcoding it.
                    ui.dummy([68.0, 0.0]);
                    ui.same_line_with_spacing(0.0, 0.0);
                }
            }

            f(self);
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ControlFlow {
    Continue,
    Exit,
}

impl Builder {
    pub async fn new(
        title: impl Into<String>,
        features: wgpu::Features,
        adapter: AdapterSelection,
        default_logical_size: (u32, u32),
        #[cfg(target_os = "macos")] macos_title_bar_hidden: bool,
    ) -> Self {
        let event_loop = EventLoop::new().expect("couldn't create event loop");
        let window_builder = WinitWindowBuilder::new()
            .with_title(title)
            .with_inner_size(LogicalSize::new(
                default_logical_size.0,
                default_logical_size.1,
            ))
            // Make the window invisible for the first frame, to avoid showing invalid data
            .with_visible(false);
        #[cfg(target_os = "macos")]
        let window_builder = if macos_title_bar_hidden {
            window_builder
                .with_titlebar_transparent(true)
                .with_fullsize_content_view(true)
        } else {
            window_builder
        };
        let window = window_builder
            .build(&event_loop)
            .expect("couldn't create window");
        let scale_factor = window.scale_factor();

        let gfx = GfxState::new(&window, features, adapter).await;

        let mut imgui = imgui::Context::create();
        let imgui_state = ImGuiState::new(&window, &gfx, scale_factor, &mut imgui);

        #[allow(unused_mut)]
        let mut window = Window {
            window,
            is_hidden: true,
            scale_factor,
            gfx,
            last_frame: Instant::now(),

            imgui: imgui_state,

            is_occluded: false,
            #[cfg(target_os = "macos")]
            macos_title_bar_is_hidden: macos_title_bar_hidden,
            #[cfg(target_os = "macos")]
            macos_title_bar_height: 0.0,
        };

        #[cfg(target_os = "macos")]
        if macos_title_bar_hidden {
            if let Some(ns_window) = window.ns_window() {
                window.macos_title_bar_height = window.macos_title_bar_height(ns_window);
            }
        }

        Builder {
            window,
            event_loop,
            imgui,
        }
    }

    pub fn apply_default_imgui_style(&mut self) {
        let style = self.imgui.style_mut();
        style.window_border_size = 0.0;
        style.child_border_size = 0.0;
        style.popup_border_size = 0.0;
        style.window_rounding = 6.0;
        style.child_rounding = 4.0;
        style.frame_rounding = 4.0;
        style.popup_rounding = 4.0;
        style.scrollbar_rounding = 4.0;
        style.grab_rounding = 3.0;
        style.tab_rounding = 4.0;
    }

    pub fn run<S: 'static>(
        self,
        state: S,
        mut process_event: impl FnMut(&mut Window, &mut S, &Event<()>) + 'static,

        mut draw_imgui: impl FnMut(&mut Window, &mut S, &imgui::Ui) -> ControlFlow + 'static,
        on_exit_imgui: impl FnOnce(&mut Window, &mut S, imgui::Context) + 'static,

        mut draw: impl FnMut(
                &mut Window,
                &mut S,
                &wgpu::SurfaceTexture,
                &mut wgpu::CommandEncoder,
                Duration,
            ) -> ControlFlow
            + 'static,
        on_exit: impl FnOnce(Window, S) + 'static,
    ) -> ! {
        // Since Rust can't prove that after Event::LoopDestroyed the program will exit and prevent
        // these from being used again, they have to be wrapped in ManuallyDrop to be able to pass
        // everything by value to the on_exit callback
        let mut window_ = ManuallyDrop::new(self.window);
        let mut imgui_ = ManuallyDrop::new(self.imgui);
        let mut state = ManuallyDrop::new(state);
        let mut on_exit_imgui = ManuallyDrop::new(on_exit_imgui);
        let mut on_exit = ManuallyDrop::new(on_exit);

        let _ = self.event_loop.run(move |event, elwt| {
            let window = &mut *window_;
            let imgui = &mut *imgui_;
            window
                .imgui
                .winit
                .handle_event(imgui.io_mut(), &window.window, &event);
            process_event(window, &mut state, &event);
            match event {
                Event::NewEvents(StartCause::Init) => {
                    // *control_flow = WinitControlFlow::Poll;
                }

                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => {
                    elwt.exit();
                }

                Event::WindowEvent {
                    event: WindowEvent::Resized(_),
                    ..
                } => {
                    window.gfx.invalidate_swapchain();
                }

                Event::WindowEvent {
                    event: WindowEvent::ScaleFactorChanged { scale_factor, .. },
                    ..
                } => {
                    window.scale_factor = scale_factor;
                    window.gfx.invalidate_swapchain();
                }

                Event::WindowEvent {
                    event: WindowEvent::Occluded(is_occluded),
                    ..
                } => {
                    window.is_occluded = is_occluded;
                }

                Event::WindowEvent {
                    event: WindowEvent::RedrawRequested,
                    ..
                } => {
                    let now = Instant::now();
                    let delta_time = now - window.last_frame;
                    window.last_frame = now;

                    let frame = window.gfx.start_frame(window.window.inner_size());
                    let mut encoder = window
                        .gfx
                        .device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

                    if window.gfx.surface_format_changed {
                        window.imgui.gfx.change_swapchain_format(
                            &window.gfx.device,
                            window.gfx.surface_config.format,
                        );
                    }

                    let io = imgui.io_mut();
                    io.update_delta_time(delta_time);
                    window
                        .imgui
                        .winit
                        .prepare_frame(io, &window.window)
                        .expect("couldn't prepare imgui frame");

                    let ui = imgui.frame();
                    if draw_imgui(window, &mut state, ui) == ControlFlow::Exit {
                        elwt.exit();
                    }

                    if draw(window, &mut state, &frame, &mut encoder, delta_time)
                        == ControlFlow::Exit
                    {
                        elwt.exit();
                    }

                    window.imgui.winit.prepare_render(ui, &window.window);
                    window.imgui.gfx.render(
                        &window.gfx.device,
                        &window.gfx.queue,
                        &mut encoder,
                        &frame
                            .texture
                            .create_view(&wgpu::TextureViewDescriptor::default()),
                        imgui.render(),
                    );

                    window.gfx.queue.submit(iter::once(encoder.finish()));
                    window.window.pre_present_notify();
                    frame.present();

                    window.gfx.device.poll(wgpu::Maintain::Poll);

                    if window.is_hidden {
                        window.is_hidden = false;
                        window.window.set_visible(true);
                    }
                }

                Event::AboutToWait => {
                    if !window.is_occluded {
                        window.window.request_redraw();
                    }
                }

                Event::LoopExiting => {
                    unsafe {
                        ManuallyDrop::take(&mut on_exit_imgui)(
                            window,
                            &mut *state,
                            ManuallyDrop::take(&mut imgui_),
                        );

                        ManuallyDrop::take(&mut on_exit)(
                            ManuallyDrop::take(&mut window_),
                            ManuallyDrop::take(&mut state),
                        )
                    };
                }
                _ => {}
            }
        });
        std::process::exit(0);
    }
}
