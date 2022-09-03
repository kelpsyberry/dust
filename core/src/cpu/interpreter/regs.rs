use super::super::Regs as EngineRegs;
use crate::{
    cpu::psr::{Bank, Mode, Psr},
    utils::Savestate,
};

#[repr(C)]
#[derive(Clone, Debug, Savestate)]
pub struct Regs {
    pub cur: [u32; 16],
    pub(super) cpsr: Psr,
    is_in_priv_mode: bool,
    has_spsr: bool,
    pub spsr: Psr,
    pub r8_14_fiq: [u32; 7],
    pub r8_12_other: [u32; 5],
    pub r13_14_sys: [u32; 2],
    pub r13_14_irq: [u32; 2],
    pub r13_14_svc: [u32; 2],
    pub r13_14_abt: [u32; 2],
    pub r13_14_und: [u32; 2],
    pub spsr_fiq: Psr,
    pub spsr_irq: Psr,
    pub spsr_svc: Psr,
    pub spsr_abt: Psr,
    pub spsr_und: Psr,
}

impl Regs {
    pub const STARTUP: Self = Regs {
        cur: [0; 16],
        cpsr: Psr::from_raw(0x13),
        is_in_priv_mode: true,
        has_spsr: true,
        spsr: Psr::from_raw(0x10),
        r8_14_fiq: [0; 7],
        r8_12_other: [0; 5],
        r13_14_sys: [0; 2],
        r13_14_irq: [0; 2],
        r13_14_svc: [0; 2],
        r13_14_abt: [0; 2],
        r13_14_und: [0; 2],
        spsr_fiq: Psr::from_raw(0x10),
        spsr_irq: Psr::from_raw(0x10),
        spsr_svc: Psr::from_raw(0x10),
        spsr_abt: Psr::from_raw(0x10),
        spsr_und: Psr::from_raw(0x10),
    };

    pub(super) fn r0_3(&self) -> [u32; 4] {
        [self.cur[0], self.cur[1], self.cur[2], self.cur[3]]
    }

    pub(super) fn set_r0_3(&mut self, values: [u32; 4]) {
        self.cur[..4].copy_from_slice(&values);
    }

    pub(super) fn to_engine_regs(&self) -> EngineRegs {
        EngineRegs {
            gprs: self.cur,
            spsr: self.spsr,
            r8_14_fiq: self.r8_14_fiq,
            r8_12_other: self.r8_12_other,
            r13_14_sys: self.r13_14_sys,
            r13_14_irq: self.r13_14_irq,
            r13_14_svc: self.r13_14_svc,
            r13_14_abt: self.r13_14_abt,
            r13_14_und: self.r13_14_und,
            spsr_fiq: self.spsr_fiq,
            spsr_irq: self.spsr_irq,
            spsr_svc: self.spsr_svc,
            spsr_abt: self.spsr_abt,
            spsr_und: self.spsr_und,
        }
    }

    pub(super) fn set_from_engine_regs(&mut self, regs: &EngineRegs) {
        self.cur = regs.gprs;
        self.spsr = regs.spsr;
        self.r8_14_fiq = regs.r8_14_fiq;
        self.r8_12_other = regs.r8_12_other;
        self.r13_14_sys = regs.r13_14_sys;
        self.r13_14_irq = regs.r13_14_irq;
        self.r13_14_svc = regs.r13_14_svc;
        self.r13_14_abt = regs.r13_14_abt;
        self.r13_14_und = regs.r13_14_und;
        self.spsr_fiq = regs.spsr_fiq;
        self.spsr_irq = regs.spsr_irq;
        self.spsr_svc = regs.spsr_svc;
        self.spsr_abt = regs.spsr_abt;
        self.spsr_und = regs.spsr_und;
    }

    #[inline]
    pub const fn cpsr(&self) -> Psr {
        self.cpsr
    }

    #[inline]
    pub const fn is_in_priv_mode(&self) -> bool {
        self.is_in_priv_mode
    }

    #[inline]
    pub const fn has_spsr(&self) -> bool {
        self.has_spsr
    }

