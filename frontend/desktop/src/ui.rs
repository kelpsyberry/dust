#[cfg(feature = "log")]
mod imgui_log;
#[allow(dead_code)]
pub mod imgui_wgpu;
pub mod window;

#[cfg(feature = "log")]
use super::config::LoggingKind;
#[cfg(feature = "debug-views")]
use super::debug_views;
use super::{
    audio,
    config::{self, CommonLaunchConfig, Config},
    emu, input, triple_buffer,
    utils::{config_base, scale_to_fit},
    FrameData,
};
#[cfg(feature = "xq-audio")]
use dust_core::audio::InterpMethod as AudioXqInterpMethod;
use dust_core::{
    gpu::{SCREEN_HEIGHT, SCREEN_WIDTH},
    utils::{zeroed_box, BoxedByteSlice},
};
use parking_lot::RwLock;
use rfd::FileDialog;
#[cfg(feature = "discord-presence")]
use std::time::SystemTime;
use std::{
    env,
    fs::{self, File},
    io::Read,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

#[cfg(feature = "log")]
fn init_logging(
    imgui_log: &mut Option<(imgui_log::Console, imgui_log::Sender, bool)>,
    kind: LoggingKind,
) -> slog::Logger {
    use slog::Drain;
    match kind {
        LoggingKind::Imgui => {
            let logger_tx = if let Some((_, logger_tx, _)) = imgui_log {
                logger_tx.clone()
            } else {
                let (log_console, logger_tx) = imgui_log::Console::new(true);
                *imgui_log = Some((log_console, logger_tx.clone(), false));
                logger_tx
            };
            slog::Logger::root(imgui_log::Drain::new(logger_tx).fuse(), slog::o!())
        }
        LoggingKind::Term => {
            *imgui_log = None;
            let decorator = slog_term::TermDecorator::new().stdout().build();
            let drain = slog_term::CompactFormat::new(decorator)
                .use_custom_timestamp(|_: &mut dyn std::io::Write| Ok(()))
                .build()
                .fuse();
            slog::Logger::root(
                slog_async::Async::new(drain)
                    .overflow_strategy(slog_async::OverflowStrategy::Block)
                    .thread_name("async logger".to_string())
                    .build()
                    .fuse(),
                slog::o!(),
            )
        }
    }
}

struct UiState {
    global_config: Config<config::Global>,
    game_title: Option<String>,
    game_config: Option<Config<config::Game>>,

    playing: bool,
    limit_framerate: config::RuntimeModifiable<bool>,

    show_menu_bar: bool,

    screen_focused: bool,
    input: input::State,
    input_editor: Option<input::Editor>,

    audio_channel: Option<audio::Channel>,
    audio_volume: f32,
    audio_sample_chunk_size: u32,
    #[cfg(feature = "xq-audio")]
    audio_xq_sample_rate_shift: u8,
    #[cfg(feature = "xq-audio")]
    audio_xq_interp_method: AudioXqInterpMethod,
    audio_interp_method: audio::InterpMethod,
    sync_to_audio: config::RuntimeModifiable<bool>,

    #[cfg(feature = "log")]
    imgui_log: Option<(imgui_log::Console, imgui_log::Sender, bool)>,
    #[cfg(feature = "log")]
    logger: slog::Logger,

    frame_tx: Option<triple_buffer::Sender<FrameData>>,
    frame_rx: triple_buffer::Receiver<FrameData>,
    fps_fixed: Option<u64>,
    fb_texture_id: imgui::TextureId,

    #[cfg(feature = "debug-views")]
    debug_views: debug_views::UiState,

    message_tx: crossbeam_channel::Sender<emu::Message>,
    message_rx: crossbeam_channel::Receiver<emu::Message>,

    emu_thread: Option<thread::JoinHandle<triple_buffer::Sender<FrameData>>>,
    emu_shared_state: Option<Arc<emu::SharedState>>,

    #[cfg(feature = "discord-presence")]
    rpc_connection: discord_rpc::Rpc,
    #[cfg(feature = "discord-presence")]
    presence: discord_rpc::Presence,
    #[cfg(feature = "discord-presence")]
    presence_updated: bool,
}

static ALLOWED_ROM_EXTENSIONS: &[&str] = &["nds", "bin"];

impl UiState {
    fn send_message(&self, msg: emu::Message) {
        self.message_tx.send(msg).expect("Couldn't send UI message");
    }

    fn load_from_rom_path(&mut self, path: &Path) {
        if let Some(extension) = path.extension().and_then(|s| s.to_str()) {
            if !ALLOWED_ROM_EXTENSIONS.contains(&extension) {
                return;
            }
        } else {
            return;
        }

        let ds_slot_rom = {
            let mut rom_file = File::open(path).expect("Couldn't load the specified ROM file");
            let rom_len = rom_file
                .metadata()
                .expect("Couldn't get ROM file metadata")
                .len() as usize;
            let mut rom = BoxedByteSlice::new_zeroed(rom_len.next_power_of_two());
            rom_file
                .read_exact(&mut rom[..rom_len])
                .expect("Couldn't read ROM file");
            rom
        };

        let game_title = path
            .file_stem()
            .unwrap()
            .to_str()
            .expect("Non-UTF-8 ROM filename provided");

        let game_config = Config::<config::Game>::read_from_file_or_show_dialog(
            &config_base().join("games").join(&game_title),
            game_title,
        );

        match config::game_launch_config(
            &self.global_config.contents,
            &game_config.contents,
            game_title,
        ) {
            Ok(launch_config) => {
                self.start(
                    launch_config.common,
                    game_title.to_string(),
                    Some(game_config),
                    Some(ds_slot_rom),
                );
            }
            Err(errors) => {
                config_error!(
                    "Couldn't determine final configuration for game: {}",
                    errors.into_iter().fold(String::new(), |mut acc, err| {
                        use core::fmt::Write;
                        let _ = write!(acc, "\n- {}", err);
                        acc
                    })
                );
            }
        }
    }

    fn load_firmware(&mut self) {
        match config::firmware_launch_config(&self.global_config.contents) {
            Ok(launch_config) => {
                self.start(launch_config, "Firmware".to_string(), None, None);
            }
            Err(errors) => {
                config_error!(
                    "Couldn't determine final configuration for firmware: {}",
                    errors.into_iter().fold(String::new(), |mut acc, err| {
                        use core::fmt::Write;
                        let _ = write!(acc, "\n- {}", err);
                        acc
                    })
                );
            }
        }
    }

    fn start(
        &mut self,
        config: CommonLaunchConfig,
        game_title: String,
        game_config: Option<Config<config::Game>>,
        ds_slot_rom: Option<BoxedByteSlice>,
    ) {
        self.stop();

        #[cfg(feature = "discord-presence")]
        {
            self.presence.state = Some(format!("Playing {}", game_title));
            self.presence.timestamps = Some(discord_rpc::Timestamps {
                start: Some(SystemTime::now()),
                end: None,
            });
            self.presence_updated = true;
        }

        self.game_title = Some(game_title);
        self.game_config = game_config;

        self.limit_framerate = config.limit_framerate;
        self.sync_to_audio = config.sync_to_audio;

        if let Some(channel) = &mut self.audio_channel {
            channel
                .output_stream
                .set_interp(config.audio_interp_method.value.create_interp());
            #[cfg(feature = "xq-audio")]
            channel.set_xq_sample_rate_shift(config.audio_xq_sample_rate_shift.value);
        }

        #[cfg(feature = "log")]
        let logger = self.logger.clone();

        let frame_tx = self.frame_tx.take().unwrap();
        let message_rx = self.message_rx.clone();
        let audio_tx_data = self
            .audio_channel
            .as_ref()
            .map(|audio_channel| audio_channel.tx_data.clone());
        self.playing = !config.pause_on_launch;
        let emu_shared_state = Arc::new(emu::SharedState {
            playing: AtomicBool::new(self.playing),
            limit_framerate: AtomicBool::new(self.limit_framerate.value),
            autosave_interval: RwLock::new(Duration::from_secs_f32(
                config.autosave_interval_ms.value / 1000.0,
            )),
            stopped: AtomicBool::new(false),
        });
        self.emu_shared_state = Some(Arc::clone(&emu_shared_state));
        self.emu_thread = Some(
            thread::Builder::new()
                .name("emulation".to_string())
                .spawn(move || {
                    emu::main(
                        config,
                        ds_slot_rom,
                        audio_tx_data,
                        frame_tx,
                        message_rx,
                        emu_shared_state,
                        #[cfg(feature = "log")]
                        logger,
                    )
                })
                .expect("Couldn't spawn emulation thread"),
        );

        #[cfg(feature = "debug-views")]
        self.debug_views.reload_emu_state();
    }

    fn stop(&mut self) {
        #[cfg(feature = "discord-presence")]
        {
            self.presence.state = Some("Not playing anything".to_string());
            self.presence.timestamps = Some(discord_rpc::Timestamps {
                start: Some(SystemTime::now()),
                end: None,
            });
            self.presence_updated = true;
        }

        if let Some(emu_thread) = self.emu_thread.take() {
            self.emu_shared_state
                .take()
                .unwrap()
                .stopped
                .store(true, Ordering::Relaxed);
            self.frame_tx = Some(emu_thread.join().expect("Couldn't join emulation thread"));
        }
        if let Some(mut game_config) = self.game_config.take() {
            if let Some(dir_path) = game_config.path.as_ref().and_then(|p| p.parent()) {
                let _ = fs::create_dir_all(dir_path);
            }
            let _ = game_config.flush();
        }
        self.game_title = None;
        self.playing = false;
    }

    fn set_touchscreen_bounds(&mut self, start: [f32; 2], end: [f32; 2], window: &window::Window) {
        self.input.set_touchscreen_bounds((
            (
                start[0] as f64 * window.scale_factor,
                start[1] as f64 * window.scale_factor,
            )
                .into(),
            (
                end[0] as f64 * window.scale_factor,
                end[1] as f64 * window.scale_factor,
            )
                .into(),
        ));
    }

    #[cfg(feature = "discord-presence")]
    fn flush_presence(&mut self) {
        if !self.presence_updated {
            return;
        }
        self.presence_updated = false;
        self.rpc_connection.update_presence(Some(&self.presence));
    }
}

fn clear_fb_texture(id: imgui::TextureId, window: &mut window::Window) {
    let mut data = zeroed_box::<[u8; SCREEN_WIDTH * SCREEN_HEIGHT * 8]>();
    for i in (0..data.len()).step_by(4) {
        data[i + 3] = 0xFF;
    }
    window.gfx.imgui.texture_mut(id).set_data(
        &window.gfx.device_state.queue,
        &data[..],
        imgui_wgpu::TextureRange::default(),
    );
}

pub fn main() {
    let config_home = config_base();

    let global_config = if let Err(err) = fs::create_dir_all(config_home) {
        config_error!(
            concat!(
                "Couldn't create the configuration directory{}: {}\n\nThe default configuration ",
                "will be used, new changes will not be saved.",
            ),
            match config_home.to_str() {
                Some(str) => format!(" at `{}`", str),
                None => String::new(),
            },
            err,
        );
        Config::default()
    } else {
        Config::<config::Global>::read_from_file_or_show_dialog(
            &config_home.join("global_config.json"),
            "global_config.json",
        )
    };

    let keymap = Config::<input::Keymap>::read_from_file_or_show_dialog(
        &config_home.join("keymap.json"),
        "keymap.json",
    );

    #[cfg(feature = "log")]
    let mut imgui_log = None;
    #[cfg(feature = "log")]
    let logger = init_logging(&mut imgui_log, global_config.contents.logging_kind);

    let mut window_builder = futures_executor::block_on(window::Builder::new(
        "Dust",
        global_config.contents.window_size,
        global_config.contents.imgui_config_path.clone(),
    ));

    let audio_channel = audio::channel(
        global_config.contents.audio_interp_method,
        global_config.contents.audio_volume,
        #[cfg(feature = "xq-audio")]
        global_config.contents.audio_xq_sample_rate_shift,
    );

    let (frame_tx, frame_rx) = triple_buffer::init([
        FrameData::default(),
        FrameData::default(),
        FrameData::default(),
    ]);

    let (message_tx, message_rx) = crossbeam_channel::unbounded::<emu::Message>();

    let fb_texture_id = {
        let texture = window_builder.window.gfx.imgui.create_texture(
            &window_builder.window.gfx.device_state.device,
            &wgpu::SamplerDescriptor {
                label: Some("framebuffer sampler"),
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            },
            imgui_wgpu::TextureDescriptor {
                label: Some("framebuffer texture".to_string()),
                size: wgpu::Extent3d {
                    width: SCREEN_WIDTH as u32,
                    height: SCREEN_HEIGHT as u32 * 2,
                    depth_or_array_layers: 1,
                },
                format: Some(
                    if window_builder
                        .window
                        .gfx
                        .device_state
                        .surf_config
                        .format
                        .describe()
                        .srgb
                    {
                        wgpu::TextureFormat::Rgba8UnormSrgb
                    } else {
                        wgpu::TextureFormat::Rgba8Unorm
                    },
                ),
                ..Default::default()
            },
        );
        window_builder.window.gfx.imgui.add_texture(texture)
    };
    clear_fb_texture(fb_texture_id, &mut window_builder.window);

    let mut state = UiState {
        game_title: None,
        game_config: None,

        playing: false,
        limit_framerate: config::RuntimeModifiable::global(global_config.contents.limit_framerate),

        screen_focused: true,
        input: input::State::new(keymap),
        input_editor: None,

        audio_channel,
        audio_volume: global_config.contents.audio_volume,
        audio_sample_chunk_size: global_config.contents.audio_sample_chunk_size,
        #[cfg(feature = "xq-audio")]
        audio_xq_sample_rate_shift: global_config.contents.audio_xq_sample_rate_shift,
        #[cfg(feature = "xq-audio")]
        audio_xq_interp_method: global_config.contents.audio_xq_interp_method,
        audio_interp_method: global_config.contents.audio_interp_method,
        sync_to_audio: config::RuntimeModifiable::global(global_config.contents.sync_to_audio),

        show_menu_bar: true,

        #[cfg(feature = "log")]
        imgui_log,
        #[cfg(feature = "log")]
        logger,

        frame_tx: Some(frame_tx),
        frame_rx,
        fps_fixed: None,
        fb_texture_id,

        #[cfg(feature = "debug-views")]
        debug_views: debug_views::UiState::new(),

        message_tx,
        message_rx,

        emu_thread: None,
        emu_shared_state: None,

        global_config,

        #[cfg(feature = "discord-presence")]
        rpc_connection: discord_rpc::Rpc::new(
            "914286657849667645".to_string(),
            Default::default(),
            false,
        ),
        #[cfg(feature = "discord-presence")]
        presence: Default::default(),
        #[cfg(feature = "discord-presence")]
        presence_updated: true,
    };

    state.stop();

    if let Some(rom_path) = env::args_os().nth(1) {
        state.load_from_rom_path(Path::new(&rom_path));
    }

    window_builder.run(
        state,
        |_, state, event| {
            use winit::event::{Event, WindowEvent};

            if let Event::WindowEvent {
                event: WindowEvent::DroppedFile(path),
                ..
            } = event
            {
                state.load_from_rom_path(path);
            }

            state.input.process_event(event, state.screen_focused);
            if let Some(input_editor) = &mut state.input_editor {
                input_editor.process_event(event, &mut state.input);
            }
        },
        |window, ui, state| {
            #[cfg(feature = "discord-presence")]
            {
                state.rpc_connection.check_events();
                state.flush_presence();
            }

            if state.emu_thread.is_some() {
                if state
                    .emu_shared_state
                    .as_ref()
                    .unwrap()
                    .stopped
                    .load(Ordering::Relaxed)
                {
                    state.stop();
                    clear_fb_texture(state.fb_texture_id, window);
                } else if let Ok(frame) = state.frame_rx.get() {
                    #[cfg(feature = "debug-views")]
                    state
                        .debug_views
                        .update_from_frame_data(&frame.debug, window);

                    let fps_fixed = (frame.fps * 10.0).round() as u64;
                    if Some(fps_fixed) != state.fps_fixed {
                        state.fps_fixed = Some(fps_fixed);
                        window.window.set_title(&format!(
                            "Dust - {} - {:.01} FPS",
                            state.game_title.as_ref().unwrap(),
                            frame.fps
                        ));
                    }

                    let fb_texture = window.gfx.imgui.texture_mut(state.fb_texture_id);
                    let data = unsafe {
                        core::slice::from_raw_parts(
                            frame.fb.0.as_ptr() as *const u8,
                            SCREEN_WIDTH * SCREEN_HEIGHT * 8,
                        )
                    };
                    fb_texture.set_data(
                        &window.gfx.device_state.queue,
                        data,
                        imgui_wgpu::TextureRange::default(),
                    );
                }
            } else {
                window.window.set_title("Dust - No game loaded");
            }

            if state.playing {
                if let Some(changes) = state.input.drain_changes() {
                    state.send_message(emu::Message::UpdateInput(changes));
                }
            }

            if state.global_config.contents.fullscreen_render
                && ui.is_key_pressed(imgui::Key::Escape)
                && !ui.is_any_item_focused()
            {
                state.show_menu_bar = !state.show_menu_bar;
            }

            if state.show_menu_bar {
                ui.main_menu_bar(|| {
                    ui.menu("Emulation", || {
                        if imgui::MenuItem::new(if state.playing { "Pause" } else { "Play" })
                            .enabled(state.emu_thread.is_some())
                            .build(ui)
                        {
                            let shared_state = state.emu_shared_state.as_mut().unwrap();
                            state.playing = !state.playing;
                            shared_state.playing.store(state.playing, Ordering::Relaxed);
                        }

                        if imgui::MenuItem::new("Reset")
                            .enabled(state.emu_thread.is_some())
                            .build(ui)
                        {
                            state
                                .message_tx
                                .send(emu::Message::Reset)
                                .expect("Couldn't send UI message");
                        }

                        if imgui::MenuItem::new("Stop")
                            .enabled(state.emu_thread.is_some())
                            .build(ui)
                        {
                            state.stop();
                            clear_fb_texture(state.fb_texture_id, window);
                        }

                        if imgui::MenuItem::new("Load game...").build(ui) {
                            if let Some(path) = FileDialog::new()
                                .add_filter("NDS ROM file", ALLOWED_ROM_EXTENSIONS)
                                .pick_file()
                            {
                                state.load_from_rom_path(&path);
                            }
                        }

                        if imgui::MenuItem::new("Load firmware").build(ui) {
                            state.load_firmware();
                        }
                    });

                    ui.menu("Config", || {
                        ui.menu("Audio volume", || {
                            let mut volume = state.audio_volume * 100.0;
                            if imgui::Slider::new("", 0.0, 100.0)
                                .display_format("%.02f%%")
                                .build(ui, &mut volume)
                            {
                                state.audio_volume =
                                    (volume * 100.0).round().clamp(0.0, 10000.0) / 10000.0;
                                if let Some(audio_channel) = state.audio_channel.as_mut() {
                                    audio_channel.output_stream.set_volume(state.audio_volume)
                                }
                                state.global_config.contents.audio_volume = state.audio_volume;
                                state.global_config.dirty = true;
                            }
                        });

                        ui.menu("Audio sample chunk size", || {
                            let mut sample_chunk_size = state.audio_sample_chunk_size as i32;
                            if imgui::InputInt::new(ui, "", &mut sample_chunk_size)
                                .enter_returns_true(true)
                                .build()
                            {
                                state.audio_sample_chunk_size = sample_chunk_size.max(0) as u32;
                                state
                                    .message_tx
                                    .send(emu::Message::UpdateAudioSampleChunkSize(
                                        state.audio_sample_chunk_size,
                                    ))
                                    .expect("Couldn't send UI message");
                                state.global_config.contents.audio_sample_chunk_size =
                                    state.audio_sample_chunk_size;
                                state.global_config.dirty = true;
                            }
                        });

                        #[cfg(feature = "xq-audio")]
                        ui.menu("Core audio interpolation", || {
                            if imgui::Slider::new("Sample rate multiplier", 0, 10)
                                .display_format(&format!(
                                    "{}x",
                                    1 << state.audio_xq_sample_rate_shift
                                ))
                                .build(ui, &mut state.audio_xq_sample_rate_shift)
                            {
                                if let Some(audio_channel) = state.audio_channel.as_mut() {
                                    audio_channel
                                        .set_xq_sample_rate_shift(state.audio_xq_sample_rate_shift);
                                }
                                state
                                    .message_tx
                                    .send(emu::Message::UpdateAudioXqSampleRateShift(
                                        state.audio_xq_sample_rate_shift,
                                    ))
                                    .expect("Couldn't send UI message");
                                state.global_config.contents.audio_xq_sample_rate_shift =
                                    state.audio_xq_sample_rate_shift;
                                state.global_config.dirty = true;
                            }

                            static INTERP_METHODS: [AudioXqInterpMethod; 2] =
                                [AudioXqInterpMethod::Nearest, AudioXqInterpMethod::Cubic];
                            let mut i = INTERP_METHODS
                                .iter()
                                .position(|&m| m == state.audio_xq_interp_method)
                                .unwrap();
                            let updated = ui.combo(
                                "Interpolation method",
                                &mut i,
                                &INTERP_METHODS,
                                |interp_method| {
                                    match interp_method {
                                        AudioXqInterpMethod::Nearest => "Nearest",
                                        AudioXqInterpMethod::Cubic => "Cubic",
                                    }
                                    .into()
                                },
                            );
                            if updated {
                                state.audio_xq_interp_method = INTERP_METHODS[i];
                                state
                                    .message_tx
                                    .send(emu::Message::UpdateAudioXqInterpMethod(
                                        state.audio_xq_interp_method,
                                    ))
                                    .expect("Couldn't send UI message");
                                state.global_config.contents.audio_xq_interp_method =
                                    state.audio_xq_interp_method;
                                state.global_config.dirty = true;
                            }
                        });

                        ui.menu("Frontend audio interpolation method", || {
                            static INTERP_METHODS: [audio::InterpMethod; 2] =
                                [audio::InterpMethod::Nearest, audio::InterpMethod::Cubic];
                            let mut i = INTERP_METHODS
                                .iter()
                                .position(|&m| m == state.audio_interp_method)
                                .unwrap();
                            let updated = ui.combo("", &mut i, &INTERP_METHODS, |interp_method| {
                                match interp_method {
                                    audio::InterpMethod::Nearest => "Nearest",
                                    audio::InterpMethod::Cubic => "Cubic",
                                }
                                .into()
                            });
                            if updated {
                                state.audio_interp_method = INTERP_METHODS[i];
                                if let Some(audio_channel) = state.audio_channel.as_mut() {
                                    audio_channel
                                        .output_stream
                                        .set_interp(state.audio_interp_method.create_interp());
                                }
                                state.global_config.contents.audio_interp_method =
                                    state.audio_interp_method;
                                state.global_config.dirty = true;
                            }
                        });

                        if imgui::MenuItem::new("Limit framerate")
                            .build_with_ref(ui, &mut state.limit_framerate.value)
                        {
                            if state.limit_framerate.origin == config::SettingOrigin::Game {
                                let game_config = state.game_config.as_mut().unwrap();
                                game_config.contents.limit_framerate =
                                    Some(state.limit_framerate.value);
                                game_config.dirty = true;
                            }
                            state.global_config.contents.limit_framerate =
                                state.limit_framerate.value;
                            state.global_config.dirty = true;
                            if let Some(shared_state) = &state.emu_shared_state {
                                shared_state
                                    .limit_framerate
                                    .store(state.limit_framerate.value, Ordering::Relaxed);
                            }
                        }

                        if imgui::MenuItem::new("Sync to audio")
                            .build_with_ref(ui, &mut state.sync_to_audio.value)
                        {
                            if state.sync_to_audio.origin == config::SettingOrigin::Game {
                                let game_config = state.game_config.as_mut().unwrap();
                                game_config.contents.sync_to_audio =
                                    Some(state.sync_to_audio.value);
                                game_config.dirty = true;
                            }
                            state.global_config.contents.sync_to_audio = state.sync_to_audio.value;
                            state.global_config.dirty = true;
                            state
                                .message_tx
                                .send(emu::Message::UpdateAudioSync(state.sync_to_audio.value))
                                .expect("Couldn't send UI message");
                        }

                        if imgui::MenuItem::new("Fullscreen render")
                            .build_with_ref(ui, &mut state.global_config.contents.fullscreen_render)
                        {
                            state.global_config.dirty = true;
                            state.show_menu_bar = !state.global_config.contents.fullscreen_render;
                        }

                        let mut show_input = state.input_editor.is_some();
                        if imgui::MenuItem::new("Input").build_with_ref(ui, &mut show_input) {
                            state.input_editor = if show_input {
                                Some(input::Editor::new())
                            } else {
                                None
                            };
                        }
                    });

                    #[cfg(feature = "log")]
                    let imgui_log_enabled = state.imgui_log.is_some();
                    #[cfg(not(feature = "log"))]
                    let imgui_log_enabled = false;
                    if cfg!(feature = "debug-views") || imgui_log_enabled {
                        ui.menu("Debug", || {
                            #[cfg(feature = "log")]
                            if let Some((_, _, console_visible)) = &mut state.imgui_log {
                                imgui::MenuItem::new("Log").build_with_ref(ui, console_visible);
                            }
                            #[cfg(feature = "debug-views")]
                            {
                                if imgui_log_enabled {
                                    ui.separator();
                                }
                                state.debug_views.render_menu(ui, window);
                            }
                        });
                    }
                });
            }

            #[cfg(feature = "log")]
            if let Some((console, _, console_visible @ true)) = &mut state.imgui_log {
                let _window_padding = ui.push_style_var(imgui::StyleVar::WindowPadding([6.0; 2]));
                let _item_spacing = ui.push_style_var(imgui::StyleVar::ItemSpacing([0.0; 2]));
                console.render_window(ui, Some(window.mono_font), console_visible);
            }

            #[cfg(feature = "debug-views")]
            for message in state
                .debug_views
                .render(ui, window, state.emu_thread.is_some())
            {
                state
                    .message_tx
                    .send(emu::Message::DebugViews(message))
                    .expect("Couldn't send UI message");
            }

            if let Some(input_editor) = &mut state.input_editor {
                let mut opened = true;
                input_editor.draw(ui, &mut state.input, &mut opened);
                if !opened {
                    state.input_editor = None;
                }
            }

            let window_size = window.window.inner_size();
            const ASPECT_RATIO: f32 = SCREEN_WIDTH as f32 / (2 * SCREEN_HEIGHT) as f32;
            if state.global_config.contents.fullscreen_render {
                let ([x_base, y_base], [width, height]) = scale_to_fit(
                    ASPECT_RATIO,
                    [
                        (window_size.width as f64 / window.scale_factor) as f32,
                        (window_size.height as f64 / window.scale_factor) as f32,
                    ],
                );
                ui.get_background_draw_list()
                    .add_image(
                        state.fb_texture_id,
                        [x_base, y_base],
                        [x_base + width, y_base + height],
                    )
                    .build();
                state.screen_focused =
                    !ui.is_window_focused_with_flags(imgui::WindowFocusedFlags::ANY_WINDOW);
                state.set_touchscreen_bounds(
                    [x_base, y_base + height * 0.5],
                    [x_base + width, y_base + height],
                    window,
                );
            } else {
                let style = ui.clone_style();
                let _window_padding = ui.push_style_var(imgui::StyleVar::WindowPadding([0.0; 2]));
                let titlebar_height = style.frame_padding[1] * 2.0 + ui.current_font_size();
                const DEFAULT_SCALE: f32 = 2.0;
                state.screen_focused = false;
                imgui::Window::new("Screen")
                    .size(
                        [
                            SCREEN_WIDTH as f32 * DEFAULT_SCALE,
                            (SCREEN_HEIGHT * 2) as f32 * DEFAULT_SCALE + titlebar_height,
                        ],
                        imgui::Condition::FirstUseEver,
                    )
                    .position(
                        [
                            (window_size.width as f64 * 0.5 / window.scale_factor) as f32,
                            (window_size.height as f64 * 0.5 / window.scale_factor) as f32,
                        ],
                        imgui::Condition::FirstUseEver,
                    )
                    .position_pivot([0.5; 2])
                    .build(ui, || {
                        let ([x_base, y_base], [width, height]) =
                            scale_to_fit(ASPECT_RATIO, ui.content_region_avail());
                        ui.set_cursor_pos([x_base, titlebar_height + y_base]);
                        imgui::Image::new(state.fb_texture_id, [width, height]).build(ui);
                        state.screen_focused = ui.is_window_focused();
                        let window_pos = ui.window_pos();
                        let abs_base = [x_base + window_pos[0], y_base + window_pos[1]];
                        state.set_touchscreen_bounds(
                            [abs_base[0], abs_base[1] + height * 0.5],
                            [abs_base[0] + width, abs_base[1] + height * 0.5],
                            window,
                        );
                    });
            }

            window::ControlFlow::Continue
        },
        move |window, mut state| {
            state.stop();
            state.global_config.contents.window_size = window
                .window
                .inner_size()
                .to_logical::<u32>(window.scale_factor)
                .into();
            state.global_config.dirty = true;
            state
                .global_config
                .flush()
                .expect("Couldn't save global configuration");
            state.input.keymap.flush().expect("Couldn't save keymap");
        },
    );
}
