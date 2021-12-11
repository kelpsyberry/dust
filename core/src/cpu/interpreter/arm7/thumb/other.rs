use super::super::{super::Engine, handle_swi, handle_undefined};
use crate::emu::Emu;

pub fn swi(emu: &mut Emu<Engine>, _instr: u16) {
    handle_swi::<true>(
        emu,
        #[cfg(feature = "debug-hooks")]
        {
            _instr as u8
        },
    );
}

pub fn undefined(emu: &mut Emu<Engine>, _instr: u16) {
    // TODO: Check timing, the ARM7TDMI manual is unclear
    handle_undefined::<true>(emu);
}
