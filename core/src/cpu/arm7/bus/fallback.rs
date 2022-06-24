use super::super::{IrqFlags, BIOS_SIZE};
#[cfg(any(feature = "bft-r", feature = "bft-w"))]
use crate::utils::MemValue;
use crate::{
    cpu::{bus::AccessType, dma, timers, CoreData, Engine},
    ds_slot,
    emu::{input::KeyIrqControl, AudioWifiPowerControl, Emu, LocalExMemControl},
    gpu, ipc, rtc, spi,
};

// TODO:
// - Check what happens to the DS slot registers when ROMCTRL.bit15 is 0 and when they're allocated
//   to the other CPU
// - GBATEK says HALTCNT is R/W...? Maybe the last written value should get read (i.e. 0x80 after
//   halting)

#[inline(never)]
pub fn read_8<A: AccessType, E: Engine>(emu: &mut Emu<E>, addr: u32) -> u8 {
    #[cfg(feature = "debugger-hooks")]
    check_watchpoints!(emu, emu.arm7, addr, 0, 1, Read);
    match addr >> 24 {
        0x00 if addr < BIOS_SIZE as u32 => {
            let max_pc = if addr < emu.arm7.bios_prot as u32 {
                emu.arm7.bios_prot as u32
            } else {
                BIOS_SIZE as u32
            };
            let pc = emu.arm7.engine_data.r15();
            if pc < max_pc || A::IS_DEBUG {
                if !A::IS_DEBUG {
                    emu.arm7.last_bios_word = emu.arm7.bios.read_le(addr as usize & !3);
                }
                emu.arm7.bios.read(addr as usize)
            } else {
                #[cfg(feature = "log")]
                if !A::IS_DEBUG {
                    slog::warn!(
                        emu.arm7.logger,
                        "Forbidden read8 from BIOS region @ {:#06X} (PC = {:#010X})",
                        addr,
                        pc,
                    );
                }
                (emu.arm7.last_bios_word >> ((addr & 3) << 3)) as u8
            }
        }

        #[cfg(feature = "bft-r")]
        0x02 => emu.main_mem().read(addr as usize & 0x3F_FFFF),

        #[cfg(feature = "bft-r")]
        0x03 => {
            if addr & 1 << 23 == 0 {
                unsafe {
                    emu.swram
                        .arm7_ptr()
                        .add(addr as usize & emu.swram.arm7_mask() as usize)
                        .read()
                }
            } else {
                emu.arm7.wram.read(addr as usize & 0xFFFF)
            }
        }

        0x04 => {
            if addr & 1 << 23 == 0 {
                #[allow(clippy::match_same_arms)]
                match addr & 0x007F_FFFF {
                    0x004 => emu.gpu.disp_status_7().0 as u8,
                    0x005 => (emu.gpu.disp_status_7().0 >> 8) as u8,

                    0x006 => emu.gpu.vcount() as u8,
                    0x007 => (emu.gpu.vcount() >> 8) as u8,

                    0x100 => emu.arm7.timers.read_counter(
                        timers::Index::new(0),
                        &mut emu.arm7.schedule,
                        &mut emu.arm7.irqs,
                    ) as u8,
                    0x101 => {
                        (emu.arm7.timers.read_counter(
                            timers::Index::new(0),
                            &mut emu.arm7.schedule,
                            &mut emu.arm7.irqs,
                        ) >> 8) as u8
                    }
                    0x102 => emu.arm7.timers.0[0].control().0,
                    0x103 => 0,

                    0x104 => emu.arm7.timers.read_counter(
                        timers::Index::new(1),
                        &mut emu.arm7.schedule,
                        &mut emu.arm7.irqs,
                    ) as u8,
                    0x105 => {
                        (emu.arm7.timers.read_counter(
                            timers::Index::new(1),
                            &mut emu.arm7.schedule,
                            &mut emu.arm7.irqs,
                        ) >> 8) as u8
                    }
                    0x106 => emu.arm7.timers.0[1].control().0,
                    0x107 => 0,

                    0x108 => emu.arm7.timers.read_counter(
                        timers::Index::new(2),
                        &mut emu.arm7.schedule,
                        &mut emu.arm7.irqs,
                    ) as u8,
                    0x109 => {
                        (emu.arm7.timers.read_counter(
                            timers::Index::new(2),
                            &mut emu.arm7.schedule,
                            &mut emu.arm7.irqs,
                        ) >> 8) as u8
                    }
                    0x10A => emu.arm7.timers.0[2].control().0,
                    0x10B => 0,

                    0x10C => emu.arm7.timers.read_counter(
                        timers::Index::new(3),
                        &mut emu.arm7.schedule,
                        &mut emu.arm7.irqs,
                    ) as u8,
                    0x10D => {
                        (emu.arm7.timers.read_counter(
                            timers::Index::new(3),
                            &mut emu.arm7.schedule,
                            &mut emu.arm7.irqs,
                        ) >> 8) as u8
                    }
                    0x10E => emu.arm7.timers.0[3].control().0,
                    0x10F => 0,

                    0x130 => emu.input.status().0 as u8,
                    0x131 => (emu.input.status().0 >> 8) as u8,
                    0x132 => emu.input.arm7_key_irq_control().0 as u8,
                    0x133 => (emu.input.arm7_key_irq_control().0 >> 8) as u8,

                    0x134 => emu.rcnt() as u8,
                    0x135 => (emu.rcnt() >> 8) as u8,

                    0x136 => (emu.input.status().0 >> 16) as u8,
                    0x137 => 0,

                    0x138 => emu.rtc.control().0 as u8,
                    0x139 => (emu.rtc.control().0 >> 8) as u8,
                    0x13A..=0x13B => 0,

                    0x180 => emu.ipc.sync_7().0 as u8,
                    0x181 => (emu.ipc.sync_7().0 >> 8) as u8,
                    0x182 | 0x183 => 0,
                    0x184 => emu.ipc.fifo_control_7().0 as u8,
                    0x185 => (emu.ipc.fifo_control_7().0 >> 8) as u8,
                    0x186 | 0x187 => 0,

                    0x1A0 => emu.ds_slot.spi_control().0 as u8,
                    0x1A1 => (emu.ds_slot.spi_control().0 >> 8) as u8,

                    0x1A2 => emu.ds_slot.spi_data_out(),
                    0x1A3 => 0,

                    0x1A4 => emu.ds_slot.rom_control().0 as u8,
                    0x1A5 => (emu.ds_slot.rom_control().0 >> 8) as u8,
                    0x1A6 => (emu.ds_slot.rom_control().0 >> 16) as u8,
                    0x1A7 => (emu.ds_slot.rom_control().0 >> 24) as u8,

                    0x1A8..=0x1AF => emu.ds_slot.rom_cmd[addr as usize & 7],

                    0x1C0 => emu.spi.control().0 as u8,
                    0x1C1 => (emu.spi.control().0 >> 8) as u8,

                    0x1C2 => emu.spi.read_data(),
                    0x1C3 => 0,

                    0x204 => emu.arm7.local_ex_mem_control.0 | emu.global_ex_mem_control().0 as u8,
                    0x205 => (emu.global_ex_mem_control().0 >> 8) as u8,

                    0x208 => emu.arm7.irqs.master_enable() as u8,
                    0x209..=0x20B => 0,

                    0x240 => emu.gpu.vram.arm7_status().0,

                    0x241 => emu.swram.control().0,

                    0x300 => emu.arm7.post_boot_flag as u8,
                    0x302..=0x303 => 0,

                    0x304 => emu.audio_wifi_power_control().0,
                    0x305..=0x307 => 0,

                    0x308 => emu.arm7.bios_prot as u8,
                    0x309 => (emu.arm7.bios_prot >> 8) as u8,
                    0x30A..=0x30B => 0,

                    0x400..=0x51F => emu.audio.read_8::<A>(addr),

                    _ => {
                        #[cfg(feature = "log")]
                        if !A::IS_DEBUG {
                            slog::warn!(
                                emu.arm7.logger,
                                "Unknown IO read8 @ {:#05X}",
                                addr & 0x007F_FFFF
                            );
                        }
                        0
                    }
                }
            } else {
                // TODO: Wi-Fi
                0
            }
        }

        #[cfg(feature = "bft-r")]
        0x06 => emu.gpu.vram.read_arm7(addr),

        0x08 | 0x09 => {
            if emu.global_ex_mem_control().arm7_gba_slot_access() {
                (emu.arm7.local_ex_mem_control().gba_rom_halfword(addr) >> ((addr & 1) << 3)) as u8
            } else {
                0
            }
        }

        0x0A => {
            if emu.global_ex_mem_control().arm7_gba_slot_access() {
                0xFF
            } else {
                0
            }
        }

        _ => {
            #[cfg(feature = "log")]
            if !A::IS_DEBUG {
                slog::warn!(emu.arm7.logger, "Unknown read8 @ {:#010X}", addr);
            }
            0
        }
    }
}

