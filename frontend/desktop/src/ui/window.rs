pub use imgui_wgpu::SrgbMode;

#[cfg(target_os = "macos")]
use cocoa::base::id;
use copypasta::{ClipboardContext, ClipboardProvider};
use emu_utils::resource;
#[cfg(target_os = "macos")]
use std::path::Path;
use std::{
    hint::unreachable_unchecked,
    iter,
    mem::ManuallyDrop,
    sync::Arc,
    time::{Duration, Instant},
};
#[cfg(any(target_os = "linux", target_os = "windows"))]
use winit::window::Icon;
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

pub struct GfxDevice {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
}

pub struct GfxSurface {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    format_changed: bool,
    needs_rebuild: bool,
}

impl GfxSurface {
    fn new(window: &WinitWindow, gfx: &GfxDevice, srgb_mode: SrgbMode) -> Self {
        let surface = unsafe {
            gfx.instance.create_surface_unsafe(
                wgpu::SurfaceTargetUnsafe::from_window(window)
                    .expect("couldn't get surface target from window"),
            )
        }
        .expect("couldn't create surface");

        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: {
                let formats = surface.get_capabilities(&gfx.adapter).formats;
                let preferred = formats
                    .first()
                    .expect("couldn't get surface preferred format");
                if srgb_mode == SrgbMode::Srgb {
                    preferred.add_srgb_suffix()
                } else {
                    preferred.remove_srgb_suffix()
                }
            },
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: Vec::new(),
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&gfx.device, &config);

        GfxSurface {
            surface,
            config,
            format_changed: false,
            needs_rebuild: false,
        }
    }

    pub fn surface(&self) -> &wgpu::Surface {
        &self.surface
    }

    pub fn config(&self) -> &wgpu::SurfaceConfiguration {
        &self.config
    }

    pub fn surface_format_changed(&self) -> bool {
        self.format_changed
    }

    fn invalidate_swapchain(&mut self) {
        self.needs_rebuild = true;
    }

    fn rebuild_swapchain(&mut self, gfx: &GfxDevice, size: PhysicalSize<u32>) {
        self.needs_rebuild = false;
        self.config.width = size.width.max(1);
        self.config.height = size.height.max(1);
        if size.width != 0 && size.height != 0 {
            self.surface.configure(&gfx.device, &self.config);
        }
    }

    fn update_format_and_rebuild_swapchain(&mut self, gfx: &GfxDevice, size: PhysicalSize<u32>) {
        let new_format = *self
            .surface
            .get_capabilities(&gfx.adapter)
            .formats
            .first()
            .expect("couldn't get surface preferred format");
        if new_format != self.config.format {
            self.config.format = new_format;
            self.format_changed = true;
        }
        self.rebuild_swapchain(gfx, size);
    }

    pub fn start_frame(
        &mut self,
        gfx: &GfxDevice,
        size: PhysicalSize<u32>,
    ) -> wgpu::SurfaceTexture {
        self.format_changed = false;
        if self.needs_rebuild {
            self.rebuild_swapchain(gfx, size);
        }
        loop {
            match self.surface.get_current_texture() {
                Ok(frame) => {
                    if frame.suboptimal {
                        drop(frame);
                        self.update_format_and_rebuild_swapchain(gfx, size);
                    } else {
                        break frame;
                    }
                }
                Err(error) => match error {
                    wgpu::SurfaceError::Timeout | wgpu::SurfaceError::Outdated => {}
                    wgpu::SurfaceError::Lost => {
                        self.update_format_and_rebuild_swapchain(gfx, size);
                    }
                    wgpu::SurfaceError::OutOfMemory => panic!("swapchain ran out of memory"),
                },
            }
        }
    }
}

