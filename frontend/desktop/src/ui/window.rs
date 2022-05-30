use super::imgui_wgpu;
#[cfg(target_os = "macos")]
use cocoa::{
    appkit::{NSWindow, NSWindowOcclusionState, NSWindowStyleMask},
    base::id,
    foundation::NSRect,
};
use std::{iter, mem::ManuallyDrop, path::PathBuf, time::Instant};
#[cfg(target_os = "macos")]
use winit::platform::macos::{WindowBuilderExtMacOS, WindowExtMacOS};
use winit::{
    dpi::{LogicalSize, PhysicalSize},
    event::{Event, StartCause, WindowEvent},
    event_loop::{ControlFlow as WinitControlFlow, EventLoop},
    window::{Window as WinitWindow, WindowBuilder as WinitWindowBuilder},
};

pub struct GfxDeviceState {
    pub instance: wgpu::Instance,
    pub surface: wgpu::Surface,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub sc_needs_rebuild: bool,
    pub surf_config: wgpu::SurfaceConfiguration,
}

impl GfxDeviceState {
    pub async fn new(window: &WinitWindow) -> Self {
        let instance = wgpu::Instance::new(wgpu::Backends::all());
        let surface = unsafe { instance.create_surface(window) };
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .expect("Couldn't create graphics adapter");
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    features: wgpu::Features::empty(),
                    limits: wgpu::Limits::default(),
                },
                None,
            )
            .await
            .expect("Couldn't open connection to graphics device");
        let size = window.inner_size();
        let surf_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface
                .get_preferred_format(&adapter)
                .expect("Couldn't get surface preferred format"),
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
        };
        surface.configure(&device, &surf_config);
        GfxDeviceState {
            instance,
            surface,
            adapter,
            device,
            queue,
            sc_needs_rebuild: false,
            surf_config,
        }
    }

    pub fn invalidate_swapchain(&mut self) {
        self.sc_needs_rebuild = true;
    }

    pub fn rebuild_swapchain(&mut self, size: PhysicalSize<u32>) {
        self.sc_needs_rebuild = false;
        self.surf_config.width = size.width;
        self.surf_config.height = size.height;
        if size.width != 0 && size.height != 0 {
            self.surface.configure(&self.device, &self.surf_config);
        }
    }

    pub fn update_format_and_rebuild_swapchain(&mut self, size: PhysicalSize<u32>) {
        self.surf_config.format = self
            .surface
            .get_preferred_format(&self.adapter)
            .expect("Couldn't get surface preferred format");
        self.rebuild_swapchain(size);
    }
}

pub struct GfxState {
    pub device_state: GfxDeviceState,
    pub imgui: imgui_wgpu::Renderer,
}

impl GfxState {
    pub async fn new(window: &WinitWindow, imgui: &mut imgui::Context) -> Self {
        let device_state = GfxDeviceState::new(window).await;
        let imgui = imgui_wgpu::Renderer::new(
            &device_state.device,
            &device_state.queue,
            imgui,
            device_state.surf_config.format,
        );
        GfxState {
            device_state,
            imgui,
        }
    }

    fn update_format_and_rebuild_swapchain(&mut self, size: PhysicalSize<u32>) {
        self.device_state.update_format_and_rebuild_swapchain(size);
        self.imgui.change_swapchain_format(
            &self.device_state.device,
            self.device_state.surf_config.format,
        );
    }

    pub fn redraw(&mut self, imgui_draw_data: &imgui::DrawData, size: PhysicalSize<u32>) {
        if self.device_state.sc_needs_rebuild {
            self.device_state.rebuild_swapchain(size);
        }
        let frame = loop {
            match self.device_state.surface.get_current_texture() {
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
                    wgpu::SurfaceError::OutOfMemory => panic!("Swapchain ran out of memory"),
                },
            }
        };
        let mut encoder = self
            .device_state
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        self.imgui.render(
            &self.device_state.device,
            &self.device_state.queue,
            &mut encoder,
            &frame
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default()),
            imgui_draw_data,
        );
        self.device_state.queue.submit(iter::once(encoder.finish()));
        frame.present();
    }
}

pub struct Builder {
    pub event_loop: EventLoop<()>,
    pub window: Window,
    pub imgui: imgui::Context,
}

pub struct Window {
    pub window: WinitWindow,
    pub is_hidden: bool,
    pub scale_factor: f64,
    pub last_frame: Instant,
    pub imgui_winit_platform: imgui_winit_support::WinitPlatform,
    pub gfx: GfxState,
    pub normal_font: imgui::FontId,
    pub mono_font: imgui::FontId,
    #[cfg(target_os = "macos")]
    pub macos_title_bar_height: f32,
}

impl Window {
    #[cfg(target_os = "macos")]
    fn macos_title_bar_height(&self) -> f32 {
        let content_layout_rect: NSRect =
            unsafe { msg_send![self.window.ns_window() as id, contentLayoutRect] };
        (self.window.outer_size().height as f64 / self.scale_factor
            - content_layout_rect.size.height) as f32
    }