#[inline(never)]
pub fn read_16<A: AccessType, E: Engine>(emu: &mut Emu<E>, mut addr: u32) -> u16 {
    #[cfg(feature = "debugger-hooks")]
    check_watchpoints!(emu, emu.arm7, addr, 1, 5, Read);
    addr &= !1;
    match addr >> 24 {
        0x00 if addr < BIOS_SIZE as u32 => {
            let max_pc = if addr < emu.arm7.bios_prot as u32 {
                emu.arm7.bios_prot as u32
            } else {
                BIOS_SIZE as u32
            };
            let pc = emu.arm7.engine_data.r15();
            if pc < max_pc || A::IS_DEBUG {
                if !A::IS_DEBUG {
                    emu.arm7.last_bios_word = emu.arm7.bios.read_le(addr as usize & !3);
                }
                emu.arm7.bios.read_le(addr as usize)
            } else {
                #[cfg(feature = "log")]
                if !A::IS_DEBUG {
                    slog::warn!(
                        emu.arm7.logger,
                        "Forbidden read16 from BIOS region @ {:#06X} (PC = {:#010X})",
                        addr,
                        pc,
                    );
                }
                (emu.arm7.last_bios_word >> ((addr & 2) << 3)) as u16
            }
        }

        #[cfg(feature = "bft-r")]
        0x02 => emu.main_mem().read_le(addr as usize & 0x3F_FFFE),

        #[cfg(feature = "bft-r")]
        0x03 => {
            if addr & 1 << 23 == 0 {
                unsafe {
                    u16::read_le_aligned(
                        emu.swram
                            .arm7_ptr()
                            .add(addr as usize & emu.swram.arm7_mask() as usize)
                            as *const u16,
                    )
                }
            } else {
                emu.arm7.wram.read_le(addr as usize & 0xFFFE)
            }
        }

        0x04 => {
            if addr & 1 << 23 == 0 {
                #[allow(clippy::match_same_arms)]
                match addr & 0x007F_FFFE {
                    0x004 => emu.gpu.disp_status_7().0,
                    0x006 => emu.gpu.vcount(),

                    0x0B0 => emu.arm7.dma.channels[0].src_addr as u16,
                    0x0B2 => (emu.arm7.dma.channels[0].src_addr >> 16) as u16,
                    0x0B4 => emu.arm7.dma.channels[0].dst_addr as u16,
                    0x0B6 => (emu.arm7.dma.channels[0].dst_addr >> 16) as u16,
                    0x0B8 => emu.arm7.dma.channels[0].control.0 as u16,
                    0x0BA => (emu.arm7.dma.channels[0].control.0 >> 16) as u16,

                    0x0BC => emu.arm7.dma.channels[1].src_addr as u16,
                    0x0BE => (emu.arm7.dma.channels[1].src_addr >> 16) as u16,
                    0x0C0 => emu.arm7.dma.channels[1].dst_addr as u16,
                    0x0C2 => (emu.arm7.dma.channels[1].dst_addr >> 16) as u16,
                    0x0C4 => emu.arm7.dma.channels[1].control.0 as u16,
                    0x0C6 => (emu.arm7.dma.channels[1].control.0 >> 16) as u16,

                    0x0C8 => emu.arm7.dma.channels[2].src_addr as u16,
                    0x0CA => (emu.arm7.dma.channels[2].src_addr >> 16) as u16,
                    0x0CC => emu.arm7.dma.channels[2].dst_addr as u16,
                    0x0CE => (emu.arm7.dma.channels[2].dst_addr >> 16) as u16,
                    0x0D0 => emu.arm7.dma.channels[2].control.0 as u16,
                    0x0D2 => (emu.arm7.dma.channels[2].control.0 >> 16) as u16,

                    0x0D4 => emu.arm7.dma.channels[3].src_addr as u16,
                    0x0D6 => (emu.arm7.dma.channels[3].src_addr >> 16) as u16,
                    0x0D8 => emu.arm7.dma.channels[3].dst_addr as u16,
                    0x0DA => (emu.arm7.dma.channels[3].dst_addr >> 16) as u16,
                    0x0DC => emu.arm7.dma.channels[3].control.0 as u16,
                    0x0DE => (emu.arm7.dma.channels[3].control.0 >> 16) as u16,

                    0x100 => emu.arm7.timers.read_counter(
                        timers::Index::new(0),
                        &mut emu.arm7.schedule,
                        &mut emu.arm7.irqs,
                    ),
                    0x102 => emu.arm7.timers.0[0].control().0 as u16,

                    0x104 => emu.arm7.timers.read_counter(
                        timers::Index::new(1),
                        &mut emu.arm7.schedule,
                        &mut emu.arm7.irqs,
                    ),
                    0x106 => emu.arm7.timers.0[1].control().0 as u16,

                    0x108 => emu.arm7.timers.read_counter(
                        timers::Index::new(2),
                        &mut emu.arm7.schedule,
                        &mut emu.arm7.irqs,
                    ),
                    0x10A => emu.arm7.timers.0[2].control().0 as u16,

                    0x10C => emu.arm7.timers.read_counter(
                        timers::Index::new(3),
                        &mut emu.arm7.schedule,
                        &mut emu.arm7.irqs,
                    ),
                    0x10E => emu.arm7.timers.0[3].control().0 as u16,

                    0x130 => emu.input.status().0 as u16,
                    0x132 => emu.input.arm7_key_irq_control().0,

                    0x134 => emu.rcnt(),

                    0x136 => (emu.input.status().0 >> 16) as u16,

                    0x138 => emu.rtc.control().0,
                    0x13A => 0,

                    0x180 => emu.ipc.sync_7().0,
                    0x182 => 0,
                    0x184 => emu.ipc.fifo_control_7().0,
                    0x186 => 0,

                    0x1A0 => emu.ds_slot.spi_control().0,
                    0x1A2 => emu.ds_slot.spi_data_out() as u16,
                    0x1A4 => emu.ds_slot.rom_control().0 as u16,
                    0x1A6 => (emu.ds_slot.rom_control().0 >> 16) as u16,
                    0x1A8..=0x1AE => emu.ds_slot.rom_cmd.read_le(addr as usize & 6),

                    0x1C0 => emu.spi.control().0,
                    0x1C2 => emu.spi.read_data() as u16,

                    0x204 => emu.arm7.local_ex_mem_control.0 as u16 | emu.global_ex_mem_control().0,

                    0x208 => emu.arm7.irqs.master_enable() as u16,
                    0x20A => 0,

                    0x210 => emu.arm7.irqs.enabled().0 as u16,
                    0x212 => (emu.arm7.irqs.enabled().0 >> 16) as u16,

                    0x214 => emu.arm7.irqs.requested().0 as u16,
                    0x216 => (emu.arm7.irqs.requested().0 >> 16) as u16,

                    0x240 => {
                        emu.gpu.vram.arm7_status().0 as u16 | (emu.swram.control().0 as u16) << 8
                    }

                    0x300 => emu.arm7.post_boot_flag as u16,
                    0x302 => 0,

                    0x304 => emu.audio_wifi_power_control().0 as u16,
                    0x306 => 0,

                    0x308 => emu.arm7.bios_prot,
                    0x30A => 0,

                    0x400..=0x51E => emu.audio.read_16::<A>(addr),

                    _ => {
                        #[cfg(feature = "log")]
                        if !A::IS_DEBUG {
                            slog::warn!(
                                emu.arm7.logger,
                                "Unknown IO read16 @ {:#05X}",
                                addr & 0x007F_FFFE
                            );
                        }
                        0
                    }
                }
            } else {
                // TODO: Wi-Fi
                0
            }
        }

        #[cfg(feature = "bft-r")]
        0x06 => emu.gpu.vram.read_arm7(addr),

        0x08 | 0x09 => {
            if emu.global_ex_mem_control().arm7_gba_slot_access() {
                emu.arm7.local_ex_mem_control().gba_rom_halfword(addr)
            } else {
                0
            }
        }

        0x0A => {
            if emu.global_ex_mem_control().arm7_gba_slot_access() {
                0xFFFF
            } else {
                0
            }
        }

        _ => {
            #[cfg(feature = "log")]
            if !A::IS_DEBUG {
                slog::warn!(emu.arm7.logger, "Unknown read16 @ {:#010X}", addr);
            }
            0
        }
    }
}

