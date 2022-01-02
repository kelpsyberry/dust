use super::{common::regs::regs_32, FrameDataSlot, View};
use crate::ui::window::Window;
use dust_core::{
    cpu::{psr::Cpsr, CoreData, Engine},
    emu::Emu,
};
use imgui::StyleVar;

#[derive(Clone, Debug)]
pub struct RegValues {
    pub gprs: [u32; 16],
    pub cpsr: Cpsr,
}

pub struct Arm7State {
    reg_values: Option<RegValues>,
}

impl View for Arm7State {
    const NAME: &'static str = "ARM7 state";

    type FrameData = RegValues;
    type EmuState = ();

    fn new(_window: &mut Window) -> Self {
        Arm7State { reg_values: None }
    }

    fn destroy(self, _window: &mut Window) {}

    fn emu_state(&self) -> Self::EmuState {}

    fn prepare_frame_data<'a, E: Engine, S: FrameDataSlot<'a, Self::FrameData>>(
        _emu_state: &Self::EmuState,
        emu: &mut Emu<E>,
        frame_data: S,
    ) {
        let (gprs, cpsr) = emu.arm7.engine_data.regs();
        frame_data.insert(RegValues { gprs, cpsr });
    }

    fn update_from_frame_data(&mut self, frame_data: &Self::FrameData, _window: &mut Window) {
        self.reg_values = Some(frame_data.clone());
    }

    fn customize_window<'ui, 'a, T: AsRef<str>>(
        &mut self,
        _ui: &imgui::Ui,
        window: imgui::Window<'ui, 'a, T>,
    ) -> imgui::Window<'ui, 'a, T> {
        window.always_auto_resize(true)
    }

    fn render(
        &mut self,
        ui: &imgui::Ui,
        window: &mut Window,
        _emu_running: bool,
    ) -> Option<Self::EmuState> {
        if let Some(reg_values) = self.reg_values.as_mut() {
            let _mono_font_token = ui.push_font(window.mono_font);
            let _frame_rounding = ui.push_style_var(StyleVar::FrameRounding(0.0));
            let _item_spacing = ui.push_style_var(StyleVar::ItemSpacing([
                0.0,
                ui.clone_style().item_spacing[1],
            ]));

            regs_32(ui, &reg_values.gprs);
        }
        None
    }
}
