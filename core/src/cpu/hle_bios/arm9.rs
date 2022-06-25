use super::common;
use crate::{
    cpu::{
        arm9::{self, bus},
        bus::CpuAccess,
        CoreData, Engine, Schedule,
    },
    emu::Emu,
};

#[allow(clippy::unusual_byte_groupings)]
pub const BIOS_CALL_INSTR: u32 = 0xFF0_B105_0;
pub const BIOS_CALL_INSTR_MASK: u32 = 0xFFFF_FFF0;

#[allow(clippy::unusual_byte_groupings)]
pub static BIOS: [u8; arm9::BIOS_SIZE] = {
    let mut bytes = [0; arm9::BIOS_SIZE];

    let mut i = 0;
    while i < arm9::BIOS_SIZE {
        bytes[i] = (0xE7FF_DEFF_u32 >> ((i & 3) << 3)) as u8;
        i += 1;
    }

    macro_rules! write_32 {
        ($addr: expr, $value: expr) => {
            bytes[$addr] = $value as u8;
            bytes[$addr | 1] = ($value >> 8) as u8;
            bytes[$addr | 2] = ($value >> 16) as u8;
            bytes[$addr | 3] = ($value >> 24) as u8;
        };
    }

    write_32!(0x004, BIOS_CALL_INSTR | 3);
    write_32!(0x008, BIOS_CALL_INSTR | 5);
    write_32!(0x018, BIOS_CALL_INSTR | 1);
    write_32!(0x200, BIOS_CALL_INSTR);
    write_32!(0x204, BIOS_CALL_INSTR | 2);
    write_32!(0x298, BIOS_CALL_INSTR | 5);

    bytes
};

fn bit_unpack<E: Engine>(emu: &mut Emu<E>, src_addr: u32, dst_addr: u32, unpack_data_addr: u32) {
    // TODO
    #[cfg(feature = "log")]
    slog::error!(emu.arm9.logger, "Unimplemented BitUnPack SWI");
}

fn diff_8_unfilter_write_8<E: Engine>(emu: &mut Emu<E>) {
    // TODO
    #[cfg(feature = "log")]
    slog::error!(
        emu.arm9.logger,
        "Unimplemented Diff8bitUnFilterWrite8bit SWI"
    );
}

fn diff_16_unfilter<E: Engine>(emu: &mut Emu<E>) {
    // TODO
    #[cfg(feature = "log")]
    slog::error!(
        emu.arm9.logger,
        "Unimplemented Diff8bitUnFilterWrite16bit SWI"
    );
}

fn huff_uncomp_read_callback<E: Engine>(emu: &mut Emu<E>) {
    // TODO
    #[cfg(feature = "log")]
    slog::error!(
        emu.arm9.logger,
        "Unimplemented HuffUnCompReadByCallback SWI"
    );
}

fn lz77_uncomp_read_normal_write_8<E: Engine>(emu: &mut Emu<E>) {
    // TODO
    #[cfg(feature = "log")]
    slog::error!(
        emu.arm9.logger,
        "Unimplemented LZ77UnCompReadNormalWrite8bit SWI"
    );
}

fn lz77_uncomp_read_callback_write_16<E: Engine>(emu: &mut Emu<E>) {
    // TODO
    #[cfg(feature = "log")]
    slog::error!(
        emu.arm9.logger,
        "Unimplemented LZ77UnCompReadByCallbackWrite16bit SWI"
    );
}

fn rl_uncomp_read_normal_write_8<E: Engine>(emu: &mut Emu<E>) {
    // TODO
    #[cfg(feature = "log")]
    slog::error!(
        emu.arm9.logger,
        "Unimplemented RLUnCompReadNormalWrite8bit SWI"
    );
}

fn rl_uncomp_read_callback_write_16<E: Engine>(emu: &mut Emu<E>) {
    // TODO
    #[cfg(feature = "log")]
    slog::error!(
        emu.arm9.logger,
        "Unimplemented RLUnCompReadByCallbackWrite16bit SWI"
    );
}