#[inline(never)]
pub fn read_32<A: AccessType, E: Engine>(emu: &mut Emu<E>, mut addr: u32) -> u32 {
    #[cfg(feature = "debugger-hooks")]
    check_watchpoints!(emu, emu.arm7, addr, 3, 0x55, Read);
    addr &= !3;
    match addr >> 24 {
        0x00 if addr < BIOS_SIZE as u32 => {
            let max_pc = if addr < emu.arm7.bios_prot as u32 {
                emu.arm7.bios_prot as u32
            } else {
                BIOS_SIZE as u32
            };
            let pc = emu.arm7.engine_data.r15();
            if pc < max_pc || A::IS_DEBUG {
                let value = unsafe { emu.arm7.bios.read_le_aligned(addr as usize) };
                if !A::IS_DEBUG {
                    emu.arm7.last_bios_word = value;
                }
                value
            } else {
                #[cfg(feature = "log")]
                if !A::IS_DEBUG {
                    slog::warn!(
                        emu.arm7.logger,
                        "Forbidden read32 from BIOS region @ {:#06X} (PC = {:#010X})",
                        addr,
                        pc,
                    );
                }
                emu.arm7.last_bios_word
            }
        }

        #[cfg(feature = "bft-r")]
        0x02 => emu.main_mem().read_le(addr as usize & 0x3F_FFFC),

        #[cfg(feature = "bft-r")]
        0x03 => {
            if addr & 1 << 23 == 0 {
                unsafe {
                    u32::read_le_aligned(
                        emu.swram
                            .arm7_ptr()
                            .add(addr as usize & emu.swram.arm7_mask() as usize)
                            as *const u32,
                    )
                }
            } else {
                emu.arm7.wram.read_le(addr as usize & 0xFFFC)
            }
        }

        0x04 => {
            if addr & 1 << 23 == 0 {
                match addr & 0x007F_FFFC {
                    0x004 => emu.gpu.disp_status_7().0 as u32 | (emu.gpu.vcount() as u32) << 16,

                    0x0B0 => emu.arm7.dma.channels[0].src_addr,
                    0x0B4 => emu.arm7.dma.channels[0].dst_addr,
                    0x0B8 => emu.arm7.dma.channels[0].control.0,

                    0x0BC => emu.arm7.dma.channels[1].src_addr,
                    0x0C0 => emu.arm7.dma.channels[1].dst_addr,
                    0x0C4 => emu.arm7.dma.channels[1].control.0,

                    0x0C8 => emu.arm7.dma.channels[2].src_addr,
                    0x0CC => emu.arm7.dma.channels[2].dst_addr,
                    0x0D0 => emu.arm7.dma.channels[2].control.0,

                    0x0D4 => emu.arm7.dma.channels[3].src_addr,
                    0x0D8 => emu.arm7.dma.channels[3].dst_addr,
                    0x0DC => emu.arm7.dma.channels[3].control.0,

                    0x100 => {
                        emu.arm7.timers.read_counter(
                            timers::Index::new(0),
                            &mut emu.arm7.schedule,
                            &mut emu.arm7.irqs,
                        ) as u32
                            | (emu.arm7.timers.0[0].control().0 as u32) << 16
                    }

                    0x104 => {
                        emu.arm7.timers.read_counter(
                            timers::Index::new(1),
                            &mut emu.arm7.schedule,
                            &mut emu.arm7.irqs,
                        ) as u32
                            | (emu.arm7.timers.0[1].control().0 as u32) << 16
                    }

                    0x108 => {
                        emu.arm7.timers.read_counter(
                            timers::Index::new(2),
                            &mut emu.arm7.schedule,
                            &mut emu.arm7.irqs,
                        ) as u32
                            | (emu.arm7.timers.0[2].control().0 as u32) << 16
                    }

                    0x10C => {
                        emu.arm7.timers.read_counter(
                            timers::Index::new(3),
                            &mut emu.arm7.schedule,
                            &mut emu.arm7.irqs,
                        ) as u32
                            | (emu.arm7.timers.0[3].control().0 as u32) << 16
                    }

                    0x130 => {
                        (emu.input.status().0 & 0xFFFF)
                            | (emu.input.arm7_key_irq_control().0 as u32) << 16
                    }

                    0x134 => emu.rcnt() as u32 | (emu.input.status().0 & 0xFFFF_0000) as u32,

                    0x138 => emu.rtc.control().0 as u32,

                    0x180 => emu.ipc.sync_7().0 as u32,
                    0x184 => emu.ipc.fifo_control_7().0 as u32,

                    0x1A0 => {
                        emu.ds_slot.spi_control().0 as u32
                            | (emu.ds_slot.spi_data_out() as u32) << 16
                    }
                    0x1A4 => emu.ds_slot.rom_control().0,
                    0x1A8..=0x1AC => emu.ds_slot.rom_cmd.read_le(addr as usize & 4),

                    0x1C0 => emu.spi.control().0 as u32 | (emu.spi.read_data() as u32) << 16,

                    0x204 => {
                        emu.arm7.local_ex_mem_control.0 as u32
                            | emu.global_ex_mem_control().0 as u32
                    }

                    0x208 => emu.arm7.irqs.master_enable() as u32,
                    0x210 => emu.arm7.irqs.enabled().0,
                    0x214 => emu.arm7.irqs.requested().0,

                    0x240 => {
                        emu.gpu.vram.arm7_status().0 as u32 | (emu.swram.control().0 as u32) << 8
                    }

                    0x300 => emu.arm7.post_boot_flag as u32,

                    0x304 => emu.audio_wifi_power_control().0 as u32,

                    0x400..=0x51C => emu.audio.read_32::<A>(addr),

                    0x10_0000 => {
                        if A::IS_DEBUG {
                            emu.ipc.peek_7()
                        } else {
                            emu.ipc.recv_7(&mut emu.arm9.irqs)
                        }
                    }

                    0x10_0010 => {
                        if emu.ds_slot.arm7_access() {
                            if A::IS_DEBUG {
                                emu.ds_slot.peek_rom_data()
                            } else {
                                emu.ds_slot
                                    .read_rom_data_arm7(&mut emu.arm7.irqs, &mut emu.arm7.schedule)
                            }
                        } else {
                            // TODO: What happens?
                            0
                        }
                    }

                    _ => {
                        #[cfg(feature = "log")]
                        if !A::IS_DEBUG {
                            slog::warn!(
                                emu.arm7.logger,
                                "Unknown IO read32 @ {:#05X}",
                                addr & 0x007F_FFFC
                            );
                        }
                        0
                    }
                }
            } else {
                // TODO: Wi-Fi
                0
            }
        }

        #[cfg(feature = "bft-r")]
        0x06 => emu.gpu.vram.read_arm7(addr),

        0x08 | 0x09 => {
            if emu.global_ex_mem_control().arm7_gba_slot_access() {
                emu.arm7.local_ex_mem_control().gba_rom_word(addr)
            } else {
                0
            }
        }

        0x0A => {
            if emu.global_ex_mem_control().arm7_gba_slot_access() {
                0xFFFF_FFFF
            } else {
                0
            }
        }

        _ => {
            #[cfg(feature = "log")]
            if !A::IS_DEBUG {
                slog::warn!(emu.arm7.logger, "Unknown read32 @ {:#010X}", addr);
            }
            0
        }
    }
}