impl GfxDevice {
    async fn new(features: wgpu::Features, adapter: AdapterSelection) -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let adapter = match adapter {
            AdapterSelection::Auto(power_preference) => {
                instance
                    .request_adapter(&wgpu::RequestAdapterOptions {
                        power_preference,
                        force_fallback_adapter: false,
                        compatible_surface: None,
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
                        max_bind_groups: 5,
                        ..wgpu::Limits::downlevel_webgl2_defaults()
                    },
                },
                None,
            )
            .await
            .expect("couldn't open connection to graphics device");

        GfxDevice {
            instance,
            adapter,
            device: Arc::new(device),
            queue: Arc::new(queue),
        }
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
}

pub struct ImGuiState {
    pub normal_font: imgui::FontId,
    pub mono_font: imgui::FontId,
    pub large_icon_font: imgui::FontId,
}

impl ImGuiState {
    fn new(scale_factor: f64, imgui: &mut imgui::Context) -> Self {
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
        imgui_io.config_flags |= imgui::ConfigFlags::DOCKING_ENABLE;
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

        ImGuiState {
            normal_font,
            mono_font,
            large_icon_font,
        }
    }
}

pub struct Builder {
    pub event_loop: EventLoop<()>,
    window: HiddenWindow,
    pub imgui: imgui::Context,
}

struct HiddenWindow {
    window: WinitWindow,
    scale_factor: f64,
    gfx_device: GfxDevice,
    imgui: ImGuiState,
    imgui_winit: imgui_winit_support::WinitPlatform,
    srgb_mode: SrgbMode,
    #[cfg(target_os = "macos")]
    macos_title_bar_is_hidden: bool,
}

pub struct Window {
    window: WinitWindow,
    scale_factor: f64,
    last_frame: Instant,

    gfx_device: GfxDevice,
    gfx_surface: GfxSurface,

    pub imgui: ImGuiState,
    imgui_winit: imgui_winit_support::WinitPlatform,
    pub imgui_gfx: imgui_wgpu::Renderer,

    is_occluded: bool,
    #[cfg(target_os = "macos")]
    macos_title_bar_is_transparent: bool,
    #[cfg(target_os = "macos")]
    macos_title_bar_height: f32,
}

impl Window {
    #[inline]
    pub fn inner_size(&self) -> LogicalSize<f64> {
        self.window.inner_size().to_logical(self.scale_factor)
    }

    #[inline]
    pub fn scale_factor(&self) -> f64 {
        self.scale_factor
    }

    #[inline]
    pub fn set_title(&self, title: &str) {
        self.window.set_title(title)
    }

    #[cfg(target_os = "macos")]
    pub fn set_file_path(&self, file_path: Option<&Path>) {
        use cocoa::appkit::NSWindow;
        let Some(ns_window) = self.ns_window() else {
            return;
        };
        unsafe {
            let string: id = msg_send![class!(NSString), alloc];
            ns_window.setRepresentedFilename_(match file_path {
                Some(path) => {
                    const UTF8_ENCODING: usize = 4;
                    let bytes = path.as_os_str().as_encoded_bytes();
                    msg_send![string,
                              initWithBytes:bytes.as_ptr()
                              length:bytes.len()
                              encoding:UTF8_ENCODING as id]
                }
                None => msg_send![string, init],
            });
        }
    }

    #[cfg(any(target_os = "linux", target_os = "windows"))]
    #[inline]
    pub fn set_icon(&self, icon: Option<Icon>) {
        self.window.set_window_icon(icon)
    }

    #[inline]
    pub fn gfx_device(&self) -> &Arc<wgpu::Device> {
        &self.gfx_device.device
    }

    #[inline]
    pub fn gfx_queue(&self) -> &Arc<wgpu::Queue> {
        &self.gfx_device.queue
    }

    #[cfg(target_os = "macos")]
    #[inline]
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
    fn compute_macos_title_bar_height(&self, ns_window: id) -> f32 {
        use cocoa::foundation::NSRect;
        let content_layout_rect: NSRect = unsafe { msg_send![ns_window, contentLayoutRect] };
        (self.window.outer_size().height as f64 / self.scale_factor
            - content_layout_rect.size.height) as f32
    }

    #[cfg(target_os = "macos")]
    fn update_macos_title_bar_height(&mut self) {
        self.macos_title_bar_height =
            if self.macos_title_bar_is_transparent && self.window.fullscreen().is_none() {
                if let Some(ns_window) = self.ns_window() {
                    self.compute_macos_title_bar_height(ns_window)
                } else {
                    0.0
                }
            } else {
                0.0
            }
    }

