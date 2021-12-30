use super::super::{Engine, IrqFlags};

#[cfg(any(feature = "bft-r", feature = "bft-w"))]
use crate::utils::MemValue;
use crate::{
    cpu::{
        arm9::{div_engine, sqrt_engine},
        bus::AccessType,
        dma, timers,
    },
    ds_slot,
    emu::{swram, Emu, GlobalExMemControl, LocalExMemControl},
    gpu, ipc,
};

// TODO: Fix GBA ROM open bus values, depending on the selected access time they're ORed with
// another value according to GBATEK
// TODO: Check what happens to the DS slot registers when ROMCTRL.bit15 is 0 and when they're
// allocated to the other CPU

#[inline(never)]
pub fn read_8<A: AccessType, E: Engine>(emu: &mut Emu<E>, addr: u32) -> u8 {
    #[allow(clippy::shadow_unrelated)]
    match addr >> 24 {
        #[cfg(feature = "bft-r")]
        0x02 => emu.mem.main_mem.read(addr as usize & 0x3F_FFFF),

        #[cfg(feature = "bft-r")]
        0x03 => unsafe {
            emu.swram
                .arm9_ptr()
                .add((addr & emu.swram.arm9_mask()) as usize)
                .read()
        },

        #[allow(clippy::match_same_arms)]
        0x04 => match addr & 0x00FF_FFFF {
            0x000..=0x003 | 0x008..=0x057 | 0x06C..=0x06D => emu.gpu.engine_2d_a.read_8::<A>(addr),
            0x004 => emu.gpu.disp_status_9().0 as u8,
            0x005 => (emu.gpu.disp_status_9().0 >> 8) as u8,
            0x006 => emu.gpu.vcount() as u8,
            0x007 => (emu.gpu.vcount() >> 8) as u8,
            0x100 => emu.arm9.timers.counter(
                timers::Index::new(0),
                &mut emu.arm9.schedule,
                &mut emu.arm9.irqs,
            ) as u8,
            0x101 => {
                (emu.arm9.timers.counter(
                    timers::Index::new(0),
                    &mut emu.arm9.schedule,
                    &mut emu.arm9.irqs,
                ) >> 8) as u8
            }
            0x102 => emu.arm9.timers.0[0].control().0 as u8,
            0x103 => 0,
            0x104 => emu.arm9.timers.counter(
                timers::Index::new(1),
                &mut emu.arm9.schedule,
                &mut emu.arm9.irqs,
            ) as u8,
            0x105 => {
                (emu.arm9.timers.counter(
                    timers::Index::new(1),
                    &mut emu.arm9.schedule,
                    &mut emu.arm9.irqs,
                ) >> 8) as u8
            }
            0x106 => emu.arm9.timers.0[1].control().0 as u8,
            0x107 => 0,
            0x108 => emu.arm9.timers.counter(
                timers::Index::new(2),
                &mut emu.arm9.schedule,
                &mut emu.arm9.irqs,
            ) as u8,
            0x109 => {
                (emu.arm9.timers.counter(
                    timers::Index::new(2),
                    &mut emu.arm9.schedule,
                    &mut emu.arm9.irqs,
                ) >> 8) as u8
            }
            0x10A => emu.arm9.timers.0[2].control().0 as u8,
            0x10B => 0,
            0x10C => emu.arm9.timers.counter(
                timers::Index::new(3),
                &mut emu.arm9.schedule,
                &mut emu.arm9.irqs,
            ) as u8,
            0x10D => {
                (emu.arm9.timers.counter(
                    timers::Index::new(3),
                    &mut emu.arm9.schedule,
                    &mut emu.arm9.irqs,
                ) >> 8) as u8
            }
            0x10E => emu.arm9.timers.0[3].control().0 as u8,
            0x10F => 0,
            0x1A0 => emu.ds_slot.aux_spi_control().0 as u8,
            0x1A1 => (emu.ds_slot.aux_spi_control().0 >> 8) as u8,
            0x1A2 => emu.ds_slot.spi_data_out(),
            0x208 => emu.arm9.irqs.master_enable() as u8,
            0x247 => emu.swram.control().0,
            0x300 => emu.arm9.post_boot_flag.0,
            0x1000..=0x1003 | 0x1008..=0x1057 | 0x106C..=0x106D => {
                emu.gpu.engine_2d_b.read_8::<A>(addr)
            }
            _ => {
                #[cfg(feature = "log")]
                if !A::IS_DEBUG {
                    slog::warn!(
                        emu.arm9.logger,
                        "Unknown IO read8 @ {:#05X}",
                        addr & 0x00FF_FFFF
                    );
                }
                0
            }
        },

        0x05 => emu.gpu.vram.palette.read(addr as usize & 0x7FF),

        #[cfg(feature = "bft-r")]
        0x06 => match addr >> 21 & 7 {
            0 => emu.gpu.vram.read_a_bg(addr),
            1 => emu.gpu.vram.read_b_bg(addr),
            2 => emu.gpu.vram.read_a_obj(addr),
            3 => emu.gpu.vram.read_b_obj(addr),
            _ => emu.gpu.vram.read_lcdc(addr),
        },

        0x07 => emu.gpu.vram.oam.read(addr as usize & 0x7FF),

        0x08 | 0x09 => {
            if emu.global_ex_mem_control().arm7_gba_slot_access() {
                0
            } else {
                (addr >> ((addr & 1) << 3 | 1)) as u8
            }
        }

        0x0A => {
            if emu.global_ex_mem_control().arm7_gba_slot_access() {
                0
            } else {
                u8::MAX
            }
        }

        #[cfg(feature = "bft-r")]
        0xFF => {
            if addr & 0xFFFF_F000 == 0xFFFF_0000 {
                emu.mem.arm9_bios[addr as usize & 0xFFF]
            } else {
                0
            }
        }

        _ => {
            #[cfg(feature = "log")]
            if !A::IS_DEBUG {
                slog::warn!(emu.arm9.logger, "Unknown read8 @ {:#010X}", addr);
            }
            0
        }
    }
}

