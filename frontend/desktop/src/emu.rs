#[cfg(feature = "debug-views")]
use super::debug_views;
use super::{audio, config::CommonLaunchConfig, input, triple_buffer, FrameData};
use dust_core::{
    audio::DummyBackend as DummyAudioBackend, cpu::interpreter::Interpreter, ds_slot,
    utils::BoxedByteSlice,
};
use parking_lot::RwLock;
use std::{
    hint,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

pub struct SharedState {
    pub playing: AtomicBool,
    pub limit_framerate: AtomicBool,
    pub autosave_interval: RwLock<Duration>,
    pub stopped: AtomicBool,
}

pub enum Message {
    UpdateInput(input::Changes),
    UpdateSavePath(Option<PathBuf>),
    UpdateAudioSampleChunkSize(u32),
    #[cfg(feature = "xq-audio")]
    UpdateAudioXqSampleRateShift(u8),
    #[cfg(feature = "xq-audio")]
    UpdateAudioXqInterpMethod(dust_core::audio::InterpMethod),
    UpdateAudioSync(bool),
    #[cfg(feature = "debug-views")]
    DebugViews(debug_views::Message),
    Reset,
}

pub(super) fn main(
    mut config: CommonLaunchConfig,
    ds_slot_rom: Option<BoxedByteSlice>,
    audio_tx_data: Option<audio::SenderData>,
    mut frame_tx: triple_buffer::Sender<FrameData>,
    message_rx: crossbeam_channel::Receiver<Message>,
    shared_state: Arc<SharedState>,
    #[cfg(feature = "log")] logger: slog::Logger,
) -> triple_buffer::Sender<FrameData> {
    let mut emu_builder = dust_core::emu::Builder::new();

    emu_builder
        .arm7_bios
        .copy_from_slice(&config.sys_files.arm7_bios[..]);
    emu_builder
        .arm9_bios
        .copy_from_slice(&config.sys_files.arm9_bios[..]);

    emu_builder.audio_sample_chunk_size = config.audio_sample_chunk_size as usize;
    #[cfg(feature = "xq-audio")]
    {
        emu_builder.audio_xq_sample_rate_shift = config.audio_xq_sample_rate_shift.value;
        emu_builder.audio_xq_interp_method = config.audio_xq_interp_method.value;
    }

    // TODO: Set first_launch in emu_builder?
    let mut emu = emu_builder
        .build(
            config.model,
            config.sys_files.firmware,
            if let Some(rom) = ds_slot_rom.clone() {
                Box::new(
                    ds_slot::rom::Normal::new(
                        rom,
                        &config.sys_files.arm7_bios,
                        #[cfg(feature = "log")]
                        logger.new(slog::o!("ds_rom" => "normal")),
                    )
                    .unwrap(),
                )
            } else {
                Box::new(ds_slot::rom::Empty::new(
                    #[cfg(feature = "log")]
                    logger.new(slog::o!("ds_rom" => "empty")),
                ))
            },
            Box::new(ds_slot::spi::Empty::new(
                #[cfg(feature = "log")]
                logger.new(slog::o!("ds_spi" => "normal")),
            )),
            config.skip_firmware && ds_slot_rom.is_some(),
            Interpreter,
            match &audio_tx_data {
                Some(data) => Box::new(audio::Sender::new(data, config.sync_to_audio.value)),
                None => Box::new(DummyAudioBackend),
            },
            #[cfg(feature = "log")]
            &logger,
        )
        .expect("Couldn't setup emulator");

    const FRAME_INTERVAL: Duration = Duration::from_nanos(1_000_000_000 / 60);
    let mut last_frame_time = Instant::now();

    const FPS_CALC_INTERVAL: Duration = Duration::from_secs(1);
    let mut frames_since_last_fps_calc = 0;
    let mut last_fps_calc_time = last_frame_time;
    let mut fps = 0.0;

    #[cfg(feature = "debug-views")]
    let mut debug_views = debug_views::EmuState::new();

    loop {
        if shared_state.stopped.load(Ordering::Relaxed) {
            break;
        }

        for message in message_rx.try_iter() {
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

                Message::UpdateSavePath(_new_path) => {
                    // TOOD
                }

                Message::UpdateAudioSampleChunkSize(chunk_size) => {
                    config.audio_sample_chunk_size = chunk_size;
                    emu.audio.sample_chunk_size = chunk_size as usize;
                }

                #[cfg(feature = "xq-audio")]
                Message::UpdateAudioXqSampleRateShift(shift) => {
                    config.audio_xq_sample_rate_shift.value = shift;
                    dust_core::audio::Audio::set_xq_sample_rate_shift(&mut emu, shift);
                }

                #[cfg(feature = "xq-audio")]
                Message::UpdateAudioXqInterpMethod(interp_method) => {
                    config.audio_xq_interp_method.value = interp_method;
                    emu.audio.set_xq_interp_method(interp_method);
                }

                Message::UpdateAudioSync(audio_sync) => {
                    config.sync_to_audio.value = audio_sync;
                    if let Some(data) = &audio_tx_data {
                        emu.audio.backend = Box::new(audio::Sender::new(data, audio_sync));
                    }
                }

                #[cfg(feature = "debug-views")]
                Message::DebugViews(message) => {
                    debug_views.handle_message(message);
                }

                Message::Reset => {
                    let mut emu_builder = dust_core::emu::Builder::new();
                    emu_builder
                        .arm7_bios
                        .copy_from_slice(&config.sys_files.arm7_bios[..]);
                    emu_builder
                        .arm9_bios
                        .copy_from_slice(&config.sys_files.arm9_bios[..]);

                    emu_builder.audio_sample_chunk_size = config.audio_sample_chunk_size as usize;
                    #[cfg(feature = "xq-audio")]
                    {
                        emu_builder.audio_xq_sample_rate_shift =
                            config.audio_xq_sample_rate_shift.value;
                        emu_builder.audio_xq_interp_method = config.audio_xq_interp_method.value;
                    }

                    // TODO: Same as above
                    emu = emu_builder
                        .build(
                            config.model,
                            emu.spi.firmware.contents,
                            if let Some(rom) = ds_slot_rom.clone() {
                                Box::new(
                                    ds_slot::rom::Normal::new(
                                        rom,
                                        &config.sys_files.arm7_bios,
                                        #[cfg(feature = "log")]
                                        logger.new(slog::o!("ds_rom" => "normal")),
                                    )
                                    .unwrap(),
                                )
                            } else {
                                Box::new(ds_slot::rom::Empty::new(
                                    #[cfg(feature = "log")]
                                    logger.new(slog::o!("ds_rom" => "empty")),
                                ))
                            },
                            Box::new(ds_slot::spi::Empty::new(
                                #[cfg(feature = "log")]
                                logger.new(slog::o!("ds_spi" => "normal")),
                            )),
                            config.skip_firmware && ds_slot_rom.is_some(),
                            Interpreter,
                            match &audio_tx_data {
                                Some(data) => {
                                    Box::new(audio::Sender::new(data, config.sync_to_audio.value))
                                }
                                None => Box::new(DummyAudioBackend),
                            },
                            #[cfg(feature = "log")]
                            &logger,
                        )
                        .expect("Couldn't setup emulator");
                }
            }
        }

        let playing = shared_state.playing.load(Ordering::Relaxed);

        let frame = frame_tx.start();

        if playing && !emu.run_frame() {
            shared_state.stopped.store(true, Ordering::Relaxed);
        }
        frame.fb.0.copy_from_slice(&emu.gpu.framebuffer.0);

        #[cfg(feature = "debug-views")]
        debug_views.prepare_frame_data(&mut emu, &mut frame.debug);

        #[cfg(feature = "debug-views")]
        debug_views.prepare_frame_data(&mut emu, &mut frame.debug);

        frames_since_last_fps_calc += 1;
        let now = Instant::now();
        let elapsed = now - last_fps_calc_time;
        if elapsed >= FPS_CALC_INTERVAL {
            fps = frames_since_last_fps_calc as f64 / elapsed.as_secs_f64();
            last_fps_calc_time = now;
            frames_since_last_fps_calc = 0;
        }
        frame.fps = fps;

        frame_tx.finish();

        if !playing || shared_state.limit_framerate.load(Ordering::Relaxed) {
            let now = Instant::now();
            let elapsed = now - last_frame_time;
            if elapsed < FRAME_INTERVAL {
                last_frame_time += FRAME_INTERVAL;
                let sleep_interval =
                    (FRAME_INTERVAL - elapsed).saturating_sub(Duration::from_millis(1));
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

    frame_tx
}
