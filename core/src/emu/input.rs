use super::Emu;
use crate::{cpu, utils::bitfield_debug};
use bitflags::bitflags;

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct Input(pub u32) {
        pub a: bool @ 0,
        pub b: bool @ 1,
        pub select: bool @ 2,
        pub start: bool @ 3,
        pub right: bool @ 4,
        pub left: bool @ 5,
        pub up: bool @ 6,
        pub down: bool @ 7,
        pub r: bool @ 8,
        pub l: bool @ 9,
        pub x: bool @ 16,
        pub y: bool @ 17,
        pub debug: bool @ 19,
        pub pen_down: bool @ 22,
        pub lid_closed: bool @ 23,
    }
}

bitflags! {
    pub struct Keys: u32 {
        const A = 1;
        const B = 1 << 1;
        const SELECT = 1 << 2;
        const START = 1 << 3;
        const RIGHT = 1 << 4;
        const LEFT = 1 << 5;
        const UP = 1 << 6;
        const DOWN = 1 << 7;
        const R = 1 << 8;
        const L = 1 << 9;
        const X = 1 << 16;
        const Y = 1 << 17;
        const DEBUG = 1 << 19;
    }
}

impl<E: cpu::Engine> Emu<E> {
    pub fn press_keys(&mut self, keys: Keys) {
        self.input.0 &= !keys.bits();
        // TODO: KEYCNT
    }

    pub fn release_keys(&mut self, keys: Keys) {
        self.input.0 |= keys.bits();
        // TODO: KEYCNT
    }

    pub fn set_touch_pos(&mut self, pos: [u16; 2]) {
        self.spi.tsc.set_x_pos(pos[0]);
        self.spi.tsc.set_y_pos(pos[1]);
        self.spi.tsc.set_pen_down(true, &mut self.input);
    }

    pub fn end_touch(&mut self) {
        self.spi.tsc.clear_x_pos();
        self.spi.tsc.clear_y_pos();
        self.spi.tsc.set_pen_down(false, &mut self.input);
    }
}
