use super::{
    common::{
        disasm::{Addr, DisassemblyView},
        RangeInclusive,
    },
    FrameDataSlot, View,
};
use crate::ui::window::Window;
use dust_core::{
    cpu::{
        self,
        disasm::{disassemble_range, Instr},
    },
    emu::Emu,
};
use imgui::StyleColor;

pub struct CpuDisasm<const ARM9: bool> {
    view: DisassemblyView,
    thumb: bool,
    last_visible_addrs: RangeInclusive<Addr>,
    last_bytes_per_line: u8,
    disasm_results: DisassemblyResults,
}

#[derive(Clone)]
pub struct EmuState {
    visible_addrs: RangeInclusive<Addr>,
    thumb: bool,
}

#[derive(Clone)]
pub struct DisassemblyResults {
    visible_addrs: RangeInclusive<Addr>,
    cpu_pc: u32,
    cpu_thumb: bool,
    thumb: bool,
    instrs: Vec<Instr>,
}

impl<const ARM9: bool> View for CpuDisasm<ARM9> {
    const NAME: &'static str = if ARM9 {
        "ARM9 disassembly"
    } else {
        "ARM7 disassembly"
    };

    type FrameData = DisassemblyResults;
    type EmuState = EmuState;

    fn new(_window: &mut Window) -> Self {
        CpuDisasm {
            view: DisassemblyView::new()
                .bytes_per_line(4)
                .show_range(false)
                .addr_range((0, 0xFFFF_FFFF).into()),
            last_visible_addrs: (0, 0).into(),
            last_bytes_per_line: 4,
            thumb: false,
            disasm_results: DisassemblyResults {
                visible_addrs: (0, 0).into(),
                cpu_pc: 0,
                cpu_thumb: false,
                thumb: false,
                instrs: Vec::new(),
            },
        }
    }

    fn destroy(self, _window: &mut Window) {}

    fn emu_state(&self) -> Self::EmuState {
        EmuState {
            visible_addrs: self.last_visible_addrs,
            thumb: false,
        }
    }

    fn prepare_frame_data<'a, E: cpu::Engine, S: FrameDataSlot<'a, Self::FrameData>>(
        emu_state: &Self::EmuState,
        emu: &mut Emu<E>,
        frame_data: S,
    ) {
        let frame_data = frame_data.get_or_insert_with(|| DisassemblyResults {
            visible_addrs: (0, 0).into(),
            cpu_pc: 0,
            cpu_thumb: false,
            thumb: false,
            instrs: Vec::new(),
        });
        let (regs, cpsr) = if ARM9 {
            emu.arm9.regs()
        } else {
            emu.arm7.regs()
        };
        frame_data.visible_addrs = emu_state.visible_addrs;
        frame_data.cpu_pc = regs[15];
        frame_data.cpu_thumb = cpsr.thumb_state();
        frame_data.thumb = emu_state.thumb;
        frame_data.instrs.clear();
        disassemble_range::<_, ARM9>(
            emu,
            (
                emu_state.visible_addrs.start as u32,
                emu_state.visible_addrs.end as u32,
            ),
            emu_state.thumb,
            &mut frame_data.instrs,
        );
    }

    fn update_from_frame_data(&mut self, frame_data: &Self::FrameData, _window: &mut Window) {
        self.disasm_results.visible_addrs = frame_data.visible_addrs;
        self.disasm_results.cpu_pc = frame_data.cpu_pc;
        self.disasm_results.cpu_thumb = frame_data.cpu_thumb;
        self.disasm_results.thumb = frame_data.thumb;
        self.disasm_results.instrs.clear();
        self.disasm_results
            .instrs
            .extend_from_slice(&frame_data.instrs);
    }

    fn customize_window<'a, T: AsRef<str>>(
        &mut self,
        _ui: &imgui::Ui,
        window: imgui::Window<'a, T>,
    ) -> imgui::Window<'a, T> {
        window
    }

    fn render(
        &mut self,
        ui: &imgui::Ui,
        window: &mut Window,
        _emu_running: bool,
    ) -> Option<Self::EmuState> {
        let mut emu_state_changed = false;

        let _mono_font = ui.push_font(window.mono_font);
        ui.align_text_to_frame_padding();

        if ui.checkbox("Thumb", &mut self.thumb) {
            emu_state_changed = true;
        }

        ui.same_line();

        if ui.button("Disassemble at PC") {
            self.view
                .set_selected_addr(self.disasm_results.cpu_pc as Addr);
            self.thumb = self.disasm_results.cpu_thumb;
            emu_state_changed = true;
        }

        ui.separator();

        self.view.handle_options_right_click(ui);

        let instr_size_shift = 2 - self.disasm_results.thumb as u8;
        let disabled_color = ui.style_color(StyleColor::TextDisabled);
        let bytes_per_line = 1 << instr_size_shift;
        if bytes_per_line != self.last_bytes_per_line {
            self.last_bytes_per_line = bytes_per_line;
            self.view.set_bytes_per_line(bytes_per_line as Addr);
        }
        self.view.draw_callbacks(ui, None, &mut (), |ui, _, addr| {
            if self.disasm_results.visible_addrs.contains(&addr) {
                let offset = (addr - self.disasm_results.visible_addrs.start) as usize;
                if offset < self.disasm_results.instrs.len() << instr_size_shift {
                    let instr = &self.disasm_results.instrs[offset >> instr_size_shift];

                    ui.text_colored(
                        disabled_color,
                        &if self.disasm_results.thumb {
                            format!("{:04X} ", instr.raw)
                        } else {
                            format!("{:08X} ", instr.raw)
                        },
                    );

                    ui.same_line_with_spacing(0.0, 0.0);
                    ui.text(&instr.opcode);

                    if !instr.comment.is_empty() {
                        ui.same_line_with_spacing(0.0, 0.0);
                        ui.text_colored(disabled_color, &format!(" ; {}", instr.comment));
                    }
                }
            }
        });

        let visible_addrs = self.view.visible_addrs(1);
        if emu_state_changed || visible_addrs != self.last_visible_addrs {
            self.last_visible_addrs = visible_addrs;
            Some(EmuState {
                visible_addrs,
                thumb: self.thumb,
            })
        } else {
            None
        }
    }
}
