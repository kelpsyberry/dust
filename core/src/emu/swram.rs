use crate::{
    cpu::{self, arm7::Arm7, arm9::Arm9},
    utils::{bitfield_debug, OwnedBytesCellPtr},
};
#[cfg(any(feature = "bft-r", feature = "bft-w"))]
use core::ptr;

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct Control(pub u8) {
        pub layout: u8 @ 0..=1,
    }
}

pub struct Swram {
    contents: OwnedBytesCellPtr<0x8000>,
    control: Control,
    #[cfg(feature = "bft-r")]
    arm7_r_ptr: *const u8,
    #[cfg(feature = "bft-w")]
    arm7_w_ptr: *mut u8,
    #[cfg(any(feature = "bft-r", feature = "bft-w"))]
    arm7_mask: u16,
    #[cfg(feature = "bft-r")]
    arm9_r_ptr: *const u8,
    #[cfg(feature = "bft-w")]
    arm9_w_ptr: *mut u8,
    #[cfg(any(feature = "bft-r", feature = "bft-w"))]
    arm9_mask: u16,
    #[cfg(feature = "bft-r")]
    zero_buffer: OwnedBytesCellPtr<4>,
    #[cfg(feature = "bft-w")]
    ignore_buffer: OwnedBytesCellPtr<4>,
}

impl Swram {
    pub(super) fn new() -> Self {
        Swram {
            contents: OwnedBytesCellPtr::new_zeroed(),
            control: Control(0),
            #[cfg(feature = "bft-r")]
            arm7_r_ptr: ptr::null(),
            #[cfg(feature = "bft-w")]
            arm7_w_ptr: ptr::null_mut(),
            #[cfg(any(feature = "bft-r", feature = "bft-w"))]
            arm7_mask: 0,
            #[cfg(feature = "bft-r")]
            arm9_r_ptr: ptr::null(),
            #[cfg(feature = "bft-w")]
            arm9_w_ptr: ptr::null_mut(),
            #[cfg(any(feature = "bft-r", feature = "bft-w"))]
            arm9_mask: 0,
            #[cfg(feature = "bft-r")]
            zero_buffer: OwnedBytesCellPtr::new_zeroed(),
            #[cfg(feature = "bft-w")]
            ignore_buffer: OwnedBytesCellPtr::new_zeroed(),
        }
    }

    #[inline]
    pub fn contents(&self) -> &OwnedBytesCellPtr<0x8000> {
        &self.contents
    }

    #[inline]
    pub fn control(&self) -> Control {
        self.control
    }

    #[inline]
    pub fn set_control<E: cpu::Engine>(
        &mut self,
        value: Control,
        arm7: &mut Arm7<E>,
        arm9: &mut Arm9<E>,
    ) {
        let new_value = value.0 & 3;
        if new_value == self.control.0 {
            return;
        }
        self.control.0 = new_value;
        self.recalc(arm7, arm9);
    }

    pub(super) fn recalc<E: cpu::Engine>(&mut self, arm7: &mut Arm7<E>, arm9: &mut Arm9<E>) {
        arm7.recalc_swram(self);
        arm9.recalc_swram(self);
        #[cfg(any(feature = "bft-r", feature = "bft-w"))]
        match self.control.0 & 3 {
            0 => {
                #[cfg(feature = "bft-r")]
                {
                    self.arm7_r_ptr = arm7.wram.as_ptr();
                }
                #[cfg(feature = "bft-w")]
                {
                    self.arm7_w_ptr = arm7.wram.as_ptr();
                }
                self.arm7_mask = 0xFFFF;
                #[cfg(feature = "bft-r")]
                {
                    self.arm9_r_ptr = self.contents.as_ptr();
                }
                #[cfg(feature = "bft-w")]
                {
                    self.arm9_w_ptr = self.contents.as_ptr();
                }
                self.arm9_mask = 0x7FFF;
            }
            1 => {
                #[cfg(feature = "bft-r")]
                {
                    self.arm7_r_ptr = self.contents.as_ptr();
                }
                #[cfg(feature = "bft-w")]
                {
                    self.arm7_w_ptr = self.contents.as_ptr();
                }
                self.arm7_mask = 0x3FFF;
                #[cfg(feature = "bft-r")]
                {
                    self.arm9_r_ptr = self.contents[0x4000..].as_ptr();
                }
                #[cfg(feature = "bft-w")]
                {
                    self.arm9_w_ptr = self.contents[0x4000..].as_ptr();
                }
                self.arm9_mask = 0x3FFF;
            }
            2 => {
                #[cfg(feature = "bft-r")]
                {
                    self.arm7_r_ptr = self.contents[0x4000..].as_ptr();
                }
                #[cfg(feature = "bft-w")]
                {
                    self.arm7_w_ptr = self.contents[0x4000..].as_ptr();
                }
                self.arm7_mask = 0x3FFF;
                #[cfg(feature = "bft-r")]
                {
                    self.arm9_r_ptr = self.contents.as_ptr();
                }
                #[cfg(feature = "bft-w")]
                {
                    self.arm9_w_ptr = self.contents.as_ptr();
                }
                self.arm9_mask = 0x3FFF;
            }
            _ => {
                #[cfg(feature = "bft-r")]
                {
                    self.arm7_r_ptr = self.contents.as_ptr();
                }
                #[cfg(feature = "bft-w")]
                {
                    self.arm7_w_ptr = self.contents.as_ptr();
                }
                self.arm7_mask = 0x7FFF;
                #[cfg(feature = "bft-r")]
                {
                    self.arm9_r_ptr = self.zero_buffer.as_ptr();
                }
                #[cfg(feature = "bft-w")]
                {
                    self.arm9_w_ptr = self.ignore_buffer.as_ptr();
                }
                self.arm9_mask = 0;
            }
        }
    }

    #[cfg(feature = "bft-r")]
    pub(crate) fn arm7_r_ptr(&self) -> *mut u8 {
        self.arm7_r_ptr
    }

    #[cfg(feature = "bft-w")]
    pub(crate) fn arm7_w_ptr(&self) -> *mut u8 {
        self.arm7_w_ptr
    }

    #[cfg(any(feature = "bft-r", feature = "bft-w"))]
    pub(crate) fn arm7_mask(&self) -> u16 {
        self.arm7_mask
    }

    #[cfg(feature = "bft-r")]
    pub(crate) fn arm9_r_ptr(&self) -> *mut u8 {
        self.arm9_r_ptr
    }

    #[cfg(feature = "bft-w")]
    pub(crate) fn arm9_w_ptr(&self) -> *mut u8 {
        self.arm9_w_ptr
    }

    #[cfg(any(feature = "bft-r", feature = "bft-w"))]
    pub(crate) fn arm9_mask(&self) -> u16 {
        self.arm9_mask
    }
}
