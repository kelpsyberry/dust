use super::super::{super::Engine, handle_prefetch_abort, handle_swi, handle_undefined};
use crate::emu::Emu;

pub fn bkpt(emu: &mut Emu<Engine>, _instr: u16) {
    handle_prefetch_abort::<true>(emu);
}

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
    handle_undefined::<true>(emu);
}
