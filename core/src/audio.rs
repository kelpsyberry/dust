pub mod capture;
pub mod channel;
mod io;

use crate::{
    cpu::{self, arm7, Schedule as _},
    emu::Emu,
    utils::schedule::RawTimestamp,
};
use capture::CaptureUnit;
use channel::Channel;
#[cfg(feature = "xq-audio")]
use core::num::NonZeroU32;

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub const struct Control(pub u16): Debug {
        pub master_volume_raw: u8 @ 0..=6,
        pub l_output_src: u8 @ 8..=9,
        pub r_output_src: u8 @ 10..=11,
        pub channel_1_mixer_output_disabled: bool @ 12,
        pub channel_3_mixer_output_disabled: bool @ 13,
        pub master_enable: bool @ 15,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChannelInterpMethod {
    Nearest,
    Cubic,
}

type RawChannelSample = i32;
type RawMixerInterpSample = i64;

cfg_if::cfg_if! {
    if #[cfg(feature = "xq-audio")] {
        type InterpSample = f64;
        pub type OutputSample = f32;
    } else {
        pub type OutputSample = u16;
    }
}

#[cfg(feature = "xq-audio")]
const SYS_CLOCK_RATE: RawTimestamp = 1 << 25;

// One sample is produced every 1024 cycles (33.554432 MHz / 32.768 kHz)
const CYCLES_PER_SAMPLE: RawTimestamp = 1024;

// Default to at most 15.625 ms of audio, assuming the default sample rate
pub const DEFAULT_OUTPUT_SAMPLE_CHUNK_SIZE: usize = 0x200;

pub trait Backend {
    #[allow(clippy::ptr_arg)] // Intended behavior, the Vec gets drained
    fn handle_sample_chunk(&mut self, samples: &mut Vec<[OutputSample; 2]>);
}

pub struct DummyBackend;