#[inline(never)]
pub fn read_16<A: AccessType, E: Engine>(emu: &mut Emu<E>, mut addr: u32) -> u16 {
    addr &= !1;
    match addr >> 24 {
        #[cfg(feature = "bft-r")]
        0x02 => emu.main_mem.read_le(addr as usize & 0x3F_FFFE),

        #[cfg(feature = "bft-r")]
        0x03 => unsafe {
            u16::read_le_aligned(
                emu.swram
                    .arm9_ptr()
                    .add((addr & emu.swram.arm9_mask()) as usize) as *const u16,
            )
        },

        0x04 => match addr & 0x00FF_FFFE {
            0x000..=0x002 | 0x008..=0x056 | 0x06C => emu.gpu.engine_2d_a.read_16::<A>(addr),
            0x004 => emu.gpu.disp_status_9().0,
            0x006 => emu.gpu.vcount(),
            0x0B0 => emu.arm9.dma.channels[0].src_addr as u16,
            0x0B2 => (emu.arm9.dma.channels[0].src_addr >> 16) as u16,
            0x0B4 => emu.arm9.dma.channels[0].dst_addr as u16,
            0x0B6 => (emu.arm9.dma.channels[0].dst_addr >> 16) as u16,
            0x0B8 => emu.arm9.dma.channels[0].control.0 as u16,
            0x0BA => (emu.arm9.dma.channels[0].control.0 >> 16) as u16,
            0x0BC => emu.arm9.dma.channels[1].src_addr as u16,
            0x0BE => (emu.arm9.dma.channels[1].src_addr >> 16) as u16,
            0x0C0 => emu.arm9.dma.channels[1].dst_addr as u16,
            0x0C2 => (emu.arm9.dma.channels[1].dst_addr >> 16) as u16,
            0x0C4 => emu.arm9.dma.channels[1].control.0 as u16,
            0x0C6 => (emu.arm9.dma.channels[1].control.0 >> 16) as u16,
            0x0C8 => emu.arm9.dma.channels[2].src_addr as u16,
            0x0CA => (emu.arm9.dma.channels[2].src_addr >> 16) as u16,
            0x0CC => emu.arm9.dma.channels[2].dst_addr as u16,
            0x0CE => (emu.arm9.dma.channels[2].dst_addr >> 16) as u16,
            0x0D0 => emu.arm9.dma.channels[2].control.0 as u16,
            0x0D2 => (emu.arm9.dma.channels[2].control.0 >> 16) as u16,
            0x0D4 => emu.arm9.dma.channels[3].src_addr as u16,
            0x0D6 => (emu.arm9.dma.channels[3].src_addr >> 16) as u16,
            0x0D8 => emu.arm9.dma.channels[3].dst_addr as u16,
            0x0DA => (emu.arm9.dma.channels[3].dst_addr >> 16) as u16,
            0x0DC => emu.arm9.dma.channels[3].control.0 as u16,
            0x0DE => (emu.arm9.dma.channels[3].control.0 >> 16) as u16,
            0x100 => emu.arm9.timers.counter(
                timers::Index::new(0),
                &mut emu.arm9.schedule,
                &mut emu.arm9.irqs,
            ),
            0x102 => emu.arm9.timers.0[0].control().0 as u16,
            0x104 => emu.arm9.timers.counter(
                timers::Index::new(1),
                &mut emu.arm9.schedule,
                &mut emu.arm9.irqs,
            ),
            0x106 => emu.arm9.timers.0[1].control().0 as u16,
            0x108 => emu.arm9.timers.counter(
                timers::Index::new(2),
                &mut emu.arm9.schedule,
                &mut emu.arm9.irqs,
            ),
            0x10A => emu.arm9.timers.0[2].control().0 as u16,
            0x10C => emu.arm9.timers.counter(
                timers::Index::new(3),
                &mut emu.arm9.schedule,
                &mut emu.arm9.irqs,
            ),
            0x10E => emu.arm9.timers.0[3].control().0 as u16,
            0x130 => emu.input.0 as u16,
            0x180 => emu.ipc.sync_9().0,
            0x184 => emu.ipc.fifo_control_9().0,
            0x1A0 => emu.ds_slot.aux_spi_control().0,
            0x1A2 => emu.ds_slot.spi_data_out() as u16,
            0x204 => emu.arm9.local_ex_mem_control.0 | emu.global_ex_mem_control().0,
            0x208 => emu.arm9.irqs.master_enable() as u16,
            0x280 => emu.arm9.div_engine.control().0,
            0x2B0 => emu.arm9.sqrt_engine.control().0,
            0x300 => emu.arm9.post_boot_flag.0 as u16,
            0x304 => emu.gpu.power_control().0,
            0x1000..=0x1002 | 0x1008..=0x1056 | 0x106C => emu.gpu.engine_2d_b.read_16::<A>(addr),
            _ => {
                #[cfg(feature = "log")]
                if !A::IS_DEBUG {
                    slog::warn!(
                        emu.arm9.logger,
                        "Unknown IO read16 @ {:#05X}",
                        addr & 0x00FF_FFFE
                    );
                }
                0
            }
        },

        0x05 => emu.gpu.vram.palette.read_le(addr as usize & 0x7FE),

        #[cfg(feature = "bft-r")]
        0x06 => match addr >> 21 & 7 {
            0 => emu.gpu.vram.read_a_bg(addr),
            1 => emu.gpu.vram.read_b_bg(addr),
            2 => emu.gpu.vram.read_a_obj(addr),
            3 => emu.gpu.vram.read_b_obj(addr),
            _ => emu.gpu.vram.read_lcdc(addr),
        },

        0x07 => emu.gpu.vram.oam.read_le(addr as usize & 0x7FE),

        0x08 | 0x09 => {
            if emu.global_ex_mem_control().arm7_gba_slot_access() {
                0
            } else {
                (addr >> 1) as u16
            }
        }

        0x0A => {
            if emu.global_ex_mem_control().arm7_gba_slot_access() {
                0
            } else {
                u16::MAX
            }
        }

        #[cfg(feature = "bft-r")]
        0xFF => {
            if addr & 0xFFFF_F000 == 0xFFFF_0000 {
                emu.arm9.bios.read_le(addr as usize & 0xFFE)
            } else {
                0
            }
        }

        _ => {
            #[cfg(feature = "log")]
            if !A::IS_DEBUG {
                slog::warn!(emu.arm9.logger, "Unknown read16 @ {:#010X}", addr);
            }
            0
        }
    }
}

