use super::super::{super::Interpreter, handle_prefetch_abort, handle_swi, handle_undefined};
use crate::emu::Emu;

pub fn bkpt(emu: &mut Emu<Interpreter>, _instr: u16) {
    handle_prefetch_abort::<true>(emu);
}

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
    handle_undefined::<true>(emu);
}
