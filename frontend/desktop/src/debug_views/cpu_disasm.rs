use super::{
    common::{
        disasm::{Addr, DisassemblyView},
        RangeInclusive,
    },
    BaseView, FrameDataSlot, FrameView, FrameViewMessages, InstanceableFrameViewEmuState,
    InstanceableView,
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

pub struct DisassemblyResults {
    visible_addrs: RangeInclusive<Addr>,
    cpu_pc: u32,
    cpu_thumb: bool,
    thumb: bool,
    instrs: Vec<Instr>,
}

pub struct EmuState<const ARM9: bool> {
    visible_addrs: RangeInclusive<Addr>,
    thumb: bool,
}

impl<const ARM9: bool> super::FrameViewEmuState for EmuState<ARM9> {
    type InitData = RangeInclusive<Addr>;
    type Message = (RangeInclusive<Addr>, bool);
    type FrameData = DisassemblyResults;

    fn new<E: cpu::Engine>(
        visible_addrs: Self::InitData,
        _visible: bool,
        _emu: &mut Emu<E>,
    ) -> Self {
        EmuState {
            visible_addrs,
            thumb: false,
        }
    }

    fn handle_message<E: cpu::Engine>(
        &mut self,
        (visible_addrs, thumb): Self::Message,
        _emu: &mut Emu<E>,
    ) {
        self.visible_addrs = visible_addrs;
        self.thumb = thumb;
    }

    fn prepare_frame_data<'a, E: cpu::Engine, S: FrameDataSlot<'a, Self::FrameData>>(
        &mut self,
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
        let (r15, cpsr) = if ARM9 {
            (emu.arm9.r15(), emu.arm9.cpsr())
        } else {
            (emu.arm7.r15(), emu.arm7.cpsr())
        };
        frame_data.visible_addrs = self.visible_addrs;
        frame_data.cpu_pc = r15;
        frame_data.cpu_thumb = cpsr.thumb_state();
        frame_data.thumb = self.thumb;
        frame_data.instrs.clear();
        disassemble_range::<_, ARM9>(
            emu,
            (
                self.visible_addrs.start as u32,
                self.visible_addrs.end as u32,
            ),
            self.thumb,
            &mut frame_data.instrs,
        );
    }
}

impl<const ARM9: bool> InstanceableFrameViewEmuState for EmuState<ARM9> {}

pub struct CpuDisasm<const ARM9: bool> {
    view: DisassemblyView,
    thumb: bool,
    last_visible_addrs: RangeInclusive<Addr>,
    last_bytes_per_line: u8,
    disasm_results: DisassemblyResults,
}

impl<const ARM9: bool> BaseView for CpuDisasm<ARM9> {
    const MENU_NAME: &'static str = if ARM9 {
        "ARM9 disassembly"
    } else {
        "ARM7 disassembly"
    };
}

impl<const ARM9: bool> FrameView for CpuDisasm<ARM9> {
    type EmuState = EmuState<ARM9>;

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

    fn emu_state(&self) -> <Self::EmuState as super::FrameViewEmuState>::InitData {
        self.last_visible_addrs
    }

    fn update_from_frame_data(
        &mut self,
        frame_data: &<Self::EmuState as super::FrameViewEmuState>::FrameData,
        _window: &mut Window,
    ) {
        self.disasm_results.visible_addrs = frame_data.visible_addrs;
        self.disasm_results.cpu_pc = frame_data.cpu_pc;
        self.disasm_results.cpu_thumb = frame_data.cpu_thumb;
        self.disasm_results.thumb = frame_data.thumb;
        self.disasm_results.instrs.clear();
        self.disasm_results
            .instrs
            .extend_from_slice(&frame_data.instrs);
    }

    fn draw(
        &mut self,
        ui: &imgui::Ui,
        window: &mut Window,
        mut messages: impl FrameViewMessages<Self>,
    ) {
        let mut emu_state_changed = false;

        let _mono_font = ui.push_font(window.imgui.mono_font);
        ui.align_text_to_frame_padding();

        if ui.checkbox("Thumb", &mut self.thumb) {
            emu_state_changed = true;
        }

        ui.same_line();

        if ui.button("Disassemble at PC") {
            self.view.set_selected_addr(
                self.disasm_results
                    .cpu_pc
                    .wrapping_sub(8 >> self.disasm_results.cpu_thumb as u8) as Addr,
            );
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
                        if self.disasm_results.thumb {
                            format!("{:04X} ", instr.raw)
                        } else {
                            format!("{:08X} ", instr.raw)
                        },
                    );

                    ui.same_line_with_spacing(0.0, 0.0);
                    ui.text(&instr.opcode);

                    if !instr.comment.is_empty() {
                        ui.same_line_with_spacing(0.0, 0.0);
                        ui.text_colored(disabled_color, format!(" ; {}", instr.comment));
                    }
                }
            }
        });

        let visible_addrs = self.view.visible_addrs(1);
        if emu_state_changed || visible_addrs != self.last_visible_addrs {
            self.last_visible_addrs = visible_addrs;
            messages.push((visible_addrs, self.thumb));
        }
    }
}

impl<const ARM9: bool> InstanceableView for CpuDisasm<ARM9> {}