#[inline(never)]
#[allow(clippy::single_match)]
pub fn write_8<A: AccessType, E: Engine>(emu: &mut Emu<E>, addr: u32, value: u8) {
    emu.arm7.engine_data.invalidate_word(addr);
    #[cfg(feature = "debugger-hooks")]
    check_watchpoints!(emu, emu.arm7, addr, 0, 2, Write);
    match addr >> 24 {
        #[cfg(feature = "bft-w")]
        0x02 => emu.main_mem().write(addr as usize & 0x3F_FFFF, value),

        #[cfg(feature = "bft-w")]
        0x03 => {
            if addr & 1 << 23 == 0 {
                unsafe {
                    emu.swram
                        .arm7_ptr()
                        .add(addr as usize & emu.swram.arm7_mask() as usize)
                        .write(value);
                }
            } else {
                emu.arm7.wram.write(addr as usize & 0xFFFF, value);
            }
        }

        0x04 => {
            if addr & 1 << 23 == 0 {
                #[allow(clippy::match_same_arms)]
                match addr & 0x007F_FFFF {
                    0x004 => emu.gpu.write_disp_status_7(gpu::DispStatus(
                        (emu.gpu.disp_status_7().0 & 0xFF00) | value as u16,
                    )),
                    0x005 => emu.gpu.write_disp_status_7(gpu::DispStatus(
                        (emu.gpu.disp_status_7().0 & 0x00FF) | (value as u16) << 8,
                    )),

                    0x132 => emu.write_arm7_key_irq_control(KeyIrqControl(
                        (emu.input.arm7_key_irq_control().0 & 0xFF00) | value as u16,
                    )),
                    0x133 => emu.write_arm7_key_irq_control(KeyIrqControl(
                        (emu.input.arm7_key_irq_control().0 & 0x00FF) | (value as u16) << 8,
                    )),

                    0x134 => emu.write_rcnt((emu.rcnt() & 0xFF00) | value as u16),
                    0x135 => emu.write_rcnt((emu.rcnt() & 0x00FF) | (value as u16) << 8),

                    0x138 => emu
                        .rtc
                        .write_control(rtc::Control((emu.rtc.control().0 & 0xFF00) | value as u16)),
                    0x139 => emu.rtc.write_control(rtc::Control(
                        (emu.rtc.control().0 & 0x00FF) | (value as u16) << 8,
                    )),

                    0x180 | 0x182 | 0x183 => {}
                    0x181 => emu.ipc.write_sync_7(
                        ipc::Sync((emu.ipc.sync_7().0 & 0x00FF) | (value as u16) << 8),
                        &mut emu.arm9.irqs,
                    ),
                    0x184 => emu.ipc.write_fifo_control_7(
                        ipc::FifoControl((emu.ipc.fifo_control_7().0 & 0xBF00) | value as u16),
                        &mut emu.arm7.irqs,
                        &mut emu.arm7.schedule,
                    ),
                    0x185 => emu.ipc.write_fifo_control_7(
                        ipc::FifoControl(
                            (emu.ipc.fifo_control_7().0 & 0x00FF) | (value as u16) << 8,
                        ),
                        &mut emu.arm7.irqs,
                        &mut emu.arm7.schedule,
                    ),
                    0x186 | 0x187 => {}

                    0x1A0 => {
                        if emu.ds_slot.arm7_access() {
                            emu.ds_slot.write_spi_control(ds_slot::AuxSpiControl(
                                (emu.ds_slot.spi_control().0 & 0xFF00) | value as u16,
                            ));
                        } else {
                            #[cfg(feature = "log")]
                            if !A::IS_DEBUG {
                                slog::warn!(
                                    emu.arm7.logger,
                                    "Tried to write to AUXSPICNT while inaccessible"
                                );
                            }
                        }
                    }
                    0x1A1 => {
                        if emu.ds_slot.arm7_access() {
                            emu.ds_slot.write_spi_control(ds_slot::AuxSpiControl(
                                (emu.ds_slot.spi_control().0 & 0x00FF) | (value as u16) << 8,
                            ));
                        } else {
                            #[cfg(feature = "log")]
                            if !A::IS_DEBUG {
                                slog::warn!(
                                    emu.arm7.logger,
                                    "Tried to write to AUXSPICNT while inaccessible"
                                );
                            }
                        }
                    }

                    0x1A2 => {
                        if emu.ds_slot.arm7_access() {
                            emu.ds_slot.write_spi_data(
                                value,
                                &mut emu.arm7.schedule,
                                &mut emu.arm9.schedule,
                            );
                        } else {
                            #[cfg(feature = "log")]
                            if !A::IS_DEBUG {
                                slog::warn!(
                                    emu.arm7.logger,
                                    "Tried to write to AUXSPIDATA while inaccessible"
                                );
                            }
                        }
                    }
                    0x1A3 => {
                        if !emu.ds_slot.arm7_access() {
                            #[cfg(feature = "log")]
                            if !A::IS_DEBUG {
                                slog::warn!(
                                    emu.arm7.logger,
                                    "Tried to write to AUXSPIDATA while inaccessible"
                                );
                            }
                        }
                    }

                    0x1A8..=0x1AF => {
                        if emu.ds_slot.arm7_access() {
                            emu.ds_slot.rom_cmd[addr as usize & 7] = value;
                        } else {
                            #[cfg(feature = "log")]
                            if !A::IS_DEBUG {
                                slog::warn!(
                                    emu.arm7.logger,
                                    "Tried to write to DS slot ROM command while inaccessible"
                                );
                            }
                        }
                    }

                    0x1C0 => emu
                        .spi
                        .write_control(spi::Control((emu.spi.control().0 & 0xFF00) | value as u16)),
                    0x1C1 => emu.spi.write_control(spi::Control(
                        (emu.spi.control().0 & 0x00FF) | (value as u16) << 8,
                    )),

                    0x1C2 => {
                        emu.spi.write_data(
                            value,
                            &mut emu.arm7.schedule,
                            &mut emu.schedule,
                            &mut emu.input.status,
                        );
                    }
                    0x1C3 => {}

                    0x204 => emu
                        .arm7
                        .write_local_ex_mem_control(LocalExMemControl(value)),
                    0x205 => {}

                    0x208 => emu
                        .arm7
                        .irqs
                        .write_master_enable(value & 1 != 0, &mut emu.arm7.schedule),
                    0x209..=0x20B => {}

                    0x300 => emu.arm7.post_boot_flag |= value & 1 != 0,

                    0x301 => match value >> 6 {
                        0 => {}
                        1 => {
                            unimplemented!("GBA mode switch");
                        }
                        2 => {
                            emu.arm7.irqs.halt(&mut emu.arm7.schedule);
                        }
                        _ => {
                            todo!("Sleep mode switch");
                        }
                    },

                    0x304 => emu.write_audio_wifi_power_control(AudioWifiPowerControl(value)),
                    0x305..=0x307 => {}

                    0x400..=0x51F => emu.audio.write_8::<A>(addr, value),

                    _ =>
                    {
                        #[cfg(feature = "log")]
                        if !A::IS_DEBUG {
                            slog::warn!(
                                emu.arm7.logger,
                                "Unknown IO write8 @ {:#05X}: {:#04X}",
                                addr & 0x007F_FFFF,
                                value
                            );
                        }
                    }
                }
            } else {
                // TODO: Wi-Fi
            }
        }

        0x06 => emu.gpu.vram.write_arm7(addr, value),

        _ =>
        {
            #[cfg(feature = "log")]
            if !A::IS_DEBUG {
                slog::warn!(
                    emu.arm7.logger,
                    "Unknown write8 @ {:#010X}: {:#04X}",
                    addr,
                    value
                );
            }
        }
    }
}

