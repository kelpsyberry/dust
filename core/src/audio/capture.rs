use crate::{
    cpu::{self, arm7, bus::DmaAccess},
    emu::Emu,
    utils::{Bytes, Savestate},
};

// TODO: The capture units could actually need a higher resolution than the mixer (capturing
// channel output, for example), maybe they should get their own events on the scheduler in that
// case?

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq, Savestate)]
    pub const struct Control(pub u8): Debug {
        pub addition: bool @ 0,
        pub capture_channel: bool @ 1,
        pub one_shot: bool @ 2,
        pub pcm8: bool @ 3,
        pub running: bool @ 7,
    }
}

mod bounded {
    use crate::utils::{bounded_int_lit, bounded_int_savestate};
    bounded_int_lit!(pub struct Index(u8), max 15);
    bounded_int_savestate!(Index(u8));
    bounded_int_lit!(pub struct FifoReadPos(u8), max 0x1C, mask 0x1C);
    bounded_int_savestate!(FifoReadPos(u8));
    bounded_int_lit!(pub struct FifoWritePos(u8), max 0x1F);
    bounded_int_savestate!(FifoWritePos(u8));
}
pub use bounded::*;

#[derive(Savestate)]
pub struct CaptureUnit {
    control: Control,
    addition_enabled: bool,
    buffer_pos: u32,
    buffer_len: u32,
    dst_start_addr: u32,
    dst_end_addr: u32,
    cur_dst_addr: u32,
    pub timer_reload: u16,
    pub(super) timer_counter: u32,
    fifo_read_half: bool,
    fifo_write_pos: FifoWritePos,
    fifo: Bytes<0x20>,
}

impl CaptureUnit {
    pub(super) fn new() -> Self {
        CaptureUnit {
            control: Control(0),
            addition_enabled: false,
            buffer_pos: 0,
            buffer_len: 0,
            dst_start_addr: 0,
            dst_end_addr: 0,
            cur_dst_addr: 0,
            timer_reload: 0,
            timer_counter: 0,
            fifo_read_half: false,
            fifo_write_pos: FifoWritePos::new(0),
            fifo: Bytes::new([0; 0x20]),
        }
    }

    #[inline]
    pub fn control(&self) -> Control {
        self.control
    }

    pub fn write_control(&mut self, value: Control) {
        let prev = self.control;
        self.control.0 = value.0 & 0x8F;
        self.addition_enabled = self.control.addition() && self.control.running();

        if prev.running() || !self.control.running() {
            return;
        }

        self.buffer_pos = 0;
        self.cur_dst_addr = self.dst_start_addr;
        self.timer_counter = self.timer_reload as u32;
        self.fifo_read_half = false;
        self.fifo_write_pos = FifoWritePos::new(0);
    }

    #[inline]
    pub(super) fn addition_enabled(&self) -> bool {
        self.addition_enabled
    }

    #[inline]
    pub fn dst_addr(&self) -> u32 {
        self.dst_start_addr
    }

    #[inline]
    pub fn write_dst_addr(&mut self, value: u32) {
        self.dst_start_addr = value & 0x0FFF_FFFC;
        self.dst_end_addr = (self.dst_start_addr + self.buffer_len) & 0x0FFF_FFFC;
    }

    #[inline]
    pub fn buffer_words(&self) -> u16 {
        (self.buffer_len >> 2) as u16
    }

    #[inline]
    pub fn write_buffer_words(&mut self, value: u16) {
        self.buffer_len = (value.max(1) as u32) << 2;
        self.dst_end_addr = self.dst_start_addr + self.buffer_len;
    }

    fn flush_fifo(emu: &mut Emu<impl cpu::Engine>, i: Index) {
        let capture = &emu.audio.capture[i.get() as usize];
        let fifo_read_base = (capture.fifo_read_half as u8) << 4;
        let mut cur_dst_addr = capture.cur_dst_addr;
        let dst_end_addr = capture.dst_end_addr;
        for read_pos in (fifo_read_base..fifo_read_base + 0x10).step_by(4) {
            arm7::bus::write_32::<DmaAccess, _>(emu, cur_dst_addr, unsafe {
                emu.audio.capture[i.get() as usize]
                    .fifo
                    .read_le_aligned(read_pos as usize)
            });
            cur_dst_addr = (cur_dst_addr + 4) & 0x0FFF_FFFC;
            if cur_dst_addr == dst_end_addr {
                break;
            }
        }
        let capture = &mut emu.audio.capture[i.get() as usize];
        capture.fifo_read_half ^= true;
        capture.cur_dst_addr = cur_dst_addr;
    }

    pub(super) fn run(emu: &mut Emu<impl cpu::Engine>, i: Index, sample: i16) {
        // TODO: The exact sample rounding algorithm is off from the one on GBATEK, both inside this
        // function and in `Audio::handle_sample_ready`.
        loop {
            let capture = &mut emu.audio.capture[i.get() as usize];

            if capture.control.pcm8() {
                capture.fifo[capture.fifo_write_pos.get() as usize] = (sample >> 8) as u8;
                capture.buffer_pos += 1;
                capture.fifo_write_pos =
                    FifoWritePos::new((capture.fifo_write_pos.get() + 1) & 0x1F);
            } else {
                capture
                    .fifo
                    .write_le(capture.fifo_write_pos.get() as usize & !1, sample);
                capture.buffer_pos += 2;
                capture.fifo_write_pos =
                    FifoWritePos::new((capture.fifo_write_pos.get() + 2) & 0x1E);
            }

            if capture.buffer_pos >= capture.buffer_len {
                Self::flush_fifo(emu, i);
                let capture = &mut emu.audio.capture[i.get() as usize];
                if capture.control.one_shot() {
                    capture.control.set_running(false);
                    capture.addition_enabled = false;
                } else {
                    capture.buffer_pos = 0;
                    capture.cur_dst_addr = capture.dst_start_addr;
                    capture.fifo_read_half = false;
                    capture.fifo_write_pos = FifoWritePos::new(0);
                }
            } else if capture
                .fifo_write_pos
                .get()
                .wrapping_sub((capture.fifo_read_half as u8) << 4)
                & 0x1F
                >= 16
            {
                Self::flush_fifo(emu, i);
            }

            let capture = &mut emu.audio.capture[i.get() as usize];
            capture.timer_counter = capture.timer_counter - (1 << 16) + capture.timer_reload as u32;
            if capture.timer_counter >> 16 == 0 {
                break;
            }
        }
    }
}
