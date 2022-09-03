mod regs;
pub use regs::Regs;
mod alu_utils;
mod common;

use super::Engine;
use crate::utils::Savestate;

macro_rules! reg {
    ($cpu: expr, $reg: expr) => {
        $cpu.engine_data.regs.cur[$reg as usize]
    };
}

macro_rules! inc_r15 {
    ($cpu: expr, $incr: literal) => {{
        #[cfg(feature = "interp-pipeline-accurate-reloads")]
        {
            reg!($cpu, 15) = reg!($cpu, 15).wrapping_add($cpu.engine_data.r15_increment);
        }
        #[cfg(not(feature = "interp-pipeline-accurate-reloads"))]
        {
            reg!($cpu, 15) = reg!($cpu, 15).wrapping_add($incr);
        }
    }};
}

macro_rules! spsr {
    ($cpu: expr) => {
        if $cpu.engine_data.regs.has_spsr() {
            $cpu.engine_data.regs.spsr
        } else {
            #[cfg(feature = "log")]
            slog::warn!(
                $cpu.logger,
                "Unpredictable SPSR read in non-exception mode, reading CPSR"
            );
            $cpu.engine_data.regs.cpsr
        }
    };
}

macro_rules! update_spsr {
    ($cpu: expr, $mask: expr, $value: expr) => {
        if $cpu.engine_data.regs.has_spsr() {
            $cpu.engine_data.regs.spsr = crate::cpu::psr::Psr::from_raw(
                ($cpu.engine_data.regs.spsr.raw() & !$mask) | ($value & $mask),
            );
        } else {
            #[cfg(feature = "log")]
            slog::warn!(
                $cpu.logger,
                "Unpredictable SPSR write in non-exception mode, ignoring"
            );
        }
    };
}

mod arm7;
mod arm9;

#[derive(Savestate)]
pub struct Interpreter;

impl Engine for Interpreter {
    type GlobalData = ();
    type Arm7Data = arm7::EngineData;
    type Arm9Data = arm9::EngineData;

    fn into_data(self) -> (Self::GlobalData, Self::Arm7Data, Self::Arm9Data) {
        ((), arm7::EngineData::new(), arm9::EngineData::new())
    }
}