#[inline(never)]
pub fn read_32<A: AccessType, E: Engine>(emu: &mut Emu<E>, mut addr: u32) -> u32 {
    addr &= !3;
    match addr >> 24 {
        #[cfg(feature = "bft-r")]
        0x02 => emu.main_mem.read_le(addr as usize & 0x3F_FFFC),

        #[cfg(feature = "bft-r")]
        0x03 => unsafe {
            u32::read_le_aligned(
                emu.swram
                    .arm9_ptr()
                    .add((addr & emu.swram.arm9_mask()) as usize) as *const u32,
            )
        },

        0x04 => match addr & 0x00FF_FFFC {
            0x000 | 0x008..=0x054 | 0x06C => emu.gpu.engine_2d_a.read_32::<A>(addr),
            0x004 => emu.gpu.disp_status_9().0 as u32 | (emu.gpu.vcount() as u32) << 16,
            0x0B0 => emu.arm9.dma.channels[0].src_addr,
            0x0B4 => emu.arm9.dma.channels[0].dst_addr,
            0x0B8 => emu.arm9.dma.channels[0].control.0,
            0x0BC => emu.arm9.dma.channels[1].src_addr,
            0x0C0 => emu.arm9.dma.channels[1].dst_addr,
            0x0C4 => emu.arm9.dma.channels[1].control.0,
            0x0C8 => emu.arm9.dma.channels[2].src_addr,
            0x0CC => emu.arm9.dma.channels[2].dst_addr,
            0x0D0 => emu.arm9.dma.channels[2].control.0,
            0x0D4 => emu.arm9.dma.channels[3].src_addr,
            0x0D8 => emu.arm9.dma.channels[3].dst_addr,
            0x0DC => emu.arm9.dma.channels[3].control.0,
            0x0E0..=0x0EC => emu.arm9.dma_fill.read_le(addr as usize & 0xC),
            0x180 => emu.ipc.sync_9().0 as u32,
            0x1A0 => {
                emu.ds_slot.aux_spi_control().0 as u32 | (emu.ds_slot.spi_data_out() as u32) << 16
            }
            0x1A4 => emu.ds_slot.rom_control().0,
            0x208 => emu.arm9.irqs.master_enable() as u32,
            0x210 => emu.arm9.irqs.enabled().0,
            0x214 => emu.arm9.irqs.requested().0,
            0x280 => emu.arm9.div_engine.control().0 as u32,
            0x290 => emu.arm9.div_engine.numerator() as u32,
            0x294 => (emu.arm9.div_engine.numerator() >> 32) as u32,
            0x298 => emu.arm9.div_engine.denominator() as u32,
            0x29C => (emu.arm9.div_engine.denominator() >> 32) as u32,
            0x2A0 => emu.arm9.div_engine.quotient() as u32,
            0x2A4 => (emu.arm9.div_engine.quotient() >> 32) as u32,
            0x2A8 => emu.arm9.div_engine.remainder() as u32,
            0x2AC => (emu.arm9.div_engine.remainder() >> 32) as u32,
            0x2B0 => emu.arm9.sqrt_engine.control().0 as u32,
            0x2B4 => emu.arm9.sqrt_engine.result(),
            0x2B8 => emu.arm9.sqrt_engine.input() as u32,
            0x2BC => (emu.arm9.sqrt_engine.input() >> 32) as u32,
            0x1000 | 0x1008..=0x1054 | 0x106C => emu.gpu.engine_2d_b.read_32::<A>(addr),
            0x10_0000 => {
                if A::IS_DEBUG {
                    emu.ipc.peek_9()
                } else {
                    emu.ipc.recv_9(&mut emu.arm7.irqs)
                }
            }
            0x10_0010 => {
                if emu.ds_slot.arm9_access() {
                    if A::IS_DEBUG {
                        emu.ds_slot.peek_rom_data()
                    } else {
                        emu.ds_slot
                            .consume_rom_data_arm9(&mut emu.arm9.irqs, &mut emu.arm9.schedule)
                    }
                } else {
                    0
                }
            }
            _ => {
                #[cfg(feature = "log")]
                if !A::IS_DEBUG {
                    slog::warn!(
                        emu.arm9.logger,
                        "Unknown IO read32 @ {:#05X}",
                        addr & 0x00FF_FFFC
                    );
                }
                0
            }
        },

        0x05 => emu.gpu.vram.palette.read_le(addr as usize & 0x7FC),

        #[cfg(feature = "bft-r")]
        0x06 => match addr >> 21 & 7 {
            0 => emu.gpu.vram.read_a_bg(addr),
            1 => emu.gpu.vram.read_b_bg(addr),
            2 => emu.gpu.vram.read_a_obj(addr),
            3 => emu.gpu.vram.read_b_obj(addr),
            _ => emu.gpu.vram.read_lcdc(addr),
        },

        0x07 => emu.gpu.vram.oam.read_le(addr as usize & 0x7FC),

        0x08 | 0x09 => {
            if emu.global_ex_mem_control().arm7_gba_slot_access() {
                0
            } else {
                (addr >> 1 & 0xFFFF) | (addr >> 1 | 1) << 16
            }
        }

        0x0A => {
            if emu.global_ex_mem_control().arm7_gba_slot_access() {
                0
            } else {
                u32::MAX
            }
        }

        #[cfg(feature = "bft-r")]
        0xFF => {
            if addr & 0xFFFF_F000 == 0xFFFF_0000 {
                emu.arm9.bios.read_le(addr as usize & 0xFFC)
            } else {
                0
            }
        }

        _ => {
            #[cfg(feature = "log")]
            if !A::IS_DEBUG {
                slog::warn!(emu.arm9.logger, "Unknown read32 @ {:#010X}", addr);
            }
            0
        }
    }
}

