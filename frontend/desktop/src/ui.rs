#[cfg(feature = "log")]
mod imgui_log;
#[allow(dead_code)]
pub mod imgui_wgpu;
#[cfg(feature = "log")]
mod logging;
pub mod window;

#[cfg(feature = "debug-views")]
use super::debug_views;
use super::{
    audio,
    config::{self, CommonLaunchConfig, Config},
    emu, game_db, input, triple_buffer,
    utils::{config_base, scale_to_fit_rotated},
    FrameData,
};
#[cfg(feature = "xq-audio")]
use dust_core::audio::ChannelInterpMethod as AudioChannelInterpMethod;
use dust_core::{
    gpu::{SCREEN_HEIGHT, SCREEN_WIDTH},
    utils::{zeroed_box, BoxedByteSlice},
};
use parking_lot::RwLock;
use rfd::FileDialog;
#[cfg(feature = "xq-audio")]
use std::num::NonZeroU32;
#[cfg(feature = "discord-presence")]
use std::time::SystemTime;
use std::{
    env,
    fmt::Write,
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
    slice,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

struct CurrentConfig {
    limit_framerate: config::GameOverridable<bool>,
    screen_rotation: config::GameOverridable<i16>,

    sync_to_audio: config::GameOverridable<bool>,
    audio_volume: config::GameOverridable<f32>,
    audio_sample_chunk_size: config::GameOverridable<u32>,
    audio_interp_method: config::GameOverridable<audio::InterpMethod>,
    #[cfg(feature = "xq-audio")]
    audio_custom_sample_rate: config::GameOverridable<(Option<NonZeroU32>, Option<NonZeroU32>)>,
    #[cfg(feature = "xq-audio")]
    audio_channel_interp_method: config::GameOverridable<AudioChannelInterpMethod>,
}

macro_rules! update_setting {
    ($ui_state: expr, $setting: ident, $value: expr, $update_fn: expr) => {
        if $ui_state.current_config.$setting.update($value) {
            $update_fn($ui_state);
        }
    };
}

macro_rules! update_setting_value {
    ($ui_state: expr, $setting: ident, $value: expr, $update_fn: expr) => {
        update_setting_value!($ui_state, $setting, $value, $update_fn, |value| value)
    };
    (
        $ui_state: expr, $setting: ident, $value: expr, $update_fn: expr,
        |$cur_value: ident| $saved_value: expr
    ) => {
        let $cur_value = $value;
        if $ui_state.current_config.$setting.update_value($cur_value) {
            $update_fn($ui_state);
            if $ui_state.current_config.$setting.origin == config::SettingOrigin::Game {
                let game_config = $ui_state
                    .emu_state
                    .as_mut()
                    .and_then(|emu| emu.game_config.as_mut())
                    .unwrap();
                game_config.contents.$setting = Some($saved_value);
                game_config.dirty = true;
            }
            $ui_state.global_config.contents.$setting = $saved_value;
            $ui_state.global_config.dirty = true;
        }
    };
}

#[cfg(feature = "xq-audio")]
fn adjust_custom_sample_rate(
    sample_rate: Option<NonZeroU32>,
) -> (Option<NonZeroU32>, Option<NonZeroU32>) {
    (
        sample_rate,
        sample_rate.map(|sample_rate| {
            NonZeroU32::new((sample_rate.get() as f64 / audio::SAMPLE_RATE_ADJUSTMENT_RATIO) as u32)
                .unwrap_or(NonZeroU32::new(1).unwrap())
        }),
    )
}

impl CurrentConfig {
    fn from_global(global_config: &Config<config::Global>) -> Self {
        CurrentConfig {
            limit_framerate: config::GameOverridable::global(
                global_config.contents.limit_framerate,
            ),
            screen_rotation: config::GameOverridable::global(
                global_config.contents.screen_rotation,
            ),

            sync_to_audio: config::GameOverridable::global(global_config.contents.sync_to_audio),
            audio_volume: config::GameOverridable::global(global_config.contents.audio_volume),
            audio_sample_chunk_size: config::GameOverridable::global(
                global_config.contents.audio_sample_chunk_size,
            ),
            audio_interp_method: config::GameOverridable::global(
                global_config.contents.audio_interp_method,
            ),
            #[cfg(feature = "xq-audio")]
            audio_custom_sample_rate: config::GameOverridable::global(adjust_custom_sample_rate(
                NonZeroU32::new(global_config.contents.audio_custom_sample_rate),
            )),
            #[cfg(feature = "xq-audio")]
            audio_channel_interp_method: config::GameOverridable::global(
                global_config.contents.audio_channel_interp_method,
            ),
        }
    }
}

struct EmuState {
    playing: bool,
    game_config: Option<Config<config::Game>>,
    game_title: String,
    message_tx: crossbeam_channel::Sender<emu::Message>,
    thread: thread::JoinHandle<triple_buffer::Sender<FrameData>>,
    shared_state: Arc<emu::SharedState>,
}

impl EmuState {
    fn send_message(&self, msg: emu::Message) {
        self.message_tx.send(msg).expect("Couldn't send UI message");
    }
}

struct UiState {
    game_db: Option<game_db::Database>,

    global_config: Config<config::Global>,
    current_config: CurrentConfig,

    emu_state: Option<EmuState>,

    show_menu_bar: bool,
    screen_focused: bool,

    input: input::State,
    input_editor: Option<input::Editor>,

    audio_channel: Option<audio::Channel>,

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

    #[cfg(feature = "discord-presence")]
    rpc_connection: discord_rpc::Rpc,
    #[cfg(feature = "discord-presence")]
    presence: discord_rpc::Presence,
    #[cfg(feature = "discord-presence")]
    presence_updated: bool,
}

static ALLOWED_ROM_EXTENSIONS: &[&str] = &["nds", "bin"];

impl UiState {
    fn update_limit_framerate(&mut self) {
        if let Some(emu) = &self.emu_state {
            emu.shared_state
                .limit_framerate
                .store(self.current_config.limit_framerate.value, Ordering::Relaxed);
        }
    }

    fn update_screen_rotation(&mut self) {}

    fn update_sync_to_audio(&mut self) {
        if let Some(emu) = &self.emu_state {
            emu.send_message(emu::Message::UpdateAudioSync(
                self.current_config.sync_to_audio.value,
            ));
        }
    }

    fn update_audio_volume(&mut self) {
        if let Some(audio_channel) = &mut self.audio_channel {
            audio_channel
                .output_stream
                .set_volume(self.current_config.audio_volume.value);
        }
    }

    fn update_audio_sample_chunk_size(&mut self) {
        if let Some(emu) = &self.emu_state {
            emu.send_message(emu::Message::UpdateAudioSampleChunkSize(
                self.current_config.audio_sample_chunk_size.value,
            ));
        }
    }

    fn update_audio_interp_method(&mut self) {
        if let Some(audio_channel) = self.audio_channel.as_mut() {
            audio_channel.output_stream.set_interp(
                self.current_config
                    .audio_interp_method
                    .value
                    .create_interp(),
            );
        }
    }

    #[cfg(feature = "xq-audio")]
    fn update_audio_custom_sample_rate(&mut self) {
        if let Some(channel) = &mut self.audio_channel {
            channel.set_custom_sample_rate(self.current_config.audio_custom_sample_rate.value.1);
        }
        if let Some(emu) = &self.emu_state {
            emu.send_message(emu::Message::UpdateAudioCustomSampleRate(
                self.current_config.audio_custom_sample_rate.value.1,
            ));
        }
    }

    #[cfg(feature = "xq-audio")]
    fn update_audio_channel_interp_method(&mut self) {
        if let Some(emu) = &self.emu_state {
            emu.send_message(emu::Message::UpdateAudioChannelInterpMethod(
                self.current_config.audio_channel_interp_method.value,
            ));
        }
    }

    fn set_launch_config(&mut self, config: &CommonLaunchConfig) {
        update_setting!(
            self,
            limit_framerate,
            config.limit_framerate,
            Self::update_limit_framerate
        );
        update_setting!(
            self,
            screen_rotation,
            config.screen_rotation,
            Self::update_screen_rotation
        );

        update_setting!(
            self,
            sync_to_audio,
            config.sync_to_audio,
            Self::update_sync_to_audio
        );
        update_setting!(
            self,
            audio_volume,
            config.audio_volume,
            Self::update_audio_volume
        );
        update_setting!(
            self,
            audio_sample_chunk_size,
            config.audio_sample_chunk_size,
            Self::update_audio_sample_chunk_size
        );
        update_setting!(
            self,
            audio_interp_method,
            config.audio_interp_method,
            Self::update_audio_interp_method
        );

        #[cfg(feature = "xq-audio")]
        {
            update_setting!(
                self,
                audio_custom_sample_rate,
                config
                    .audio_custom_sample_rate
                    .map(adjust_custom_sample_rate),
                Self::update_audio_interp_method
            );
            update_setting!(
                self,
                audio_channel_interp_method,
                config.audio_channel_interp_method,
                Self::update_audio_channel_interp_method
            );
        }
    }
}

impl UiState {
    fn play_pause(&mut self) {
        if let Some(emu) = &mut self.emu_state {
            emu.playing = !emu.playing;
            emu.shared_state
                .playing
                .store(emu.playing, Ordering::Relaxed);
        }
    }

    fn reset(&mut self) {
        if let Some(emu) = &mut self.emu_state {
            emu.send_message(emu::Message::Reset);
        }
    }

    fn toggle_fullscreen_render(&mut self, value: bool) {
        self.global_config.contents.fullscreen_render = value;
        self.global_config.dirty = true;
        self.show_menu_bar = !self.global_config.contents.fullscreen_render;
    }

    fn toggle_audio_sync(&mut self, value: bool) {
        update_setting_value!(self, sync_to_audio, value, UiState::update_sync_to_audio);
    }

    fn toggle_framerate_limit(&mut self, value: bool) {
        update_setting_value!(
            self,
            limit_framerate,
            value,
            UiState::update_limit_framerate
        );
    }
}

impl UiState {
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
            Ok((launch_config, warnings)) => {
                if !warnings.is_empty() {
                    warning!("Firmware verification failed", "{}", format_list!(warnings));
                }
                self.start(
                    launch_config.common,
                    launch_config.cur_save_path,
                    game_title.to_string(),
                    Some(game_config),
                    Some(ds_slot_rom),
                );
            }
            Err(errors) => {
                config_error!(
                    "Couldn't determine final configuration for game: {}",
                    format_list!(errors)
                );
            }
        }
    }

    fn load_firmware(&mut self) {
        match config::firmware_launch_config(&self.global_config.contents) {
            Ok((launch_config, warnings)) => {
                if !warnings.is_empty() {
                    warning!("Firmware verification failed", "{}", format_list!(warnings));
                }
                self.start(launch_config, None, "Firmware".to_string(), None, None);
            }
            Err(errors) => {
                config_error!(
                    "Couldn't determine final configuration for firmware: {}",
                    format_list!(errors)
                );
            }
        }
    }

    fn start(
        &mut self,
        config: CommonLaunchConfig,
        cur_save_path: Option<PathBuf>,
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

        self.set_launch_config(&config);

        let playing = !config.pause_on_launch;

        #[cfg(feature = "log")]
        let logger = self.logger.clone();

        let ds_slot = ds_slot_rom.map(|rom| {
            let game_code = rom.read_le::<u32>(0xC);
            let save_type = self
                .game_db
                .as_ref()
                .and_then(|db| db.lookup(game_code))
                .map(|entry| {
                    if entry.rom_size as usize != rom.len() {
                        #[cfg(feature = "log")]
                        slog::error!(
                            logger,
                            "Unexpected ROM size: expected {} B, got {} B",
                            entry.rom_size,
                            rom.len()
                        );
                    }
                    entry.save_type
                });
            emu::DsSlot {
                rom,
                save_type,
                has_ir: game_code as u8 == b'I',
            }
        });

        let frame_tx = self.frame_tx.take().unwrap();

        let audio_tx_data = self
            .audio_channel
            .as_ref()
            .map(|audio_channel| audio_channel.tx_data.clone());

        #[cfg(feature = "gdb-server")]
        let gdb_server_addr = self.global_config.contents.gdb_server_addr;

        let (message_tx, message_rx) = crossbeam_channel::unbounded::<emu::Message>();

        let shared_state = Arc::new(emu::SharedState {
            playing: AtomicBool::new(playing),
            limit_framerate: AtomicBool::new(self.current_config.limit_framerate.value),
            autosave_interval: RwLock::new(Duration::from_secs_f32(
                config.autosave_interval_ms.value / 1000.0,
            )),
            stopped: AtomicBool::new(false),
            #[cfg(feature = "gdb-server")]
            gdb_server_active: AtomicBool::new(false),
        });
        let shared_state_ = Arc::clone(&shared_state);

        let thread = thread::Builder::new()
            .name("emulation".to_string())
            .spawn(move || {
                emu::main(
                    config,
                    cur_save_path,
                    ds_slot,
                    audio_tx_data,
                    frame_tx,
                    message_rx,
                    shared_state_,
                    #[cfg(feature = "gdb-server")]
                    gdb_server_addr,
                    #[cfg(feature = "log")]
                    logger,
                )
            })
            .expect("Couldn't spawn emulation thread");

        #[cfg(feature = "debug-views")]
        self.debug_views.reload_emu_state();

        self.emu_state = Some(EmuState {
            playing,
            game_config,
            game_title,
            message_tx,
            thread,
            shared_state,
        });
    }

    fn stop(&mut self) {
        if let Some(emu) = self.emu_state.take() {
            emu.shared_state.stopped.store(true, Ordering::Relaxed);
            self.frame_tx = Some(emu.thread.join().expect("Couldn't join emulation thread"));

            if let Some(mut game_config) = emu.game_config {
                if let Some(dir_path) = game_config.path.as_ref().and_then(|p| p.parent()) {
                    let _ = fs::create_dir_all(dir_path);
                }
                let _ = game_config.flush();
            }
        }

        self.current_config = CurrentConfig::from_global(&self.global_config);

        #[cfg(feature = "debug-views")]
        self.debug_views.clear_frame_data();

        triple_buffer::reset(
            (self.frame_tx.as_mut().unwrap(), &mut self.frame_rx),
            |frame_data| {
                for data in frame_data {
                    for fb in &mut data.fb.0 {
                        fb.fill(0);
                    }
                    data.fps = 0.0;
                    #[cfg(feature = "debug-views")]
                    data.debug.clear();
                }
            },
        );

        #[cfg(feature = "discord-presence")]
        {
            self.presence.state = Some("Not playing anything".to_string());
            self.presence.timestamps = Some(discord_rpc::Timestamps {
                start: Some(SystemTime::now()),
                end: None,
            });
            self.presence_updated = true;
        }
    }

    fn playing(&self) -> bool {
        match &self.emu_state {
            Some(emu) => emu.playing,
            None => false,
        }
    }

    fn set_touchscreen_bounds(
        &mut self,
        center: [f32; 2],
        points: &[[f32; 2]; 4],
        rot: f32,
        window: &window::Window,
    ) {
        fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
            let x = b[0] - a[0];
            let y = b[1] - a[1];
            (x * x + y * y).sqrt()
        }
        let screen_center = center.map(|v| v as f64 * window.scale_factor);
        let touchscreen_size = [
            distance(points[0], points[1]) as f64 * window.scale_factor,
            (distance(points[1], points[2]) * 0.5) as f64 * window.scale_factor,
        ];
        self.input.set_touchscreen_bounds(
            screen_center.into(),
            (
                screen_center[0],
                screen_center[1] + touchscreen_size[1] * 0.5,
            )
                .into(),
            touchscreen_size.into(),
            rot as f64,
        );
    }

    fn update_window_title(&self, window: &window::Window) {
        if !cfg!(target_os = "macos") || self.show_menu_bar {
            let mut buffer = "Dust - ".to_string();
            if let Some(emu) = &self.emu_state {
                buffer.push_str(&emu.game_title);
                if let Some(fps_fixed) = self.fps_fixed {
                    let _ = write!(buffer, " - {:.01} FPS", fps_fixed as f32 / 10.0);
                }
            } else {
                buffer.push_str("No game loaded");
            }
            window.window.set_title(&buffer);
        } else {
            window.window.set_title("");
        }
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

    let game_db = global_config
        .contents
        .game_db_path
        .as_deref()
        .and_then(|path| {
            let path_str = path.to_str();
            let location_str = || {
                if let Some(path_str) = path_str {
                    format!(" at `{}`", path_str)
                } else {
                    "".to_string()
                }
            };
            match game_db::Database::read_from_file(path) {
                Ok(db) => Some(db),
                Err(err) => {
                    match err {
                        game_db::Error::Io(err) => {
                            if err.kind() == io::ErrorKind::NotFound {
                                warning!(
                                    "Missing game database",
                                    "The game database was not found{}.",
                                    location_str()
                                );
                            } else {
                                config_error!(
                                    concat!("Couldn't read game database{}: {}",),
                                    location_str(),
                                    err,
                                );
                            }
                        }
                        game_db::Error::Json(err) => {
                            config_error!(
                                concat!("Couldn't load game database{}: {}",),
                                location_str(),
                                err,
                            );
                        }
                    }
                    None
                }
            }
        });

    #[cfg(feature = "log")]
    let mut imgui_log = None;
    #[cfg(feature = "log")]
    let logger = logging::init(
        &mut imgui_log,
        global_config.contents.logging_kind,
        global_config.contents.imgui_log_history_capacity,
    );

    let mut window_builder = futures_executor::block_on(window::Builder::new(
        "Dust",
        global_config.contents.window_size,
        global_config.contents.imgui_config_path.clone(),
        #[cfg(target_os = "macos")]
        global_config.contents.hide_macos_title_bar,
    ));

    let audio_channel = audio::channel(
        global_config.contents.audio_interp_method,
        global_config.contents.audio_volume,
        #[cfg(feature = "xq-audio")]
        adjust_custom_sample_rate(NonZeroU32::new(
            global_config.contents.audio_custom_sample_rate,
        ))
        .1,
    );

    let (frame_tx, frame_rx) = triple_buffer::init([
        FrameData::default(),
        FrameData::default(),
        FrameData::default(),
    ]);

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
        game_db,

        current_config: CurrentConfig::from_global(&global_config),
        global_config,

        emu_state: None,

        show_menu_bar: true,
        screen_focused: true,

        input: input::State::new(keymap),
        input_editor: None,

        audio_channel,

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

            if let Some(emu) = &mut state.emu_state {
                if emu.shared_state.stopped.load(Ordering::Relaxed) {
                    state.stop();
                    clear_fb_texture(state.fb_texture_id, window);
                } else if let Ok(frame) = state.frame_rx.get() {
                    #[cfg(feature = "debug-views")]
                    state
                        .debug_views
                        .update_from_frame_data(&frame.debug, window);

                    let fb_texture = window.gfx.imgui.texture_mut(state.fb_texture_id);
                    let data = unsafe {
                        slice::from_raw_parts(
                            frame.fb.0.as_ptr() as *const u8,
                            SCREEN_WIDTH * SCREEN_HEIGHT * 8,
                        )
                    };
                    fb_texture.set_data(
                        &window.gfx.device_state.queue,
                        data,
                        imgui_wgpu::TextureRange::default(),
                    );

                    let fps_fixed = (frame.fps * 10.0).round() as u64;
                    if Some(fps_fixed) != state.fps_fixed {
                        state.fps_fixed = Some(fps_fixed);
                    }
                }
            }

            let (input_actions, emu_input_changes) =
                state
                    .input
                    .drain_changes(if let Some(emu) = &state.emu_state {
                        emu.playing
                    } else {
                        false
                    });

            for action in input_actions {
                match action {
                    input::Action::PlayPause => state.play_pause(),
                    input::Action::Reset => state.reset(),
                    input::Action::Stop => {
                        state.stop();
                        clear_fb_texture(state.fb_texture_id, window);
                    }
                    input::Action::ToggleFullscreenRender => state
                        .toggle_fullscreen_render(!state.global_config.contents.fullscreen_render),
                    input::Action::ToggleAudioSync => {
                        state.toggle_audio_sync(!state.current_config.sync_to_audio.value)
                    }
                    input::Action::ToggleFramerateLimit => {
                        state.toggle_framerate_limit(!state.current_config.limit_framerate.value)
                    }
                }
            }

            if let Some(changes) = emu_input_changes {
                if let Some(emu) = &mut state.emu_state {
                    if emu.playing {
                        emu.send_message(emu::Message::UpdateInput(changes));
                    }
                }
            }

            if state.global_config.contents.fullscreen_render
                && ui.is_key_pressed(imgui::Key::Escape)
                && !ui.is_any_item_focused()
            {
                state.show_menu_bar = !state.show_menu_bar;
            }

            if state.show_menu_bar {
                #[cfg(target_os = "macos")]
                let frame_padding = if state.global_config.contents.hide_macos_title_bar {
                    Some(ui.push_style_var(imgui::StyleVar::FramePadding([
                        0.0,
                        0.5 * (window.macos_title_bar_height - ui.text_line_height()),
                    ])))
                } else {
                    None
                };

                ui.main_menu_bar(|| {
                    #[cfg(target_os = "macos")]
                    {
                        drop(frame_padding);
                        if state.global_config.contents.hide_macos_title_bar {
                            let _item_spacing =
                                ui.push_style_var(imgui::StyleVar::ItemSpacing([0.0; 2]));
                            // TODO: There has to be some way to compute this width instead of
                            //       hardcoding it.
                            ui.dummy([68.0, 0.0]);
                            ui.same_line();
                        }
                    }

                    ui.menu("Emulation", || {
                        if ui
                            .menu_item_config(if state.playing() { "Pause" } else { "Play" })
                            .enabled(state.emu_state.is_some())
                            .build()
                        {
                            state.play_pause();
                        }

                        if ui
                            .menu_item_config("Reset")
                            .enabled(state.emu_state.is_some())
                            .build()
                        {
                            state.reset();
                        }

                        if ui
                            .menu_item_config("Stop")
                            .enabled(state.emu_state.is_some())
                            .build()
                        {
                            state.stop();
                            clear_fb_texture(state.fb_texture_id, window);
                        }

                        if ui.menu_item("Load game...") {
                            if let Some(path) = FileDialog::new()
                                .add_filter("NDS ROM file", ALLOWED_ROM_EXTENSIONS)
                                .pick_file()
                            {
                                state.load_from_rom_path(&path);
                            }
                        }

                        if ui.menu_item("Load firmware") {
                            state.load_firmware();
                        }
                    });

                    ui.menu("Config", || {
                        ui.menu("Audio volume", || {
                            let mut volume = state.current_config.audio_volume.value * 100.0;
                            if ui
                                .slider_config("##audio_volume", 0.0, 100.0)
                                .display_format("%.02f%%")
                                .build(&mut volume)
                            {
                                let volume = (volume * 100.0).round().clamp(0.0, 10000.0) / 10000.0;
                                update_setting_value!(
                                    state,
                                    audio_volume,
                                    volume,
                                    UiState::update_audio_volume
                                );
                            }
                        });

                        ui.menu("Audio sample chunk size", || {
                            let mut audio_sample_chunk_size =
                                state.current_config.audio_sample_chunk_size.value;
                            if ui
                                .input_scalar(
                                    "##audio_sample_chunk_size",
                                    &mut audio_sample_chunk_size,
                                )
                                .enter_returns_true(true)
                                .build()
                            {
                                update_setting_value!(
                                    state,
                                    audio_sample_chunk_size,
                                    audio_sample_chunk_size,
                                    UiState::update_audio_sample_chunk_size
                                );
                            }
                        });

                        #[cfg(feature = "xq-audio")]
                        ui.menu("Core audio interpolation", || {
                            let mut audio_custom_sample_rate_enabled = state
                                .current_config
                                .audio_custom_sample_rate
                                .value
                                .0
                                .is_some();
                            let mut raw_audio_custom_sample_rate =
                                match state.current_config.audio_custom_sample_rate.value.0 {
                                    Some(value) => value.get(),
                                    None => 0,
                                };
                            let mut audio_custom_sample_rate_changed = false;
                            if ui.checkbox(
                                "Custom sample rate",
                                &mut audio_custom_sample_rate_enabled,
                            ) {
                                audio_custom_sample_rate_changed = true;
                                raw_audio_custom_sample_rate = if audio_custom_sample_rate_enabled {
                                    (audio::DEFAULT_INPUT_SAMPLE_RATE as f64
                                        * audio::SAMPLE_RATE_ADJUSTMENT_RATIO)
                                        .round() as u32
                                } else {
                                    0
                                };
                            }

                            if audio_custom_sample_rate_enabled {
                                audio_custom_sample_rate_changed |= ui
                                    .slider_config("Sample rate", 32768, 131072)
                                    .display_format("%d Hz")
                                    .build(&mut raw_audio_custom_sample_rate)
                            }

                            if audio_custom_sample_rate_changed {
                                let audio_custom_sample_rate = adjust_custom_sample_rate(
                                    NonZeroU32::new(raw_audio_custom_sample_rate),
                                );
                                update_setting_value!(
                                    state,
                                    audio_custom_sample_rate,
                                    audio_custom_sample_rate,
                                    UiState::update_audio_custom_sample_rate,
                                    |_value| raw_audio_custom_sample_rate
                                );
                            }

                            static INTERP_METHODS: [AudioChannelInterpMethod; 2] = [
                                AudioChannelInterpMethod::Nearest,
                                AudioChannelInterpMethod::Cubic,
                            ];
                            let mut i = INTERP_METHODS
                                .iter()
                                .position(|&m| {
                                    m == state.current_config.audio_channel_interp_method.value
                                })
                                .unwrap();
                            if ui.combo(
                                "Interpolation method",
                                &mut i,
                                &INTERP_METHODS,
                                |interp_method| {
                                    match interp_method {
                                        AudioChannelInterpMethod::Nearest => "Nearest",
                                        AudioChannelInterpMethod::Cubic => "Cubic",
                                    }
                                    .into()
                                },
                            ) {
                                let audio_channel_interp_method = INTERP_METHODS[i];
                                update_setting_value!(
                                    state,
                                    audio_channel_interp_method,
                                    audio_channel_interp_method,
                                    UiState::update_audio_channel_interp_method
                                );
                            }
                        });

                        ui.menu("Frontend audio interpolation method", || {
                            static INTERP_METHODS: [audio::InterpMethod; 2] =
                                [audio::InterpMethod::Nearest, audio::InterpMethod::Cubic];
                            let mut i = INTERP_METHODS
                                .iter()
                                .position(|&m| m == state.current_config.audio_interp_method.value)
                                .unwrap();
                            if ui.combo(
                                "##audio_interp_method",
                                &mut i,
                                &INTERP_METHODS,
                                |interp_method| {
                                    match interp_method {
                                        audio::InterpMethod::Nearest => "Nearest",
                                        audio::InterpMethod::Cubic => "Cubic",
                                    }
                                    .into()
                                },
                            ) {
                                let audio_interp_method = INTERP_METHODS[i];
                                update_setting_value!(
                                    state,
                                    audio_interp_method,
                                    audio_interp_method,
                                    UiState::update_audio_interp_method
                                );
                            }
                        });

                        let mut limit_framerate = state.current_config.limit_framerate.value;
                        if ui
                            .menu_item_config("Limit framerate")
                            .build_with_ref(&mut limit_framerate)
                        {
                            state.toggle_framerate_limit(limit_framerate);
                        }

                        ui.menu("Screen rotation", || {
                            let mut screen_rotation =
                                state.current_config.screen_rotation.value as i32;
                            if ui
                                .input_int("##screen_rot", &mut screen_rotation)
                                .step(1)
                                .build()
                            {
                                screen_rotation = screen_rotation.clamp(0, 359);
                            }
                            macro_rules! buttons {
                                ($($value: expr),*) => {
                                    $(
                                        if ui.button(stringify!($value)) {
                                            screen_rotation = $value;
                                        }
                                        ui.same_line();
                                    )*
                                    ui.new_line();
                                };
                            }
                            buttons!(0, 90, 180, 270);
                            if screen_rotation != state.current_config.screen_rotation.value as i32
                            {
                                update_setting_value!(
                                    state,
                                    screen_rotation,
                                    screen_rotation as i16,
                                    UiState::update_screen_rotation
                                );
                            }
                        });

                        let mut sync_to_audio = state.current_config.sync_to_audio.value;
                        if ui
                            .menu_item_config("Sync to audio")
                            .build_with_ref(&mut sync_to_audio)
                        {
                            state.toggle_audio_sync(sync_to_audio);
                        }

                        if ui
                            .menu_item_config("Fullscreen render")
                            .build_with_ref(&mut state.global_config.contents.fullscreen_render)
                        {
                            state.toggle_fullscreen_render(
                                state.global_config.contents.fullscreen_render,
                            );
                        }

                        state.global_config.dirty |= ui
                            .menu_item_config("Limit screen scale to integers")
                            .build_with_ref(&mut state.global_config.contents.screen_integer_scale);

                        let mut show_input = state.input_editor.is_some();
                        if ui.menu_item_config("Input").build_with_ref(&mut show_input) {
                            state.input_editor = if show_input {
                                Some(input::Editor::new())
                            } else {
                                None
                            };
                        }

                        #[cfg(target_os = "macos")]
                        if ui
                            .menu_item_config("Hide title bar")
                            .build_with_ref(&mut state.global_config.contents.hide_macos_title_bar)
                        {
                            state.global_config.dirty = true;
                            window.set_macos_titlebar_hidden(
                                state.global_config.contents.hide_macos_title_bar,
                            );
                        }
                    });

                    #[cfg(feature = "log")]
                    let imgui_log_enabled = state.imgui_log.is_some();
                    #[cfg(not(feature = "log"))]
                    let imgui_log_enabled = false;
                    if cfg!(any(feature = "debug-views", feature = "gdb-server"))
                        || imgui_log_enabled
                    {
                        #[allow(unused_assignments)]
                        ui.menu("Debug", || {
                            #[allow(unused_mut, unused_variables)]
                            let mut separator_needed = false;

                            #[cfg(feature = "log")]
                            if let Some((_, _, console_visible)) = &mut state.imgui_log {
                                ui.menu_item_config("Log").build_with_ref(console_visible);
                                separator_needed = true;
                            }
                            #[cfg(feature = "gdb-server")]
                            {
                                if separator_needed {
                                    ui.separator();
                                }
                                let gdb_server_active = match &state.emu_state {
                                    Some(emu) => {
                                        emu.shared_state.gdb_server_active.load(Ordering::Relaxed)
                                    }
                                    None => false,
                                };
                                if ui
                                    .menu_item_config(if gdb_server_active {
                                        "Stop GDB server"
                                    } else {
                                        "Start GDB server"
                                    })
                                    .enabled(state.emu_state.is_some())
                                    .build()
                                {
                                    if let Some(emu) = &state.emu_state {
                                        emu.shared_state
                                            .gdb_server_active
                                            .store(!gdb_server_active, Ordering::Relaxed);
                                    }
                                }
                                separator_needed = true;
                            }
                            #[cfg(feature = "debug-views")]
                            {
                                if separator_needed {
                                    ui.separator();
                                }
                                state.debug_views.render_menu(ui, window);
                            }
                        });
                    }

                    #[cfg(feature = "gdb-server")]
                    if let Some(emu) = &state.emu_state {
                        if emu.shared_state.gdb_server_active.load(Ordering::Relaxed) {
                            let text =
                                format!("GDB: {}", state.global_config.contents.gdb_server_addr);
                            let width =
                                ui.calc_text_size(&text)[0] + unsafe { ui.style().item_spacing[0] };
                            ui.set_cursor_pos([
                                ui.content_region_max()[0] - width,
                                ui.cursor_pos()[1],
                            ]);
                            ui.separator();
                            ui.text(&text);
                        }
                    }
                });
            }

            #[cfg(feature = "log")]
            if let Some((console, _, console_visible)) = &mut state.imgui_log {
                console.process_messages();
                if *console_visible {
                    let _window_padding =
                        ui.push_style_var(imgui::StyleVar::WindowPadding([6.0; 2]));
                    let _item_spacing = ui.push_style_var(imgui::StyleVar::ItemSpacing([0.0; 2]));
                    console.render_window(ui, Some(window.mono_font), console_visible);
                }
            }

            #[cfg(feature = "debug-views")]
            for message in state
                .debug_views
                .render(ui, window, state.emu_state.is_some())
            {
                if let Some(emu) = &state.emu_state {
                    emu.send_message(emu::Message::DebugViews(message));
                }
            }

            if let Some(input_editor) = &mut state.input_editor {
                let mut opened = true;
                input_editor.draw(ui, &mut state.input, &mut opened);
                if !opened {
                    state.input_editor = None;
                }
            }

            let window_size = window.window.inner_size();
            let screen_rot = (state.current_config.screen_rotation.value as f32).to_radians();
            if state.global_config.contents.fullscreen_render {
                let (center, points) = scale_to_fit_rotated(
                    [SCREEN_WIDTH as f32, (2 * SCREEN_HEIGHT) as f32],
                    state.global_config.contents.screen_integer_scale,
                    screen_rot,
                    [
                        (window_size.width as f64 / window.scale_factor) as f32,
                        (window_size.height as f64 / window.scale_factor) as f32,
                    ],
                );
                ui.get_background_draw_list()
                    .add_image_quad(
                        state.fb_texture_id,
                        points[0],
                        points[1],
                        points[2],
                        points[3],
                    )
                    .build();
                state.screen_focused =
                    !ui.is_window_focused_with_flags(imgui::WindowFocusedFlags::ANY_WINDOW);
                state.set_touchscreen_bounds(center, &points, screen_rot, window);
            } else {
                let _window_padding = ui.push_style_var(imgui::StyleVar::WindowPadding([0.0; 2]));
                let titlebar_height =
                    unsafe { ui.style().frame_padding[1] } * 2.0 + ui.current_font_size();
                const DEFAULT_SCALE: f32 = 2.0;
                state.screen_focused = false;
                ui.window("Screen")
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
                    .build(|| {
                        let (center, points) = scale_to_fit_rotated(
                            [SCREEN_WIDTH as f32, (2 * SCREEN_HEIGHT) as f32],
                            state.global_config.contents.screen_integer_scale,
                            screen_rot,
                            ui.content_region_avail(),
                        );
                        let mut min = [f32::INFINITY; 2];
                        for point in &points {
                            min[0] = min[0].min(point[0]);
                            min[1] = min[1].min(point[1]);
                        }
                        ui.dummy([0, 1].map(|i| {
                            (points[0][i] - points[2][i])
                                .abs()
                                .max((points[1][i] - points[3][i]).abs())
                        }));
                        let window_pos = ui.window_pos();
                        let content_region_min = ui.window_content_region_min();
                        let upper_left = [
                            window_pos[0] + content_region_min[0],
                            window_pos[1] + content_region_min[1],
                        ];
                        let abs_points =
                            points.map(|[x, y]| [x + upper_left[0], y + upper_left[1]]);
                        ui.get_window_draw_list()
                            .add_image_quad(
                                state.fb_texture_id,
                                abs_points[0],
                                abs_points[1],
                                abs_points[2],
                                abs_points[3],
                            )
                            .build();
                        state.screen_focused = ui.is_window_focused();
                        state.set_touchscreen_bounds(
                            [center[0] + upper_left[0], center[1] + upper_left[1]],
                            &abs_points,
                            screen_rot,
                            window,
                        );
                    });
            }

            state.update_window_title(window);

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
