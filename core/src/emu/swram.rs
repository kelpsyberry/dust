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
    #[cfg(any(feature = "bft-r", feature = "bft-w"))]
    arm7_ptr: *mut u8,
    #[cfg(any(feature = "bft-r", feature = "bft-w"))]
    arm7_mask: u16,
    #[cfg(feature = "bft-r")]
    arm9_r_ptr: *mut u8,
    #[cfg(feature = "bft-w")]
    arm9_w_ptr: *mut u8,
    #[cfg(any(feature = "bft-r", feature = "bft-w"))]
    arm9_mask: u16,
}

impl Swram {
    pub(super) fn new() -> Self {
        Swram {
            contents: OwnedBytesCellPtr::new_zeroed(),
            control: Control(0),
            #[cfg(any(feature = "bft-r", feature = "bft-w"))]
            arm7_ptr: ptr::null_mut(),
            #[cfg(any(feature = "bft-r", feature = "bft-w"))]
            arm7_mask: 0,
            #[cfg(feature = "bft-r")]
            arm9_r_ptr: ptr::null_mut(),
            #[cfg(feature = "bft-w")]
            arm9_w_ptr: ptr::null_mut(),
            #[cfg(any(feature = "bft-r", feature = "bft-w"))]
            arm9_mask: 0,
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
                self.arm7_ptr = arm7.wram.as_mut_ptr();
                self.arm7_mask = 0xFFFF;
                #[cfg(feature = "bft-r")]
                {
                    self.arm9_r_ptr = self.mem.swram.as_mut_ptr();
                }
                #[cfg(feature = "bft-w")]
                {
                    self.arm9_w_ptr = self.mem.swram.as_mut_ptr();
                }
                self.arm9_mask = 0x7FFF;
            }
            1 => {
                self.arm7_ptr = self.mem.swram.as_mut_ptr();
                self.arm7_mask = 0x3FFF;
                #[cfg(feature = "bft-r")]
                {
                    self.arm9_r_ptr = self.mem.swram[0x4000..].as_mut_ptr();
                }
                #[cfg(feature = "bft-w")]
                {
                    self.arm9_w_ptr = self.mem.swram[0x4000..].as_mut_ptr();
                }
                self.arm9_mask = 0x3FFF;
            }
            2 => {
                self.arm7_ptr = self.mem.swram[0x4000..].as_mut_ptr();
                self.arm7_mask = 0x3FFF;
                #[cfg(feature = "bft-r")]
                {
                    self.arm9_r_ptr = self.mem.swram.as_mut_ptr();
                }
                #[cfg(feature = "bft-w")]
                {
                    self.arm9_w_ptr = self.mem.swram.as_mut_ptr();
                }
                self.arm9_mask = 0x3FFF;
            }
            _ => {
                self.arm7_ptr = self.mem.swram.as_mut_ptr();
                self.arm7_mask = 0x7FFF;
                #[cfg(feature = "bft-r")]
                {
                    self.arm9_r_ptr = ptr::null_mut();
                }
                #[cfg(feature = "bft-w")]
                {
                    self.arm9_w_ptr = ptr::null_mut();
                }
                self.arm9_mask = 0;
            }
        }
    }

    #[cfg(any(feature = "bft-r", feature = "bft-w"))]
    pub(crate) fn arm7_swram_ptr(&self) -> *mut u8 {
        self.arm7_swram_ptr
    }

    #[cfg(any(feature = "bft-r", feature = "bft-w"))]
    pub(crate) fn arm7_swram_mask(&self) -> u16 {
        self.arm7_swram_mask
    }

    #[cfg(feature = "bft-r")]
    pub(crate) fn arm9_swram_r_ptr(&self) -> *mut u8 {
        self.arm9_swram_r_ptr
    }

    #[cfg(feature = "bft-w")]
    pub(crate) fn arm9_swram_w_ptr(&self) -> *mut u8 {
        self.arm9_swram_w_ptr
    }

    #[cfg(any(feature = "bft-r", feature = "bft-w"))]
    pub(crate) fn arm9_swram_mask(&self) -> u16 {
        self.arm9_swram_mask
    }
}
