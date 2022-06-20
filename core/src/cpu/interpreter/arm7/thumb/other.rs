use super::super::{super::Interpreter, handle_swi, handle_undefined};
use crate::emu::Emu;

pub fn swi(emu: &mut Emu<Interpreter>, _instr: u16) {
    handle_swi::<true>(
        emu,
        #[cfg(feature = "debugger-hooks")]
        {
            _instr as u8
        },
    );
}

pub fn undefined(emu: &mut Emu<Interpreter>, _instr: u16) {
    // TODO: Check timing, the ARM7TDMI manual is unclear
    handle_undefined::<true>(emu);
}