#[inline(never)]
#[allow(clippy::single_match)]
pub fn write_8<A: AccessType, E: Engine>(emu: &mut Emu<E>, addr: u32, value: u8) {
    match addr >> 24 {
        #[cfg(feature = "bft-w")]
        0x02 => emu.main_mem.write(addr as usize & 0x3F_FFFF, value),

        #[cfg(feature = "bft-w")]
        0x03 => unsafe {
            emu.swram
                .arm9_w_ptr()
                .add((addr & emu.swram.arm9_mask()) as usize)
                .write(value);
        },

        0x04 => match addr & 0x00FF_FFFF {
            0x000..=0x003 | 0x008..=0x057 | 0x06C..=0x06D => {
                emu.gpu.engine_2d_a.write_8::<A>(addr, value);
            }
            0x1A0 => {
                if emu.ds_slot.arm9_access() {
                    emu.ds_slot.set_aux_spi_control(ds_slot::AuxSpiControl(
                        (emu.ds_slot.aux_spi_control().0 & 0xFF00) | value as u16,
                    ));
                } else {
                    #[cfg(feature = "log")]
                    if !A::IS_DEBUG {
                        slog::warn!(
                            emu.arm9.logger,
                            "Tried to write to AUXSPICNT while inaccessible"
                        );
                    }
                }
            }
            0x1A1 => {
                if emu.ds_slot.arm9_access() {
                    emu.ds_slot.set_aux_spi_control(ds_slot::AuxSpiControl(
                        (emu.ds_slot.aux_spi_control().0 & 0x00FF) | (value as u16) << 8,
                    ));
                } else {
                    #[cfg(feature = "log")]
                    if !A::IS_DEBUG {
                        slog::warn!(
                            emu.arm9.logger,
                            "Tried to write to AUXSPICNT while inaccessible"
                        );
                    }
                }
            }
            0x1A2 => {
                if emu.ds_slot.arm9_access() {
                    emu.ds_slot.write_spi_data(
                        value,
                        &mut emu.arm7.schedule,
                        &mut emu.arm9.schedule,
                    );
                } else {
                    #[cfg(feature = "log")]
                    if !A::IS_DEBUG {
                        slog::warn!(
                            emu.arm9.logger,
                            "Tried to write to AUXSPIDATA while inaccessible"
                        );
                    }
                }
            }
            0x1A8..=0x1AF => {
                if emu.ds_slot.arm9_access() {
                    emu.ds_slot.rom_cmd[(addr & 7) as usize] = value;
                } else {
                    #[cfg(feature = "log")]
                    if !A::IS_DEBUG {
                        slog::warn!(
                            emu.arm9.logger,
                            "Tried to write to DS slot ROM command while inaccessible"
                        );
                    }
                }
            }
            0x208 => emu
                .arm9
                .irqs
                .set_master_enable(value & 1 != 0, &mut emu.arm9.schedule),
            0x240 => emu
                .gpu
                .vram
                .set_bank_control_a(gpu::vram::BankControl(value), &mut emu.arm9),
            0x241 => emu
                .gpu
                .vram
                .set_bank_control_b(gpu::vram::BankControl(value), &mut emu.arm9),
            0x242 => emu
                .gpu
                .vram
                .set_bank_control_c(gpu::vram::BankControl(value), &mut emu.arm9),
            0x243 => emu
                .gpu
                .vram
                .set_bank_control_d(gpu::vram::BankControl(value), &mut emu.arm9),
            0x244 => emu
                .gpu
                .vram
                .set_bank_control_e(gpu::vram::BankControl(value), &mut emu.arm9),
            0x245 => emu
                .gpu
                .vram
                .set_bank_control_f(gpu::vram::BankControl(value), &mut emu.arm9),
            0x246 => emu
                .gpu
                .vram
                .set_bank_control_g(gpu::vram::BankControl(value), &mut emu.arm9),
            0x247 => emu
                .swram
                .set_control(swram::Control(value), &mut emu.arm7, &mut emu.arm9),
            0x248 => emu
                .gpu
                .vram
                .set_bank_control_h(gpu::vram::BankControl(value), &mut emu.arm9),
            0x249 => emu
                .gpu
                .vram
                .set_bank_control_i(gpu::vram::BankControl(value), &mut emu.arm9),
            0x300 => emu.arm9.post_boot_flag.0 = (emu.arm9.post_boot_flag.0 & 1) | (value & 3),
            0x1000..=0x1003 | 0x1008..=0x1057 | 0x106C..=0x106D => {
                emu.gpu.engine_2d_b.write_8::<A>(addr, value);
            }
            _ =>
            {
                #[cfg(feature = "log")]
                if !A::IS_DEBUG {
                    slog::warn!(
                        emu.arm9.logger,
                        "Unknown IO write8 @ {:#05X}: {:#04X}",
                        addr & 0x00FF_FFFF,
                        value
                    );
                }
            }
        },

        _ =>
        {
            #[cfg(feature = "log")]
            if !A::IS_DEBUG {
                slog::warn!(
                    emu.arm9.logger,
                    "Unknown write8 @ {:#010X}: {:#04X}",
                    addr,
                    value
                );
            }
        }
    }
}

