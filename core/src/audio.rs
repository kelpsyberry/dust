pub mod channel;
mod io;

use crate::{
    cpu::{self, arm7, Schedule as _},
    emu::Emu,
    utils::{bitfield_debug, schedule::RawTimestamp},
};
use cfg_if::cfg_if;
use channel::Channel;

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct Control(pub u16) {
        pub master_volume_raw: u8 @ 0..=6,
        pub l_output_src: u8 @ 8..=9,
        pub r_output_src: u8 @ 10..=11,
        pub channel_1_mixer_output_disabled: bool @ 12,
        pub channel_3_mixer_output_disabled: bool @ 13,
        pub master_enable: bool @ 15,
    }
}

cfg_if! {
    if #[cfg(not(feature = "xq-audio"))] {
        pub type Sample = u16;
        type InterpSample = i64;
        const SAMPLE_ZERO: Sample = 0;
        const INTERP_SAMPLE_ZERO: InterpSample = 0;
    } else {
        pub type Sample = f32;
        type InterpSample = f64;
        const SAMPLE_ZERO: Sample = 0.0;
        const INTERP_SAMPLE_ZERO: InterpSample = 0.0;

        #[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
        #[serde(rename_all = "kebab-case")]
        pub enum InterpMethod {
            Nearest,
            Cubic,
        }
    }
}

// One sample is produced every 1024 cycles (33.554432 MHz / 32.768 kHz)
const CYCLES_PER_SAMPLE: RawTimestamp = 1024;

// Default to at most 15.625 ms of audio, assuming the default sample rate
pub const DEFAULT_SAMPLE_CHUNK_SIZE: usize = 0x200;

pub trait Backend {
    fn handle_sample_chunk(&mut self, samples: &mut Vec<[Sample; 2]>);
}

pub struct DummyBackend;

impl Backend for DummyBackend {
    fn handle_sample_chunk(&mut self, samples: &mut Vec<[Sample; 2]>) {
        samples.clear();
    }
}

#[cfg(feature = "channel-audio-capture")]
pub struct ChannelAudioCaptureData {
    pub mask: u16,
    pub buffers: [Vec<i16>; 16],
}

pub struct Audio {
    #[cfg(feature = "log")]
    logger: slog::Logger,
    pub backend: Box<dyn Backend>,
    #[cfg(feature = "xq-audio")]
    xq_sample_rate_shift: u8,
    #[cfg(feature = "xq-audio")]
    xq_last_sample_ready_time: arm7::Timestamp,
    #[cfg(feature = "xq-audio")]
    xq_interp_method: InterpMethod,
    sample_chunk: Vec<[Sample; 2]>,
    pub sample_chunk_size: usize,
    pub channels: [Channel; 16],
    control: Control,
    bias: u16,
    master_volume: u8,
    #[cfg(feature = "channel-audio-capture")]
    pub channel_audio_capture_data: ChannelAudioCaptureData,
}

impl Audio {
    pub(crate) fn new(
        backend: Box<dyn Backend>,
        arm7_schedule: &mut arm7::Schedule,
        sample_chunk_size: usize,
        #[cfg(feature = "xq-audio")] xq_sample_rate_shift: u8,
        #[cfg(feature = "xq-audio")] xq_interp_method: InterpMethod,
        #[cfg(feature = "log")] logger: slog::Logger,
    ) -> Self {
        macro_rules! channels {
            ($($i: expr),*) => {
                [$(Channel::new(
                    channel::Index::new($i),
                    #[cfg(feature = "log")] logger.new(slog::o!("channel" => $i)),
                )),*]
            }
        }
        #[cfg(not(feature = "xq-audio"))]
        let xq_sample_rate_shift = 0;
        arm7_schedule.set_event(arm7::event_slots::AUDIO, arm7::Event::AudioSampleReady);
        arm7_schedule.schedule_event(
            arm7::event_slots::AUDIO,
            arm7::Timestamp(CYCLES_PER_SAMPLE >> xq_sample_rate_shift),
        );
        let channels = channels!(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15);
        Audio {
            #[cfg(feature = "log")]
            logger,
            backend,
            #[cfg(feature = "xq-audio")]
            xq_sample_rate_shift,
            #[cfg(feature = "xq-audio")]
            xq_last_sample_ready_time: arm7::Timestamp(0),
            #[cfg(feature = "xq-audio")]
            xq_interp_method,
            sample_chunk: Vec::with_capacity(sample_chunk_size),
            sample_chunk_size,
            channels,
            control: Control(0),
            bias: 0,
            master_volume: 0,
            #[cfg(feature = "channel-audio-capture")]
            channel_audio_capture_data: {
                macro_rules! buffers {
                    ($($i: expr),*) => {
                        [
                            $({
                                let _ = $i;
                                Vec::new()
                            }),*
                        ]
                    };
                }
                ChannelAudioCaptureData {
                    mask: 0,
                    buffers: buffers!(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15),
                }
            },
        }
    }