    pub(super) fn update_mode<const REG_BANK_ONLY: bool>(
        &mut self,
        prev_mode: Mode,
        new_mode: Mode,
    ) {
        if new_mode == prev_mode {
            return;
        }

        let prev_reg_bank = prev_mode.reg_bank();
        let new_reg_bank = new_mode.reg_bank();
        if prev_reg_bank != new_reg_bank {
            match prev_reg_bank {
                Bank::System => {
                    self.r13_14_sys[0] = self.cur[13];
                    self.r13_14_sys[1] = self.cur[14];
                }
                Bank::Fiq => {
                    self.r8_14_fiq[0] = self.cur[8];
                    self.r8_14_fiq[1] = self.cur[9];
                    self.r8_14_fiq[2] = self.cur[10];
                    self.r8_14_fiq[3] = self.cur[11];
                    self.r8_14_fiq[4] = self.cur[12];
                    self.r8_14_fiq[5] = self.cur[13];
                    self.r8_14_fiq[6] = self.cur[14];
                    self.cur[8] = self.r8_12_other[0];
                    self.cur[9] = self.r8_12_other[1];
                    self.cur[10] = self.r8_12_other[2];
                    self.cur[11] = self.r8_12_other[3];
                    self.cur[12] = self.r8_12_other[4];
                }
                Bank::Irq => {
                    self.r13_14_irq[0] = self.cur[13];
                    self.r13_14_irq[1] = self.cur[14];
                }
                Bank::Supervisor => {
                    self.r13_14_svc[0] = self.cur[13];
                    self.r13_14_svc[1] = self.cur[14];
                }
                Bank::Abort => {
                    self.r13_14_abt[0] = self.cur[13];
                    self.r13_14_abt[1] = self.cur[14];
                }
                Bank::Undefined => {
                    self.r13_14_und[0] = self.cur[13];
                    self.r13_14_und[1] = self.cur[14];
                }
            }
            match new_reg_bank {
                Bank::System => {
                    self.cur[13] = self.r13_14_sys[0];
                    self.cur[14] = self.r13_14_sys[1];
                }
                Bank::Fiq => {
                    self.r8_12_other[0] = self.cur[8];
                    self.r8_12_other[1] = self.cur[9];
                    self.r8_12_other[2] = self.cur[10];
                    self.r8_12_other[3] = self.cur[11];
                    self.r8_12_other[4] = self.cur[12];
                    self.cur[8] = self.r8_14_fiq[0];
                    self.cur[9] = self.r8_14_fiq[1];
                    self.cur[10] = self.r8_14_fiq[2];
                    self.cur[11] = self.r8_14_fiq[3];
                    self.cur[12] = self.r8_14_fiq[4];
                    self.cur[13] = self.r8_14_fiq[5];
                    self.cur[14] = self.r8_14_fiq[6];
                }
                Bank::Irq => {
                    self.cur[13] = self.r13_14_irq[0];
                    self.cur[14] = self.r13_14_irq[1];
                }
                Bank::Supervisor => {
                    self.cur[13] = self.r13_14_svc[0];
                    self.cur[14] = self.r13_14_svc[1];
                }
                Bank::Abort => {
                    self.cur[13] = self.r13_14_abt[0];
                    self.cur[14] = self.r13_14_abt[1];
                }
                Bank::Undefined => {
                    self.cur[13] = self.r13_14_und[0];
                    self.cur[14] = self.r13_14_und[1];
                }
            }
        }

        if REG_BANK_ONLY {
            return;
        }

        self.is_in_priv_mode = new_mode.is_privileged();
        self.has_spsr = new_mode.has_spsr();

        let prev_spsr_bank = prev_mode.spsr_bank();
        let new_spsr_bank = new_mode.spsr_bank();

        if prev_spsr_bank != new_spsr_bank {
            match prev_spsr_bank {
                Bank::System => {}
                Bank::Fiq => {
                    self.spsr_fiq = self.spsr;
                }
                Bank::Irq => {
                    self.spsr_irq = self.spsr;
                }
                Bank::Supervisor => {
                    self.spsr_svc = self.spsr;
                }
                Bank::Abort => {
                    self.spsr_abt = self.spsr;
                }
                Bank::Undefined => {
                    self.spsr_und = self.spsr;
                }
            }
            match new_spsr_bank {
                Bank::System => {}
                Bank::Fiq => {
                    self.spsr = self.spsr_fiq;
                }
                Bank::Irq => {
                    self.spsr = self.spsr_irq;
                }
                Bank::Supervisor => {
                    self.spsr = self.spsr_svc;
                }
                Bank::Abort => {
                    self.spsr = self.spsr_abt;
                }
                Bank::Undefined => {
                    self.spsr = self.spsr_und;
                }
            }
        }
    }
}
