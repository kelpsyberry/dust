use super::WiFi;
use crate::cpu::bus::AccessType;

impl WiFi {
    fn read_io<A: AccessType>(&mut self, addr: u16) -> u8 {
        self.mmio[(addr & 0xFFF) as usize]
    }

    fn write_io<A: AccessType>(&mut self, mut addr: u16, value: u8) {
        addr &= 0xFFF;
        #[allow(clippy::match_same_arms)]
        match addr {
            0x03D => return,

            0x159 => {
                let index = self.mmio[0x158];
                match value >> 4 {
                    5 => {
                        if let 0x01..=0x0C
                        | 0x13..=0x15
                        | 0x1B..=0x26
                        | 0x28..=0x4C
                        | 0x4E..=0x5C
                        | 0x62
                        | 0x63
                        | 0x65
                        | 0x67
                        | 0x68 = index
                        {
                            self.bb_regs[index as usize] = self.mmio[0x15A];
                        }
                    }
                    6 => self.mmio[0x15C] = self.bb_regs[index as usize],
                    _ => {}
                }
            }

            0x15C..=0x15F => return,

            _ => {}
        }
        self.mmio[addr as usize] = value;
    }

    pub fn read_8<A: AccessType>(&mut self, addr: u16) -> u8 {
        match addr >> 13 & 3 {
            0 | 3 => self.read_io::<A>(addr),
            2 => self.ram[(addr as usize) & 0x1FFF],
            _ => 0,
        }
    }

    pub fn read_16<A: AccessType>(&mut self, addr: u16) -> u16 {
        match addr >> 13 & 3 {
            0 | 3 => self.read_io::<A>(addr) as u16 | (self.read_io::<A>(addr | 1) as u16) << 8,
            2 => self.ram.read_le((addr as usize) & 0x1FFE),
            _ => 0,
        }
    }

    pub fn read_32<A: AccessType>(&mut self, addr: u16) -> u32 {
        match addr >> 13 & 3 {
            0 | 3 => {
                self.read_io::<A>(addr) as u32
                    | (self.read_io::<A>(addr | 1) as u32) << 8
                    | (self.read_io::<A>(addr | 2) as u32) << 16
                    | (self.read_io::<A>(addr | 3) as u32) << 24
            }
            2 => self.ram.read_le((addr as usize) & 0x1FFC),
            _ => 0,
        }
    }

    pub fn write_8<A: AccessType>(&mut self, addr: u16, value: u8) {
        match addr >> 13 & 3 {
            0 | 3 => self.write_io::<A>(addr, value),
            2 => self.ram[(addr as usize) & 0x1FFF] = value,
            _ => {}
        }
    }

    pub fn write_16<A: AccessType>(&mut self, addr: u16, value: u16) {
        match addr >> 13 & 3 {
            0 | 3 => {
                self.write_io::<A>(addr, value as u8);
                self.write_io::<A>(addr | 1, (value >> 8) as u8);
            }
            2 => self.ram.write_le((addr as usize) & 0x1FFE, value),
            _ => {}
        }
    }

    pub fn write_32<A: AccessType>(&mut self, addr: u16, value: u32) {
        match addr >> 13 & 3 {
            0 | 3 => {
                self.write_io::<A>(addr, value as u8);
                self.write_io::<A>(addr | 1, (value >> 8) as u8);
                self.write_io::<A>(addr | 2, (value >> 16) as u8);
                self.write_io::<A>(addr | 3, (value >> 24) as u8);
            }
            2 => self.ram.write_le((addr as usize) & 0x1FFC, value),
            _ => {}
        }
    }
}
