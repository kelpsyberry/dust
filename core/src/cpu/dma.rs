use crate::utils::bitfield_debug;

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct Control(pub u32) {
        pub dst_addr_control: u8 @ 21..=22,
        pub src_addr_control: u8 @ 23..=24,
        pub repeat: bool @ 25,
        pub is_32_bit: bool @ 26,
        pub timing_arm7: u8 @ 28..=29,
        pub timing_arm9: u8 @ 27..=29,
        pub fire_irq: bool @ 30,
        pub enabled: bool @ 31,
    }
}

mod bounded {
    use crate::utils::bounded_int_lit;
    bounded_int_lit!(pub struct Index(u8), max 3);
}
pub use bounded::Index;

pub struct Channel<T: Copy, BU> {
    pub(crate) control: Control,
    pub(crate) src_addr_incr: i32,
    pub(crate) dst_addr_incr: i32,
    unit_count_mask: u32,
    pub(crate) unit_count: u32,
    pub(crate) remaining_units: u32,
    pub(crate) remaining_batch_units: BU,
    src_addr_mask: u32,
    pub(crate) src_addr: u32,
    pub(crate) cur_src_addr: u32,
    dst_addr_mask: u32,
    pub(crate) dst_addr: u32,
    pub(crate) cur_dst_addr: u32,
    pub(crate) timing: T,
    pub(crate) repeat: bool,
    pub(crate) next_access_is_nseq: bool,
}

impl<T: Copy, BU> Channel<T, BU> {
    #[inline]
    pub(crate) fn new(
        unit_count_mask: u32,
        src_addr_mask: u32,
        dst_addr_mask: u32,
        timing: T,
        remaining_batch_units: BU,
    ) -> Self {
        Channel {
            control: Control(0),
            src_addr_incr: 2,
            dst_addr_incr: 2,
            unit_count_mask,
            unit_count: unit_count_mask + 1,
            remaining_units: 0,
            remaining_batch_units,
            src_addr_mask,
            src_addr: 0,
            cur_src_addr: 0,
            dst_addr_mask,
            dst_addr: 0,
            cur_dst_addr: 0,
            timing,
            repeat: false,
            next_access_is_nseq: false,
        }
    }

    #[inline]
    pub fn control(&self) -> Control {
        self.control
    }

    pub(crate) fn write_control_low(&mut self, value: u16) {
        self.control.0 = (self.control.0 & 0xFFFF_0000) | (value as u32 & self.unit_count_mask);
        self.unit_count = ((self.unit_count & 0xFFFF_0000) | value as u32) & self.unit_count_mask;
        if self.unit_count == 0 {
            self.unit_count = self.unit_count_mask + 1;
        }
    }

    #[inline]
    pub fn unit_count_mask(&self) -> u32 {
        self.unit_count_mask
    }

    #[inline]
    pub fn timing(&self) -> T {
        self.timing
    }

    #[inline]
    pub fn src_addr_mask(&self) -> u32 {
        self.src_addr_mask
    }

    #[inline]
    pub fn src_addr(&self) -> u32 {
        self.src_addr
    }

    #[inline]
    pub fn write_src_addr(&mut self, value: u32) {
        self.src_addr = value & self.src_addr_mask;
    }

    #[inline]
    pub fn dst_addr_mask(&self) -> u32 {
        self.dst_addr_mask
    }

    #[inline]
    pub fn dst_addr(&self) -> u32 {
        self.dst_addr
    }

    #[inline]
    pub fn write_dst_addr(&mut self, value: u32) {
        self.dst_addr = value & self.dst_addr_mask;
    }
}

pub struct Controller<T: Copy, BU> {
    pub channels: [Channel<T, BU>; 4],
    pub(crate) cur_channel: Option<Index>,
    pub(crate) running_channels: u8,
}

impl<T: Copy, BU> Controller<T, BU> {
    #[inline]
    pub fn cur_channel(&self) -> Option<Index> {
        self.cur_channel
    }

    #[inline]
    pub fn running_channels(&self) -> u8 {
        self.running_channels
    }
}