impl Backend for DummyBackend {
    fn handle_sample_chunk(&mut self, samples: &mut Vec<[OutputSample; 2]>) {
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
    sample_chunk: Vec<[OutputSample; 2]>,
    pub sample_chunk_size: usize,
    pub channels: [Channel; 16],
    pub capture: [CaptureUnit; 2],
    control: Control,
    bias: u16,
    master_volume: u8,
    #[cfg(feature = "xq-audio")]
    custom_sample_rate: Option<NonZeroU32>,
    #[cfg(feature = "xq-audio")]
    next_scaled_sample_index: RawTimestamp,
    #[cfg(feature = "xq-audio")]
    channel_interp_method: ChannelInterpMethod,
    #[cfg(feature = "channel-audio-capture")]
    pub channel_audio_capture_data: ChannelAudioCaptureData,
}

impl Audio {
    pub(crate) fn new(
        backend: Box<dyn Backend>,
        arm7_schedule: &mut arm7::Schedule,
        sample_chunk_size: usize,
        #[cfg(feature = "xq-audio")] custom_sample_rate: Option<NonZeroU32>,
        #[cfg(feature = "xq-audio")] channel_interp_method: ChannelInterpMethod,
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

        arm7_schedule.set_event(arm7::event_slots::AUDIO, arm7::Event::AudioSampleReady);
        arm7_schedule.schedule_event(arm7::event_slots::AUDIO, arm7::Timestamp(CYCLES_PER_SAMPLE));

        #[cfg(feature = "xq-audio")]
        {
            arm7_schedule.set_event(arm7::event_slots::XQ_AUDIO, arm7::Event::XqAudioSampleReady);
            if let Some(custom_sample_rate) = custom_sample_rate {
                arm7_schedule.schedule_event(
                    arm7::event_slots::XQ_AUDIO,
                    arm7::Timestamp(SYS_CLOCK_RATE / custom_sample_rate.get() as RawTimestamp),
                );
            }
        }

        let channels = channels!(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15);

        Audio {
            #[cfg(feature = "log")]
            logger,
            backend,
            sample_chunk: Vec::with_capacity(sample_chunk_size),
            sample_chunk_size,
            channels,
            capture: [CaptureUnit::new(), CaptureUnit::new()],
            control: Control(0),
            bias: 0,
            master_volume: 0,
            #[cfg(feature = "xq-audio")]
            custom_sample_rate,
            #[cfg(feature = "xq-audio")]
            next_scaled_sample_index: 0,
            #[cfg(feature = "xq-audio")]
            channel_interp_method,
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

    cfg_if::cfg_if! {
        if #[cfg(feature = "xq-audio")] {
            #[inline]
            pub fn custom_sample_rate(&self) -> Option<NonZeroU32> {
                self.custom_sample_rate
            }

            #[inline]
            pub fn set_custom_sample_rate<E: cpu::Engine>(emu: &mut Emu<E>, value: Option<NonZeroU32>) {
                if value == emu.audio.custom_sample_rate {
                    return;
                }
                emu.audio.sample_chunk.clear();
                if emu.audio.custom_sample_rate.is_some() {
                    emu.arm7.schedule.cancel_event(arm7::event_slots::XQ_AUDIO);
                }
                emu.audio.custom_sample_rate = value;
                if let Some(custom_sample_rate) = value {
                    emu.audio.next_scaled_sample_index = ((emu.arm7.schedule.cur_time().0 as u128
                        * custom_sample_rate.get() as u128
                        + (SYS_CLOCK_RATE - 1) as u128)
                        / SYS_CLOCK_RATE as u128)
                        as RawTimestamp;
                    let next_scaled_sample_timestamp = arm7::Timestamp(
                        (emu.audio.next_scaled_sample_index as u128
                            * SYS_CLOCK_RATE as u128
                            / custom_sample_rate.get() as u128)
                            as RawTimestamp,
                    );
                    emu.arm7
                        .schedule
                        .schedule_event(arm7::event_slots::XQ_AUDIO, next_scaled_sample_timestamp);
                }
            }

            #[inline]
            pub fn channel_interp_method(&self) -> ChannelInterpMethod {
                self.channel_interp_method
            }

            #[inline]
            pub fn set_channel_interp_method(&mut self, value: ChannelInterpMethod) {
                self.channel_interp_method = value;
            }
        }
    }

    #[inline]
    pub fn control(&self) -> Control {
        self.control
    }

    #[inline]
    pub fn write_control(&mut self, value: Control) {
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
    pub fn write_bias(&mut self, value: u16) {
        self.bias = value & 0x3FF;
    }

    #[inline(never)]
    #[allow(clippy::let_unit_value)]
    pub(crate) fn handle_sample_ready<E: cpu::Engine>(emu: &mut Emu<E>, time: arm7::Timestamp) {
        #[cfg(feature = "xq-audio")]
        if emu.audio.custom_sample_rate.is_none() {
            Self::handle_xq_sample_ready(emu, time);
        }

        #[allow(unused_variables)]
        let output = if emu.audio.control.master_enable() {
            fn raw_channel_sample_to_i16(sample: RawChannelSample) -> i16 {
                (sample >> 11) as i16
            }

            fn raw_mixer_interp_sample_to_i32(sample: RawMixerInterpSample) -> i32 {
                (sample >> 8) as i32
            }

            macro_rules! channel_output {
                ($i: expr$(, |$ident: ident| $code: expr)?) => {
                    if emu.audio.channels[$i].control().running() {
                        Channel::run::<_, true>(emu, channel::Index::new($i as u8), time);
                        let sample = emu.audio.channels[$i].raw_output();
                        #[cfg(feature = "channel-audio-capture")]
                        if emu.audio.channel_audio_capture_data.mask & 1 << $i != 0 {
                            emu.audio.channel_audio_capture_data.buffers[$i].push(
                                raw_channel_sample_to_i16(sample),
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
                        Default::default()
                    }
                };
            }

            macro_rules! pan {
                ($sample: expr, $i: expr) => {{
                    let sample = $sample as RawMixerInterpSample;
                    let r_vol = emu.audio.channels[$i].pan();
                    let l_vol = (128 - r_vol) as RawMixerInterpSample;
                    let r_vol = r_vol as RawMixerInterpSample;
                    [(sample * l_vol) >> 10, (sample * r_vol) >> 10]
                }};
            }

            let mut mixer_output = [0; 2];

            macro_rules! output_to_mixer {
                ($samples: expr) => {{
                    let samples = $samples;
                    mixer_output[0] += samples[0];
                    mixer_output[1] += samples[1];
                }};
            }

            let mut channel_0_output = 0;
            channel_output!(0, |sample| {
                channel_0_output = sample;
                output_to_mixer!(pan!(sample, 0));
            });

            let channel_1_output = channel_output!(1);
            let channel_1_panned_output = pan!(channel_1_output, 1);

            if !emu.audio.control.channel_1_mixer_output_disabled()
                && (!emu.audio.capture[0].addition_enabled()
                    || emu.audio.channels[0].control().running())
            {
                output_to_mixer!(channel_1_panned_output);
            }

            let mut channel_2_output = 0;
            channel_output!(2, |sample| {
                channel_2_output = sample;
                output_to_mixer!(pan!(sample, 2));
            });

            let channel_3_output = channel_output!(3);
            let channel_3_panned_output = pan!(channel_3_output, 3);

            if !emu.audio.control.channel_3_mixer_output_disabled()
                && (!emu.audio.capture[1].addition_enabled()
                    || emu.audio.channels[1].control().running())
            {
                output_to_mixer!(channel_3_panned_output);
            }

            for i in 4..16 {
                channel_output!(i, |sample| output_to_mixer!(pan!(sample, i)));
            }

            macro_rules! update_capture_unit {
                ($i: literal, $sample: expr) => {
                    if emu.audio.capture[$i].control().running() {
                        emu.audio.capture[$i].timer_counter += 512;
                        if emu.audio.capture[$i].timer_counter >> 16 != 0 {
                            CaptureUnit::run(emu, capture::Index::new($i), $sample);
                        }
                    }
                };
            }

            update_capture_unit!(0, {
                if emu.audio.capture[0].control().capture_channel() {
                    let channel_0_capture_output = raw_channel_sample_to_i16(channel_0_output);
                    if emu.audio.capture[0].addition_enabled() {
                        channel_0_capture_output
                            .wrapping_add(raw_channel_sample_to_i16(channel_1_output))
                    } else if channel_0_capture_output < 0 && channel_1_output < 0 {
                        -0x8000
                    } else {
                        channel_0_capture_output
                    }
                } else {
                    raw_mixer_interp_sample_to_i32(mixer_output[0]).clamp(-0x8000, 0x7FFF) as i16
                }
            });

            update_capture_unit!(1, {
                if emu.audio.capture[1].control().capture_channel() {
                    let channel_2_capture_output = raw_channel_sample_to_i16(channel_2_output);
                    if emu.audio.capture[1].addition_enabled() {
                        channel_2_capture_output
                            .wrapping_add(raw_channel_sample_to_i16(channel_3_output))
                    } else if channel_2_capture_output < 0 && channel_3_output < 0 {
                        -0x8000
                    } else {
                        channel_2_capture_output
                    }
                } else {
                    raw_mixer_interp_sample_to_i32(mixer_output[1]).clamp(-0x8000, 0x7FFF) as i16
                }
            });

            #[cfg(not(feature = "xq-audio"))]
            {
                [
                    (0, emu.audio.control.l_output_src()),
                    (1, emu.audio.control.r_output_src()),
                ]
                .map(|(i, src)| {
                    let sample = match src {
                        0 => mixer_output[i],
                        1 => channel_1_panned_output[i],
                        2 => channel_3_panned_output[i],
                        _ => channel_1_panned_output[i] + channel_3_panned_output[i],
                    };
                    (((sample * emu.audio.master_volume as RawMixerInterpSample) >> 21)
                        + emu.audio.bias as RawMixerInterpSample)
                        .clamp(0, 0x3FF) as OutputSample
                })
            }
        } else {
            #[cfg(not(feature = "xq-audio"))]
            {
                [0; 2]
            }
        };
        #[cfg(not(feature = "xq-audio"))]
        {
            emu.audio.sample_chunk.push(output);
            if emu.audio.sample_chunk.len() >= emu.audio.sample_chunk_size {
                emu.audio
                    .backend
                    .handle_sample_chunk(&mut emu.audio.sample_chunk);
            }
        }
        emu.arm7.schedule.schedule_event(
            arm7::event_slots::AUDIO,
            time + arm7::Timestamp(CYCLES_PER_SAMPLE),
        );
    }

    #[inline(never)]
    #[cfg(feature = "xq-audio")]
    pub(crate) fn handle_xq_sample_ready<E: cpu::Engine>(emu: &mut Emu<E>, time: arm7::Timestamp) {
        let output = if emu.audio.control.master_enable() {
            macro_rules! channel_output {
                ($i: expr$(, |$ident: ident| $code: expr)?) => {
                    if emu.audio.channels[$i].control().running()
                        || emu.audio.channel_interp_method != ChannelInterpMethod::Nearest
                    {
                        Channel::run::<_, false>(emu, channel::Index::new($i as u8), time);
                        let sample = emu.audio.channels[$i].interp_output(
                            time,
                            emu.audio.channel_interp_method,
                        );
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
                        Default::default()
                    }
                };
            }

            macro_rules! pan {
                ($sample: expr, $i: expr) => {{
                    let r_vol = emu.audio.channels[$i].pan();
                    let l_vol = (128 - r_vol) as InterpSample;
                    let r_vol = r_vol as InterpSample;
                    [
                        ($sample * l_vol) * (1.0 / (1 << 18) as InterpSample),
                        ($sample * r_vol) * (1.0 / (1 << 18) as InterpSample),
                    ]
                }};
            }

            let mut mixer_output = [0.0; 2];

            macro_rules! output_to_mixer {
                ($samples: expr) => {{
                    let samples = $samples;
                    mixer_output[0] += samples[0];
                    mixer_output[1] += samples[1];
                }};
            }

            channel_output!(0, |sample| output_to_mixer!(pan!(sample, 0)));

            let channel_1_output = channel_output!(1);
            let channel_1_panned_output = pan!(channel_1_output, 1);

            if !emu.audio.control.channel_1_mixer_output_disabled()
                && (!emu.audio.capture[0].addition_enabled()
                    || emu.audio.channels[0].control().running())
            {
                output_to_mixer!(channel_1_panned_output);
            }

            channel_output!(2, |sample| output_to_mixer!(pan!(sample, 2)));

            let channel_3_output = channel_output!(3);
            let channel_3_panned_output = pan!(channel_3_output, 3);

            if !emu.audio.control.channel_3_mixer_output_disabled()
                && (!emu.audio.capture[1].addition_enabled()
                    || emu.audio.channels[1].control().running())
            {
                output_to_mixer!(channel_3_panned_output);
            }

            for i in 4..16 {
                channel_output!(i, |sample| output_to_mixer!(pan!(sample, i)));
            }

            let volume_factor = emu.audio.master_volume as InterpSample * (1.0 / 128.0);
            let bias = emu.audio.bias as InterpSample * (1.0 / 512.0);

            [
                (0, emu.audio.control.l_output_src()),
                (1, emu.audio.control.r_output_src()),
            ]
            .map(|(i, src)| {
                let sample = match src {
                    0 => mixer_output[i],
                    1 => channel_1_panned_output[i],
                    2 => channel_3_panned_output[i],
                    _ => channel_1_panned_output[i] + channel_3_panned_output[i],
                };
                ((sample * volume_factor + bias).clamp(0.0, 2.0) - 1.0) as OutputSample
            })
        } else {
            [0.0; 2]
        };

        emu.audio.sample_chunk.push(output);
        if emu.audio.sample_chunk.len() >= emu.audio.sample_chunk_size {
            emu.audio
                .backend
                .handle_sample_chunk(&mut emu.audio.sample_chunk);
        }

        if let Some(custom_sample_rate) = emu.audio.custom_sample_rate {
            emu.audio.next_scaled_sample_index += 1;
            emu.arm7.schedule.schedule_event(
                arm7::event_slots::XQ_AUDIO,
                arm7::Timestamp(
                    (emu.audio.next_scaled_sample_index as u128 * SYS_CLOCK_RATE as u128
                        / custom_sample_rate.get() as u128) as RawTimestamp,
                ),
            );
        }
    }
}
