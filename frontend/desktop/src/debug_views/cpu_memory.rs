use super::{FrameDataSlot, InstanceableView, Messages, View};
use crate::ui::window::Window;
use dust_core::{
    cpu::{self, arm7, arm9, bus},
    emu::Emu,
};
use imgui_memory_editor::{Addr, MemoryEditor, RangeInclusive};

pub struct CpuMemory<const ARM9: bool> {
    editor: MemoryEditor,
    last_visible_addrs: RangeInclusive<Addr>,
    mem_contents: MemContents,
}

#[derive(Clone)]
pub struct EmuState {
    visible_addrs: RangeInclusive<Addr>,
}

#[derive(Clone)]
pub struct MemContents {
    visible_addrs: RangeInclusive<Addr>,
    data: Vec<u32>,
}

impl<const ARM9: bool> View for CpuMemory<ARM9> {
    const NAME: &'static str = if ARM9 { "ARM9 memory" } else { "ARM7 memory" };

    type FrameData = MemContents;
    type EmuState = EmuState;
    type Message = (u32, u8);

    fn new(_window: &mut Window) -> Self {
        let mut editor = MemoryEditor::new();
        editor.set_show_range(false);
        editor.set_addr_range((0, 0xFFFF_FFFF).into());
        CpuMemory {
            editor,
            last_visible_addrs: (0, 0).into(),
            mem_contents: MemContents {
                visible_addrs: (0, 0).into(),
                data: Vec::new(),
            },
        }
    }

    fn destroy(self, _window: &mut Window) {}

    fn emu_state(&self) -> Self::EmuState {
        EmuState {
            visible_addrs: self.last_visible_addrs,
        }
    }

    fn handle_emu_state_changed<E: cpu::Engine>(
        _prev: Option<&Self::EmuState>,
        _new: Option<&Self::EmuState>,
        _emu: &mut Emu<E>,
    ) {
    }

    fn prepare_frame_data<'a, E: cpu::Engine, S: FrameDataSlot<'a, Self::FrameData>>(
        emu_state: &Self::EmuState,
        emu: &mut Emu<E>,
        frame_data: S,
    ) {
        let frame_data = frame_data.get_or_insert_with(|| MemContents {
            visible_addrs: RangeInclusive { start: 0, end: 0 },
            data: Vec::new(),
        });
        frame_data.data.clear();
        frame_data
            .data
            .reserve(((emu_state.visible_addrs.end - emu_state.visible_addrs.start) >> 2) as usize);
        for addr in (emu_state.visible_addrs.start..=emu_state.visible_addrs.end).step_by(4) {
            frame_data.data.push(if ARM9 {
                arm9::bus::read_32::<bus::DebugCpuAccess, E, false>(emu, addr as u32)
            } else {
                arm7::bus::read_32::<bus::DebugCpuAccess, E>(emu, addr as u32)
            });
        }
        frame_data.visible_addrs = emu_state.visible_addrs;
    }

    fn handle_custom_message<E: cpu::Engine>(
        (addr, value): Self::Message,
        _emu_state: &Self::EmuState,
        emu: &mut Emu<E>,
    ) {
        if ARM9 {
            arm9::bus::write_8::<bus::DebugCpuAccess, E>(emu, addr, value);
        } else {
            arm7::bus::write_8::<bus::DebugCpuAccess, E>(emu, addr, value);
        }
    }

    fn clear_frame_data(&mut self) {
        self.mem_contents.data.clear();
    }

    fn update_from_frame_data(&mut self, frame_data: &Self::FrameData, _window: &mut Window) {
        self.mem_contents.data.clear();
        self.mem_contents.data.extend_from_slice(&frame_data.data);
        self.mem_contents.visible_addrs = frame_data.visible_addrs;
    }

    fn customize_window<'ui, 'a, T: AsRef<str>>(
        &mut self,
        ui: &imgui::Ui,
        window: imgui::Window<'ui, 'a, T>,
    ) -> imgui::Window<'ui, 'a, T> {
        let width = self.editor.window_width(ui);
        window.size_constraints([width, 0.0], [width, f32::INFINITY])
    }

    fn render(
        &mut self,
        ui: &imgui::Ui,
        window: &mut Window,
        _emu_running: bool,
        mut messages: impl Messages<Self>,
    ) -> Option<Self::EmuState> {
        let _mono_font = ui.push_font(window.mono_font);

        self.editor.handle_options_right_click(ui);
        self.editor.draw_callbacks(
            ui,
            None,
            &mut (),
            |_, addr| {
                if self.mem_contents.visible_addrs.contains(&addr) {
                    let offset = (addr - self.mem_contents.visible_addrs.start) as usize;
                    if offset < self.mem_contents.data.len() << 2 {
                        Some((self.mem_contents.data[offset >> 2] >> ((offset & 3) << 3)) as u8)
                    } else {
                        None
                    }
                } else {
                    None
                }
            },
            |_, addr, value| {
                messages.push_custom((addr as u32, value));
            },
        );

        let mut visible_addrs = self.editor.visible_addrs(1);
        visible_addrs.start &= !3;
        visible_addrs.end = (visible_addrs.end + 3) & !3;
        if visible_addrs != self.last_visible_addrs {
            self.last_visible_addrs = visible_addrs;
            Some(EmuState { visible_addrs })
        } else {
            None
        }
    }
}

impl<const ARM9: bool> InstanceableView for CpuMemory<ARM9> {
    fn finish_preparing_frame_data<E: cpu::Engine>(_emu: &mut Emu<E>) {}
}