fn cpu_set<E: Engine>(emu: &mut Emu<E>, r0_3: [u32; 4]) -> [u32; 2] {
    let mut src_addr = r0_3[0];
    let mut dst_addr = r0_3[1];
    let units = r0_3[3] & 0xF_FFFF;
    let fixed = r0_3[3] & 1 << 24 != 0;
    if r0_3[3] & 1 << 26 != 0 {
        if fixed {
            let value = bus::read_32::<CpuAccess, _, false>(emu, src_addr);
            for _ in 0..units {
                bus::write_32::<CpuAccess, _>(emu, dst_addr, value);
                src_addr = src_addr.wrapping_add(4);
                dst_addr = dst_addr.wrapping_add(4);
            }
        } else {
            for _ in 0..units {
                let value = bus::read_32::<CpuAccess, _, false>(emu, src_addr);
                bus::write_32::<CpuAccess, _>(emu, dst_addr, value);
                src_addr = src_addr.wrapping_add(4);
                dst_addr = dst_addr.wrapping_add(4);
            }
        }
    } else if fixed {
        let value = bus::read_16::<CpuAccess, _>(emu, src_addr);
        for _ in 0..units {
            bus::write_16::<CpuAccess, _>(emu, dst_addr, value);
            src_addr = src_addr.wrapping_add(2);
            dst_addr = dst_addr.wrapping_add(2);
        }
    } else {
        for _ in 0..units {
            let value = bus::read_16::<CpuAccess, _>(emu, src_addr);
            bus::write_16::<CpuAccess, _>(emu, dst_addr, value);
            src_addr = src_addr.wrapping_add(2);
            dst_addr = dst_addr.wrapping_add(2);
        }
    }
    [src_addr, dst_addr]
}

fn cpu_fast_set<E: Engine>(emu: &mut Emu<E>, r0_3: [u32; 4]) -> (u32, u32, u32) {
    let mut src_addr = r0_3[0];
    let mut dst_addr = r0_3[1];
    let units = r0_3[3] & 0xF_FFFF;
    let fixed = r0_3[3] & 1 << 24 != 0;
    let mut r3 = r0_3[3];
    if fixed {
        let value = bus::read_32::<CpuAccess, _, false>(emu, src_addr);
        r3 = value;
        for _ in 0..units {
            bus::write_32::<CpuAccess, _>(emu, dst_addr, value);
            src_addr = src_addr.wrapping_add(4);
            dst_addr = dst_addr.wrapping_add(4);
        }
    } else if units != 0 {
        let r3_i = if units >= 8 { (units & !7) - 6 } else { units };
        for i in 0..units {
            let value = bus::read_32::<CpuAccess, _, false>(emu, src_addr);
            if i == r3_i {
                r3 = value;
            }
            bus::write_32::<CpuAccess, _>(emu, dst_addr, value);
            src_addr = src_addr.wrapping_add(4);
            dst_addr = dst_addr.wrapping_add(4);
        }
    }
    (src_addr, dst_addr, r3)
}

fn halt<E: Engine>(emu: &mut Emu<E>) {
    emu.arm9.irqs.halt(&mut emu.arm9.schedule);
}

fn intr_wait<E: Engine>(emu: &mut Emu<E>, discard_old: bool, mask: u32) {
    emu.arm9.hle_bios.intr_wait_mask = mask;
    if discard_old {
        process_intr_wait_irqs(emu);
    }
    E::Arm9Data::jump(emu, 0xFFFF_0200);
    halt(emu);
}

fn process_intr_wait_irqs<E: Engine>(emu: &mut Emu<E>) -> bool {
    let requested_mask_addr = emu
        .arm9
        .cp15
        .dtcm_control()
        .base_addr()
        .wrapping_add(0x3FF8);

    let mut requested = bus::read_32::<CpuAccess, _, false>(emu, requested_mask_addr);
    let masked = requested & emu.arm9.hle_bios.intr_wait_mask;
    requested ^= masked;
    bus::write_32::<CpuAccess, _>(emu, requested_mask_addr, requested);

    emu.arm9
        .irqs
        .write_master_enable(true, &mut emu.arm9.schedule);

    emu.arm9.hle_bios.swi_r0_3[0] = 1;

    masked != 0
}

pub fn resume_intr_wait<E: Engine>(emu: &mut Emu<E>) {
    if process_intr_wait_irqs(emu) {
        E::Arm9Data::return_from_hle_swi(emu, emu.arm9.hle_bios.swi_r0_3);
    } else {
        E::Arm9Data::jump(emu, 0xFFFF_0200);
        halt(emu);
    }
}

fn soft_reset<E: Engine>(emu: &mut Emu<E>) {
    // TODO
    #[cfg(feature = "log")]
    slog::error!(emu.arm9.logger, "Unimplemented SoftReset SWI");
}

fn wait_by_loop<E: Engine>(emu: &mut Emu<E>, iterations: i32) -> u32 {
    emu.arm9.schedule.set_cur_time(
        emu.arm9.schedule.cur_time() + arm9::Timestamp(16 * iterations.max(1) as u64),
    );
    (iterations - 1).min(0) as u32
}