#[inline(never)]
#[allow(clippy::single_match)]
pub fn write_16<A: AccessType, E: Engine>(emu: &mut Emu<E>, mut addr: u32, value: u16) {
    emu.arm7.engine_data.invalidate_word(addr);
    #[cfg(feature = "debugger-hooks")]
    check_watchpoints!(emu, emu.arm7, addr, 1, 0xA, Write);
    addr &= !1;
    match addr >> 24 {
        #[cfg(feature = "bft-w")]
        0x02 => emu.main_mem().write_le(addr as usize & 0x3F_FFFE, value),

        #[cfg(feature = "bft-w")]
        0x03 => {
            if addr & 1 << 23 == 0 {
                unsafe {
                    value.write_le_aligned(
                        emu.swram
                            .arm7_ptr()
                            .add(addr as usize & emu.swram.arm7_mask() as usize)
                            as *mut u16,
                    );
                }
            } else {
                emu.arm7.wram.write_le(addr as usize & 0xFFFE, value);
            }
        }

        0x04 => {
            if addr & 1 << 23 == 0 {
                #[allow(clippy::match_same_arms)]
                match addr & 0x007F_FFFE {
                    0x004 => emu.gpu.write_disp_status_7(gpu::DispStatus(value)),
                    0x006 => emu.gpu.write_vcount(value),

                    0x0B0 => emu.arm7.dma.channels[0].write_src_addr(
                        (emu.arm7.dma.channels[0].src_addr & 0xFFFF_0000) | value as u32,
                    ),
                    0x0B2 => emu.arm7.dma.channels[0].write_src_addr(
                        (emu.arm7.dma.channels[0].src_addr & 0x0000_FFFF) | (value as u32) << 16,
                    ),
                    0x0B4 => emu.arm7.dma.channels[0].write_dst_addr(
                        (emu.arm7.dma.channels[0].dst_addr & 0xFFFF_0000) | value as u32,
                    ),
                    0x0B6 => emu.arm7.dma.channels[0].write_dst_addr(
                        (emu.arm7.dma.channels[0].dst_addr & 0x0000_FFFF) | (value as u32) << 16,
                    ),
                    0x0B8 => emu.arm7.dma.channels[0].write_control_low(value),
                    0x0BA => emu.arm7.write_dma_channel_control(
                        dma::Index::new(0),
                        dma::Control(
                            (emu.arm7.dma.channels[0].control.0 & 0x0000_FFFF)
                                | (value as u32) << 16,
                        ),
                    ),

                    0x0BC => emu.arm7.dma.channels[1].write_src_addr(
                        (emu.arm7.dma.channels[1].src_addr & 0xFFFF_0000) | value as u32,
                    ),
                    0x0BE => emu.arm7.dma.channels[1].write_src_addr(
                        (emu.arm7.dma.channels[1].src_addr & 0x0000_FFFF) | (value as u32) << 16,
                    ),
                    0x0C0 => emu.arm7.dma.channels[1].write_dst_addr(
                        (emu.arm7.dma.channels[1].dst_addr & 0xFFFF_0000) | value as u32,
                    ),
                    0x0C2 => emu.arm7.dma.channels[1].write_dst_addr(
                        (emu.arm7.dma.channels[1].dst_addr & 0x0000_FFFF) | (value as u32) << 16,
                    ),
                    0x0C4 => emu.arm7.dma.channels[1].write_control_low(value),
                    0x0C6 => emu.arm7.write_dma_channel_control(
                        dma::Index::new(1),
                        dma::Control(
                            (emu.arm7.dma.channels[1].control.0 & 0x0000_FFFF)
                                | (value as u32) << 16,
                        ),
                    ),

                    0x0C8 => emu.arm7.dma.channels[2].write_src_addr(
                        (emu.arm7.dma.channels[2].src_addr & 0xFFFF_0000) | value as u32,
                    ),
                    0x0CA => emu.arm7.dma.channels[2].write_src_addr(
                        (emu.arm7.dma.channels[2].src_addr & 0x0000_FFFF) | (value as u32) << 16,
                    ),
                    0x0CC => emu.arm7.dma.channels[2].write_dst_addr(
                        (emu.arm7.dma.channels[2].dst_addr & 0xFFFF_0000) | value as u32,
                    ),
                    0x0CE => emu.arm7.dma.channels[2].write_dst_addr(
                        (emu.arm7.dma.channels[2].dst_addr & 0x0000_FFFF) | (value as u32) << 16,
                    ),
                    0x0D0 => emu.arm7.dma.channels[2].write_control_low(value),
                    0x0D2 => emu.arm7.write_dma_channel_control(
                        dma::Index::new(2),
                        dma::Control(
                            (emu.arm7.dma.channels[2].control.0 & 0x0000_FFFF)
                                | (value as u32) << 16,
                        ),
                    ),
                    0x0D4 => emu.arm7.dma.channels[3].write_src_addr(
                        (emu.arm7.dma.channels[3].src_addr & 0xFFFF_0000) | value as u32,
                    ),
                    0x0D6 => emu.arm7.dma.channels[3].write_src_addr(
                        (emu.arm7.dma.channels[3].src_addr & 0x0000_FFFF) | (value as u32) << 16,
                    ),
                    0x0D8 => emu.arm7.dma.channels[3].write_dst_addr(
                        (emu.arm7.dma.channels[3].dst_addr & 0xFFFF_0000) | value as u32,
                    ),
                    0x0DA => emu.arm7.dma.channels[3].write_dst_addr(
                        (emu.arm7.dma.channels[3].dst_addr & 0x0000_FFFF) | (value as u32) << 16,
                    ),
                    0x0DC => emu.arm7.dma.channels[3].write_control_low(value),
                    0x0DE => emu.arm7.write_dma_channel_control(
                        dma::Index::new(3),
                        dma::Control(
                            (emu.arm7.dma.channels[3].control.0 & 0x0000_FFFF)
                                | (value as u32) << 16,
                        ),
                    ),

                    0x100 => emu.arm7.timers.write_reload(
                        timers::Index::new(0),
                        value,
                        &mut emu.arm7.schedule,
                        &mut emu.arm7.irqs,
                    ),
                    0x102 => emu.arm7.timers.write_control(
                        timers::Index::new(0),
                        timers::Control(value as u8),
                        &mut emu.arm7.schedule,
                        &mut emu.arm7.irqs,
                    ),

                    0x104 => emu.arm7.timers.write_reload(
                        timers::Index::new(1),
                        value,
                        &mut emu.arm7.schedule,
                        &mut emu.arm7.irqs,
                    ),
                    0x106 => emu.arm7.timers.write_control(
                        timers::Index::new(1),
                        timers::Control(value as u8),
                        &mut emu.arm7.schedule,
                        &mut emu.arm7.irqs,
                    ),

                    0x108 => emu.arm7.timers.write_reload(
                        timers::Index::new(2),
                        value,
                        &mut emu.arm7.schedule,
                        &mut emu.arm7.irqs,
                    ),
                    0x10A => emu.arm7.timers.write_control(
                        timers::Index::new(2),
                        timers::Control(value as u8),
                        &mut emu.arm7.schedule,
                        &mut emu.arm7.irqs,
                    ),

                    0x10C => emu.arm7.timers.write_reload(
                        timers::Index::new(3),
                        value,
                        &mut emu.arm7.schedule,
                        &mut emu.arm7.irqs,
                    ),
                    0x10E => emu.arm7.timers.write_control(
                        timers::Index::new(3),
                        timers::Control(value as u8),
                        &mut emu.arm7.schedule,
                        &mut emu.arm7.irqs,
                    ),

                    0x132 => emu.write_arm7_key_irq_control(KeyIrqControl(value)),

                    0x134 => emu.write_rcnt(value),

                    0x138 => emu.rtc.write_control(rtc::Control(value)),

                    0x180 => emu.ipc.write_sync_7(ipc::Sync(value), &mut emu.arm9.irqs),
                    0x182 => {}
                    0x184 => emu.ipc.write_fifo_control_7(
                        ipc::FifoControl(value),
                        &mut emu.arm7.irqs,
                        &mut emu.arm7.schedule,
                    ),
                    0x186 => {}

                    0x1A0 => {
                        if emu.ds_slot.arm7_access() {
                            emu.ds_slot.write_spi_control(ds_slot::AuxSpiControl(value));
                        } else {
                            #[cfg(feature = "log")]
                            if !A::IS_DEBUG {
                                slog::warn!(
                                    emu.arm7.logger,
                                    "Tried to write to AUXSPICNT while inaccessible"
                                );
                            }
                        }
                    }

                    0x1A2 => {
                        if emu.ds_slot.arm7_access() {
                            emu.ds_slot.write_spi_data(
                                value as u8,
                                &mut emu.arm7.schedule,
                                &mut emu.arm9.schedule,
                            );
                        } else {
                            #[cfg(feature = "log")]
                            if !A::IS_DEBUG {
                                slog::warn!(
                                    emu.arm7.logger,
                                    "Tried to write to AUXSPIDATA while inaccessible"
                                );
                            }
                        }
                    }

                    0x1A4 => {
                        if emu.ds_slot.arm7_access() {
                            emu.ds_slot.write_rom_control(
                                ds_slot::RomControl(
                                    (emu.ds_slot.rom_control().0 & 0xFFFF_0000) | value as u32,
                                ),
                                &mut emu.arm7.schedule,
                                &mut emu.arm9.schedule,
                            );
                        } else {
                            #[cfg(feature = "log")]
                            if !A::IS_DEBUG {
                                slog::warn!(
                                    emu.arm7.logger,
                                    "Tried to write to ROMCTRL while inaccessible"
                                );
                            }
                        }
                    }
                    0x1A6 => {
                        if emu.ds_slot.arm7_access() {
                            emu.ds_slot.write_rom_control(
                                ds_slot::RomControl(
                                    (emu.ds_slot.rom_control().0 & 0x0000_FFFF)
                                        | (value as u32) << 16,
                                ),
                                &mut emu.arm7.schedule,
                                &mut emu.arm9.schedule,
                            );
                        } else {
                            #[cfg(feature = "log")]
                            if !A::IS_DEBUG {
                                slog::warn!(
                                    emu.arm7.logger,
                                    "Tried to write to ROMCTRL while inaccessible"
                                );
                            }
                        }
                    }

                    0x1A8..=0x1AE => {
                        if emu.ds_slot.arm7_access() {
                            emu.ds_slot.rom_cmd.write_le(addr as usize & 6, value);
                        } else {
                            #[cfg(feature = "log")]
                            if !A::IS_DEBUG {
                                slog::warn!(
                                    emu.arm7.logger,
                                    "Tried to write to DS slot ROM command while inaccessible"
                                );
                            }
                        }
                    }

                    // The KEY2 encryption seeds aren't used
                    0x1B8 | 0x1BA => {}

                    0x1C0 => emu.spi.write_control(spi::Control(value)),
                    0x1C2 => emu.spi.write_data(
                        value as u8,
                        &mut emu.arm7.schedule,
                        &mut emu.schedule,
                        &mut emu.input.status,
                    ),

                    0x204 => emu
                        .arm7
                        .write_local_ex_mem_control(LocalExMemControl(value as u8)),

                    0x208 => emu
                        .arm7
                        .irqs
                        .write_master_enable(value & 1 != 0, &mut emu.arm7.schedule),
                    0x20A => {}

                    0x304 => emu.write_audio_wifi_power_control(AudioWifiPowerControl(value as u8)),
                    0x306 => {}

                    0x308 => {
                        if emu.arm7.bios_prot == 0 {
                            emu.arm7.write_bios_prot(value);
                        }
                    }

                    0x400..=0x51E => emu.audio.write_16::<A>(addr, value),

                    _ =>
                    {
                        #[cfg(feature = "log")]
                        if !A::IS_DEBUG {
                            slog::warn!(
                                emu.arm7.logger,
                                "Unknown IO write16 @ {:#05X}: {:#06X}",
                                addr & 0x007F_FFFE,
                                value
                            );
                        }
                    }
                }
            } else {
                // TODO: Wi-Fi
            }
        }

        0x06 => emu.gpu.vram.write_arm7(addr, value),

        _ =>
        {
            #[cfg(feature = "log")]
            if !A::IS_DEBUG {
                slog::warn!(
                    emu.arm7.logger,
                    "Unknown write16 @ {:#010X}: {:#06X}",
                    addr,
                    value
                );
            }
        }
    }
}

