use crate::{
    cpu::Engine,
    cpu::{arm7::bus as arm7_bus, arm9::bus as arm9_bus, bus::CpuAccess},
    ds_slot,
    emu::Emu,
    utils::{make_zero, zeroed_box, ByteMutSlice, Bytes},
};

pub trait Provider {
    fn setup(&mut self) -> bool;
    fn supports_writes(&self) -> bool;
    fn read_sector(&mut self, sector: u32, buffer: &mut Bytes<0x200>) -> bool;
    fn write_sector(&mut self, sector: u32, buffer: &Bytes<0x200>) -> bool;
}

pub struct Dldi {
    provider: Box<dyn Provider>,
    buffer: Box<Bytes<0x200>>,
}

#[allow(clippy::unusual_byte_groupings)]
pub(crate) const CALL_INSTR: u32 = 0xE60_D1D1_0;
pub(crate) const CALL_INSTR_MASK: u32 = 0xFFFF_FFF0;
pub(crate) const CALL_INSTR_FUNCTIONS: u8 = 6;

const SHORT_DRIVER_NAME: [u8; 4] = *b"DUST";
const DRIVER_NAME: &[u8] = b"Dust DLDI driver";
static DRIVER_CODE: [u32; 6] = [
    CALL_INSTR,
    CALL_INSTR | 1,
    CALL_INSTR | 2,
    CALL_INSTR | 3,
    CALL_INSTR | 4,
    CALL_INSTR | 5,
];
const TOTAL_DRIVER_LEN: usize = 0x98;

impl Dldi {
    pub(crate) fn new_if_supported(
        ds_rom: &mut Box<dyn ds_slot::rom::Contents>,
        mut provider: Box<dyn Provider>,
    ) -> Option<Self> {
        let rom_offset = 'search: {
            let mut block_buffer = zeroed_box::<Bytes<0x400>>();
            let mut magic_string_buffer = zeroed_box::<Bytes<12>>();
            for addr in (0..ds_rom.len().checked_sub(TOTAL_DRIVER_LEN)?).step_by(4) {
                if addr & 0x3FF == 0 {
                    ds_rom.read_slice(
                        addr,
                        ByteMutSlice::new(&mut block_buffer[..0x400.min(ds_rom.len() - addr)]),
                    );
                }
                let value = unsafe { block_buffer.read_le_aligned::<u32>(addr & 0x3FC) };
                // Look for the DLDI magic string
                if value == 0xBF8D_A5ED {
                    ds_rom.read_slice(addr, magic_string_buffer.as_byte_mut_slice());
                    if magic_string_buffer[4..12] == b" Chishm\0"[..] {
                        break 'search addr;
                    }
                }
            }
            // If no DLDI section was found, exit
            return None;
        };

        let mut dldi_area = ds_rom.dldi_area_mut(rom_offset, TOTAL_DRIVER_LEN)?;

        // If the driver can't fit into the allocated space, exit
        let driver_size_shift = TOTAL_DRIVER_LEN.next_power_of_two().trailing_zeros() as u8;
        if driver_size_shift > dldi_area[0x0F] {
            return None;
        }

        if !provider.setup() {
            return None;
        }

        // Read the address where the DLDI driver will be stored at run time, in this order:
        // - First try using the start address entry stored in the ROM file
        // - If 0, calculate it by assuming the already-present startup() is stored right after the
        //   header, and subtract 0x80 (the header's size) from its address
        let mut start_addr = dldi_area.read_le::<u32>(0x40);
        if start_addr == 0 {
            start_addr = dldi_area.read_le::<u32>(0x68) - 0x80;
        }
        let end_addr = start_addr + TOTAL_DRIVER_LEN as u32;