    #[cfg(target_os = "macos")]
    pub fn set_macos_titlebar_hidden(&mut self, hidden: bool) {
        self.macos_title_bar_height = if hidden {
            self.macos_title_bar_height()
        } else {
            0.0
        };
        unsafe {
            let window = self.window.ns_window() as id;
            window.setTitlebarAppearsTransparent_(hidden as i8);
            let prev_style_mask = window.styleMask();
            window.setStyleMask_(if hidden {
                prev_style_mask | NSWindowStyleMask::NSFullSizeContentViewWindowMask
            } else {
                prev_style_mask & !NSWindowStyleMask::NSFullSizeContentViewWindowMask
            });
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlFlow {
    Continue,
    Exit,
}

impl Builder {
    pub async fn new(
        title: impl Into<String>,
        default_logical_size: (u32, u32),
        imgui_config_path: Option<PathBuf>,
        #[cfg(target_os = "macos")] macos_title_bar_hidden: bool,
    ) -> Self {
        let event_loop = EventLoop::new();
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
            .expect("Couldn't create window");

        let scale_factor = window.scale_factor();

        let mut imgui = imgui::Context::create();

        imgui.set_ini_filename(imgui_config_path);
        let imgui_io = imgui.io_mut();
        imgui_io.config_windows_move_from_title_bar_only = true;
        imgui_io.config_flags |= imgui::ConfigFlags::IS_SRGB | imgui::ConfigFlags::DOCKING_ENABLE;

        imgui_io.font_global_scale = (1.0 / scale_factor) as f32;
        let normal_font = imgui.fonts().add_font(&[imgui::FontSource::TtfData {
            data: include_bytes!("../../fonts/OpenSans-Regular.ttf"),
            size_pixels: (16.0 * scale_factor) as f32,
            config: None,
        }]);
        let mono_font = imgui.fonts().add_font(&[imgui::FontSource::TtfData {
            data: include_bytes!("../../fonts/FiraMono-Regular.ttf"),
            size_pixels: (13.0 * scale_factor) as f32,
            config: None,
        }]);

        let style = imgui.style_mut();
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

        let mut imgui_winit_platform = imgui_winit_support::WinitPlatform::init(&mut imgui);
        imgui_winit_platform.attach_window(
            imgui.io_mut(),
            &window,
            imgui_winit_support::HiDpiMode::Default,
        );

        let gfx = GfxState::new(&window, &mut imgui).await;

        #[allow(unused_mut)]
        let mut window = Window {
            window,
            is_hidden: true,
            scale_factor,
            gfx,
            last_frame: Instant::now(),
            imgui_winit_platform,
            normal_font,
            mono_font,
            #[cfg(target_os = "macos")]
            macos_title_bar_height: 0.0,
        };

        #[cfg(target_os = "macos")]
        if macos_title_bar_hidden {
            window.macos_title_bar_height = window.macos_title_bar_height();
        }

        Builder {
            window,
            event_loop,
            imgui,
        }
    }

    pub fn run<S: 'static>(
        mut self,
        state: S,
        mut process_event: impl FnMut(&mut Window, &mut S, &Event<()>) + 'static,
        mut run_frame: impl FnMut(&mut Window, &imgui::Ui, &mut S) -> ControlFlow + 'static,
        on_exit: impl FnOnce(Window, S) + 'static,
    ) -> ! {
        // Since Rust can't prove that after Event::LoopDestroyed the program will exit and prevent
        // these from being used again, they have to be wrapped in ManuallyDrop to be able to pass
        // everything by value to the on_exit callback
        let mut window_ = ManuallyDrop::new(self.window);
        let mut state = ManuallyDrop::new(state);
        let mut on_exit = ManuallyDrop::new(on_exit);

        self.event_loop.run(move |event, _, control_flow| {
            let window = &mut *window_;
            window
                .imgui_winit_platform
                .handle_event(self.imgui.io_mut(), &window.window, &event);
            process_event(window, &mut state, &event);
            match event {
                Event::NewEvents(StartCause::Init) => {
                    *control_flow = WinitControlFlow::Poll;
                }

                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => {
                    *control_flow = WinitControlFlow::Exit;
                }

                Event::WindowEvent {
                    event: WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. },
                    ..
                } => {
                    window.gfx.device_state.invalidate_swapchain();
                }

                Event::RedrawRequested(_) => {
                    let now = Instant::now();
                    let io = self.imgui.io_mut();
                    io.update_delta_time(now - window.last_frame);
                    window.last_frame = now;
                    window
                        .imgui_winit_platform
                        .prepare_frame(io, &window.window)
                        .expect("Couldn't prepare imgui frame");

                    let ui = self.imgui.frame();
                    if run_frame(window, ui, &mut state) == ControlFlow::Exit {
                        *control_flow = WinitControlFlow::Exit;
                    }

                    window
                        .imgui_winit_platform
                        .prepare_render(ui, &window.window);
                    window
                        .gfx
                        .redraw(self.imgui.render(), window.window.inner_size());
                    window.gfx.device_state.device.poll(wgpu::Maintain::Poll);
                }

                Event::RedrawEventsCleared => {
                    if window.is_hidden {
                        window.is_hidden = false;
                        window.window.set_visible(true);
                    }

                    // TODO: https://github.com/rust-windowing/winit/issues/2022
                    // Mitigation for https://github.com/gfx-rs/wgpu/issues/1783
                    #[cfg(target_os = "macos")]
                    let window_visible =
                        unsafe { (window.window.ns_window() as id).occlusionState() }
                            .contains(NSWindowOcclusionState::NSWindowOcclusionStateVisible);
                    #[cfg(not(target_os = "macos"))]
                    let window_visible = true;
                    if window_visible {
                        window.window.request_redraw();
                    }
                }

                Event::LoopDestroyed => {
                    unsafe {
                        ManuallyDrop::take(&mut on_exit)(
                            ManuallyDrop::take(&mut window_),
                            ManuallyDrop::take(&mut state),
                        )
                    };
                }
                _ => {}
            }
        });
    }
}
