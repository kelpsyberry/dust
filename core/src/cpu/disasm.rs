mod arm;
mod common;
mod thumb;

use super::Engine;
use crate::{
    cpu::{
        arm7::bus::{read_16 as arm7_read_16, read_32 as arm7_read_32},
        arm9::bus::read_32 as arm9_read_32,
        bus::DebugCpuAccess,
    },
    emu::Emu,
};
use core::mem::replace;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instr {
    pub addr: u32,
    pub raw: u32,
    pub opcode: String,
    pub comment: String,
}

struct Context {
    pc: u32,
    branch_addr_base: Option<u32>,
    thumb: bool,
    next_instr: Instr,
}

impl Context {
    fn new(pc: u32, thumb: bool) -> Self {
        Context {
            pc: pc.wrapping_add(8 >> thumb as u8),
            branch_addr_base: None,
            thumb,
            next_instr: Instr {
                addr: pc,
                raw: 0,
                opcode: String::new(),
                comment: String::new(),
            },
        }
    }

    fn disassemble_range<E: Engine, const ARM9: bool>(
        mut self,
        emu: &mut Emu<E>,
        end: u32,
        result: &mut Vec<Instr>,
    ) {
        let end_pc = (end & !(1 | (!self.thumb as u32) << 1)).wrapping_add(8 >> self.thumb as u8);
        loop {
            let raw_instr = if ARM9 {
                let word = arm9_read_32::<DebugCpuAccess, _, true>(emu, self.next_instr.addr);
                if self.thumb {
                    word >> ((self.next_instr.addr & 3) << 3) & 0xFFFF
                } else {
                    word
                }
            } else if self.thumb {
                arm7_read_16::<DebugCpuAccess, _>(emu, self.next_instr.addr) as u32
            } else {
                arm7_read_32::<DebugCpuAccess, _>(emu, self.next_instr.addr)
            };
            self.next_instr.raw = raw_instr;

            if self.thumb {
                thumb::handle_instr::<ARM9>(&mut self, raw_instr as u16);
            } else {
                arm::handle_instr::<ARM9>(&mut self, raw_instr);
            }

            if self.pc == end_pc {
                result.push(self.next_instr);
                return;
            }
            self.pc = self.pc.wrapping_add(4 >> self.thumb as u8);
            result.push(replace(
                &mut self.next_instr,
                Instr {
                    addr: self.pc.wrapping_sub(8 >> self.thumb as usize),
                    raw: 0,
                    opcode: String::new(),
                    comment: String::new(),
                },
            ));
        }
    }

    fn disassemble_single<E: Engine, const ARM9: bool>(mut self, emu: &mut Emu<E>) -> Instr {
        let raw_instr = if ARM9 {
            let word = arm9_read_32::<DebugCpuAccess, _, true>(emu, self.next_instr.addr);
            if self.thumb {
                word >> ((self.next_instr.addr & 3) << 3) & 0xFFFF
            } else {
                word
            }
        } else if self.thumb {
            arm7_read_16::<DebugCpuAccess, _>(emu, self.next_instr.addr) as u32
        } else {
            arm7_read_32::<DebugCpuAccess, _>(emu, self.next_instr.addr)
        };
        self.next_instr.raw = raw_instr;

        if self.thumb {
            thumb::handle_instr::<ARM9>(&mut self, raw_instr as u16);
        } else {
            arm::handle_instr::<ARM9>(&mut self, raw_instr);
        }

        self.next_instr
    }
}

pub fn disassemble_single<E: Engine, const ARM9: bool>(
    emu: &mut Emu<E>,
    addr: u32,
    thumb: bool,
) -> Instr {
    Context::new(addr, thumb).disassemble_single::<_, ARM9>(emu)
}

pub fn disassemble_range<E: Engine, const ARM9: bool>(
    emu: &mut Emu<E>,
    (start, end): (u32, u32),
    thumb: bool,
    result: &mut Vec<Instr>,
) {
    Context::new(start, thumb).disassemble_range::<_, ARM9>(emu, end, result);
}