pub struct State {
    pub enabled: bool,
    swi_r0_3: [u32; 4],
    intr_wait_mask: u32,
}

impl State {
    pub fn new(enabled: bool) -> Self {
        State {
            enabled,
            swi_r0_3: [0; 4],
            intr_wait_mask: 0,
        }
    }
}

static SWI_NAMES: [&str; 0x20] = [
    "SoftReset",
    "?",
    "?",
    "WaitByLoop",
    "IntrWait",
    "VBlankIntrWait",
    "Halt",
    "?",
    "?",
    "Div",
    "",
    "CpuSet",
    "CpuFastSet",
    "Sqrt",
    "GetCRC16",
    "IsDebugger",
    "BitUnPack",
    "LZ77UnCompReadNormalWrite8bit",
    "LZ77UnCompReadByCallbackWrite16bit",
    "HuffUnCompReadByCallback",
    "RLUnCompReadNormalWrite8bit",
    "RLUnCompReadByCallbackWrite16bit",
    "Diff8bitUnFilterWrite8bit",
    "?",
    "Diff16bitUnFilter",
    "?",
    "?",
    "?",
    "?",
    "?",
    "",
    "CustomPost",
];

pub fn handle_swi<E: Engine>(emu: &mut Emu<E>, number: u8, mut r0_3: [u32; 4]) {
    #[cfg(feature = "log")]
    slog::debug!(
        emu.arm9.logger,
        "SWI {:#04X} ({})",
        number,
        SWI_NAMES[number as usize]
    );

    match number {
        0x00 => soft_reset(emu),

        0x03 => r0_3[0] = wait_by_loop(emu, r0_3[0] as i32),

        0x04 => {
            intr_wait(emu, r0_3[0] != 0, r0_3[1]);
            return;
        }

        0x05 => {
            intr_wait(emu, true, 1);
            return;
        }

        0x06 => {
            halt(emu);
            r0_3[0] = 0;
        }

        0x09 => (r0_3[0], r0_3[1], r0_3[3]) = common::div(r0_3[0], r0_3[1]),

        // TODO: r3 value
        0x0B => [r0_3[0], r0_3[1]] = cpu_set(emu, r0_3),

        0x0C => (r0_3[0], r0_3[1], r0_3[3]) = cpu_fast_set(emu, r0_3),

        // TODO: Other regs
        0x0D => r0_3[0] = common::sqrt(r0_3[0]),

        0x0E => {
            (r0_3[0], r0_3[3]) = common::crc16(r0_3[0], r0_3[2] >> 1, r0_3[3], || {
                let half = bus::read_16::<CpuAccess, _>(emu, r0_3[1]);
                r0_3[1] = r0_3[1].wrapping_add(2);
                half
            });
        }

        0x0F => (r0_3[0], r0_3[1], r0_3[3]) = common::is_debugger::<_, 0x7F_FFF8>(emu),

        0x10 => bit_unpack(emu, r0_3[0], r0_3[1], r0_3[2]),

        0x11 => lz77_uncomp_read_normal_write_8(emu),

        0x12 => lz77_uncomp_read_callback_write_16(emu),

        0x13 => huff_uncomp_read_callback(emu),

        0x14 => rl_uncomp_read_normal_write_8(emu),

        0x15 => rl_uncomp_read_callback_write_16(emu),

        0x16 => diff_8_unfilter_write_8(emu),

        0x18 => diff_16_unfilter(emu),

        0x1F => {
            bus::write_32::<CpuAccess, _>(emu, 0x0400_0300, r0_3[0]);
        }

        _ => {
            unimplemented!("Invalid ARM9 SWI {:#X}", number);
        }
    }

    E::Arm9Data::return_from_hle_swi(emu, r0_3);
}

pub fn handle_irq<E: Engine>(emu: &mut Emu<E>) -> u32 {
    let dtcm_top = emu
        .arm9
        .cp15
        .dtcm_control()
        .base_addr()
        .wrapping_add(0x4000);
    let handler_addr = bus::read_32::<CpuAccess, _, false>(emu, dtcm_top.wrapping_sub(4));
    E::Arm9Data::jump_and_link(emu, handler_addr, 0xFFFF_0204);
    dtcm_top
}

pub fn handle_undefined_instr<E: Engine>(emu: &mut Emu<E>) {
    E::Arm9Data::jump(emu, 0xFFFF_0004);
}
