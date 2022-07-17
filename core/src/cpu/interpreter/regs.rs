use super::super::Regs as EngineRegs;
use crate::{
    cpu::psr::{Cpsr, Mode, Spsr},
    utils::Savestate,
};

#[repr(C)]
#[derive(Clone, Debug, Savestate)]
pub struct Regs {
    pub cur: [u32; 16],
    pub(super) cpsr: Cpsr,
    is_in_priv_mode: bool,
    is_in_exc_mode: bool,
    pub spsr: Spsr,
    pub r8_14_fiq: [u32; 7],
    pub r8_12_other: [u32; 5],
    pub r13_14_irq: [u32; 2],
    pub r13_14_svc: [u32; 2],
    pub r13_14_abt: [u32; 2],
    pub r13_14_und: [u32; 2],
    pub r13_14_user: [u32; 2],
    pub spsr_fiq: Spsr,
    pub spsr_irq: Spsr,
    pub spsr_svc: Spsr,
    pub spsr_abt: Spsr,
    pub spsr_und: Spsr,
}

impl Regs {
    pub const STARTUP: Self = Regs {
        cur: [0; 16],
        cpsr: Cpsr::from_raw::<false>(0x13),
        is_in_priv_mode: true,
        is_in_exc_mode: true,
        spsr: Spsr::from_raw::<false>(0),
        r8_14_fiq: [0; 7],
        r8_12_other: [0; 5],
        r13_14_irq: [0; 2],
        r13_14_svc: [0; 2],
        r13_14_abt: [0; 2],
        r13_14_und: [0; 2],
        r13_14_user: [0; 2],
        spsr_fiq: Spsr::from_raw::<false>(0),
        spsr_irq: Spsr::from_raw::<false>(0),
        spsr_svc: Spsr::from_raw::<false>(0),
        spsr_abt: Spsr::from_raw::<false>(0),
        spsr_und: Spsr::from_raw::<false>(0),
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
            cpsr: self.cpsr,
            spsr: self.spsr,
            r8_14_fiq: self.r8_14_fiq,
            r8_12_other: self.r8_12_other,
            r13_14_irq: self.r13_14_irq,
            r13_14_svc: self.r13_14_svc,
            r13_14_abt: self.r13_14_abt,
            r13_14_und: self.r13_14_und,
            r13_14_user: self.r13_14_user,
            spsr_fiq: self.spsr_fiq,
            spsr_irq: self.spsr_irq,
            spsr_svc: self.spsr_svc,
            spsr_abt: self.spsr_abt,
            spsr_und: self.spsr_und,
        }
    }

    pub(super) fn set_from_engine_regs(&mut self, regs: &EngineRegs) {
        self.cur = regs.gprs;
        self.cpsr = regs.cpsr;
        let mode = self.cpsr.mode();
        self.is_in_priv_mode = mode.is_privileged();
        self.is_in_exc_mode = mode.is_exception();
        self.spsr = regs.spsr;
        self.r8_14_fiq = regs.r8_14_fiq;
        self.r8_12_other = regs.r8_12_other;
        self.r13_14_irq = regs.r13_14_irq;
        self.r13_14_svc = regs.r13_14_svc;
        self.r13_14_abt = regs.r13_14_abt;
        self.r13_14_und = regs.r13_14_und;
        self.r13_14_user = regs.r13_14_user;
        self.spsr_fiq = regs.spsr_fiq;
        self.spsr_irq = regs.spsr_irq;
        self.spsr_svc = regs.spsr_svc;
        self.spsr_abt = regs.spsr_abt;
        self.spsr_und = regs.spsr_und;
    }

    #[inline]
    pub const fn cpsr(&self) -> Cpsr {
        self.cpsr
    }

    #[inline]
    pub const fn is_in_priv_mode(&self) -> bool {
        self.is_in_priv_mode
    }

    #[inline]
    pub const fn is_in_exc_mode(&self) -> bool {
        self.is_in_exc_mode
    }

    pub(super) fn update_mode<const REG_BANK_ONLY: bool>(
        &mut self,
        prev_mode: Mode,
        new_mode: Mode,
    ) {
        if new_mode == prev_mode {
            return;
        }
        if !REG_BANK_ONLY {
            self.is_in_priv_mode = new_mode.is_privileged();
            self.is_in_exc_mode = new_mode.is_exception();
        }
        match prev_mode {
            Mode::Fiq => {
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
                if !REG_BANK_ONLY {
                    self.spsr_fiq = self.spsr;
                }
            }
            Mode::Irq => {
                self.r13_14_irq[0] = self.cur[13];
                self.r13_14_irq[1] = self.cur[14];
                if !REG_BANK_ONLY {
                    self.spsr_irq = self.spsr;
                }
            }
            Mode::Supervisor => {
                self.r13_14_svc[0] = self.cur[13];
                self.r13_14_svc[1] = self.cur[14];
                if !REG_BANK_ONLY {
                    self.spsr_svc = self.spsr;
                }
            }
            Mode::Abort => {
                self.r13_14_abt[0] = self.cur[13];
                self.r13_14_abt[1] = self.cur[14];
                if !REG_BANK_ONLY {
                    self.spsr_abt = self.spsr;
                }
            }
            Mode::Undefined => {
                self.r13_14_und[0] = self.cur[13];
                self.r13_14_und[1] = self.cur[14];
                if !REG_BANK_ONLY {
                    self.spsr_und = self.spsr;
                }
            }
            Mode::User | Mode::System => {
                self.r13_14_user[0] = self.cur[13];
                self.r13_14_user[1] = self.cur[14];
            }
        }
        match new_mode {
            Mode::Fiq => {
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
                if !REG_BANK_ONLY {
                    self.spsr = self.spsr_fiq;
                }
            }
            Mode::Irq => {
                self.cur[13] = self.r13_14_irq[0];
                self.cur[14] = self.r13_14_irq[1];
                if !REG_BANK_ONLY {
                    self.spsr = self.spsr_irq;
                }
            }
            Mode::Supervisor => {
                self.cur[13] = self.r13_14_svc[0];
                self.cur[14] = self.r13_14_svc[1];
                if !REG_BANK_ONLY {
                    self.spsr = self.spsr_svc;
                }
            }
            Mode::Abort => {
                self.cur[13] = self.r13_14_abt[0];
                self.cur[14] = self.r13_14_abt[1];
                if !REG_BANK_ONLY {
                    self.spsr = self.spsr_abt;
                }
            }
            Mode::Undefined => {
                self.cur[13] = self.r13_14_und[0];
                self.cur[14] = self.r13_14_und[1];
                if !REG_BANK_ONLY {
                    self.spsr = self.spsr_und;
                }
            }
            Mode::User | Mode::System => {
                self.cur[13] = self.r13_14_user[0];
                self.cur[14] = self.r13_14_user[1];
            }
        }
    }
}
