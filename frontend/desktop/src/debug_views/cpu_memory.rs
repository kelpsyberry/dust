use super::{
    BaseView, FrameDataSlot, FrameView, FrameViewMessages, InstanceableFrameViewEmuState,
    InstanceableView,
};
use crate::ui::window::Window;
use dust_core::{
    cpu::{self, arm7, arm9, bus},
    emu::Emu,
};
use imgui_memory_editor::{MemoryEditor, RangeInclusive};

pub struct MemContents {
    visible_addrs: RangeInclusive<u32>,
    data: Vec<u32>,
}

pub enum Message {
    Write { addr: u32, value: u8 },
    UpdateVisibleAddrs(RangeInclusive<u32>),
}

pub struct EmuState<const ARM9: bool> {
    visible_addrs: RangeInclusive<u32>,
}

impl<const ARM9: bool> super::FrameViewEmuState for EmuState<ARM9> {
    type InitData = RangeInclusive<u32>;
    type Message = Message;
    type FrameData = MemContents;

    fn new<E: cpu::Engine>(
        visible_addrs: Self::InitData,
        _visible: bool,
        _emu: &mut Emu<E>,
    ) -> Self {
        EmuState { visible_addrs }
    }

    fn handle_message<E: cpu::Engine>(&mut self, message: Self::Message, emu: &mut Emu<E>) {
        match message {
            Message::Write { addr, value } => {
                if ARM9 {
                    arm9::bus::write_8::<bus::DebugCpuAccess, E>(emu, addr, value);
                } else {
                    arm7::bus::write_8::<bus::DebugCpuAccess, E>(emu, addr, value);
                }
            }
            Message::UpdateVisibleAddrs(addrs) => self.visible_addrs = addrs,
        }
    }

    fn prepare_frame_data<'a, E: cpu::Engine, S: FrameDataSlot<'a, Self::FrameData>>(
        &mut self,
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
            .reserve(((self.visible_addrs.end - self.visible_addrs.start) >> 2) as usize);
        for addr in (self.visible_addrs.start..=self.visible_addrs.end).step_by(4) {
            frame_data.data.push(if ARM9 {
                arm9::bus::read_32::<bus::DebugCpuAccess, E, false>(emu, addr)
            } else {
                arm7::bus::read_32::<bus::DebugCpuAccess, E>(emu, addr)
            });
        }
        frame_data.visible_addrs = self.visible_addrs;
    }
}

impl<const ARM9: bool> InstanceableFrameViewEmuState for EmuState<ARM9> {}

pub struct CpuMemory<const ARM9: bool> {
    editor: MemoryEditor,
    last_visible_addrs: RangeInclusive<u32>,
    mem_contents: MemContents,
}

impl<const ARM9: bool> InstanceableView for CpuMemory<ARM9> {
    fn window<'ui>(
        &mut self,
        key: u32,
        ui: &'ui imgui::Ui,
    ) -> imgui::Window<'ui, 'ui, impl AsRef<str> + 'static> {
        let width = self.editor.window_auto_width(ui);
        ui.window(format!("{} {key}", Self::MENU_NAME))
            .size_constraints([width, 0.0], [width, f32::INFINITY])
    }
}

impl<const ARM9: bool> BaseView for CpuMemory<ARM9> {
    const MENU_NAME: &'static str = if ARM9 { "ARM9 memory" } else { "ARM7 memory" };
}

impl<const ARM9: bool> FrameView for CpuMemory<ARM9> {
    type EmuState = EmuState<ARM9>;

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

    fn emu_state(&self) -> <Self::EmuState as super::FrameViewEmuState>::InitData {
        self.last_visible_addrs
    }

    fn update_from_frame_data(
        &mut self,
        frame_data: &<Self::EmuState as super::FrameViewEmuState>::FrameData,
        _window: &mut Window,
    ) {
        self.mem_contents.data.clear();
        self.mem_contents.data.extend_from_slice(&frame_data.data);
        self.mem_contents.visible_addrs = frame_data.visible_addrs;
    }

    fn draw(
        &mut self,
        ui: &imgui::Ui,
        window: &mut Window,
        mut messages: impl FrameViewMessages<Self>,
    ) {
        let _mono_font = ui.push_font(window.imgui.mono_font);

        self.editor.handle_options_right_click(ui);
        self.editor.draw_callbacks(
            ui,
            imgui_memory_editor::DisplayMode::Child {
                height: ui.content_region_avail()[1],
            },
            &mut (),
            |_, addr| {
                if self.mem_contents.visible_addrs.contains(&(addr as u32)) {
                    let offset = (addr as u32 - self.mem_contents.visible_addrs.start) as usize;
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
                messages.push(Message::Write {
                    addr: addr as u32,
                    value,
                });
            },
        );

        let visible_addrs = self.editor.visible_addrs(1, ui);
        let visible_addrs = (
            visible_addrs.start as u32 & !3,
            (((visible_addrs.end + 3) & !3) - 1) as u32,
        )
            .into();
        if visible_addrs != self.last_visible_addrs {
            self.last_visible_addrs = visible_addrs;
            messages.push(Message::UpdateVisibleAddrs(visible_addrs));
        }
    }
}
