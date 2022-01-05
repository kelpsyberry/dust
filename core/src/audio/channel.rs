#[cfg(feature = "xq-audio")]
use super::InterpMethod;
use super::InterpSample;
#[cfg(feature = "xq-audio")]
use crate::utils::schedule::RawTimestamp;
use crate::{
    cpu::{self, arm7, bus::DmaAccess},
    emu::Emu,
    utils::{bitfield_debug, Bytes, MemValue},
};
use core::mem;

// TODO: Check behavior when:
// - Using format 3 (PSG) for channels 0..=7 (melonDS seems to output silence, which is what is
//   attempted here right now too)
// - Using repeat mode 0 and going out of bounds (right now loop bounds are just ignored)
// - Setting the hold bit (should only work in one-shot mode and simply avoid resetting the sample)
// - Specifying a loop start + loop size < 16 bytes for PCM modes (GBATEK says they'll hang)
// - Changing the source address while running
// - Changing the format while running (what if the FIFO becomes misaligned?)
// TODO: Check how the sample FIFO actually works, GBATEK barely mentions it

// TODO: Maybe run channels per-channel-sample instead of per-mixer-sample

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct Control(pub u32) {
        pub volume_raw: u8 @ 0..=6,
        pub volume_shift_raw: u8 @ 8..=9,
        pub hold: bool @ 15,
        pub pan_raw: u8 @ 16..=22,
        pub psg_wave_duty: u8 @ 24..=26,
        pub repeat_mode_raw: u8 @ 27..=28,
        pub format_raw: u8 @ 29..=30,
        pub running: bool @ 31,
    }
}

impl Control {
    #[inline]
    pub fn volume(&self) -> u8 {
        let volume_raw = self.volume_raw();
        if volume_raw == 127 {
            128
        } else {
            volume_raw
        }
    }

    #[inline]
    pub fn volume_shift(&self) -> u8 {
        [4, 3, 2, 0][self.volume_shift_raw() as usize]
    }

    #[inline]
    pub fn pan(&self) -> u8 {
        let pan_raw = self.pan_raw();
        if pan_raw == 127 {
            128
        } else {
            pan_raw
        }
    }

    #[inline]
    pub fn repeat_mode(&self) -> RepeatMode {
        match self.repeat_mode_raw() {
            0 => RepeatMode::Manual,
            2 => RepeatMode::OneShot,
            // Arisotura documented in the GBATEK addendum that repeat mode 3 behaves the same as 1
            _ => RepeatMode::LoopInfinite,
        }
    }