        // Copy the DLDI driver into the ROM file
        dldi_area[0x0C] = 1_u8;
        dldi_area[0x0D] = driver_size_shift;
        dldi_area[0x0E] = 0_u8;
        // Preserve the allocated size
        make_zero(&mut dldi_area[0x10..0x40]);
        dldi_area[0x10..0x10 + DRIVER_NAME.len()].copy_from_slice(DRIVER_NAME);
        dldi_area.write_le(0x40, start_addr);
        dldi_area.write_le(0x44, end_addr);
        dldi_area.write_le(0x48, end_addr);
        dldi_area.write_le(0x4C, end_addr);
        dldi_area.write_le(0x50, end_addr);
        dldi_area.write_le(0x54, end_addr);
        dldi_area.write_le(0x58, end_addr);
        dldi_area.write_le(0x5C, end_addr);
        dldi_area[0x60..0x64].copy_from_slice(&SHORT_DRIVER_NAME);
        dldi_area.write_le(0x64, 0x21 | (provider.supports_writes() as u32) << 1);
        dldi_area.write_le(0x68, start_addr + 0x80);
        dldi_area.write_le(0x6C, start_addr + 0x84);
        dldi_area.write_le(0x70, start_addr + 0x88);
        dldi_area.write_le(0x74, start_addr + 0x8C);
        dldi_area.write_le(0x78, start_addr + 0x90);
        dldi_area.write_le(0x7C, start_addr + 0x94);

        for (offset, word) in (0x80_usize..).step_by(4).zip(&DRIVER_CODE) {
            dldi_area.write_le(offset, *word);
        }

        Some(Dldi {
            provider,
            buffer: zeroed_box(),
        })
    }

    pub fn into_provider(self) -> Box<dyn Provider> {
        self.provider
    }
}

pub(crate) fn handle_call_instr_function<E: Engine, const ARM9: bool>(
    emu: &mut Emu<E>,
    function: u8,
    r0_2: [u32; 3],
) -> bool {
    if let Some(mut dldi) = emu.dldi.take() {
        let result = 'outer: {
            match function {
                0 | 1 | 4 | 5 => true,
                2 => {
                    let [start_sector, sectors, mut dst_addr] = r0_2;
                    for sector in start_sector..start_sector + sectors {
                        if !dldi.provider.read_sector(sector, &mut dldi.buffer) {
                            break 'outer false;
                        };
                        if dst_addr & 3 == 0 {
                            for i in (0..0x200).step_by(4) {
                                (if ARM9 {
                                    arm9_bus::write_32::<CpuAccess, _>
                                } else {
                                    arm7_bus::write_32::<CpuAccess, _>
                                })(
                                    emu, dst_addr + i as u32, dldi.buffer.read_le(i)
                                );
                            }
                        } else {
                            for i in 0..0x200 {
                                (if ARM9 {
                                    arm9_bus::write_8::<CpuAccess, _>
                                } else {
                                    arm7_bus::write_8::<CpuAccess, _>
                                })(
                                    emu, dst_addr + i as u32, dldi.buffer[i]
                                );
                            }
                        }
                        dst_addr += 0x200;
                    }
                    true
                }
                3 => {
                    let [start_sector, sectors, mut src_addr] = r0_2;
                    for sector in start_sector..start_sector + sectors {
                        if src_addr & 3 == 0 {
                            for i in (0..0x200).step_by(4) {
                                dldi.buffer.write_le(
                                    i,
                                    (if ARM9 {
                                        arm9_bus::read_32::<CpuAccess, _, false>
                                    } else {
                                        arm7_bus::read_32::<CpuAccess, _>
                                    })(emu, src_addr + i as u32),
                                );
                            }
                        } else {
                            for i in 0..0x200 {
                                dldi.buffer[i] =
                                    (if ARM9 {
                                        arm9_bus::read_8::<CpuAccess, _>
                                    } else {
                                        arm7_bus::read_8::<CpuAccess, _>
                                    })(emu, src_addr + i as u32);
                            }
                        }
                        src_addr += 0x200;
                        if !dldi.provider.write_sector(sector, &dldi.buffer) {
                            break 'outer false;
                        };
                    }
                    true
                }
                _ => false,
            }
        };
        emu.dldi = Some(dldi);
        result
    } else {
        false
    }
}