#[inline(never)]
#[allow(clippy::single_match)]
pub fn write_32<A: AccessType, E: Engine>(emu: &mut Emu<E>, mut addr: u32, value: u32) {
    emu.arm7.engine_data.invalidate_word(addr);
    #[cfg(feature = "debugger-hooks")]
    check_watchpoints!(emu, emu.arm7, addr, 3, 0xAA, Write);
    addr &= !3;
    match addr >> 24 {
        #[cfg(feature = "bft-w")]
        0x02 => emu.main_mem().write_le(addr as usize & 0x3F_FFFC, value),

        #[cfg(feature = "bft-w")]
        0x03 => {
            if addr & 1 << 23 == 0 {
                unsafe {
                    value.write_le_aligned(
                        emu.swram
                            .arm7_ptr()
                            .add(addr as usize & emu.swram.arm7_mask() as usize)
                            as *mut u32,
                    );
                }
            } else {
                emu.arm7.wram.write_le(addr as usize & 0xFFFC, value);
            }
        }

        0x04 => {
            if addr & 1 << 23 == 0 {
                match addr & 0x007F_FFFC {
                    0x004 => {
                        emu.gpu.write_disp_status_7(gpu::DispStatus(value as u16));
                        emu.gpu.write_vcount((value >> 16) as u16);
                    }

                    0x0B0 => emu.arm7.dma.channels[0].write_src_addr(value),
                    0x0B4 => emu.arm7.dma.channels[0].write_dst_addr(value),
                    0x0B8 => emu
                        .arm7
                        .write_dma_channel_control(dma::Index::new(0), dma::Control(value)),

                    0x0BC => emu.arm7.dma.channels[1].write_src_addr(value),
                    0x0C0 => emu.arm7.dma.channels[1].write_dst_addr(value),
                    0x0C4 => emu
                        .arm7
                        .write_dma_channel_control(dma::Index::new(1), dma::Control(value)),

                    0x0C8 => emu.arm7.dma.channels[2].write_src_addr(value),
                    0x0CC => emu.arm7.dma.channels[2].write_dst_addr(value),
                    0x0D0 => emu
                        .arm7
                        .write_dma_channel_control(dma::Index::new(2), dma::Control(value)),

                    0x0D4 => emu.arm7.dma.channels[3].write_src_addr(value),
                    0x0D8 => emu.arm7.dma.channels[3].write_dst_addr(value),
                    0x0DC => emu
                        .arm7
                        .write_dma_channel_control(dma::Index::new(3), dma::Control(value)),

                    0x100 => emu.arm7.timers.write_control_reload(
                        timers::Index::new(0),
                        value as u16,
                        timers::Control((value >> 16) as u8),
                        &mut emu.arm7.schedule,
                        &mut emu.arm7.irqs,
                    ),

                    0x104 => emu.arm7.timers.write_control_reload(
                        timers::Index::new(1),
                        value as u16,
                        timers::Control((value >> 16) as u8),
                        &mut emu.arm7.schedule,
                        &mut emu.arm7.irqs,
                    ),

                    0x108 => emu.arm7.timers.write_control_reload(
                        timers::Index::new(2),
                        value as u16,
                        timers::Control((value >> 16) as u8),
                        &mut emu.arm7.schedule,
                        &mut emu.arm7.irqs,
                    ),

                    0x10C => emu.arm7.timers.write_control_reload(
                        timers::Index::new(3),
                        value as u16,
                        timers::Control((value >> 16) as u8),
                        &mut emu.arm7.schedule,
                        &mut emu.arm7.irqs,
                    ),

                    0x130 => emu.write_arm7_key_irq_control(KeyIrqControl((value >> 16) as u16)),

                    0x134 => emu.write_rcnt(value as u16),

                    0x138 => emu.rtc.write_control(rtc::Control(value as u16)),

                    0x180 => emu
                        .ipc
                        .write_sync_7(ipc::Sync(value as u16), &mut emu.arm9.irqs),
                    0x184 => emu.ipc.write_fifo_control_7(
                        ipc::FifoControl(value as u16),
                        &mut emu.arm7.irqs,
                        &mut emu.arm7.schedule,
                    ),
                    0x188 => emu.ipc.send_7(value, &mut emu.arm9.irqs),

                    0x1A0 => {
                        if emu.ds_slot.arm7_access() {
                            emu.ds_slot
                                .write_spi_control(ds_slot::AuxSpiControl(value as u16));
                            emu.ds_slot.write_spi_data(
                                (value >> 16) as u8,
                                &mut emu.arm7.schedule,
                                &mut emu.arm9.schedule,
                            );
                        } else {
                            #[cfg(feature = "log")]
                            if !A::IS_DEBUG {
                                slog::warn!(
                                    emu.arm7.logger,
                                    "Tried to write to AUXSPICNT while inaccessible"
                                );
                                slog::warn!(
                                    emu.arm7.logger,
                                    "Tried to write to AUXSPIDATA while inaccessible"
                                );
                            }
                        }
                    }

                    0x1A4 => {
                        if emu.ds_slot.arm7_access() {
                            emu.ds_slot.write_rom_control(
                                ds_slot::RomControl(value),
                                &mut emu.arm7.schedule,
                                &mut emu.arm9.schedule,
                            );
                        } else {
                            #[cfg(feature = "log")]
                            if !A::IS_DEBUG {
                                slog::warn!(
                                    emu.arm7.logger,
                                    "Tried to write to ROMCTRL while inaccessible"
                                );
                            }
                        }
                    }

                    0x1A8 | 0x1AC => {
                        if emu.ds_slot.arm7_access() {
                            emu.ds_slot.rom_cmd.write_le(addr as usize & 4, value);
                        } else {
                            #[cfg(feature = "log")]
                            if !A::IS_DEBUG {
                                slog::warn!(
                                    emu.arm7.logger,
                                    "Tried to write to DS slot ROM command while inaccessible"
                                );
                            }
                        }
                    }

                    // The KEY2 encryption seeds aren't used
                    0x1B0 | 0x1B4 => {}

                    0x1C0 => {
                        emu.spi.write_control(spi::Control(value as u16));
                        emu.spi.write_data(
                            (value >> 16) as u8,
                            &mut emu.arm7.schedule,
                            &mut emu.schedule,
                            &mut emu.input.status,
                        );
                    }

                    0x204 => emu
                        .arm7
                        .write_local_ex_mem_control(LocalExMemControl(value as u8)),

                    0x208 => emu
                        .arm7
                        .irqs
                        .write_master_enable(value & 1 != 0, &mut emu.arm7.schedule),
                    0x210 => emu
                        .arm7
                        .irqs
                        .write_enabled(IrqFlags(value), &mut emu.arm7.schedule),
                    0x214 => emu
                        .arm7
                        .irqs
                        .write_requested(IrqFlags(emu.arm7.irqs.requested().0 & !value), ()),

                    0x304 => emu.write_audio_wifi_power_control(AudioWifiPowerControl(value as u8)),

                    0x308 => {
                        if emu.arm7.bios_prot == 0 {
                            emu.arm7.write_bios_prot(value as u16);
                        }
                    }

                    0x400..=0x51C => emu.audio.write_32::<A>(addr, value),

                    _ =>
                    {
                        #[cfg(feature = "log")]
                        if !A::IS_DEBUG {
                            slog::warn!(
                                emu.arm7.logger,
                                "Unknown IO write32 @ {:#05X}: {:#010X}",
                                addr & 0x007F_FFFC,
                                value
                            );
                        }
                    }
                }
            } else {
                // TODO: Wi-Fi
            }
        }

        0x06 => emu.gpu.vram.write_arm7(addr, value),

        _ =>
        {
            #[cfg(feature = "log")]
            if !A::IS_DEBUG {
                slog::warn!(
                    emu.arm7.logger,
                    "Unknown write32 @ {:#010X}: {:#010X}",
                    addr,
                    value
                );
            }
        }
    }
}
