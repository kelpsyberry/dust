mod access;
mod bank_cnt;

use crate::{
    cpu::{arm7, arm9, Engine},
    utils::{zero, zeroed_box, OwnedBytesCellPtr, Savestate, Zero},
};
use core::cell::{Cell, UnsafeCell};

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq, Savestate)]
    pub const struct BankControl(pub u8): Debug {
        pub mst: u8 @ 0..=2,
        pub offset: u8 @ 3..=4,
        pub enabled: bool @ 7,
    }
}

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq, Savestate)]
    pub const struct Arm7Status(pub u8): Debug {
        pub c_used_as_arm7: bool @ 0,
        pub d_used_as_arm7: bool @ 1,
    }
}

#[repr(C)]
#[derive(Savestate)]
pub struct Banks {
    pub a: OwnedBytesCellPtr<0x2_0000>,
    pub b: OwnedBytesCellPtr<0x2_0000>,
    pub c: OwnedBytesCellPtr<0x2_0000>,
    pub d: OwnedBytesCellPtr<0x2_0000>,
    pub e: OwnedBytesCellPtr<0x1_0000>,
    pub f: OwnedBytesCellPtr<0x4000>,
    pub g: OwnedBytesCellPtr<0x4000>,
    pub h: OwnedBytesCellPtr<0x8006>,
    pub i: OwnedBytesCellPtr<0x4000>,
}

#[repr(C)]
#[derive(Savestate)]
struct Map {
    // NOTE: The cells are an ugly hack to avoid macros but also work around simultaneous mutable
    // and immutable borrows
    a_bg: [Cell<u8>; 0x20],
    a_obj: [Cell<u8>; 0x10],
    a_bg_ext_pal: [Cell<u8>; 2],
    a_obj_ext_pal: [Cell<u8>; 1],
    b_bg: [Cell<u8>; 4],
    b_obj: [Cell<u8>; 1],
    texture: [Cell<u8>; 4],
    tex_pal: [Cell<u8>; 6],
    arm7: [Cell<u8>; 2],
}

unsafe impl Zero for Map {}

#[repr(C)]
struct Writeback {
    // NOTE: Same as `Map`
    a_bg: UnsafeCell<[usize; 0x8_0000 / usize::BITS as usize]>,
    a_obj: UnsafeCell<[usize; 0x4_0000 / usize::BITS as usize]>,
    b_bg: UnsafeCell<[usize; 0x2_0000 / usize::BITS as usize]>,
    b_obj: UnsafeCell<[usize; 0x2_0000 / usize::BITS as usize]>,
    arm7: UnsafeCell<[usize; 0x4_0000 / usize::BITS as usize]>,
}

unsafe impl Zero for Writeback {}

#[derive(Default)]
pub struct Updates {
    pub bg: u32,               // 8-32 16 KiB (0x4000 bytes) regions
    pub obj: u16,              // 8-16 16 KiB (0x4000 bytes) regions
    pub bg_ext_palette: u8,    // 2 16 KiB (0x4000 bytes) regions
    pub obj_ext_palette: bool, // One 8 KiB (0x2000 bytes) region
    pub palette: bool,         // One 1 KiB (0x400 bytes) region
    pub oam: bool,             // One 1 KiB (0x400 bytes) region
}

#[repr(C)]
#[derive(Savestate)]
#[load(in_place_only, post = "self.post_load()")]
#[store(pre = "self.flush_writeback()")]
pub struct Vram {
    // Six bytes need to be added to the palette and seven to A/B BG VRAM to allow for 64-bit loads
    // from the last color
    bank_control: [BankControl; 9],
    arm7_status: Arm7Status,
    pub banks: Banks,
    map: Map,
    #[savestate(skip)]
    writeback: Box<Writeback>,

    #[savestate(skip)]
    pub bg_obj_updates: Option<UnsafeCell<[Updates; 2]>>,

    #[savestate(skip)]
    lcdc_r_ptrs: [*const u8; 0x40], // 0x4000 B granularity
    #[savestate(skip)]
    lcdc_w_ptrs: [*mut u8; 0x40], // 0x4000 B granularity
    #[savestate(skip)]
    pub a_bg: OwnedBytesCellPtr<0x8_0007>,
    #[savestate(skip)]
    pub a_obj: OwnedBytesCellPtr<0x4_0007>,
    #[savestate(skip)]
    pub a_bg_ext_pal: OwnedBytesCellPtr<0x8006>,
    #[savestate(skip)]
    pub a_obj_ext_pal: OwnedBytesCellPtr<0x2006>,
    #[savestate(skip)]
    pub b_bg: OwnedBytesCellPtr<0x2_0007>,
    #[savestate(skip)]
    pub b_obj: OwnedBytesCellPtr<0x2_0007>,
    #[savestate(skip)]
    pub b_bg_ext_pal_ptr: *const u8,
    #[savestate(skip)]
    pub b_obj_ext_pal_ptr: *const u8,
    #[savestate(skip)]
    pub(super) texture: OwnedBytesCellPtr<0x8_0000>,
    #[savestate(skip)]
    pub(super) tex_pal: OwnedBytesCellPtr<0x1_8000>,
    #[savestate(skip)]
    arm7: OwnedBytesCellPtr<0x4_0000>,