    cfg_if! {
        if #[cfg(feature = "xq-audio")] {
            #[inline]
            pub fn xq_sample_rate_shift(&self) -> u8 {
                self.xq_sample_rate_shift
            }

            #[inline]
            pub fn set_xq_sample_rate_shift<E: cpu::Engine>(emu: &mut Emu<E>, value: u8) {
                emu.audio.xq_sample_rate_shift = value;
                emu.arm7
                    .schedule
                    .cancel_event(arm7::event_slots::AUDIO);
                emu.arm7.schedule.schedule_event(
                    arm7::event_slots::AUDIO,
                    emu.audio.xq_last_sample_ready_time
                        + arm7::Timestamp(CYCLES_PER_SAMPLE >> value),
                );
            }

            #[inline]
            pub fn xq_interp_method(&self) -> InterpMethod {
                self.xq_interp_method
            }

            #[inline]
            pub fn set_xq_interp_method(&mut self, value: InterpMethod) {
                self.xq_interp_method = value;
            }
        }
    }

    #[inline]
    pub fn control(&self) -> Control {
        self.control
    }

    #[inline]
    pub fn set_control(&mut self, value: Control) {
        self.control.0 = value.0 & 0xBF7F;
        self.master_volume = self.control.master_volume_raw();
        if self.master_volume == 127 {
            self.master_volume = 128;
        }
    }

    #[inline]
    pub fn bias(&self) -> u16 {
        self.bias
    }

    #[inline]
    pub fn set_bias(&mut self, value: u16) {
        self.bias = value & 0x3FF;
    }

    #[inline(never)]
    pub fn handle_sample_ready<E: cpu::Engine>(emu: &mut Emu<E>, time: arm7::Timestamp) {
        #[cfg(feature = "xq-audio")]
        {
            emu.audio.xq_last_sample_ready_time = time;
        }
        let output = if emu.audio.control.master_enable() {
            let mut total_l = INTERP_SAMPLE_ZERO;
            let mut total_r = INTERP_SAMPLE_ZERO;
            macro_rules! channel_output {
                ($i: expr$(, |$ident: ident| $code: block)?) => {
                    if cfg!(feature = "xq-audio")
                        || emu.audio.channels[$i].control().running()
                    {
                        let sample = Channel::run(
                            emu,
                            channel::Index::new($i as u8),
                            #[cfg(feature = "xq-audio")]
                            emu.audio.xq_sample_rate_shift,
                            #[cfg(feature = "xq-audio")]
                            emu.audio.xq_interp_method,
                            #[cfg(feature = "xq-audio")]
                            time,
                        );
                        #[cfg(feature = "channel-audio-capture")]
                        if emu.audio.channel_audio_capture_data.mask & 1 << $i != 0 {
                            emu.audio.channel_audio_capture_data.buffers[$i].push(
                                emu.audio.channels[$i].last_sample(),
                            );
                        }
                        #[allow(path_statements)]
                        {
                            sample
                            $(
                                ;
                                let $ident = sample;
                                $code
                            )*
                        }
                    } else {
                        #[cfg(feature = "channel-audio-capture")]
                        if emu.audio.channel_audio_capture_data.mask & 1 << $i != 0 {
                            emu.audio.channel_audio_capture_data.buffers[$i].push(0);
                        }
                        [INTERP_SAMPLE_ZERO; 2]
                        $(
                            ;
                            let $ident = ();
                            let _ = $ident;
                        )*
                    }
                };
            }
            for i in [0, 2].into_iter().chain(4..16) {
                channel_output!(i, |output| {
                    total_l += output[0];
                    total_r += output[1];
                });
            }
            let channel_1_output = channel_output!(1);
            let channel_3_output = channel_output!(3);
            if !emu.audio.control.channel_1_mixer_output_disabled() {
                total_l += channel_1_output[0];
                total_r += channel_1_output[1];
            }
            if !emu.audio.control.channel_3_mixer_output_disabled() {
                total_l += channel_3_output[0];
                total_r += channel_3_output[1];
            }
            let output_l = match emu.audio.control.l_output_src() {
                0 => total_l,
                1 => channel_1_output[0],
                2 => channel_3_output[0],
                _ => channel_1_output[0] + channel_3_output[0],
            };
            let output_r = match emu.audio.control.r_output_src() {
                0 => total_r,
                1 => channel_1_output[1],
                2 => channel_3_output[1],
                _ => channel_1_output[1] + channel_3_output[1],
            };
            #[cfg(not(feature = "xq-audio"))]
            {
                [
                    (((output_l * emu.audio.master_volume as InterpSample) >> 21)
                        + emu.audio.bias as InterpSample)
                        .clamp(0, 0x3FF) as Sample,
                    (((output_r * emu.audio.master_volume as InterpSample) >> 21)
                        + emu.audio.bias as InterpSample)
                        .clamp(0, 0x3FF) as Sample,
                ]
            }
            #[cfg(feature = "xq-audio")]
            {
                let volume_fac = emu.audio.master_volume as InterpSample * (1.0 / 128.0);
                let bias = emu.audio.bias as InterpSample * (1.0 / 512.0);
                [
                    ((output_l * volume_fac + bias).clamp(0.0, 2.0) - 1.0) as Sample,
                    ((output_r * volume_fac + bias).clamp(0.0, 2.0) - 1.0) as Sample,
                ]
            }
        } else {
            [SAMPLE_ZERO; 2]
        };
        emu.audio.sample_chunk.push(output);
        if emu.audio.sample_chunk.len() >= emu.audio.sample_chunk_size {
            emu.audio
                .backend
                .handle_sample_chunk(&mut emu.audio.sample_chunk);
        }
        #[cfg(not(feature = "xq-audio"))]
        let xq_sample_rate_shift = 0;
        #[cfg(feature = "xq-audio")]
        let xq_sample_rate_shift = emu.audio.xq_sample_rate_shift;
        emu.arm7.schedule.schedule_event(
            arm7::event_slots::AUDIO,
            time + arm7::Timestamp(CYCLES_PER_SAMPLE >> xq_sample_rate_shift),
        );
    }
}
