#[cfg(feature = "dldi")]
mod dldi;
pub mod ds_slot_rom;
#[cfg(feature = "gdb-server")]
mod gdb_server;
mod rtc;
pub mod soft_renderer_3d;

#[cfg(feature = "debug-views")]
use super::debug_views;
use crate::{audio, config::SysFiles, game_db::SaveType, input, FrameData};
use ds_slot_rom::DsSlotRom;
#[cfg(feature = "xq-audio")]
use dust_core::audio::{Audio, ChannelInterpMethod as AudioChannelInterpMethod};
use dust_core::{
    audio::DummyBackend as DummyAudioBackend,
    cpu::{self, interpreter::Interpreter},
    ds_slot,
    emu::{self, RunOutput},
    flash::Flash,
    gpu::{engine_2d, engine_3d, Framebuffer},
    spi::{self, firmware},
    utils::{
        BoxedByteSlice, PersistentReadSavestate, PersistentWriteSavestate, ReadSavestate,
        WriteSavestate,
    },
    Model, SaveContents, SaveReloadContents,
};
use emu_utils::triple_buffer;
#[cfg(feature = "gdb-server")]
use std::net::SocketAddr;
#[cfg(feature = "xq-audio")]
use std::num::NonZeroU32;
use std::{
    fs::{self, File},
    hint,
    io::{self, Read},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

pub struct SharedState {
    // UI to emu
    pub playing: AtomicBool,

    // Emu to UI
    #[cfg(feature = "gdb-server")]
    pub gdb_server_active: AtomicBool,
}

pub struct SavePathUpdate {
    pub new: Option<PathBuf>,
    pub new_prev: Option<Option<PathBuf>>,
    pub reload: bool,
    pub reset: bool,
}

pub struct Savestate {
    pub contents: Vec<u8>,
    pub save: Option<BoxedByteSlice>,
    pub framebuffer: Box<Framebuffer>,
}

pub enum Message {
    UpdateInput(input::Changes),
    #[cfg(feature = "debug-views")]
    DebugViews(debug_views::Message),
    Reset,
    Stop,

    CreateSavestate {
        name: String,
        include_save: bool,
    },
    ApplySavestate(Savestate),

    UpdateSavePath(SavePathUpdate),
    UpdateSaveIntervalMs(f32),

    UpdateRtcTimeOffsetSeconds(i64),

    UpdateRenderers {
        renderer_2d_is_accel: bool,
        renderer_2d: Box<dyn engine_2d::Renderer + Send>,
        renderer_3d_tx: Box<dyn engine_3d::RendererTx + Send>,
    },

    UpdateFramerateLimit(Option<f32>),
    UpdatePausedFramerateLimit(f32),

    UpdateSyncToAudio(bool),
    UpdateAudioSampleChunkSize(u16),
    #[cfg(feature = "xq-audio")]
    UpdateAudioCustomSampleRate(Option<NonZeroU32>),
    #[cfg(feature = "xq-audio")]
    UpdateAudioChannelInterpMethod(AudioChannelInterpMethod),

    ToggleAudioInput(Option<audio::input::Receiver>),

    #[cfg(feature = "logging")]
    UpdateLogger(slog::Logger),

    #[cfg(feature = "gdb-server")]
    ToggleGdbServer(Option<SocketAddr>),
}

pub enum Notification {
    Stopped,
    #[cfg(feature = "debug-views")]
    DebugViews(debug_views::Notification),

    RtcTimeOffsetSecondsUpdated(i64),
    SavestateCreated(String, Savestate),
    SavestateFailed(String),
}

#[cfg(feature = "debug-views")]
impl debug_views::Messages for &crossbeam_channel::Sender<Message> {
    fn push(&mut self, notif: debug_views::Message) {
        self.send(Message::DebugViews(notif))
            .expect("couldn't send message to emulation thread");
    }
}

#[cfg(feature = "debug-views")]
impl debug_views::Notifications for &crossbeam_channel::Sender<Notification> {
    fn push(&mut self, notif: debug_views::Notification) {
        self.send(Notification::DebugViews(notif))
            .expect("couldn't send notification to UI thread");
    }
}

pub struct DsSlot {
    pub rom: DsSlotRom,
    pub save_type: Option<SaveType>,
    pub has_ir: bool,
}

#[cfg(feature = "dldi")]
pub struct Dldi {
    pub root_path: PathBuf,
    pub skip_path: PathBuf,
}

fn read_save_file_contents(save_path: &PathBuf) -> io::Result<Option<BoxedByteSlice>> {
    let mut save_file = match File::open(save_path) {
        Ok(save_file) => save_file,
        Err(err) => match err.kind() {
            io::ErrorKind::NotFound => return Ok(None),
            _ => return Err(err),
        },
    };
    let save_len = save_file.metadata()?.len() as usize;
    let mut save_contents = BoxedByteSlice::new_zeroed(save_len.next_power_of_two());
    save_file.read_exact(&mut save_contents[..save_len])?;
    Ok(Some(save_contents))
}

fn setup_ds_slot(
    ds_slot: Option<DsSlot>,
    save_path: &Option<PathBuf>,
    #[cfg(feature = "log")] logger: &slog::Logger,
) -> (Option<Box<dyn ds_slot::rom::Contents>>, ds_slot::spi::Spi) {
    if let Some(ds_slot) = ds_slot {
        let rom: Box<dyn ds_slot::rom::Contents> = ds_slot.rom.into();

        let save_contents = if let Some(save_path) = save_path {
            read_save_file_contents(save_path).unwrap_or_else(|err| {
                error!("Save file error", "Couldn't read save file: {err}");
                None
            })
        } else {
            None
        };

        let save_type = if let Some(save_contents) = &save_contents {
            if let Some(save_type) = ds_slot.save_type {
                let expected_len = save_type.expected_len();
                if expected_len != Some(save_contents.len()) {
                    let (chosen_save_type, chosen) = if let Some(detected_save_type) =
                        SaveType::from_save_len(save_contents.len())
                    {
                        (detected_save_type, "existing save file")
                    } else {
                        (save_type, "database entry")
                    };
                    warning!(
                        "Save file size mismatch",
                        "Unexpected save file size: expected {}, got {} B; respecting {chosen}.",
                        if let Some(expected_len) = expected_len {
                            format!("{expected_len} B")
                        } else {
                            "no file".to_owned()
                        },
                        save_contents.len(),
                    );
                    chosen_save_type
                } else {
                    save_type
                }
            } else {
                SaveType::from_save_len(save_contents.len()).unwrap_or_else(|| {
                    error!(
                        "Unrecognized save type",
                        "Unrecognized save file size ({} B) and no database entry found, \
                         defaulting to an empty save.",
                        save_contents.len()
                    );
                    SaveType::None
                })
            }
        } else {
            ds_slot.save_type.unwrap_or_else(|| {
                error!(
                    "Unknown save type",
                    "No existing save file present and no database entry found, defaulting to an \
                     empty save.",
                );
                SaveType::None
            })
        };

        let spi = if save_type == SaveType::None {
            ds_slot::spi::Empty::new(
                #[cfg(feature = "log")]
                logger.new(slog::o!("ds_spi" => "empty")),
            )
            .into()
        } else {
            let expected_len = save_type.expected_len().unwrap();
            let save_contents = match save_contents {
                Some(save_contents) => {
                    SaveContents::Existing(if save_contents.len() != expected_len {
                        let mut new_contents = BoxedByteSlice::new_zeroed(expected_len);
                        let copy_len = save_contents.len().min(expected_len);
                        new_contents[..copy_len].copy_from_slice(&save_contents[..copy_len]);
                        new_contents
                    } else {
                        save_contents
                    })
                }
                None => SaveContents::New(expected_len),
            };
            match save_type {
                SaveType::None => unreachable!(),
                SaveType::Eeprom4k => ds_slot::spi::eeprom_4k::Eeprom4k::new(
                    save_contents,
                    None,
                    #[cfg(feature = "log")]
                    logger.new(slog::o!("ds_spi" => "eeprom_4k")),
                )
                // NOTE: The save contents' size is ensured beforehand, this should never occur.
                .expect("couldn't create 4 Kib EEPROM DS slot SPI device")
                .into(),
                SaveType::EepromFram64k | SaveType::EepromFram512k | SaveType::EepromFram1m => {
                    ds_slot::spi::eeprom_fram::EepromFram::new(
                        save_contents,
                        None,
                        #[cfg(feature = "log")]
                        logger.new(slog::o!("ds_spi" => "eeprom_fram")),
                    )
                    // NOTE: The save contents' size is ensured beforehand, this should never occur.
                    .expect("couldn't create EEPROM/FRAM DS slot SPI device")
                    .into()
                }
                SaveType::Flash2m | SaveType::Flash4m | SaveType::Flash8m => {
                    ds_slot::spi::flash::Flash::new(
                        save_contents,
                        [0; 20],
                        ds_slot.has_ir,
                        #[cfg(feature = "log")]
                        logger.new(
                            slog::o!("ds_spi" => if ds_slot.has_ir { "flash" } else { "flash_ir" }),
                        ),
                    )
                    // NOTE: The save contents' size is ensured beforehand, this should never occur.
                    .expect("couldn't create FLASH DS slot SPI device")
                    .into()
                }
                SaveType::Nand64m | SaveType::Nand128m | SaveType::Nand256m => {
                    error!(
                        "Save file unsupported",
                        "TODO: NAND saves are currently unsupported, falling back to no save file.",
                    );
                    ds_slot::spi::Empty::new(
                        #[cfg(feature = "log")]
                        logger.new(slog::o!("ds_spi" => "nand_todo")),
                    )
                    .into()
                }
            }
        };

        (Some(rom), spi)
    } else {
        (
            None,
            ds_slot::spi::Empty::new(
                #[cfg(feature = "log")]
                logger.new(slog::o!("ds_spi" => "empty")),
            )
            .into(),
        )
    }
}

fn build_emu<E: cpu::Engine>(emu_builder: emu::Builder, engine: E) -> Option<emu::Emu<E>> {
    match emu_builder.build(engine) {
        Ok(emu) => Some(emu),
        Err(err) => match err {
            emu::BuildError::MissingRom => unreachable!("Missing DS slot ROM"),
            emu::BuildError::MissingSysFiles => unreachable!("Missing emulator system files"),
            emu::BuildError::RomCreation(err) => match err {
                ds_slot::rom::normal::CreationError::InvalidSize => {
                    unreachable!("Invalid DS slot ROM file size")
                }
            },
            emu::BuildError::RomNeedsDecryptionButNoBiosProvided => {
                error!(
                    "Emulator error",
                    "Couldn't start emulator: ROM needs decryption but no BIOS provided."
                );
                None
            }
        },
    }
}

pub struct LaunchData {
    pub sys_files: SysFiles,
    pub ds_slot: Option<DsSlot>,
    #[cfg(feature = "dldi")]
    pub dldi: Option<Dldi>,

    pub model: Model,
    pub skip_firmware: bool,

    pub save_path: Option<PathBuf>,
    pub save_interval_ms: f32,

    pub shared_state: Arc<SharedState>,
    pub from_ui: crossbeam_channel::Receiver<Message>,
    pub to_ui: crossbeam_channel::Sender<Notification>,

    pub audio_tx_data: Option<audio::output::SenderData>,
    pub mic_rx: Option<audio::input::Receiver>,
    pub frame_tx: triple_buffer::Sender<FrameData>,

    pub framerate_ratio_limit: Option<f32>,
    pub paused_framerate_limit: f32,

    pub sync_to_audio: bool,
    pub audio_sample_chunk_size: u16,
    #[cfg(feature = "xq-audio")]
    pub audio_custom_sample_rate: Option<NonZeroU32>,
    #[cfg(feature = "xq-audio")]
    pub audio_channel_interp_method: AudioChannelInterpMethod,

    pub rtc_time_offset_seconds: i64,

    pub renderer_2d_is_accel: bool,
    pub renderer_2d: Box<dyn engine_2d::Renderer + Send>,
    pub renderer_3d_tx: Box<dyn engine_3d::RendererTx + Send>,

    #[cfg(feature = "logging")]
    pub logger: slog::Logger,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn run(
    LaunchData {
        sys_files,
        ds_slot,
        #[cfg(feature = "dldi")]
        dldi,

        model,
        skip_firmware,

        mut save_path,
        save_interval_ms,

        shared_state,
        from_ui,
        to_ui,

        audio_tx_data,
        mic_rx,
        mut frame_tx,

        framerate_ratio_limit,
        paused_framerate_limit,

        mut sync_to_audio,
        audio_sample_chunk_size,
        #[cfg(feature = "xq-audio")]
        audio_custom_sample_rate,
        #[cfg(feature = "xq-audio")]
        audio_channel_interp_method,

        mut rtc_time_offset_seconds,

        mut renderer_2d_is_accel,
        renderer_2d,
        renderer_3d_tx,

        #[cfg(feature = "logging")]
        logger,
    }: LaunchData,
) -> triple_buffer::Sender<FrameData> {
    macro_rules! notif {
        ($value: expr) => {
            to_ui
                .send($value)
                .expect("couldn't send notification to UI thread");
        };
    }

    let firmware_flash = Flash::new(
        SaveContents::Existing(
            sys_files
                .firmware
                .unwrap_or_else(|| firmware::default(model)),
        ),
        firmware::id_for_model(model),
        #[cfg(feature = "log")]
        logger.new(slog::o!("fw" => "")),
    )
    // NOTE: The firmware's size is checked before launch, this should never occur.
    .expect("couldn't build firmware");

    let (ds_slot_rom, ds_slot_spi) = setup_ds_slot(
        ds_slot,
        &save_path,
        #[cfg(feature = "log")]
        &logger,
    );

    let mut emu_builder = emu::Builder::new(
        firmware_flash,
        ds_slot_rom,
        ds_slot_spi,
        match &audio_tx_data {
            Some(data) => Box::new(audio::output::Sender::new(data, sync_to_audio)),
            None => Box::new(DummyAudioBackend),
        },
        mic_rx.map(|mic_rx| Box::new(mic_rx) as Box<dyn spi::tsc::MicBackend>),
        Box::new(rtc::Backend::new(rtc_time_offset_seconds)),
        renderer_2d,
        renderer_3d_tx,
        #[cfg(feature = "dldi")]
        dldi.map(|dldi| {
            Box::new(dldi::FsProvider::new(dldi.root_path, dldi.skip_path))
                as Box<dyn dust_core::dldi::Provider>
        }),
        #[cfg(not(feature = "dldi"))]
        None,
        #[cfg(feature = "log")]
        logger.clone(),
    );

    emu_builder.arm7_bios.clone_from(&sys_files.arm7_bios);
    emu_builder.arm9_bios.clone_from(&sys_files.arm9_bios);

    emu_builder.model = model;
    emu_builder.direct_boot = skip_firmware;
    // TODO: Set batch_duration and first_launch?
    emu_builder.audio_sample_chunk_size = audio_sample_chunk_size;
    #[cfg(feature = "xq-audio")]
    {
        emu_builder.audio_custom_sample_rate = audio_custom_sample_rate;
        emu_builder.audio_channel_interp_method = audio_channel_interp_method;
    }

    let Some(mut emu) = build_emu(emu_builder, Interpreter) else {
        return frame_tx;
    };

    const FRAME_BASE_INTERVAL: Duration = Duration::from_nanos(1_000_000_000 / 60);
    let mut frame_interval = framerate_ratio_limit.map(|value| FRAME_BASE_INTERVAL.div_f32(value));
    let mut paused_frame_interval = Duration::SECOND.div_f32(paused_framerate_limit);
    let mut last_frame_time = Instant::now();

    const FPS_CALC_INTERVAL: Duration = Duration::from_secs(1);
    let mut frames_since_last_fps_calc = 0;
    let mut last_fps_calc_time = last_frame_time;
    let mut fps = 0.0;

    let mut save_interval = Duration::from_secs_f32(save_interval_ms);
    let mut last_save_flush_time = last_frame_time;

    #[cfg(feature = "debug-views")]
    let mut debug_views = debug_views::EmuState::new();

    #[cfg(feature = "gdb-server")]
    let mut gdb_server = None;

    macro_rules! save {
        () => {
            if let Some(save_path) = &save_path {
                if emu.ds_slot.spi.contents_dirty()
                    && save_path
                        .parent()
                        .map(|parent| fs::create_dir_all(parent).is_ok())
                        .unwrap_or(true)
                    && fs::write(save_path, emu.ds_slot.spi.contents()).is_ok()
                {
                    emu.ds_slot.spi.mark_contents_flushed();
                }
            }
        };
    }

    'run_loop: loop {
        let mut reset_triggered = false;

        for message in from_ui.try_iter() {
            match message {
                Message::UpdateInput(changes) => {
                    emu.press_keys(changes.pressed);
                    emu.release_keys(changes.released);
                    if let Some(new_touch_pos) = changes.touch_pos {
                        if let Some(touch_pos) = new_touch_pos {
                            emu.set_touch_pos(touch_pos);
                        } else {
                            emu.end_touch();
                        }
                    }
                }

                #[cfg(feature = "debug-views")]
                Message::DebugViews(message) => {
                    debug_views.handle_message(&mut emu, message, &to_ui);
                }

                Message::Reset => {
                    reset_triggered = true;
                }

                Message::Stop => {
                    break 'run_loop;
                }

                Message::CreateSavestate { name, include_save } => {
                    let mut contents = Vec::new();
                    if PersistentWriteSavestate::new(&mut contents)
                        .store(&mut emu)
                        .is_ok()
                    {
                        notif!(Notification::SavestateCreated(
                            name,
                            Savestate {
                                contents,
                                save: if include_save {
                                    let spi_contents = emu.ds_slot.spi.contents();
                                    let mut save = BoxedByteSlice::new_zeroed(spi_contents.len());
                                    save.copy_from_slice(spi_contents);
                                    Some(save)
                                } else {
                                    None
                                },
                                framebuffer: unsafe {
                                    if renderer_2d_is_accel {
                                        // TODO: Capture the framebuffer on the UI thread
                                        Box::new_zeroed().assume_init()
                                    } else {
                                        let mut framebuffer = Box::<Framebuffer>::new_uninit();
                                        framebuffer.as_mut_ptr().copy_from_nonoverlapping(
                                            emu.gpu.renderer_2d().framebuffer(),
                                            1,
                                        );
                                        framebuffer.assume_init()
                                    }
                                },
                            }
                        ));
                    } else {
                        notif!(Notification::SavestateFailed(name));
                    }
                }

                Message::ApplySavestate(savestate) => {
                    if PersistentReadSavestate::new(&savestate.contents)
                        .and_then(|mut savestate| savestate.load_into(&mut emu).map_err(drop))
                        .is_ok()
                    {
                        if let Some(save) = savestate.save {
                            emu.ds_slot
                                .spi
                                .reload_contents(SaveReloadContents::Existing(save));
                        }
                    }
                }

                Message::UpdateSavePath(SavePathUpdate {
                    new,
                    new_prev,
                    reload,
                    reset,
                }) => {
                    save!();
                    last_save_flush_time = Instant::now();

                    if let Some((prev, new_prev)) = save_path.as_ref().zip(new_prev) {
                        if let Some(new_prev) = new_prev {
                            if new_prev != *prev {
                                let _ = fs::rename(prev, new_prev);
                            }
                        } else {
                            let _ = fs::remove_file(prev);
                        }
                    }
                    save_path = new;

                    if reload {
                        if let Some(save_path) = save_path.as_ref() {
                            let save_contents = match read_save_file_contents(save_path) {
                                Ok(contents) => match contents {
                                    Some(contents) => SaveReloadContents::Existing(contents),
                                    None => SaveReloadContents::New,
                                },
                                Err(err) => {
                                    error!("Save file error", "Couldn't read save file: {err}");
                                    SaveReloadContents::New
                                }
                            };
                            emu.ds_slot.spi.reload_contents(save_contents);
                        }
                    }

                    if reset {
                        reset_triggered = true;
                    }
                }

                Message::UpdateSaveIntervalMs(value) => {
                    save_interval = Duration::from_secs_f32(value);
                }

                Message::UpdateRtcTimeOffsetSeconds(value) => {
                    rtc_time_offset_seconds = value;
                    emu.rtc
                        .backend
                        .as_any_mut()
                        .downcast_mut::<rtc::Backend>()
                        .unwrap()
                        .set_time_offset_seconds(value);
                }

                Message::UpdateRenderers {
                    renderer_2d_is_accel: new_renderer_2d_is_accel,
                    renderer_2d,
                    renderer_3d_tx,
                } => {
                    renderer_2d_is_accel = new_renderer_2d_is_accel;
                    emu.gpu.engine_3d.set_renderer_tx(renderer_3d_tx);
                    emu.gpu.set_renderer_2d(renderer_2d, &mut emu.arm9);
                }

                Message::UpdateFramerateLimit(value) => {
                    frame_interval = value.map(|value| FRAME_BASE_INTERVAL.div_f32(value));
                }

                Message::UpdatePausedFramerateLimit(value) => {
                    paused_frame_interval = Duration::SECOND.div_f32(value);
                }

                Message::UpdateSyncToAudio(value) => {
                    sync_to_audio = value;
                    if let Some(data) = &audio_tx_data {
                        emu.audio.backend =
                            Box::new(audio::output::Sender::new(data, sync_to_audio));
                    }
                }

                Message::UpdateAudioSampleChunkSize(value) => {
                    emu.audio.sample_chunk_size = value;
                }

                #[cfg(feature = "xq-audio")]
                Message::UpdateAudioCustomSampleRate(value) => {
                    Audio::set_custom_sample_rate(&mut emu, value);
                }

                #[cfg(feature = "xq-audio")]
                Message::UpdateAudioChannelInterpMethod(value) => {
                    emu.audio.set_channel_interp_method(value);
                }

                Message::ToggleAudioInput(mic_rx) => {
                    emu.spi.tsc.mic_data =
                        mic_rx.map(|mic_rx| spi::tsc::MicData::new(Box::new(mic_rx)));
                }

                #[cfg(feature = "logging")]
                Message::UpdateLogger(_logger) => {
                    // TODO
                }

                #[cfg(feature = "gdb-server")]
                Message::ToggleGdbServer(addr) => {
                    let mut enabled = addr.is_some();
                    if gdb_server.is_some() != enabled {
                        if let Some(addr) = addr {
                            match gdb_server::GdbServer::new(
                                addr,
                                #[cfg(feature = "logging")]
                                logger.new(slog::o!("gdb" => "")),
                            ) {
                                Ok(server) => {
                                    gdb_server = Some(server);
                                }
                                Err(err) => {
                                    error!(
                                        "GDB server not started",
                                        "Couldn't start GDB server: {err}"
                                    );
                                    enabled = false;
                                }
                            }
                        } else {
                            gdb_server.take().unwrap().detach(&mut emu);
                        }
                        shared_state
                            .gdb_server_active
                            .store(enabled, Ordering::Relaxed);
                    }
                }
            }
        }

        let mut playing = true;

        #[cfg(feature = "gdb-server")]
        if let Some(gdb_server) = &mut gdb_server {
            reset_triggered |= gdb_server.poll(&mut emu) == gdb_server::EmuControlFlow::Reset;
            playing &= gdb_server.is_running();
        }

        if reset_triggered {
            #[cfg(feature = "xq-audio")]
            let audio_custom_sample_rate = emu.audio.custom_sample_rate();
            #[cfg(feature = "xq-audio")]
            let audio_channel_interp_method = emu.audio.channel_interp_method();

            let (renderer_2d, renderer_3d_tx) = emu.gpu.into_renderers();

            let mut emu_builder = emu::Builder::new(
                emu.spi.firmware.reset(),
                emu.ds_slot.rom.into_contents(),
                emu.ds_slot.spi.reset(),
                emu.audio.backend,
                emu.spi.tsc.mic_data.map(|mic_data| mic_data.backend),
                emu.rtc.backend,
                renderer_2d,
                renderer_3d_tx,
                emu.dldi.map(|dldi| dldi.into_provider()),
                #[cfg(feature = "log")]
                logger.clone(),
            );

            emu_builder.arm7_bios.clone_from(&sys_files.arm7_bios);
            emu_builder.arm9_bios.clone_from(&sys_files.arm9_bios);

            emu_builder.model = model;
            emu_builder.direct_boot = skip_firmware;
            // TODO: Set batch_duration and first_launch?
            emu_builder.audio_sample_chunk_size = emu.audio.sample_chunk_size;
            #[cfg(feature = "xq-audio")]
            {
                emu_builder.audio_custom_sample_rate = audio_custom_sample_rate;
                emu_builder.audio_channel_interp_method = audio_channel_interp_method;
            }

            if let Some(new_emu) = build_emu(emu_builder, Interpreter) {
                emu = new_emu;
            } else {
                return frame_tx;
            };
        }

        playing &= shared_state.playing.load(Ordering::Relaxed);

        let frame = frame_tx.current();

        if playing {
            #[cfg(not(feature = "gdb-server"))]
            let run_output = emu.run();
            #[cfg(feature = "gdb-server")]
            let run_output = {
                let mut run_forever = [0; 2];
                emu.run_with_cycles(if let Some(gdb_server) = &mut gdb_server {
                    &mut gdb_server.remaining_step_cycles
                } else {
                    &mut run_forever
                })
            };
            match run_output {
                RunOutput::FrameFinished => {}
                RunOutput::Shutdown => {
                    notif!(Notification::Stopped);
                    playing = false;
                    #[cfg(feature = "gdb-server")]
                    if let Some(gdb_server) = &mut gdb_server {
                        gdb_server.emu_shutdown();
                    }
                }
                #[cfg(feature = "gdb-server")]
                RunOutput::StoppedByDebugHook => {}
                #[cfg(feature = "gdb-server")]
                RunOutput::CyclesOver(core_mask) => {
                    if let Some(gdb_server) = &mut gdb_server {
                        gdb_server.cycles_over(&mut emu, core_mask);
                    }
                }
            }
        }

        if !renderer_2d_is_accel {
            frame
                .fb
                .copy_from_slice(emu.gpu.renderer_2d().framebuffer());
        }

        #[cfg(feature = "debug-views")]
        debug_views.update(&mut emu, &mut frame.debug, &to_ui);

        frames_since_last_fps_calc += 1;
        let now = Instant::now();
        let elapsed = now - last_fps_calc_time;
        if elapsed >= FPS_CALC_INTERVAL {
            fps = (frames_since_last_fps_calc as f64 / elapsed.as_secs_f64()) as f32;
            last_fps_calc_time = now;
            frames_since_last_fps_calc = 0;
        }
        frame.fps = fps;

        frame_tx.finish();

        let now = Instant::now();
        if now - last_save_flush_time >= save_interval {
            last_save_flush_time = now;
            save!();
        }

        let new_rtc_time_offset_seconds = emu
            .rtc
            .backend
            .as_any()
            .downcast_ref::<rtc::Backend>()
            .unwrap()
            .time_offset_seconds();
        if new_rtc_time_offset_seconds != rtc_time_offset_seconds {
            rtc_time_offset_seconds = new_rtc_time_offset_seconds;
            notif!(Notification::RtcTimeOffsetSecondsUpdated(
                new_rtc_time_offset_seconds,
            ));
        }

        if let Some(frame_interval) = if playing {
            frame_interval
        } else {
            Some(paused_frame_interval)
        } {
            let now = Instant::now();
            let elapsed = now - last_frame_time;
            if elapsed < frame_interval {
                last_frame_time += frame_interval;
                let sleep_interval =
                    (frame_interval - elapsed).saturating_sub(Duration::from_millis(1));
                if !sleep_interval.is_zero() {
                    std::thread::sleep(sleep_interval);
                }
                while Instant::now() < last_frame_time {
                    hint::spin_loop();
                }
            } else {
                last_frame_time = now;
            }
        }
    }

    save!();

    frame_tx
}