    pub palette: OwnedBytesCellPtr<0x806>,
    pub oam: OwnedBytesCellPtr<0x800>,

    #[savestate(skip)]
    zero_buffer: OwnedBytesCellPtr<0x8006>, // Used to return zero for reads
    #[savestate(skip)]
    ignore_buffer: OwnedBytesCellPtr<0x8000>, // Used to ignore writes
}

impl Vram {
    #[inline]
    pub(super) fn new(bg_obj_vram_tracking: bool) -> Self {
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
        };

        let zero_buffer = OwnedBytesCellPtr::new_zeroed();
        let ignore_buffer = OwnedBytesCellPtr::new_zeroed();

        Vram {
            bank_control: [BankControl(0); 9],
            arm7_status: Arm7Status(0),
            banks,
            map: zero(),
            writeback: zeroed_box(),

            bg_obj_updates: bg_obj_vram_tracking.then(UnsafeCell::default),

            lcdc_r_ptrs: [zero_buffer.as_ptr(); 0x40],
            lcdc_w_ptrs: [ignore_buffer.as_ptr(); 0x40],
            a_bg: OwnedBytesCellPtr::new_zeroed(),
            a_obj: OwnedBytesCellPtr::new_zeroed(),
            a_bg_ext_pal: OwnedBytesCellPtr::new_zeroed(),
            a_obj_ext_pal: OwnedBytesCellPtr::new_zeroed(),
            b_bg: OwnedBytesCellPtr::new_zeroed(),
            b_obj: OwnedBytesCellPtr::new_zeroed(),
            b_bg_ext_pal_ptr: zero_buffer.as_ptr(),
            b_obj_ext_pal_ptr: zero_buffer.as_ptr(),
            texture: OwnedBytesCellPtr::new_zeroed(),
            tex_pal: OwnedBytesCellPtr::new_zeroed(),
            arm7: OwnedBytesCellPtr::new_zeroed(),

            palette: OwnedBytesCellPtr::new_zeroed(),
            oam: OwnedBytesCellPtr::new_zeroed(),

            zero_buffer,
            ignore_buffer,
        }
    }

    fn post_load(&mut self) {
        self.mark_all_vram_updated();
    }

    fn mark_all_vram_updated(&mut self) {
        if let Some(updates) = &mut self.bg_obj_updates {
            let updates = updates.get_mut();
            updates[0] = Updates {
                bg: 0xFFFF_FFFF,
                obj: 0xFFFF,
                bg_ext_palette: 3,
                obj_ext_palette: true,
                palette: true,
                oam: true,
            };
            updates[1] = Updates {
                bg: 0xFF,
                obj: 0xFF,
                bg_ext_palette: 3,
                obj_ext_palette: true,
                palette: true,
                oam: true,
            };
        }
    }

    #[inline]
    pub(super) fn set_vram_tracking<E: Engine>(
        &mut self,
        bg_obj_vram_tracking: bool,
        arm9: &mut arm9::Arm9<E>,
    ) {
        if bg_obj_vram_tracking == self.bg_obj_updates.is_some() {
            return;
        }
        self.bg_obj_updates = bg_obj_vram_tracking.then(UnsafeCell::default);
        self.mark_all_vram_updated();
        self.restore_cpu_bg_obj_mappings(arm9);
    }

    #[inline]
    pub const fn bank_control(&self) -> &[BankControl; 9] {
        &self.bank_control
    }

    #[inline]
    pub const fn arm7_status(&self) -> Arm7Status {
        self.arm7_status
    }

    pub(crate) fn setup_arm7_bus_ptrs(&mut self, ptrs: &mut arm7::bus::ptrs::Ptrs) {
        unsafe {
            ptrs.map_range(
                arm7::bus::ptrs::mask::R,
                self.arm7.as_ptr(),
                0x4_0000,
                (0x0600_0000, 0x06FF_FFFF),
            );
        }
    }

    pub(crate) fn setup_arm9_bus_ptrs(&mut self, ptrs: &mut arm9::bus::ptrs::Ptrs) {
        unsafe {
            ptrs.map_range(
                arm9::bus::ptrs::mask::R,
                self.a_bg.as_ptr(),
                0x8_0000,
                (0x0600_0000, 0x061F_FFFF),
            );
            ptrs.map_range(
                arm9::bus::ptrs::mask::R,
                self.b_bg.as_ptr(),
                0x2_0000,
                (0x0620_0000, 0x063F_FFFF),
            );
            ptrs.map_range(
                arm9::bus::ptrs::mask::R,
                self.a_obj.as_ptr(),
                0x4_0000,
                (0x0640_0000, 0x065F_FFFF),
            );
            ptrs.map_range(
                arm9::bus::ptrs::mask::R,
                self.b_obj.as_ptr(),
                0x2_0000,
                (0x0660_0000, 0x067F_FFFF),
            );
            ptrs.map_range(
                arm9::bus::ptrs::mask::R,
                self.zero_buffer.as_ptr(),
                0x8000,
                (0x0680_0000, 0x06FF_FFFF),
            );
        }
    }
}