    #[cfg(target_os = "macos")]
    pub fn set_macos_title_bar_transparent(&mut self, transparent: bool) {
        use cocoa::{
            appkit::{NSWindow, NSWindowStyleMask},
            base::BOOL,
        };
        self.macos_title_bar_is_transparent = transparent;
        self.update_macos_title_bar_height();
        let Some(ns_window) = self.ns_window() else {
            return;
        };
        unsafe {
            ns_window.setTitlebarAppearsTransparent_(transparent as BOOL);
            let prev_style_mask = ns_window.styleMask();
            ns_window.setStyleMask_(if transparent {
                prev_style_mask | NSWindowStyleMask::NSFullSizeContentViewWindowMask
            } else {
                prev_style_mask & !NSWindowStyleMask::NSFullSizeContentViewWindowMask
            });
        }
    }

    pub fn main_menu_bar(&mut self, ui: &imgui::Ui, f: impl FnOnce(&mut Self)) {
        #[cfg(target_os = "macos")]
        if (self.macos_title_bar_height != 0.0)
            != (self.macos_title_bar_is_transparent
                && self.window.fullscreen().is_none()
                && self.ns_window().is_some())
        {
            self.update_macos_title_bar_height();
        }

        #[cfg(target_os = "macos")]
        let frame_padding = if self.macos_title_bar_height != 0.0 {
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
                if self.macos_title_bar_height != 0.0 {
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

enum WindowState {
    Hidden(HiddenWindow),
    Shown(Window),
}

impl Builder {
    pub async fn new(
        title: impl Into<String>,
        features: wgpu::Features,
        adapter: AdapterSelection,
        default_logical_size: (u32, u32),
        srgb_mode: SrgbMode,
        #[cfg(target_os = "macos")] macos_title_bar_is_hidden: bool,
    ) -> Self {
        let event_loop = EventLoop::new().expect("couldn't create event loop");
        let window_builder = WinitWindowBuilder::new()
            .with_title(title)
            .with_inner_size(LogicalSize::new(
                default_logical_size.0,
                default_logical_size.1,
            ))
            .with_visible(false);
        #[cfg(target_os = "macos")]
        let window_builder = if macos_title_bar_is_hidden {
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

        let gfx_device = GfxDevice::new(features, adapter).await;

        let mut imgui = imgui::Context::create();

        let imgui_state = ImGuiState::new(scale_factor, &mut imgui);

        let mut imgui_winit = imgui_winit_support::WinitPlatform::init(&mut imgui);
        imgui_winit.attach_window(
            imgui.io_mut(),
            &window,
            imgui_winit_support::HiDpiMode::Default,
        );

        #[allow(unused_mut)]
        let mut window = HiddenWindow {
            window,
            scale_factor,
            gfx_device,
            imgui: imgui_state,
            imgui_winit,
            srgb_mode,
            #[cfg(target_os = "macos")]
            macos_title_bar_is_hidden,
        };

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

        init_state: impl FnOnce(&mut Window) -> S,
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
        // Since Rust can't prove that after Event::LoopExiting the program will exit and prevent
        // these from being used again, they have to be wrapped in ManuallyDrop to be able to pass
        // everything by value to the on_exit callback
        let mut init_state = ManuallyDrop::new(init_state);
        let mut on_exit_imgui = ManuallyDrop::new(on_exit_imgui);
        let mut on_exit = ManuallyDrop::new(on_exit);

        let mut window_ = ManuallyDrop::new(WindowState::Hidden(self.window));
        let mut imgui_ = ManuallyDrop::new(self.imgui);
        let mut state_ = ManuallyDrop::new(None);

        let _ = self.event_loop.run(move |event, elwt| {
            let imgui = &mut *imgui_;

            if let Event::WindowEvent {
                event: WindowEvent::RedrawRequested,
                ..
            } = &event
            {
                if matches!(&*window_, WindowState::Hidden(_)) {
                    return;
                }
            }

            if let Event::NewEvents(StartCause::Init) = &event {
                let window = unsafe {
                    let WindowState::Hidden(window) = ManuallyDrop::take(&mut window_) else {
                        unreachable_unchecked()
                    };
                    window
                };

                let gfx_surface =
                    GfxSurface::new(&window.window, &window.gfx_device, window.srgb_mode);
                let imgui_gfx = imgui_wgpu::Renderer::new(
                    &window.gfx_device.device,
                    &window.gfx_device.queue,
                    imgui,
                    gfx_surface.config.format,
                    window.srgb_mode,
                );

                let mut window = Window {
                    window: window.window,
                    scale_factor: window.scale_factor,
                    last_frame: Instant::now(),
                    gfx_device: window.gfx_device,
                    gfx_surface,
                    imgui: window.imgui,
                    imgui_winit: window.imgui_winit,
                    imgui_gfx,
                    is_occluded: false,
                    #[cfg(target_os = "macos")]
                    macos_title_bar_is_transparent: window.macos_title_bar_is_hidden,
                    #[cfg(target_os = "macos")]
                    macos_title_bar_height: 0.0,
                };

                #[cfg(target_os = "macos")]
                window.update_macos_title_bar_height();

                state_ = ManuallyDrop::new(Some(unsafe { ManuallyDrop::take(&mut init_state) }(
                    &mut window,
                )));
                window_ = ManuallyDrop::new(WindowState::Shown(window));
            }

            let window = unsafe {
                let WindowState::Shown(window) = &mut *window_ else {
                    unreachable_unchecked()
                };
                window
            };
            let state = unsafe { state_.as_mut().unwrap_unchecked() };

            window
                .imgui_winit
                .handle_event(imgui.io_mut(), &window.window, &event);

            process_event(window, state, &event);

            let mut redraw = || {
                let now = Instant::now();
                let delta_time = now - window.last_frame;
                window.last_frame = now;

                let frame = window
                    .gfx_surface
                    .start_frame(&window.gfx_device, window.window.inner_size());
                let mut encoder = window
                    .gfx_device
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

                if window.gfx_surface.format_changed {
                    window.imgui_gfx.change_swapchain_format(
                        &window.gfx_device.device,
                        window.gfx_surface.config.format,
                    );
                }

                let io = imgui.io_mut();
                io.update_delta_time(delta_time);
                window
                    .imgui_winit
                    .prepare_frame(io, &window.window)
                    .expect("couldn't prepare imgui frame");

                let ui = imgui.frame();
                if draw_imgui(window, state, ui) == ControlFlow::Exit {
                    elwt.exit();
                }

                if draw(window, state, &frame, &mut encoder, delta_time) == ControlFlow::Exit {
                    elwt.exit();
                }

                window.imgui_winit.prepare_render(ui, &window.window);
                window.imgui_gfx.render(
                    &window.gfx_device.device,
                    &window.gfx_device.queue,
                    &mut encoder,
                    &frame
                        .texture
                        .create_view(&wgpu::TextureViewDescriptor::default()),
                    imgui.render(),
                );

                window.gfx_device.queue.submit(iter::once(encoder.finish()));
                window.window.pre_present_notify();
                frame.present();

                window.gfx_device.device.poll(wgpu::Maintain::Poll);

                if !window.is_occluded {
                    window.window.request_redraw();
                }
            };

            match event {
                Event::NewEvents(StartCause::Init) => {
                    redraw();
                    window.window.set_visible(true);
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
                    window.gfx_surface.invalidate_swapchain();
                    window.window.request_redraw();
                }

                Event::WindowEvent {
                    event: WindowEvent::ScaleFactorChanged { scale_factor, .. },
                    ..
                } => {
                    window.scale_factor = scale_factor;
                    window.gfx_surface.invalidate_swapchain();
                    window.window.request_redraw();
                }

                Event::WindowEvent {
                    event: WindowEvent::Occluded(is_occluded),
                    ..
                } => {
                    window.is_occluded = is_occluded;
                    if !is_occluded {
                        window.window.request_redraw();
                    }
                }

                Event::WindowEvent {
                    event: WindowEvent::RedrawRequested,
                    ..
                } => {
                    redraw();
                }

                Event::LoopExiting => {
                    unsafe {
                        ManuallyDrop::take(&mut on_exit_imgui)(
                            window,
                            &mut *state,
                            ManuallyDrop::take(&mut imgui_),
                        );

                        let WindowState::Shown(window) = ManuallyDrop::take(&mut window_) else {
                            unreachable_unchecked()
                        };
                        let state = ManuallyDrop::take(&mut state_).unwrap_unchecked();

                        ManuallyDrop::take(&mut on_exit)(window, state)
                    };
                }
                _ => {}
            }
        });
        std::process::exit(0);
    }
}
