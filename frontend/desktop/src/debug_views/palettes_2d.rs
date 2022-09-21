use super::{
    common::{rgb32f_to_rgb5, rgb5_to_rgb32f, rgb5_to_rgba32f},
    FrameDataSlot, InstanceableView, Messages, View,
};
use crate::ui::{utils::combo_value, window::Window};
use dust_core::{
    cpu,
    emu::Emu,
    utils::{ByteSlice, Bytes},
};
use imgui::{StyleVar, TableFlags, Ui};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Engine2d {
    A,
    B,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Palette {
    Bg,
    Obj,
    ExtBg,
    ExtObj,
}

#[derive(Clone, Copy, PartialEq, Eq)]
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
            data: unsafe { Box::new_zeroed().assume_init() },
        }
    }
}

pub struct Palettes2d {
    cur_selection: Selection,
    data: PaletteData,
    cur_color_index: u16,
    cur_color: [f32; 3],
}

impl View for Palettes2d {
    const NAME: &'static str = "2D engine palettes";

    type FrameData = PaletteData;
    type EmuState = Selection;
    type Message = (Selection, u16, u16);

    fn new(_window: &mut Window) -> Self {
        Palettes2d {
            cur_selection: Selection {
                engine: Engine2d::A,
                palette: Palette::Bg,
            },
            data: PaletteData::default(),
            cur_color_index: 0,
            cur_color: [0.0; 3],
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

    fn handle_custom_message<E: cpu::Engine>(
        (selection, index, value): Self::Message,
        _emu_state: &Self::EmuState,
        emu: &mut Emu<E>,
    ) {
        match selection.palette {
            Palette::Bg => {
                let base = ((selection.engine == Engine2d::B) as usize) << 10;
                emu.gpu
                    .vram
                    .palette
                    .write_le(base | ((index as usize) << 1), value);
            }

            Palette::Obj => {
                let base = ((selection.engine == Engine2d::B) as usize) << 10 | 0x200;
                emu.gpu
                    .vram
                    .palette
                    .write_le(base | ((index as usize) << 1), value);
            }

            Palette::ExtBg => match selection.engine {
                Engine2d::A => emu.gpu.vram.write_a_bg_ext_pal((index as u32) << 1, value),
                Engine2d::B => emu.gpu.vram.write_b_bg_ext_pal((index as u32) << 1, value),
            },

            Palette::ExtObj => match selection.engine {
                Engine2d::A => emu.gpu.vram.write_a_obj_ext_pal((index as u32) << 1, value),
                Engine2d::B => emu.gpu.vram.write_b_obj_ext_pal((index as u32) << 1, value),
            },
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

    fn draw(
        &mut self,
        ui: &Ui,
        _window: &mut Window,
        _emu_running: bool,
        mut messages: impl Messages<Self>,
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
        let selection_updated = combo_value(
            ui,
            "##palette",
            &mut self.cur_selection,
            &POSSIBLE_SELECTIONS,
            |selection| {
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
            },
        );

        let new_state = if selection_updated {
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
            fn color_table(
                ui: &Ui,
                colors: ByteSlice,
                cur_color_index: &mut u16,
                cur_color: &mut [f32; 3],
            ) {
                for i in 0..colors.len() >> 1 {
                    ui.table_next_column();
                    let raw_color = colors.read_le::<u16>(i << 1);
                    if ui
                        .color_button_config(&format!("Color {i:#05X}"), rgb5_to_rgba32f(raw_color))
                        .border(false)
                        .alpha(false)
                        .size([16.0, 16.0])
                        .build()
                    {
                        ui.open_popup("color_picker");
                        *cur_color_index = i as u16;
                        *cur_color = rgb5_to_rgb32f(raw_color);
                    }
                }
            }

            match self.cur_selection.palette {
                Palette::ExtBg => color_table(
                    ui,
                    self.data.data.as_byte_slice(),
                    &mut self.cur_color_index,
                    &mut self.cur_color,
                ),

                Palette::ExtObj => color_table(
                    ui,
                    ByteSlice::new(&self.data.data[..0x2000]),
                    &mut self.cur_color_index,
                    &mut self.cur_color,
                ),

                _ => color_table(
                    ui,
                    ByteSlice::new(&self.data.data[..0x200]),
                    &mut self.cur_color_index,
                    &mut self.cur_color,
                ),
            }

            ui.popup("color_picker", || {
                let i = self.cur_color_index;
                if ui
                    .color_picker3_config(&format!("Color {i:#05X}"), &mut self.cur_color)
                    .alpha(false)
                    .build()
                {
                    messages.push_custom((self.cur_selection, i, rgb32f_to_rgb5(self.cur_color)))
                }
            });
        }

        new_state
    }
}

impl InstanceableView for Palettes2d {
    fn finish_preparing_frame_data<E: cpu::Engine>(_emu: &mut Emu<E>) {}
}
