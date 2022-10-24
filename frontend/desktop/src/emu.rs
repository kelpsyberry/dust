#[cfg(feature = "gdb-server")]
mod gdb_server;
mod rtc;
pub mod soft_renderer_3d;

#[cfg(feature = "debug-views")]
use super::debug_views;
use crate::{audio, config::SysFiles, game_db::SaveType, input, DsSlotRom, FrameData};
#[cfg(feature = "xq-audio")]
use dust_core::audio::{Audio, ChannelInterpMethod as AudioChannelInterpMethod};
use dust_core::{
    audio::DummyBackend as DummyAudioBackend,
    cpu::{arm7, interpreter::Interpreter},
    ds_slot::{self, spi::Spi as DsSlotSpi},
    emu::{self, RunOutput},
    flash::Flash,
    gpu::{engine_2d, engine_3d, Framebuffer},
    spi::{self, firmware},
    utils::{
        BoxedByteSlice, Bytes, PersistentReadSavestate, PersistentWriteSavestate, ReadSavestate,
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
    pub save: Option<Box<[u8]>>,
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

    #[cfg(feature = "log")]
    UpdateLogger(slog::Logger),

    #[cfg(feature = "gdb-server")]
    ToggleGdbServer(Option<SocketAddr>),
}

pub enum Notification {
    Stopped,
    RtcTimeOffsetSecondsUpdated(i64),
    SavestateCreated(String, Savestate),
    SavestateFailed(String),
}

pub struct DsSlot {
    pub rom: DsSlotRom,
    pub save_type: Option<SaveType>,
    pub has_ir: bool,
}

fn setup_ds_slot(
    ds_slot: Option<DsSlot>,
    arm7_bios: &Option<Box<Bytes<{ arm7::BIOS_SIZE }>>>,
    save_path: &Option<PathBuf>,
    #[cfg(feature = "log")] logger: &slog::Logger,
) -> (ds_slot::rom::Rom, ds_slot::spi::Spi) {
    if let Some(ds_slot) = ds_slot {
        let rom = ds_slot::rom::normal::Normal::new(
            ds_slot.rom.into(),
            arm7_bios.as_deref(),
            #[cfg(feature = "log")]
            logger.new(slog::o!("ds_rom" => "normal")),
        )
        .unwrap()
        .into();

        let save_contents = save_path
            .as_deref()
            .and_then(|path| match File::open(path) {
                Ok(mut save_file) => {
                    let save_len = save_file
                        .metadata()
                        .expect("Couldn't get save file metadata")
                        .len() as usize;
                    let mut save = BoxedByteSlice::new_zeroed(save_len.next_power_of_two());
                    save_file
                        .read_exact(&mut save[..save_len])
                        .expect("Couldn't read save file");
                    Some(save)
                }
                Err(err) => match err.kind() {
                    io::ErrorKind::NotFound => None,
                    _err => {
                        #[cfg(feature = "log")]
                        slog::error!(logger, "Couldn't read save file: {_err:?}.");
                        None
                    }
                },
            });

        let save_type = if let Some(save_contents) = &save_contents {
            if let Some(save_type) = ds_slot.save_type {
                let expected_len = save_type.expected_len();
                if expected_len != Some(save_contents.len()) {
                    let (chosen_save_type, _chosen) = if let Some(detected_save_type) =
                        SaveType::from_save_len(save_contents.len())
                    {
                        (detected_save_type, "existing save file")
                    } else {
                        (save_type, "database entry")
                    };
                    #[cfg(feature = "log")]
                    slog::error!(
                        logger,
                        "Unexpected save file size: expected {}, got {} B; respecting {_chosen}.",
                        if let Some(expected_len) = expected_len {
                            format!("{expected_len} B")
                        } else {
                            "no file".to_string()
                        },
                        save_contents.len(),
                    );
                    chosen_save_type
                } else {
                    save_type
                }
            } else {
                #[allow(clippy::unnecessary_lazy_evaluations)]
                SaveType::from_save_len(save_contents.len()).unwrap_or_else(|| {
                    #[cfg(feature = "log")]
                    slog::error!(
                        logger,
                        "Unrecognized save file size ({} B) and no database entry found, \
                         defaulting to an empty save.",
                        save_contents.len()
                    );
                    SaveType::None
                })
            }
        } else {
            #[allow(clippy::unnecessary_lazy_evaluations)]
            ds_slot.save_type.unwrap_or_else(|| {
                #[cfg(feature = "log")]
                slog::error!(
                    logger,
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
                    SaveContents::Existing(if save_contents.len() == expected_len {
                        let mut new_contents = BoxedByteSlice::new_zeroed(expected_len);
                        new_contents[..save_contents.len()].copy_from_slice(&save_contents);
                        drop(save_contents);
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
                .expect("Couldn't create 4 Kib EEPROM DS slot SPI device")
                .into(),
                SaveType::EepromFram64k | SaveType::EepromFram512k | SaveType::EepromFram1m => {
                    ds_slot::spi::eeprom_fram::EepromFram::new(
                        save_contents,
                        None,
                        #[cfg(feature = "log")]
                        logger.new(slog::o!("ds_spi" => "eeprom_fram")),
                    )
                    .expect("Couldn't create EEPROM/FRAM DS slot SPI device")
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
                    .expect("Couldn't create FLASH DS slot SPI device")
                    .into()
                }
                SaveType::Nand64m | SaveType::Nand128m | SaveType::Nand256m => {
                    #[cfg(feature = "log")]
                    slog::error!(logger, "TODO: NAND saves");
                    ds_slot::spi::Empty::new(
                        #[cfg(feature = "log")]
                        logger.new(slog::o!("ds_spi" => "nand_todo")),
                    )
                    .into()
                }
            }
        };

        (rom, spi)
    } else {
        (
            ds_slot::rom::Empty::new(
                #[cfg(feature = "log")]
                logger.new(slog::o!("ds_rom" => "empty")),
            )
            .into(),
            ds_slot::spi::Empty::new(
                #[cfg(feature = "log")]
                logger.new(slog::o!("ds_spi" => "empty")),
            )
            .into(),
        )
    }
}

pub struct LaunchData {
    pub sys_files: SysFiles,
    pub ds_slot: Option<DsSlot>,

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

    #[cfg(feature = "log")]
    pub logger: slog::Logger,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn run(
    LaunchData {
        sys_files,
        ds_slot,

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

        #[cfg(feature = "log")]
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

    let (ds_slot_rom, ds_slot_spi) = setup_ds_slot(
        ds_slot,
        &sys_files.arm7_bios,
        &save_path,
        #[cfg(feature = "log")]
        &logger,
    );

    let mut emu_builder = emu::Builder::new(
        Flash::new(
            SaveContents::Existing(
                sys_files
                    .firmware
                    .unwrap_or_else(|| firmware::default(model)),
            ),
            firmware::id_for_model(model),
            #[cfg(feature = "log")]
            logger.new(slog::o!("fw" => "")),
        )
        .expect("Couldn't build firmware"),
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
        #[cfg(feature = "log")]
        logger.clone(),
    );

    emu_builder.arm7_bios = sys_files.arm7_bios.clone();
    emu_builder.arm9_bios = sys_files.arm9_bios.clone();

    emu_builder.model = model;
    emu_builder.direct_boot = skip_firmware;
    // TODO: Set batch_duration and first_launch?
    emu_builder.audio_sample_chunk_size = audio_sample_chunk_size;
    #[cfg(feature = "xq-audio")]
    {
        emu_builder.audio_custom_sample_rate = audio_custom_sample_rate;
        emu_builder.audio_channel_interp_method = audio_channel_interp_method;
    }

    let mut emu = emu_builder.build(Interpreter).unwrap();

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
    #[cfg(feature = "gdb-server")]
    let mut start_new_frame = true;

    macro_rules! save {
        () => {
            if let Some(save_path) = &save_path {
                if emu.ds_slot.spi.contents_dirty()
                    && save_path
                        .parent()
                        .map(|parent| fs::create_dir_all(parent).is_ok())
                        .unwrap_or(true)
                    && fs::write(save_path, &emu.ds_slot.spi.contents()[..]).is_ok()
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
                    debug_views.handle_message(&mut emu, message);
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
                                    Some((&*emu.ds_slot.spi.contents()).into())
                                } else {
                                    None
                                },
                                framebuffer: unsafe {
                                    let mut framebuffer = Box::<Framebuffer>::new_uninit();
                                    framebuffer.as_mut_ptr().copy_from_nonoverlapping(
                                        emu.gpu.renderer_2d().framebuffer(),
                                        1,
                                    );
                                    framebuffer.assume_init()
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
                            // TODO: Avoid this copy
                            let mut contents = BoxedByteSlice::new_zeroed(save.len());
                            contents.copy_from_slice(&save[..]);
                            emu.ds_slot
                                .spi
                                .reload_contents(SaveReloadContents::Existing(contents));
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
                            let save_contents = if let Ok(mut save_file) = File::open(save_path) {
                                let save_len = save_file
                                    .metadata()
                                    .expect("Couldn't get save file metadata")
                                    .len() as usize;
                                let mut contents = BoxedByteSlice::new_zeroed(save_len);
                                save_file
                                    .read_exact(&mut contents[..])
                                    .expect("Couldn't read save file");
                                SaveReloadContents::Existing(contents)
                            } else {
                                SaveReloadContents::New
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

                #[cfg(feature = "log")]
                Message::UpdateLogger(_logger) => {
                    // TODO
                }

                #[cfg(feature = "gdb-server")]
                Message::ToggleGdbServer(addr) => {
                    let mut enabled = addr.is_some();
                    if gdb_server.is_some() != enabled {
                        if let Some(addr) = addr {
                            match gdb_server::GdbServer::new(addr) {
                                Ok(mut server) => {
                                    server.attach(&mut emu);
                                    gdb_server = Some(server);
                                }
                                Err(_err) => {
                                    #[cfg(feature = "log")]
                                    slog::error!(logger, "Couldn't start GDB server: {_err}");
                                    enabled = false;
                                }
                            }
                        } else {
                            gdb_server = None;
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
            reset_triggered |= gdb_server.poll(&mut emu);
            playing &= !gdb_server.target_stopped();
        }

        if reset_triggered {
            #[cfg(feature = "xq-audio")]
            let audio_custom_sample_rate = emu.audio.custom_sample_rate();
            #[cfg(feature = "xq-audio")]
            let audio_channel_interp_method = emu.audio.channel_interp_method();

            let (renderer_2d, renderer_3d_tx) = emu.gpu.into_renderers();

            let mut emu_builder = emu::Builder::new(
                emu.spi.firmware.reset(),
                match emu.ds_slot.rom {
                    ds_slot::rom::Rom::Empty(device) => ds_slot::rom::Rom::Empty(device.reset()),
                    ds_slot::rom::Rom::Normal(device) => ds_slot::rom::Rom::Normal(device.reset()),
                },
                match emu.ds_slot.spi {
                    DsSlotSpi::Empty(device) => DsSlotSpi::Empty(device.reset()),
                    DsSlotSpi::Eeprom4k(device) => DsSlotSpi::Eeprom4k(device.reset()),
                    DsSlotSpi::EepromFram(device) => DsSlotSpi::EepromFram(device.reset()),
                    DsSlotSpi::Flash(device) => DsSlotSpi::Flash(device.reset()),
                },
                emu.audio.backend,
                emu.spi.tsc.mic_data.map(|mic_data| mic_data.backend),
                emu.rtc.backend,
                renderer_2d,
                renderer_3d_tx,
                #[cfg(feature = "log")]
                logger.clone(),
            );

            emu_builder.arm7_bios = sys_files.arm7_bios.clone();
            emu_builder.arm9_bios = sys_files.arm9_bios.clone();

            emu_builder.model = model;
            emu_builder.direct_boot = skip_firmware;
            // TODO: Set batch_duration and first_launch?
            emu_builder.audio_sample_chunk_size = emu.audio.sample_chunk_size;
            #[cfg(feature = "xq-audio")]
            {
                emu_builder.audio_custom_sample_rate = audio_custom_sample_rate;
                emu_builder.audio_channel_interp_method = audio_channel_interp_method;
            }

            emu = emu_builder.build(Interpreter).unwrap();
            #[cfg(feature = "gdb-server")]
            if let Some(server) = &mut gdb_server {
                server.attach(&mut emu);
            }
        }

        playing &= shared_state.playing.load(Ordering::Relaxed);

        let frame = frame_tx.current();

        if playing {
            #[cfg(feature = "gdb-server")]
            let mut run_forever = 0;
            #[cfg(feature = "gdb-server")]
            let cycles = if let Some(gdb_server) = &mut gdb_server {
                &mut gdb_server.remaining_step_cycles
            } else {
                &mut run_forever
            };
            match emu.run(
                #[cfg(feature = "gdb-server")]
                start_new_frame,
                #[cfg(not(feature = "gdb-server"))]
                true,
                #[cfg(feature = "gdb-server")]
                cycles,
            ) {
                RunOutput::FrameFinished => {
                    #[cfg(feature = "gdb-server")]
                    {
                        start_new_frame = true;
                    }
                }
                RunOutput::Shutdown => {
                    notif!(Notification::Stopped);
                    playing = false;
                    #[cfg(feature = "gdb-server")]
                    if let Some(gdb_server) = &mut gdb_server {
                        gdb_server.emu_shutdown(&mut emu);
                    }
                }
                #[cfg(feature = "gdb-server")]
                RunOutput::StoppedByDebugHook { frame_finished }
                | RunOutput::CyclesOver { frame_finished } => {
                    start_new_frame = frame_finished;
                    if let Some(gdb_server) = &mut gdb_server {
                        gdb_server.emu_stopped(&mut emu);
                    }
                }
            }
        }

        if !renderer_2d_is_accel {
            frame
                .fb
                .copy_from_slice(&emu.gpu.renderer_2d().framebuffer()[..]);
        }

        #[cfg(feature = "debug-views")]
        debug_views.prepare_frame_data(&mut emu, &mut frame.debug);

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
