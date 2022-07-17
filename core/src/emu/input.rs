use super::Emu;
use crate::{cpu, utils::Savestate};
use bitflags::bitflags;

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

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq, Savestate)]
    pub const struct Status(pub u32): Debug {
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

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq, Savestate)]
    pub const struct KeyIrqControl(pub u16): Debug {
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
        pub mask: u16 @ 0..=9,
        pub enabled: bool @ 14,
        pub condition: bool @ 15,
    }
}

#[derive(Savestate)]
pub struct Input {
    pub(crate) status: Status,
    key_irq_control: [KeyIrqControl; 2],
    key_irq_triggered: [bool; 2],
}

impl Input {
    pub(super) fn new() -> Self {
        Input {
            status: Status(0x007F_03FF),
            key_irq_control: [KeyIrqControl(0); 2],
            key_irq_triggered: [false; 2],
        }
    }

    #[inline]
    pub fn status(&self) -> Status {
        self.status
    }

    #[inline]
    pub fn arm7_key_irq_control(&self) -> KeyIrqControl {
        self.key_irq_control[0]
    }

    #[inline]
    pub fn arm9_key_irq_control(&self) -> KeyIrqControl {
        self.key_irq_control[1]
    }
}

impl<E: cpu::Engine> Emu<E> {
    pub fn press_keys(&mut self, keys: Keys) {
        self.input.status.0 &= !keys.bits();
        self.update_key_irq::<false>();
        self.update_key_irq::<true>();
    }

    pub fn release_keys(&mut self, keys: Keys) {
        self.input.status.0 |= keys.bits();
        self.update_key_irq::<false>();
        self.update_key_irq::<true>();
    }

    pub fn write_arm7_key_irq_control(&mut self, value: KeyIrqControl) {
        self.input.key_irq_control[0] = value;
        self.update_key_irq::<false>();
    }

    pub fn write_arm9_key_irq_control(&mut self, value: KeyIrqControl) {
        self.input.key_irq_control[1] = value;
        self.update_key_irq::<true>();
    }

    fn update_key_irq<const ARM9: bool>(&mut self) {
        if !self.input.key_irq_control[ARM9 as usize].enabled() {
            self.input.key_irq_triggered[ARM9 as usize] = false;
            return;
        }
        let mask = self.input.key_irq_control[ARM9 as usize].mask();
        let masked = !self.input.status.0 as u16 & mask;
        let triggered = if self.input.key_irq_control[ARM9 as usize].condition() {
            masked == mask
        } else {
            masked != 0
        };
        if !self.input.key_irq_triggered[ARM9 as usize] && triggered {
            if ARM9 {
                self.arm9.irqs.write_requested(
                    self.arm9.irqs.requested().with_keypad(true),
                    &mut self.arm9.schedule,
                );
            } else {
                self.arm7.irqs.write_requested(
                    self.arm7.irqs.requested().with_keypad(true),
                    &mut self.arm7.schedule,
                );
            }
        }
        self.input.key_irq_triggered[ARM9 as usize] = triggered;
    }

    pub fn set_touch_pos(&mut self, pos: [u16; 2]) {
        self.spi.tsc.set_x_pos(pos[0]);
        self.spi.tsc.set_y_pos(pos[1]);
        self.spi.tsc.set_pen_down(true, &mut self.input.status);
    }

    pub fn end_touch(&mut self) {
        self.spi.tsc.clear_x_pos();
        self.spi.tsc.clear_y_pos();
        self.spi.tsc.set_pen_down(false, &mut self.input.status);
    }
}
