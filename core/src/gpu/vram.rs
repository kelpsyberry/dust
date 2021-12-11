mod access;
mod bank_cnt;

use crate::utils::{bitfield_debug, OwnedBytesCellPtr, Zero};

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct BankControl(pub u8) {
        pub mst: u8 @ 0..=2,
        pub offset: u8 @ 3..=4,
        pub enabled: bool @ 7,
    }
}

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct Arm7Status(pub u8) {
        pub c_used_as_arm7: bool @ 0,
        pub d_used_as_arm7: bool @ 1,
    }
}

#[repr(C)]
pub struct Banks {
    pub a: OwnedBytesCellPtr<0x2_0000>,
    pub b: OwnedBytesCellPtr<0x2_0000>,
    pub c: OwnedBytesCellPtr<0x2_0000>,
    pub d: OwnedBytesCellPtr<0x2_0000>,
    pub e: OwnedBytesCellPtr<0x1_0000>,
    pub f: OwnedBytesCellPtr<0x4000>,
    pub g: OwnedBytesCellPtr<0x4000>,
    pub h: OwnedBytesCellPtr<0x8000>,
    pub i: OwnedBytesCellPtr<0x4000>,
    pub palette: OwnedBytesCellPtr<0x800>,
    pub oam: OwnedBytesCellPtr<0x800>,
    zero: OwnedBytesCellPtr<0x2_0000>, // Used to return zero for reads
    ignore: OwnedBytesCellPtr<0x2_0000>, // Used to ignore writes
}

unsafe impl Zero for Banks {}

#[repr(C)]
struct Map {
    a_bg: [u8; 0x20],
    a_obj: [u8; 0x10],
    a_bg_ext_pal: [u8; 2],
    a_obj_ext_pal: u8,
    b_bg: [u8; 4],
    b_obj: u8,
    texture: [u8; 4],
    tex_pal: [u8; 6],
    arm7: [u8; 2],
    lcdc_r_ptrs: [*const u8; 0x40],    // 0x4000 B granularity
    lcdc_w_ptrs: [*mut u8; 0x40],      // 0x4000 B granularity
    a_bg_r_ptrs: [*const u8; 0x20],    // 0x4000 B granularity
    a_bg_w_ptrs: [*mut u8; 0x20],      // 0x4000 B granularity
    a_obj_r_ptrs: [*const u8; 0x10],   // 0x4000 B granularity
    a_obj_w_ptrs: [*mut u8; 0x10],     // 0x4000 B granularity
    a_bg_ext_pal_ptrs: [*const u8; 2], // 0x4000 B granularity
    a_obj_ext_pal_ptr: *const u8,      // 0x2000 B granularity
    b_bg_r_ptrs: [*const u8; 8],       // 0x4000 B granularity
    b_bg_w_ptrs: [*mut u8; 8],         // 0x4000 B granularity
    b_obj_r_ptrs: [*const u8; 8],      // 0x4000 B granularity
    b_obj_w_ptrs: [*mut u8; 8],        // 0x4000 B granularity
    b_bg_ext_pal_ptr: *const u8,       // 0x8000 B granularity
    b_obj_ext_pal_ptr: *const u8,      // 0x2000 B granularity
    texture_ptrs: [*const u8; 4],      // 0x2_0000 B granularity
    tex_pal_ptrs: [*const u8; 6],      // 0x4000 B granularity
    arm7_r_ptrs: [*const u8; 2],       // 0x2_0000 B granularity
    arm7_w_ptrs: [*mut u8; 2],         // 0x2_0000 B granularity
}

#[repr(C)]
pub struct Vram {
    pub banks: Banks,
    map: Map,
    bank_control: [BankControl; 9],
    arm7_status: Arm7Status,
}

impl Vram {
    #[inline]
    pub(super) fn new() -> Self {
        let banks = Banks {
            a: OwnedBytesCellPtr::new_zeroed(),
            b: OwnedBytesCellPtr::new_zeroed(),
            c: OwnedBytesCellPtr::new_zeroed(),
            d: OwnedBytesCellPtr::new_zeroed(),
            e: OwnedBytesCellPtr::new_zeroed(),
            f: OwnedBytesCellPtr::new_zeroed(),
            g: OwnedBytesCellPtr::new_zeroed(),
            h: OwnedBytesCellPtr::new_zeroed(),
            i: OwnedBytesCellPtr::new_zeroed(),
            palette: OwnedBytesCellPtr::new_zeroed(),
            oam: OwnedBytesCellPtr::new_zeroed(),
            zero: OwnedBytesCellPtr::new_zeroed(),
            ignore: OwnedBytesCellPtr::new_zeroed(),
        };
        let zero = banks.zero.as_ptr();
        let ignore = banks.ignore.as_ptr();
        Vram {
            banks,
            map: Map {
                a_bg: [0; 0x20],
                a_obj: [0; 0x10],
                a_bg_ext_pal: [0; 2],
                a_obj_ext_pal: 0,
                b_bg: [0; 4],
                b_obj: 0,
                texture: [0; 4],
                tex_pal: [0; 6],
                arm7: [0; 2],
                lcdc_r_ptrs: [zero; 0x40],
                lcdc_w_ptrs: [ignore; 0x40],
                a_bg_r_ptrs: [zero; 0x20],
                a_bg_w_ptrs: [ignore; 0x20],
                a_obj_r_ptrs: [zero; 0x10],
                a_obj_w_ptrs: [ignore; 0x10],
                a_bg_ext_pal_ptrs: [zero; 2],
                a_obj_ext_pal_ptr: zero,
                b_bg_r_ptrs: [zero; 8],
                b_bg_w_ptrs: [ignore; 8],
                b_obj_r_ptrs: [zero; 8],
                b_obj_w_ptrs: [ignore; 8],
                b_bg_ext_pal_ptr: zero,
                b_obj_ext_pal_ptr: zero,
                texture_ptrs: [zero; 4],
                tex_pal_ptrs: [zero; 6],
                arm7_r_ptrs: [zero; 2],
                arm7_w_ptrs: [ignore; 2],
            },
            bank_control: [BankControl(0); 9],
            arm7_status: Arm7Status(0),
        }
    }

    #[inline]
    pub const fn bank_control(&self) -> &[BankControl; 9] {
        &self.bank_control
    }

    #[inline]
    pub const fn arm7_status(&self) -> Arm7Status {
        self.arm7_status
    }
}