#[inline(never)]
pub fn write_16<A: AccessType, E: Engine>(emu: &mut Emu<E>, mut addr: u32, value: u16) {
    addr &= !1;
    match addr >> 24 {
        #[cfg(feature = "bft-w")]
        0x02 => emu.main_mem.write_le(addr as usize & 0x3F_FFFE, value),

        #[cfg(feature = "bft-w")]
        0x03 => unsafe {
            value.write_le_aligned(
                emu.swram
                    .arm9_w_ptr()
                    .add((addr & emu.swram.arm9_mask()) as usize) as *mut u16,
            );
        },

        0x04 => {
            match addr & 0x00FF_FFFE {
                0x000..=0x002 | 0x008..=0x056 | 0x06C => {
                    emu.gpu.engine_2d_a.write_16::<A>(addr, value);
                }
                0x004 => emu.gpu.set_disp_status_9(gpu::DispStatus(value)),
                0x006 => emu.gpu.set_vcount(value),
                0x0B0 => emu.arm9.dma.channels[0]
                    .set_src_addr((emu.arm9.dma.channels[0].src_addr & 0xFFFF_0000) | value as u32),
                0x0B2 => emu.arm9.dma.channels[0].set_src_addr(
                    (emu.arm9.dma.channels[0].src_addr & 0x0000_FFFF) | (value as u32) << 16,
                ),
                0x0B4 => emu.arm9.dma.channels[0]
                    .set_dst_addr((emu.arm9.dma.channels[0].dst_addr & 0xFFFF_0000) | value as u32),
                0x0B6 => emu.arm9.dma.channels[0].set_dst_addr(
                    (emu.arm9.dma.channels[0].dst_addr & 0x0000_FFFF) | (value as u32) << 16,
                ),
                0x0B8 => emu.arm9.dma.channels[0].set_control_low(value),
                0x0BA => emu.arm9.set_dma_channel_control(
                    dma::Index::new(0),
                    dma::Control(
                        (emu.arm9.dma.channels[0].control.0 & 0x0000_FFFF) | (value as u32) << 16,
                    ),
                ),
                0x0BC => emu.arm9.dma.channels[1]
                    .set_src_addr((emu.arm9.dma.channels[1].src_addr & 0xFFFF_0000) | value as u32),
                0x0BE => emu.arm9.dma.channels[1].set_src_addr(
                    (emu.arm9.dma.channels[1].src_addr & 0x0000_FFFF) | (value as u32) << 16,
                ),
                0x0C0 => emu.arm9.dma.channels[1]
                    .set_dst_addr((emu.arm9.dma.channels[1].dst_addr & 0xFFFF_0000) | value as u32),
                0x0C2 => emu.arm9.dma.channels[1].set_dst_addr(
                    (emu.arm9.dma.channels[1].dst_addr & 0x0000_FFFF) | (value as u32) << 16,
                ),
                0x0C4 => emu.arm9.dma.channels[1].set_control_low(value),
                0x0C6 => emu.arm9.set_dma_channel_control(
                    dma::Index::new(1),
                    dma::Control(
                        (emu.arm9.dma.channels[1].control.0 & 0x0000_FFFF) | (value as u32) << 16,
                    ),
                ),
                0x0C8 => emu.arm9.dma.channels[2]
                    .set_src_addr((emu.arm9.dma.channels[2].src_addr & 0xFFFF_0000) | value as u32),
                0x0CA => emu.arm9.dma.channels[2].set_src_addr(
                    (emu.arm9.dma.channels[2].src_addr & 0x0000_FFFF) | (value as u32) << 16,
                ),
                0x0CC => emu.arm9.dma.channels[2]
                    .set_dst_addr((emu.arm9.dma.channels[2].dst_addr & 0xFFFF_0000) | value as u32),
                0x0CE => emu.arm9.dma.channels[2].set_dst_addr(
                    (emu.arm9.dma.channels[2].dst_addr & 0x0000_FFFF) | (value as u32) << 16,
                ),
                0x0D0 => emu.arm9.dma.channels[2].set_control_low(value),
                0x0D2 => emu.arm9.set_dma_channel_control(
                    dma::Index::new(2),
                    dma::Control(
                        (emu.arm9.dma.channels[2].control.0 & 0x0000_FFFF) | (value as u32) << 16,
                    ),
                ),
                0x0D4 => emu.arm9.dma.channels[3]
                    .set_src_addr((emu.arm9.dma.channels[3].src_addr & 0xFFFF_0000) | value as u32),
                0x0D6 => emu.arm9.dma.channels[3].set_src_addr(
                    (emu.arm9.dma.channels[3].src_addr & 0x0000_FFFF) | (value as u32) << 16,
                ),
                0x0D8 => emu.arm9.dma.channels[3]
                    .set_dst_addr((emu.arm9.dma.channels[3].dst_addr & 0xFFFF_0000) | value as u32),
                0x0DA => emu.arm9.dma.channels[3].set_dst_addr(
                    (emu.arm9.dma.channels[3].dst_addr & 0x0000_FFFF) | (value as u32) << 16,
                ),
                0x0DC => emu.arm9.dma.channels[3].set_control_low(value),
                0x0DE => emu.arm9.set_dma_channel_control(
                    dma::Index::new(3),
                    dma::Control(
                        (emu.arm9.dma.channels[3].control.0 & 0x0000_FFFF) | (value as u32) << 16,
                    ),
                ),
                0x100 => emu.arm9.timers.set_reload(
                    timers::Index::new(0),
                    value,
                    &mut emu.arm9.schedule,
                    &mut emu.arm9.irqs,
                ),
                0x102 => emu.arm9.timers.set_control(
                    timers::Index::new(0),
                    timers::Control(value as u8),
                    &mut emu.arm9.schedule,
                    &mut emu.arm9.irqs,
                ),
                0x104 => emu.arm9.timers.set_reload(
                    timers::Index::new(1),
                    value,
                    &mut emu.arm9.schedule,
                    &mut emu.arm9.irqs,
                ),
                0x106 => emu.arm9.timers.set_control(
                    timers::Index::new(1),
                    timers::Control(value as u8),
                    &mut emu.arm9.schedule,
                    &mut emu.arm9.irqs,
                ),
                0x108 => emu.arm9.timers.set_reload(
                    timers::Index::new(2),
                    value,
                    &mut emu.arm9.schedule,
                    &mut emu.arm9.irqs,
                ),
                0x10A => emu.arm9.timers.set_control(
                    timers::Index::new(2),
                    timers::Control(value as u8),
                    &mut emu.arm9.schedule,
                    &mut emu.arm9.irqs,
                ),
                0x10C => emu.arm9.timers.set_reload(
                    timers::Index::new(3),
                    value,
                    &mut emu.arm9.schedule,
                    &mut emu.arm9.irqs,
                ),
                0x10E => emu.arm9.timers.set_control(
                    timers::Index::new(3),
                    timers::Control(value as u8),
                    &mut emu.arm9.schedule,
                    &mut emu.arm9.irqs,
                ),
                0x180 => emu.ipc.set_sync_9(ipc::Sync(value), &mut emu.arm7.irqs),
                0x184 => emu.ipc.set_fifo_control_9(
                    ipc::FifoControl(value),
                    &mut emu.arm9.irqs,
                    &mut emu.arm9.schedule,
                ),
                0x1A0 => {
                    if emu.ds_slot.arm9_access() {
                        emu.ds_slot
                            .set_aux_spi_control(ds_slot::AuxSpiControl(value));
                    } else {
                        #[cfg(feature = "log")]
                        if !A::IS_DEBUG {
                            slog::warn!(
                                emu.arm9.logger,
                                "Tried to write to AUXSPICNT while inaccessible"
                            );
                        }
                    }
                }
                0x1A2 => {
                    if emu.ds_slot.arm9_access() {
                        emu.ds_slot.write_spi_data(
                            value as u8,
                            &mut emu.arm7.schedule,
                            &mut emu.arm9.schedule,
                        );
                    } else {
                        #[cfg(feature = "log")]
                        if !A::IS_DEBUG {
                            slog::warn!(
                                emu.arm9.logger,
                                "Tried to write to AUXSPIDATA while inaccessible"
                            );
                        }
                    }
                }
                0x1A8 | 0x1AA | 0x1AC | 0x1AE => {
                    if emu.ds_slot.arm9_access() {
                        emu.ds_slot.rom_cmd.write_le((addr & 6) as usize, value);
                    } else {
                        #[cfg(feature = "log")]
                        if !A::IS_DEBUG {
                            slog::warn!(
                                emu.arm9.logger,
                                "Tried to write to DS slot ROM command while inaccessible"
                            );
                        }
                    }
                }
                // The KEY2 encryption seeds aren't used
                0x1B8 | 0x1BA => {}
                0x204 => {
                    emu.arm9.set_local_ex_mem_control(LocalExMemControl(value));
                    emu.set_global_ex_mem_control(GlobalExMemControl(value));
                }
                0x208 => emu
                    .arm9
                    .irqs
                    .set_master_enable(value & 1 != 0, &mut emu.arm9.schedule),
                0x240 => {
                    emu.gpu
                        .vram
                        .set_bank_control_a(gpu::vram::BankControl(value as u8), &mut emu.arm9);
                    emu.gpu.vram.set_bank_control_b(
                        gpu::vram::BankControl((value >> 8) as u8),
                        &mut emu.arm9,
                    );
                }
                0x242 => {
                    emu.gpu
                        .vram
                        .set_bank_control_c(gpu::vram::BankControl(value as u8), &mut emu.arm9);
                    emu.gpu.vram.set_bank_control_d(
                        gpu::vram::BankControl((value >> 8) as u8),
                        &mut emu.arm9,
                    );
                }
                0x244 => {
                    emu.gpu
                        .vram
                        .set_bank_control_e(gpu::vram::BankControl(value as u8), &mut emu.arm9);
                    emu.gpu.vram.set_bank_control_f(
                        gpu::vram::BankControl((value >> 8) as u8),
                        &mut emu.arm9,
                    );
                }
                0x246 => {
                    emu.gpu
                        .vram
                        .set_bank_control_g(gpu::vram::BankControl(value as u8), &mut emu.arm9);
                    emu.swram.set_control(
                        swram::Control((value >> 8) as u8),
                        &mut emu.arm7,
                        &mut emu.arm9,
                    );
                }
                0x248 => {
                    emu.gpu
                        .vram
                        .set_bank_control_h(gpu::vram::BankControl(value as u8), &mut emu.arm9);
                    emu.gpu.vram.set_bank_control_i(
                        gpu::vram::BankControl((value >> 8) as u8),
                        &mut emu.arm9,
                    );
                }
                0x280 => emu
                    .arm9
                    .div_engine
                    .set_control(div_engine::Control(value), &mut emu.arm9.schedule),
                0x2B0 => emu
                    .arm9
                    .sqrt_engine
                    .set_control(sqrt_engine::Control(value as u16), &mut emu.arm9.schedule),
                0x300 => {
                    emu.arm9.post_boot_flag.0 = (emu.arm9.post_boot_flag.0 & 1) | (value as u8 & 3);
                }
                0x304 => emu.gpu.set_power_control(gpu::PowerControl(value)),
                0x1000..=0x1002 | 0x1008..=0x1056 | 0x106C => {
                    emu.gpu.engine_2d_b.write_16::<A>(addr, value);
                }
                _ =>
                {
                    #[cfg(feature = "log")]
                    if !A::IS_DEBUG {
                        slog::warn!(
                            emu.arm9.logger,
                            "Unknown IO write16 @ {:#05X}: {:#06X}",
                            addr & 0x00FF_FFFE,
                            value
                        );
                    }
                }
            }
        }

        0x05 => emu.gpu.vram.palette.write_le(addr as usize & 0x7FE, value),

        0x06 => match addr >> 21 & 7 {
            0 => emu.gpu.vram.write_a_bg(addr, value),
            1 => emu.gpu.vram.write_b_bg(addr, value),
            2 => emu.gpu.vram.write_a_obj(addr, value),
            3 => emu.gpu.vram.write_b_obj(addr, value),
            _ => emu.gpu.vram.write_lcdc(addr, value),
        },

        0x07 => emu.gpu.vram.oam.write_le(addr as usize & 0x7FE, value),

        _ =>
        {
            #[cfg(feature = "log")]
            if !A::IS_DEBUG {
                slog::warn!(
                    emu.arm9.logger,
                    "Unknown write16 @ {:#010X}: {:#06X}",
                    addr,
                    value
                );
            }
        }
    }
}