    #[inline]
    pub fn format(&self, i: Index) -> Format {
        match self.format_raw() {
            0 => Format::Pcm8,
            1 => Format::Pcm16,
            2 => Format::Adpcm,
            _ => match i.get() {
                8..=13 => Format::PsgWave,
                14..=15 => Format::PsgNoise,
                // TODO: What happens?
                _ => Format::Silence,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepeatMode {
    Manual,
    LoopInfinite,
    OneShot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Pcm8,
    Pcm16,
    Adpcm,
    PsgWave,
    PsgNoise,
    Silence,
}

mod bounded {
    use crate::utils::bounded_int_lit;
    bounded_int_lit!(pub struct Index(u8), max 15);
    bounded_int_lit!(pub struct WaveDuty(u8), max 7);
    bounded_int_lit!(pub struct FifoReadPos(u8), max 0x1F);
    bounded_int_lit!(pub struct FifoWritePos(u8), max 0x1C);
    bounded_int_lit!(pub struct AdpcmIndex(u8), max 88);
}
pub use bounded::*;

static ADPCM_INDEX_TABLE: [i8; 8] = [-1, -1, -1, -1, 2, 4, 6, 8];

#[rustfmt::skip]
static ADPCM_TABLE: [u16; 89] = [
    0x0007, 0x0008, 0x0009, 0x000A, 0x000B, 0x000C, 0x000D, 0x000E, 0x0010, 0x0011, 0x0013, 0x0015,
    0x0017, 0x0019, 0x001C, 0x001F, 0x0022, 0x0025, 0x0029, 0x002D, 0x0032, 0x0037, 0x003C, 0x0042,
    0x0049, 0x0050, 0x0058, 0x0061, 0x006B, 0x0076, 0x0082, 0x008F, 0x009D, 0x00AD, 0x00BE, 0x00D1,
    0x00E6, 0x00FD, 0x0117, 0x0133, 0x0151, 0x0173, 0x0198, 0x01C1, 0x01EE, 0x0220, 0x0256, 0x0292,
    0x02D4, 0x031C, 0x036C, 0x03C3, 0x0424, 0x048E, 0x0502, 0x0583, 0x0610, 0x06AB, 0x0756, 0x0812,
    0x08E0, 0x09C3, 0x0ABD, 0x0BD0, 0x0CFF, 0x0E4C, 0x0FBA, 0x114C, 0x1307, 0x14EE, 0x1706, 0x1954,
    0x1BDC, 0x1EA5, 0x21B6, 0x2515, 0x28CA, 0x2CDF, 0x315B, 0x364B, 0x3BB9, 0x41B2, 0x4844, 0x4F7E,
    0x5771, 0x602F, 0x69CE, 0x7462, 0x7FFF,
];

#[rustfmt::skip]
static PSG_TABLE: [i16; 64] = [
    -0x7FFF, -0x7FFF, -0x7FFF, -0x7FFF, -0x7FFF, -0x7FFF, -0x7FFF,  0x7FFF,
    -0x7FFF, -0x7FFF, -0x7FFF, -0x7FFF, -0x7FFF, -0x7FFF,  0x7FFF,  0x7FFF,
    -0x7FFF, -0x7FFF, -0x7FFF, -0x7FFF, -0x7FFF,  0x7FFF,  0x7FFF,  0x7FFF,
    -0x7FFF, -0x7FFF, -0x7FFF, -0x7FFF,  0x7FFF,  0x7FFF,  0x7FFF,  0x7FFF,
    -0x7FFF, -0x7FFF, -0x7FFF,  0x7FFF,  0x7FFF,  0x7FFF,  0x7FFF,  0x7FFF,
    -0x7FFF, -0x7FFF,  0x7FFF,  0x7FFF,  0x7FFF,  0x7FFF,  0x7FFF,  0x7FFF,
    -0x7FFF,  0x7FFF,  0x7FFF,  0x7FFF,  0x7FFF,  0x7FFF,  0x7FFF,  0x7FFF,
    -0x7FFF, -0x7FFF, -0x7FFF, -0x7FFF, -0x7FFF, -0x7FFF, -0x7FFF, -0x7FFF,
];

pub struct Channel {
    #[cfg(feature = "log")]
    logger: slog::Logger,
    control: Control,
    index: Index,
    volume: u8,
    volume_shift: u8,
    pan: u8,
    repeat_mode: RepeatMode,
    format: Format,
    start: bool,
    src_addr: u32,
    timer_reload: u16,
    timer_counter: u16,
    loop_start: u16,
    loop_len: u32,
    loop_start_sample_index: u32,
    total_size: u32,
    total_samples: u32,
    cur_sample_index: i32,
    cur_src_off: u32,
    #[cfg(not(feature = "xq-audio"))]
    last_sample: i16,
    fifo_read_pos: FifoReadPos,
    fifo_write_pos: FifoWritePos,
    fifo: Bytes<0x20>,
    adpcm_value: i16,
    loop_start_adpcm_value: i16,
    adpcm_index: AdpcmIndex,
    loop_start_adpcm_index: AdpcmIndex,
    adpcm_byte: u8,
    noise_lfsr: u16,
    #[cfg(feature = "xq-audio")]
    hist: [InterpSample; 4],
    #[cfg(feature = "xq-audio")]
    last_sample_time: Option<arm7::Timestamp>,
    #[cfg(feature = "xq-audio")]
    sample_interval: arm7::Timestamp,
}

impl Channel {
    pub(super) fn new(index: Index, #[cfg(feature = "log")] logger: slog::Logger) -> Self {
        Channel {
            #[cfg(feature = "log")]
            logger,
            control: Control(0),
            index,
            volume: 0,
            volume_shift: 0,
            pan: 0,
            repeat_mode: RepeatMode::Manual,
            format: Format::Pcm8,
            start: false,
            src_addr: 0,
            timer_reload: 0,
            timer_counter: 0,
            loop_start: 0,
            loop_len: 0,
            loop_start_sample_index: 0,
            total_size: 0,
            total_samples: 0,
            cur_sample_index: 0,
            cur_src_off: 0,
            #[cfg(not(feature = "xq-audio"))]
            last_sample: 0,
            fifo_read_pos: FifoReadPos::new(0),
            fifo_write_pos: FifoWritePos::new(0),
            fifo: Bytes::new([0; 0x20]),
            adpcm_value: 0,
            loop_start_adpcm_value: 0,
            adpcm_index: AdpcmIndex::new(0),
            loop_start_adpcm_index: AdpcmIndex::new(0),
            adpcm_byte: 0,
            noise_lfsr: 0,
            #[cfg(feature = "xq-audio")]
            hist: [0.0; 4],
            #[cfg(feature = "xq-audio")]
            last_sample_time: None,
            #[cfg(feature = "xq-audio")]
            sample_interval: arm7::Timestamp(0x2_0000),
        }
    }

    #[inline]
    pub fn control(&self) -> Control {
        self.control
    }

    pub fn set_control(&mut self, value: Control) {
        let prev = self.control;
        self.control.0 = value.0 & 0xFF7F_837F;

        if !self.control.running() {
            self.start = false;
            return;
        }

        self.start |= !prev.running();

        self.volume = self.control.volume();
        self.volume_shift = self.control.volume_shift();
        self.pan = self.control.pan();

        self.repeat_mode = match self.control.repeat_mode_raw() {
            0 => {
                #[cfg(feature = "log")]
                slog::warn!(self.logger, "Using untested repeat mode 0 (manual)");
                RepeatMode::Manual
            }
            2 => RepeatMode::OneShot,
            // Arisotura documented in the GBATEK addendum that repeat mode 3 behaves the same as 1
            _ => RepeatMode::LoopInfinite,
        };

        self.format = match self.control.format_raw() {
            0 => Format::Pcm8,
            1 => Format::Pcm16,
            2 => Format::Adpcm,
            _ => match self.index.get() {
                8..=13 => Format::PsgWave,
                14..=15 => Format::PsgNoise,
                _ => {
                    // TODO: What happens?
                    #[cfg(feature = "log")]
                    slog::warn!(self.logger, "Using unsupported format 3 (PSG)");
                    Format::Silence
                }
            },
        };

        self.loop_start_sample_index = self.calc_loop_start_sample_index();
        self.total_samples = self.calc_total_samples();
        self.check_loop_start();
    }

    pub(super) fn pan(&self) -> u8 {
        self.pan
    }

    #[inline]
    fn check_loop_start(&self) {
        #[cfg(feature = "log")]
        if self.format == Format::Adpcm && self.loop_start == 0 {
            slog::warn!(self.logger, "Using loop start == 0 in ADPCM mode");
        }
    }

    #[inline]
    fn check_total_size(&self) {
        #[cfg(feature = "log")]
        if !matches!(self.format, Format::PsgWave | Format::PsgNoise) && self.total_size < 16 {
            slog::warn!(self.logger, "Using total size < 16 bytes in PCM mode");
        }
    }

    #[inline]
    fn calc_loop_start_sample_index(&self) -> u32 {
        match self.format {
            Format::Pcm16 => (self.loop_start as u32) << 1,
            Format::Adpcm => (self.loop_start as u32) << 3,
            _ => (self.loop_start as u32) << 2,
        }
    }

    #[inline]
    fn calc_total_samples(&self) -> u32 {
        match self.format {
            Format::Pcm16 => self.total_size >> 1,
            Format::Adpcm => self.total_size << 1,
            _ => self.total_size,
        }
    }

    #[inline]
    pub fn src_addr(&self) -> u32 {
        self.src_addr
    }

    #[inline]
    pub fn set_src_addr(&mut self, value: u32) {
        self.src_addr = value & 0x07FF_FFFC;
    }

    #[inline]
    pub fn timer_reload(&self) -> u16 {
        self.timer_reload
    }

    #[inline]
    pub fn set_timer_reload(&mut self, value: u16) {
        self.timer_reload = value;
        #[cfg(feature = "xq-audio")]
        {
            self.sample_interval = arm7::Timestamp((0x1_0000 - value as RawTimestamp) << 1);
        }
    }

    #[inline]
    pub fn loop_start(&self) -> u16 {
        self.loop_start
    }

    #[inline]
    pub fn set_loop_start(&mut self, value: u16) {
        self.loop_start = value;
        self.loop_start_sample_index = self.calc_loop_start_sample_index();
        self.total_size = (self.loop_start as u32 + self.loop_len) << 2;
        self.total_samples = self.calc_total_samples();
        self.check_loop_start();
        self.check_total_size();
    }

    #[inline]
    pub fn loop_len(&self) -> u32 {
        self.loop_len
    }

    #[inline]
    pub fn set_loop_len(&mut self, value: u32) {
        self.loop_len = value & 0x3F_FFFF;
        self.total_size = (self.loop_start as u32 + self.loop_len) << 2;
        self.total_samples = self.calc_total_samples();
        self.check_total_size();
    }

    #[inline]
    fn keep_last_sample(&mut self) {
        #[cfg(feature = "xq-audio")]
        self.hist.copy_within(1.., 0);
    }

    #[inline]
    fn push_sample(&mut self, sample: i16) {
        #[cfg(feature = "xq-audio")]
        {
            self.hist.copy_within(1.., 0);
            self.hist[3] = sample as InterpSample / 32768.0;
        }
        #[cfg(not(feature = "xq-audio"))]
        {
            self.last_sample = sample;
        }
    }

    fn refill_fifo(emu: &mut Emu<impl cpu::Engine>, i: Index) {
        let channel = &mut emu.audio.channels[i.get() as usize];
        let read_bytes = match channel.repeat_mode {
            RepeatMode::Manual => 16,
            RepeatMode::LoopInfinite => {
                if channel.cur_src_off >= channel.total_size {
                    channel.cur_src_off = (channel.loop_start as u32) << 2;
                }
                16.min(channel.total_size - channel.cur_src_off)
            }
            RepeatMode::OneShot => {
                if channel.cur_src_off >= channel.total_size {
                    return;
                }
                16.min(channel.total_size - channel.cur_src_off)
            }
        };
        let mut addr = channel.src_addr + channel.cur_src_off;
        channel.cur_src_off += read_bytes;
        let mut fifo_write_pos = channel.fifo_write_pos;
        for _ in (0..read_bytes).step_by(4) {
            let result = arm7::bus::read_32::<DmaAccess, _>(emu, addr);
            emu.audio.channels[i.get() as usize]
                .fifo
                .write_le(fifo_write_pos.get() as usize, result);
            fifo_write_pos = FifoWritePos::new((fifo_write_pos.get() + 4) & 0x1C);
            addr += 4;
        }
        emu.audio.channels[i.get() as usize].fifo_write_pos = fifo_write_pos;
    }

    fn read_fifo<T: MemValue, E: cpu::Engine>(emu: &mut Emu<E>, i: Index) -> T {
        let channel = &mut emu.audio.channels[i.get() as usize];
        let result = channel
            .fifo
            .read_le(channel.fifo_read_pos.get() as usize & !(mem::size_of::<T>() - 1));
        channel.fifo_read_pos = FifoReadPos::new(
            (channel.fifo_read_pos.get() + mem::size_of::<T>() as u8)
                & (0x1F & !(mem::size_of::<T>() - 1) as u8),
        );
        if channel
            .fifo_write_pos
            .get()
            .wrapping_sub(channel.fifo_read_pos.get())
            & 0x1F
            <= 0x10
        {
            Self::refill_fifo(emu, i);
        }
        result
    }

    fn run_pcm8(emu: &mut Emu<impl cpu::Engine>, i: Index) {
        let channel = &mut emu.audio.channels[i.get() as usize];
        channel.cur_sample_index += 1;
        if channel.cur_sample_index < 0 {
            if channel.cur_sample_index <= -2 {
                channel.keep_last_sample();
            } else {
                channel.push_sample(0);
            }
            return;
        }
        if channel.cur_sample_index as u32 >= channel.total_samples {
            match channel.repeat_mode {
                RepeatMode::Manual => {}
                RepeatMode::LoopInfinite => {
                    channel.cur_sample_index = channel.loop_start_sample_index as i32;
                }
                RepeatMode::OneShot => {
                    channel.control.set_running(false);
                    if channel.control.hold() {
                        channel.keep_last_sample();
                    } else {
                        channel.push_sample(0);
                    }
                    return;
                }
            }
        }
        let sample = Self::read_fifo::<i8, _>(emu, i);
        emu.audio.channels[i.get() as usize].push_sample((sample as i16) << 8);
    }

    fn run_pcm16(emu: &mut Emu<impl cpu::Engine>, i: Index) {
        let channel = &mut emu.audio.channels[i.get() as usize];
        channel.cur_sample_index += 1;
        if channel.cur_sample_index < 0 {
            if channel.cur_sample_index <= -2 {
                channel.keep_last_sample();
            } else {
                channel.push_sample(0);
            }
            return;
        }
        if channel.cur_sample_index as u32 >= channel.total_samples {
            match channel.repeat_mode {
                RepeatMode::Manual => {}
                RepeatMode::LoopInfinite => {
                    channel.cur_sample_index = channel.loop_start_sample_index as i32;
                }
                RepeatMode::OneShot => {
                    channel.control.set_running(false);
                    if channel.control.hold() {
                        channel.keep_last_sample();
                    } else {
                        channel.push_sample(0);
                    }
                    return;
                }
            }
        }
        let sample = Self::read_fifo::<i16, _>(emu, i);
        emu.audio.channels[i.get() as usize].push_sample(sample);
    }

    fn run_adpcm(emu: &mut Emu<impl cpu::Engine>, i: Index) {
        let channel = &mut emu.audio.channels[i.get() as usize];
        channel.cur_sample_index += 1;
        if channel.cur_sample_index < 8 {
            if channel.cur_sample_index <= -2 {
                channel.keep_last_sample();
            } else {
                channel.push_sample(0);
            }
            if channel.cur_sample_index == 0 {
                let header = Self::read_fifo::<u32, _>(emu, i);
                let channel = &mut emu.audio.channels[i.get() as usize];
                channel.adpcm_value = (header as i16).max(-0x7FFF);
                channel.adpcm_index = AdpcmIndex::new(((header >> 16) as u8).min(88));
                // Initialize the loop start values in case the loop start sample index is < 8...?
                // TODO: Check what actually happens
                channel.loop_start_adpcm_value = channel.adpcm_value;
                channel.loop_start_adpcm_index = channel.adpcm_index;
            }
            return;
        }
        if channel.cur_sample_index as u32 >= channel.total_samples {
            match channel.repeat_mode {
                RepeatMode::Manual => {}
                RepeatMode::LoopInfinite => {
                    channel.cur_sample_index = channel.loop_start_sample_index as i32;
                    channel.adpcm_value = channel.loop_start_adpcm_value;
                    channel.adpcm_index = channel.loop_start_adpcm_index;
                    channel.push_sample(channel.adpcm_value);
                    // Re-read the loop start byte and ignore it, as the values are already
                    // calculated.
                    emu.audio.channels[i.get() as usize].adpcm_byte =
                        Self::read_fifo::<u8, _>(emu, i);
                    return;
                }
                RepeatMode::OneShot => {
                    channel.control.set_running(false);
                    if channel.control.hold() {
                        channel.keep_last_sample();
                    } else {
                        channel.push_sample(0);
                    }
                    return;
                }
            }
        }
        let sample = if channel.cur_sample_index & 1 == 0 {
            let result = Self::read_fifo::<u8, _>(emu, i);
            let channel = &mut emu.audio.channels[i.get() as usize];
            channel.adpcm_byte = result;
            channel.adpcm_byte & 0xF
        } else {
            channel.adpcm_byte >> 4
        };
        let channel = &mut emu.audio.channels[i.get() as usize];
        let adpcm_table_entry = ADPCM_TABLE[channel.adpcm_index.get() as usize] as u32;
        let mut diff = adpcm_table_entry >> 3;
        if sample & 1 != 0 {
            diff += adpcm_table_entry >> 2;
        }
        if sample & 2 != 0 {
            diff += adpcm_table_entry >> 1;
        }
        if sample & 4 != 0 {
            diff += adpcm_table_entry;
        }
        channel.adpcm_value = if sample & 8 == 0 {
            (channel.adpcm_value as i32 + diff as i32).min(0x7FFF)
        } else {
            (channel.adpcm_value as i32 - diff as i32).max(-0x7FFF)
        } as i16;
        channel.adpcm_index = AdpcmIndex::new(
            (channel.adpcm_index.get() as i8 + ADPCM_INDEX_TABLE[sample as usize & 7]).clamp(0, 88)
                as u8,
        );
        if channel.cur_sample_index as u32 == channel.loop_start_sample_index {
            channel.loop_start_adpcm_value = channel.adpcm_value;
            channel.loop_start_adpcm_index = channel.adpcm_index;
        }
        channel.push_sample(channel.adpcm_value);
    }

    fn run_psg_wave(emu: &mut Emu<impl cpu::Engine>, i: Index) {
        let channel = &mut emu.audio.channels[i.get() as usize];
        channel.cur_sample_index += 1;
        channel.push_sample(
            PSG_TABLE[(channel.control.psg_wave_duty() as usize) << 3
                | (channel.cur_sample_index as usize & 7)],
        );
    }

    fn run_psg_noise(emu: &mut Emu<impl cpu::Engine>, i: Index) {
        let channel = &mut emu.audio.channels[i.get() as usize];
        let sample = if channel.noise_lfsr & 1 == 0 {
            channel.noise_lfsr >>= 1;
            0x7FFF
        } else {
            channel.noise_lfsr = channel.noise_lfsr >> 1 ^ 0x6000;
            -0x7FFF
        };
        channel.push_sample(sample);
    }

    fn run_silence(emu: &mut Emu<impl cpu::Engine>, i: Index) {
        emu.audio.channels[i.get() as usize].push_sample(0);
    }

    #[allow(clippy::many_single_char_names)]
    pub(super) fn run(
        emu: &mut Emu<impl cpu::Engine>,
        i: Index,
        #[cfg(feature = "xq-audio")] xq_sample_rate_shift: u8,
        #[cfg(feature = "xq-audio")] xq_interp_method: InterpMethod,
        #[cfg(feature = "xq-audio")] time: arm7::Timestamp,
    ) -> InterpSample {
        let channel = &mut emu.audio.channels[i.get() as usize];

        if channel.start {
            channel.start = false;
            channel.timer_counter = channel.timer_reload;
            channel.cur_src_off = 0;
            channel.fifo_read_pos = FifoReadPos::new(0);
            channel.fifo_write_pos = FifoWritePos::new(0);
            #[cfg(feature = "xq-audio")]
            {
                channel.hist = [0.0; 4];
                channel.last_sample_time = None;
            }
            if matches!(channel.format, Format::PsgNoise | Format::PsgWave) {
                channel.noise_lfsr = 0x7FFF;
                channel.cur_sample_index = -1;
            } else {
                channel.cur_sample_index = -3;
                Self::refill_fifo(emu, i);
                Self::refill_fifo(emu, i);
            }
        }
        let channel = &mut emu.audio.channels[i.get() as usize];
        let f = match channel.format {
            Format::Pcm8 => Self::run_pcm8,
            Format::Pcm16 => Self::run_pcm16,
            Format::Adpcm => Self::run_adpcm,
            Format::PsgWave => Self::run_psg_wave,
            Format::PsgNoise => Self::run_psg_noise,
            Format::Silence => Self::run_silence,
        };
        // The timer runs at 16.777 MHz (half of the ARM7 clock rate), and the mixer requests a
        // sample every 1024 ARM7 cycles, so the timer gets incremented 512 times.
        #[cfg(not(feature = "xq-audio"))]
        let elapsed = 512;
        #[cfg(feature = "xq-audio")]
        let elapsed = 512 >> xq_sample_rate_shift;
        let mut timer_counter = channel.timer_counter as u32 + elapsed;
        let timer_reload = channel.timer_reload as u32;
        if channel.control.running() {
            while timer_counter >> 16 != 0 {
                #[cfg(feature = "xq-audio")]
                {
                    let channel = &mut emu.audio.channels[i.get() as usize];
                    channel.last_sample_time = Some(arm7::Timestamp(
                        time.0 - ((timer_counter - (1 << 16)) << 1) as RawTimestamp,
                    ));
                }
                timer_counter = timer_counter - (1 << 16) + timer_reload;
                f(emu, i);
            }
        } else {
            #[cfg(feature = "xq-audio")]
            {
                channel.push_sample(0);
                while timer_counter >> 16 != 0 {
                    channel.last_sample_time = Some(arm7::Timestamp(
                        time.0 - ((timer_counter - (1 << 16)) << 1) as RawTimestamp,
                    ));
                    timer_counter = timer_counter - (1 << 16) + timer_reload;
                }
            }
        }
        let channel = &mut emu.audio.channels[i.get() as usize];
        channel.timer_counter = timer_counter as u16;
        #[cfg(not(feature = "xq-audio"))]
        {
            ((channel.last_sample as InterpSample) << channel.volume_shift)
                * channel.volume as InterpSample
        }
        #[cfg(feature = "xq-audio")]
        {
            let interp_result = match xq_interp_method {
                InterpMethod::Nearest => channel.hist[3],
                InterpMethod::Cubic => {
                    #[allow(clippy::cast_precision_loss)]
                    let mu = channel.last_sample_time.map_or(1.0, |last_sample_time| {
                        assert!(time.0 >= last_sample_time.0);
                        (time.0 - last_sample_time.0) as InterpSample
                            / channel.sample_interval.0 as InterpSample
                    });
                    let a = channel.hist[3] - channel.hist[2] - channel.hist[0] + channel.hist[1];
                    let b = channel.hist[0] - channel.hist[1] - a;
                    let c = channel.hist[2] - channel.hist[0];
                    let d = channel.hist[1];
                    (((a * mu + b) * mu + c) * mu + d).clamp(-1.0, 1.0)
                }
            };
            interp_result
                * (1 << channel.volume_shift) as InterpSample
                * channel.volume as InterpSample
        }
    }
}
