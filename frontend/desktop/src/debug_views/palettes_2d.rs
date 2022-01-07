use super::{common::rgb_5_to_rgba_f32, FrameDataSlot, InstanceableView, View};
use crate::ui::window::Window;
use dust_core::{
    cpu,
    emu::Emu,
    utils::{zeroed_box, ByteSlice, Bytes},
};
use imgui::{ColorButton, StyleVar, TableFlags, Ui};

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
    selection: Option<Selection>,
    data: Box<Bytes<0x8000>>,
}

impl Default for PaletteData {
    fn default() -> Self {
        PaletteData {
            selection: None,
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

    fn new(_window: &mut Window) -> Self {
        Palettes2D {
            cur_selection: Selection {
                engine: Engine2d::A,
                palette: Palette::Bg,
            },
            data: PaletteData::default(),
        }
    }

    fn destroy(self, _window: &mut Window) {}

    fn emu_state(&self) -> Self::EmuState {
        self.cur_selection
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
        let palette_data = frame_data.get_or_insert_with(Default::default);
        palette_data.selection = Some(*emu_state);
        unsafe {
            match emu_state.palette {
                Palette::Bg => {
                    let base = ((emu_state.engine == Engine2d::B) as usize) << 10;
                    palette_data.data[..0x200]
                        .copy_from_slice(&emu.gpu.vram.palette.as_byte_slice()[base..base + 0x200]);
                }

                Palette::Obj => {
                    let base = ((emu_state.engine == Engine2d::B) as usize) << 10 | 0x200;
                    palette_data.data[..0x200]
                        .copy_from_slice(&emu.gpu.vram.palette.as_byte_slice()[base..base + 0x200]);
                }

                Palette::ExtBg => match emu_state.engine {
                    Engine2d::A => emu.gpu.vram.read_a_bg_ext_pal_slice(
                        0,
                        0x8000,
                        palette_data.data.as_mut_ptr() as *mut usize,
                    ),
                    Engine2d::B => emu.gpu.vram.read_b_bg_ext_pal_slice(
                        0,
                        0x8000,
                        palette_data.data.as_mut_ptr() as *mut usize,
                    ),
                },

                Palette::ExtObj => match emu_state.engine {
                    Engine2d::A => emu.gpu.vram.read_a_obj_ext_pal_slice(
                        0,
                        0x2000,
                        palette_data.data.as_mut_ptr() as *mut usize,
                    ),
                    Engine2d::B => emu.gpu.vram.read_b_obj_ext_pal_slice(
                        0,
                        0x2000,
                        palette_data.data.as_mut_ptr() as *mut usize,
                    ),
                },
            }
        }
    }

    fn clear_frame_data(&mut self) {
        self.data.selection = None;
    }

    fn update_from_frame_data(&mut self, frame_data: &Self::FrameData, _window: &mut Window) {
        self.data.selection = frame_data.selection;
        let data_len = frame_data.selection.unwrap().data_len();
        self.data.data[..data_len].copy_from_slice(&frame_data.data[..data_len]);
    }

    fn customize_window<'ui, 'a, T: AsRef<str>>(
        &mut self,
        _ui: &imgui::Ui,
        window: imgui::Window<'ui, 'a, T>,
    ) -> imgui::Window<'ui, 'a, T> {
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
        let selection_updated = ui.combo("##palette", &mut i, &POSSIBLE_SELECTIONS, |selection| {
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

        if self.data.selection != Some(self.cur_selection) {
            return new_state;
        }

        let _frame_rounding = ui.push_style_var(StyleVar::FrameRounding(1.0));
        let _cell_padding = ui.push_style_var(StyleVar::CellPadding([1.0; 2]));

        if let Some(_token) = ui.begin_table_with_flags(
            "palette columns",
            16,
            TableFlags::NO_CLIP | TableFlags::SIZING_FIXED_FIT,
        ) {
            fn color_table(ui: &Ui, colors: ByteSlice) {
                for i in 0..colors.len() >> 1 {
                    ui.table_next_column();
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
        }

        new_state
    }
}

impl InstanceableView for Palettes2D {
    fn finish_preparing_frame_data<E: cpu::Engine>(_emu: &mut Emu<E>) {}
}