#[inline(never)]
pub fn write_32<A: AccessType, E: Engine>(emu: &mut Emu<E>, mut addr: u32, value: u32) {
    addr &= !3;
    match addr >> 24 {
        #[cfg(feature = "bft-w")]
        0x02 => emu.main_mem.write_le(addr as usize & 0x3F_FFFC, value),

        #[cfg(feature = "bft-w")]
        0x03 => unsafe {
            value.write_le_aligned(
                emu.swram
                    .arm9_w_ptr()
                    .add((addr & emu.swram.arm9_mask()) as usize) as *mut u32,
            );
        },

        0x04 => {
            match addr & 0x00FF_FFFC {
                0x000 | 0x008..=0x054 | 0x06C => {
                    emu.gpu.engine_2d_a.write_32::<A>(addr, value);
                }
                0x004 => {
                    emu.gpu.set_disp_status_9(gpu::DispStatus(value as u16));
                    emu.gpu.set_vcount((value >> 16) as u16);
                }
                0x0B0 => emu.arm9.dma.channels[0].set_src_addr(value),
                0x0B4 => emu.arm9.dma.channels[0].set_dst_addr(value),
                0x0B8 => emu
                    .arm9
                    .set_dma_channel_control(dma::Index::new(0), dma::Control(value)),
                0x0BC => emu.arm9.dma.channels[1].set_src_addr(value),
                0x0C0 => emu.arm9.dma.channels[1].set_dst_addr(value),
                0x0C4 => emu
                    .arm9
                    .set_dma_channel_control(dma::Index::new(1), dma::Control(value)),
                0x0C8 => emu.arm9.dma.channels[2].set_src_addr(value),
                0x0CC => emu.arm9.dma.channels[2].set_dst_addr(value),
                0x0D0 => emu
                    .arm9
                    .set_dma_channel_control(dma::Index::new(2), dma::Control(value)),
                0x0D4 => emu.arm9.dma.channels[3].set_src_addr(value),
                0x0D8 => emu.arm9.dma.channels[3].set_dst_addr(value),
                0x0DC => emu
                    .arm9
                    .set_dma_channel_control(dma::Index::new(3), dma::Control(value)),
                0x0E0..=0x0EC => emu.arm9.dma_fill.write_le(addr as usize & 0xC, value),
                0x100 => emu.arm9.timers.set_control_reload(
                    timers::Index::new(0),
                    value as u16,
                    timers::Control((value >> 16) as u8),
                    &mut emu.arm9.schedule,
                    &mut emu.arm9.irqs,
                ),
                0x104 => emu.arm9.timers.set_control_reload(
                    timers::Index::new(1),
                    value as u16,
                    timers::Control((value >> 16) as u8),
                    &mut emu.arm9.schedule,
                    &mut emu.arm9.irqs,
                ),
                0x108 => emu.arm9.timers.set_control_reload(
                    timers::Index::new(2),
                    value as u16,
                    timers::Control((value >> 16) as u8),
                    &mut emu.arm9.schedule,
                    &mut emu.arm9.irqs,
                ),
                0x10C => emu.arm9.timers.set_control_reload(
                    timers::Index::new(3),
                    value as u16,
                    timers::Control((value >> 16) as u8),
                    &mut emu.arm9.schedule,
                    &mut emu.arm9.irqs,
                ),
                0x180 => emu
                    .ipc
                    .set_sync_9(ipc::Sync(value as u16), &mut emu.arm7.irqs),
                0x188 => emu.ipc.send_9(value, &mut emu.arm7.irqs),
                0x1A0 => {
                    if emu.ds_slot.arm9_access() {
                        emu.ds_slot
                            .set_aux_spi_control(ds_slot::AuxSpiControl(value as u16));
                        emu.ds_slot.write_spi_data(
                            (value >> 16) as u8,
                            &mut emu.arm7.schedule,
                            &mut emu.arm9.schedule,
                        );
                    } else {
                        #[cfg(feature = "log")]
                        if !A::IS_DEBUG {
                            slog::warn!(
                                emu.arm9.logger,
                                "Tried to write to AUXSPICNT while inaccessible"
                            );
                            slog::warn!(
                                emu.arm9.logger,
                                "Tried to write to AUXSPIDATA while inaccessible"
                            );
                        }
                    }
                }
                0x1A4 => {
                    if emu.ds_slot.arm9_access() {
                        emu.ds_slot.set_rom_control(
                            ds_slot::RomControl(value),
                            &mut emu.arm7.schedule,
                            &mut emu.arm9.schedule,
                        );
                    } else {
                        #[cfg(feature = "log")]
                        if !A::IS_DEBUG {
                            slog::warn!(
                                emu.arm9.logger,
                                "Tried to write to ROMCTRL while inaccessible"
                            );
                        }
                    }
                }
                0x1A8 | 0x1AC => {
                    if emu.ds_slot.arm9_access() {
                        emu.ds_slot.rom_cmd.write_le((addr & 4) as usize, value);
                    } else {
                        #[cfg(feature = "log")]
                        if !A::IS_DEBUG {
                            slog::warn!(
                                emu.arm9.logger,
                                "Tried to write to DS slot ROM command while inaccessible"
                            );
                        }
                    }
                }
                // The KEY2 encryption seeds aren't used
                0x1B0 | 0x1B4 => {}
                0x208 => emu
                    .arm9
                    .irqs
                    .set_master_enable(value & 1 != 0, &mut emu.arm9.schedule),
                0x210 => emu
                    .arm9
                    .irqs
                    .set_enabled(IrqFlags(value), &mut emu.arm9.schedule),
                0x214 => emu
                    .arm9
                    .irqs
                    .set_requested(IrqFlags(emu.arm9.irqs.requested().0 & !value), ()),
                0x240 => {
                    emu.gpu
                        .vram
                        .set_bank_control_a(gpu::vram::BankControl(value as u8), &mut emu.arm9);
                    emu.gpu.vram.set_bank_control_b(
                        gpu::vram::BankControl((value >> 8) as u8),
                        &mut emu.arm9,
                    );
                    emu.gpu.vram.set_bank_control_c(
                        gpu::vram::BankControl((value >> 16) as u8),
                        &mut emu.arm9,
                    );
                    emu.gpu.vram.set_bank_control_d(
                        gpu::vram::BankControl((value >> 24) as u8),
                        &mut emu.arm9,
                    );
                }
                0x244 => {
                    emu.gpu
                        .vram
                        .set_bank_control_e(gpu::vram::BankControl(value as u8), &mut emu.arm9);
                    emu.gpu.vram.set_bank_control_f(
                        gpu::vram::BankControl((value >> 8) as u8),
                        &mut emu.arm9,
                    );
                    emu.gpu.vram.set_bank_control_g(
                        gpu::vram::BankControl((value >> 16) as u8),
                        &mut emu.arm9,
                    );
                    emu.swram.set_control(
                        swram::Control((value >> 24) as u8),
                        &mut emu.arm7,
                        &mut emu.arm9,
                    );
                }
                0x248 => {
                    emu.gpu
                        .vram
                        .set_bank_control_h(gpu::vram::BankControl(value as u8), &mut emu.arm9);
                    emu.gpu.vram.set_bank_control_i(
                        gpu::vram::BankControl((value >> 8) as u8),
                        &mut emu.arm9,
                    );
                }
                0x280 => emu
                    .arm9
                    .div_engine
                    .set_control(div_engine::Control(value as u16), &mut emu.arm9.schedule),
                0x290 => emu.arm9.div_engine.set_numerator(
                    (emu.arm9.div_engine.numerator() & 0xFFFF_FFFF << 32) | value as i64,
                    &mut emu.arm9.schedule,
                ),
                0x294 => emu.arm9.div_engine.set_numerator(
                    (emu.arm9.div_engine.numerator() & 0xFFFF_FFFF) | (value as i64) << 32,
                    &mut emu.arm9.schedule,
                ),
                0x298 => emu.arm9.div_engine.set_denominator(
                    (emu.arm9.div_engine.denominator() & 0xFFFF_FFFF << 32) | value as i64,
                    &mut emu.arm9.schedule,
                ),
                0x29C => emu.arm9.div_engine.set_denominator(
                    (emu.arm9.div_engine.denominator() & 0xFFFF_FFFF) | (value as i64) << 32,
                    &mut emu.arm9.schedule,
                ),
                0x2B0 => emu
                    .arm9
                    .sqrt_engine
                    .set_control(sqrt_engine::Control(value as u16), &mut emu.arm9.schedule),
                0x2B8 => emu.arm9.sqrt_engine.set_input(
                    (emu.arm9.sqrt_engine.input() & 0xFFFF_FFFF << 32) | value as u64,
                    &mut emu.arm9.schedule,
                ),
                0x2BC => emu.arm9.sqrt_engine.set_input(
                    (emu.arm9.sqrt_engine.input() & 0xFFFF_FFFF) | (value as u64) << 32,
                    &mut emu.arm9.schedule,
                ),
                0x304 => emu.gpu.set_power_control(gpu::PowerControl(value as u16)),
                0x1000 | 0x1008..=0x1054 | 0x106C => {
                    emu.gpu.engine_2d_b.write_32::<A>(addr, value);
                }
                _ =>
                {
                    #[cfg(feature = "log")]
                    if !A::IS_DEBUG {
                        slog::warn!(
                            emu.arm9.logger,
                            "Unknown IO write32 @ {:#05X}: {:#010X}",
                            addr & 0x00FF_FFFC,
                            value
                        );
                    }
                }
            }
        }

        0x05 => emu.gpu.vram.palette.write_le(addr as usize & 0x7FC, value),

        0x06 => match addr >> 21 & 7 {
            0 => emu.gpu.vram.write_a_bg(addr, value),
            1 => emu.gpu.vram.write_b_bg(addr, value),
            2 => emu.gpu.vram.write_a_obj(addr, value),
            3 => emu.gpu.vram.write_b_obj(addr, value),
            _ => emu.gpu.vram.write_lcdc(addr, value),
        },

        0x07 => emu.gpu.vram.oam.write_le(addr as usize & 0x7FC, value),

        _ =>
        {
            #[cfg(feature = "log")]
            if !A::IS_DEBUG {
                slog::warn!(
                    emu.arm9.logger,
                    "Unknown write32 @ {:#010X}: {:#010X}",
                    addr,
                    value
                );
            }
        }
    }
}
