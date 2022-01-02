use super::{common::rgb_5_to_rgba_f32, FrameDataSlot, View};
use crate::ui::window::Window;
use dust_core::{
    cpu::Engine,
    emu::Emu,
    utils::{zeroed_box, ByteMutSlice, ByteSlice, Bytes},
};
use imgui::{sys as imgui_sys, ColorButton, StyleVar, Ui};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Engine2d {
    A,
    B,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Palette {
    Bg,
    Obj,
    ExtBg,
    ExtObj,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    engine: Engine2d,
    palette: Palette,
}

impl Default for Selection {
    fn default() -> Self {
        Selection {
            engine: Engine2d::A,
            palette: Palette::Bg,
        }
    }
}

impl Selection {
    const fn new(engine: Engine2d, palette: Palette) -> Self {
        Selection { engine, palette }
    }

    fn data_len(&self) -> usize {
        match self.palette {
            Palette::Bg | Palette::Obj => 0x200,
            Palette::ExtBg => 0x8000,
            Palette::ExtObj => 0x2000,
        }
    }
}

pub struct PaletteData {
    selection: Selection,
    data: Box<Bytes<0x8000>>,
}

impl Default for PaletteData {
    fn default() -> Self {
        PaletteData {
            selection: Selection::default(),
            data: zeroed_box(),
        }
    }
}

pub struct Palettes2D {
    cur_selection: Selection,
    data: PaletteData,
}

impl View for Palettes2D {
    const NAME: &'static str = "2D engine palettes";

    type FrameData = PaletteData;
    type EmuState = Selection;

    #[inline]
    fn new(_window: &mut Window) -> Self {
        Palettes2D {
            cur_selection: Selection::default(),
            data: PaletteData::default(),
        }
    }

    #[inline]
    fn destroy(self, _window: &mut Window) {}

    #[inline]
    fn emu_state(&self) -> Self::EmuState {
        self.cur_selection
    }

    #[inline]
    fn prepare_frame_data<'a, E: Engine, S: FrameDataSlot<'a, Self::FrameData>>(
        emu_state: &Self::EmuState,
        emu: &mut Emu<E>,
        frame_data: S,
    ) {
        let palette_data = frame_data.get_or_insert_with(Default::default);
        palette_data.selection = *emu_state;
        match emu_state.palette {
            Palette::Bg => {
                let base = ((emu_state.engine == Engine2d::B) as usize) << 10;
                palette_data.data[..0x200].copy_from_slice(unsafe {
                    &emu.gpu.vram.palette.as_byte_slice()[base..base + 0x200]
                });
            }

            Palette::Obj => {
                let base = ((emu_state.engine == Engine2d::B) as usize) << 10 | 0x200;
                palette_data.data[..0x200].copy_from_slice(unsafe {
                    &emu.gpu.vram.palette.as_byte_slice()[base..base + 0x200]
                });
            }

            Palette::ExtBg => match emu_state.engine {
                Engine2d::A => unsafe {
                    emu.gpu.vram.read_a_bg_ext_pal_slice(
                        0,
                        0x8000,
                        palette_data.data.as_mut_ptr() as *mut usize,
                    );
                },
                Engine2d::B => unsafe {
                    emu.gpu.vram.read_b_bg_ext_pal_slice(
                        0,
                        0x8000,
                        palette_data.data.as_mut_ptr() as *mut usize,
                    );
                },
            },

            Palette::ExtObj => match emu_state.engine {
                Engine2d::A => unsafe {
                    emu.gpu.vram.read_a_obj_ext_pal_slice(
                        0,
                        0x2000,
                        palette_data.data.as_mut_ptr() as *mut usize,
                    );
                },
                Engine2d::B => unsafe {
                    emu.gpu.vram.read_b_obj_ext_pal_slice(
                        0,
                        0x2000,
                        palette_data.data.as_mut_ptr() as *mut usize,
                    );
                },
            },
        }
    }

    #[inline]
    fn update_from_frame_data(&mut self, frame_data: &Self::FrameData, _window: &mut Window) {
        self.data.selection = frame_data.selection;
        let data_len = frame_data.selection.data_len();
        self.data.data[..data_len].copy_from_slice(&frame_data.data[..data_len]);
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
        ui: &Ui,
        _window: &mut Window,
        _emu_running: bool,
    ) -> Option<Self::EmuState> {
        static POSSIBLE_SELECTIONS: [Selection; 8] = [
            Selection::new(Engine2d::A, Palette::Bg),
            Selection::new(Engine2d::A, Palette::Obj),
            Selection::new(Engine2d::A, Palette::ExtBg),
            Selection::new(Engine2d::A, Palette::ExtObj),
            Selection::new(Engine2d::B, Palette::Bg),
            Selection::new(Engine2d::B, Palette::Obj),
            Selection::new(Engine2d::B, Palette::ExtBg),
            Selection::new(Engine2d::B, Palette::ExtObj),
        ];
        let mut i = POSSIBLE_SELECTIONS
            .iter()
            .position(|s| *s == self.cur_selection)
            .unwrap();
        let selection_updated = ui.combo("", &mut i, &POSSIBLE_SELECTIONS, |selection| {
            format!(
                "Engine {} {} palette",
                match selection.engine {
                    Engine2d::A => "A",
                    Engine2d::B => "B",
                },
                match selection.palette {
                    Palette::Bg => "BG",
                    Palette::Obj => "OBJ",
                    Palette::ExtBg => "ext BG",
                    Palette::ExtObj => "ext OBJ",
                }
            )
            .into()
        });
        let new_state = if selection_updated {
            self.cur_selection = POSSIBLE_SELECTIONS[i];
            Some(self.cur_selection)
        } else {
            None
        };

        if self.data.selection != self.cur_selection {
            return new_state;
        }

        let _frame_rounding = ui.push_style_var(StyleVar::FrameRounding(1.0));

        unsafe {
            imgui_sys::igPushStyleVar_Vec2(
                imgui_sys::ImGuiStyleVar_CellPadding as imgui_sys::ImGuiStyleVar,
                imgui_sys::ImVec2 { x: 1.0, y: 1.0 },
            );
            imgui_sys::igBeginTable(
                b"palette columns\0" as *const _ as *const imgui_sys::cty::c_char,
                16,
                (imgui_sys::ImGuiTableFlags_NoClip | imgui_sys::ImGuiTableFlags_SizingFixedFit)
                    as imgui_sys::ImGuiTableFlags,
                imgui_sys::ImVec2::default(),
                0.0,
            );
        }

        fn color_table(ui: &Ui, colors: ByteSlice) {
            for i in 0..colors.len() >> 1 {
                unsafe {
                    imgui_sys::igTableNextColumn();
                }
                let color = colors.read_le::<u16>(i << 1);
                ColorButton::new(&format!("Color {:#05X}", i), rgb_5_to_rgba_f32(color))
                    .border(false)
                    .alpha(false)
                    .size([16.0, 16.0])
                    .build(ui);
            }
        }

        match self.cur_selection.palette {
            Palette::ExtBg => color_table(ui, self.data.data.as_byte_slice()),
            Palette::ExtObj => color_table(ui, ByteSlice::new(&self.data.data[..0x2000])),
            _ => color_table(ui, ByteSlice::new(&self.data.data[..0x200])),
        }
        unsafe {
            imgui_sys::igPopStyleVar(1);
            imgui_sys::igEndTable();
        }

        new_state
    }
}
