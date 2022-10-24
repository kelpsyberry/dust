use super::{common::rgb5_to_rgba8, FrameDataSlot, InstanceableView, Messages, View};
use crate::ui::{
    utils::{combo_value, scale_to_fit},
    window::Window,
};
use dust_core::{
    cpu,
    emu::Emu,
    gpu::{
        engine_2d::{self, BgIndex, Role},
        vram::Vram,
    },
    utils::{ByteMutSlice, Bytes},
};
use imgui::{Image, MouseButton, SliderFlags, StyleColor, TextureId, Ui, WindowHoveredFlags};
use std::slice;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Engine2d {
    A,
    B,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BgDisplayMode {
    Text16,
    Text256,
    Affine,
    ExtendedMap,
    ExtendedBitmap256,
    ExtendedBitmapDirect,
    LargeBitmap,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    engine: Engine2d,
    bg_index: BgIndex,
    use_ext_palettes: Option<bool>,
    display_mode: Option<BgDisplayMode>,
}

#[derive(Clone, Copy)]
struct BgData {
    display_mode: BgDisplayMode,
    uses_ext_palettes: bool,
    size: [u16; 2],
}

pub struct BgMapData {
    bgs: [[BgData; 4]; 2],
    selection: Option<Selection>,
    cur_bg: BgData,
    tiles: Box<Bytes<{ 2 * 128 * 128 }>>,
    tile_bitmap_data: Box<Bytes<{ 1024 * 512 }>>,
    palette: Box<Bytes<0x2000>>,
}

impl BgMapData {
    fn palette_len(&self) -> usize {
        if self.cur_bg.display_mode == BgDisplayMode::ExtendedBitmapDirect {
            0
        } else if self.cur_bg.uses_ext_palettes {
            0x1000
        } else {
            0x100
        }
    }

    fn default_bgs() -> [[BgData; 4]; 2] {
        [[BgData {
            display_mode: BgDisplayMode::Text16,
            uses_ext_palettes: false,
            size: [128; 2],
        }; 4]; 2]
    }
}

impl Default for BgMapData {
    fn default() -> Self {
        unsafe {
            BgMapData {
                bgs: Self::default_bgs(),
                selection: None,
                cur_bg: BgData {
                    display_mode: BgDisplayMode::Text16,
                    uses_ext_palettes: false,
                    size: [128; 2],
                },
                tiles: Box::new_zeroed().assume_init(),
                tile_bitmap_data: Box::new_zeroed().assume_init(),
                palette: Box::new_zeroed().assume_init(),
            }
        }
    }
}

pub struct BgMaps2d {
    cur_selection: Selection,
    tex_id: TextureId,
    show_transparency_checkerboard: bool,
    show_grid_lines: bool,
    palette_buffer: Box<[u32; 0x1000]>,
    pixel_buffer: Box<[u32; 1024 * 1024]>,
    data: BgMapData,
}

impl View for BgMaps2d {
    const NAME: &'static str = "2D BG maps";

    type FrameData = BgMapData;
    type EmuState = Selection;

    fn new(window: &mut Window) -> Self {
        let tex_id = window.imgui.gfx.create_and_add_owned_texture(
            Some("BG map".into()),
            imgui_wgpu::TextureDescriptor {
                width: 1024,
                height: 1024,
                format: wgpu::TextureFormat::Rgba8Unorm,
                ..Default::default()
            },
            imgui_wgpu::SamplerDescriptor {
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            },
        );
        unsafe {
            BgMaps2d {
                cur_selection: Selection {
                    engine: Engine2d::A,
                    bg_index: BgIndex::new(0),
                    use_ext_palettes: None,
                    display_mode: None,
                },
                tex_id,
                show_transparency_checkerboard: true,
                show_grid_lines: true,
                palette_buffer: Box::new_zeroed().assume_init(),
                pixel_buffer: Box::new_zeroed().assume_init(),
                data: BgMapData::default(),
            }
        }
    }

    fn destroy(self, window: &mut Window) {
        window.imgui.gfx.remove_texture(self.tex_id);
    }

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
        fn bg_size(bg: &engine_2d::Bg, display_mode: BgDisplayMode) -> [u16; 2] {
            match display_mode {
                BgDisplayMode::Text16 | BgDisplayMode::Text256 => match bg.control().size_key() {
                    0 => [256, 256],
                    1 => [512, 256],
                    2 => [256, 512],
                    _ => [512, 512],
                },
                BgDisplayMode::Affine | BgDisplayMode::ExtendedMap => {
                    [128 << bg.control().size_key(); 2]
                }
                BgDisplayMode::ExtendedBitmap256 | BgDisplayMode::ExtendedBitmapDirect => {
                    match bg.control().size_key() {
                        0 => [128, 128],
                        1 => [256, 256],
                        2 => [512, 256],
                        _ => [256, 512],
                    }
                }
                BgDisplayMode::LargeBitmap => match bg.control().size_key() {
                    0 => [512, 1024],
                    1 => [1024, 512],
                    2 => [512, 256],
                    _ => [256, 512],
                },
            }
        }

        fn get_bgs_data<R: Role>(engine: &engine_2d::Engine2d<R>) -> [BgData; 4] {
            [0, 1, 2, 3].map(|i| {
                let bg = &engine.bgs[i];

                let text = if bg.control().use_256_colors() {
                    BgDisplayMode::Text256
                } else {
                    BgDisplayMode::Text16
                };

                let extended = if bg.control().use_bitmap_extended_bg() {
                    if bg.control().use_direct_color_extended_bg() {
                        BgDisplayMode::ExtendedBitmapDirect
                    } else {
                        BgDisplayMode::ExtendedBitmap256
                    }
                } else {
                    BgDisplayMode::ExtendedMap
                };

                let display_mode = match i {
                    0 => text,
                    1 => text,
                    2 => match engine.control().bg_mode() {
                        0..=1 | 3 | 7 => text,
                        2 | 4 => BgDisplayMode::Affine,
                        5 => extended,
                        _ => BgDisplayMode::LargeBitmap,
                    },
                    _ => match engine.control().bg_mode() {
                        0 | 6..=7 => text,
                        1..=2 => BgDisplayMode::Affine,
                        _ => extended,
                    },
                };

                BgData {
                    display_mode,
                    uses_ext_palettes: engine.control().bg_ext_pal_enabled()
                        && matches!(
                            display_mode,
                            BgDisplayMode::Text256 | BgDisplayMode::ExtendedMap
                        ),
                    size: bg_size(bg, display_mode),
                }
            })
        }

        fn copy_bg_render_data<R: Role>(
            engine: &engine_2d::Engine2d<R>,
            vram: &Vram,
            selection: &Selection,
            data: &mut BgMapData,
        ) {
            let bg = &engine.bgs[selection.bg_index.get() as usize];
            data.cur_bg = {
                let orig = data.bgs[selection.engine as usize][selection.bg_index.get() as usize];
                let (display_mode, size) = match selection.display_mode {
                    Some(display_mode) => (display_mode, bg_size(bg, display_mode)),
                    None => (orig.display_mode, orig.size),
                };
                let uses_ext_palettes = selection
                    .use_ext_palettes
                    .unwrap_or_else(|| engine.control().bg_ext_pal_enabled())
                    && matches!(
                        display_mode,
                        BgDisplayMode::Text256 | BgDisplayMode::ExtendedMap
                    );
                BgData {
                    display_mode,
                    size,
                    uses_ext_palettes,
                }
            };

            let map_base = if R::IS_A {
                engine.control().a_map_base() | bg.control().map_base()
            } else {
                bg.control().map_base()
            };

            let read_bg_slice = if R::IS_A {
                Vram::read_a_bg_slice::<usize>
            } else {
                Vram::read_b_bg_slice::<usize>
            };

            let read_bg_slice_wrapping = |vram, mut addr, mut result: ByteMutSlice| {
                let mut dst_base = 0;
                while dst_base != result.len() {
                    let len = ((R::BG_VRAM_MASK + 1 - addr) as usize).min(result.len() - dst_base);
                    unsafe {
                        read_bg_slice(
                            vram,
                            addr,
                            len,
                            result.as_mut_ptr().add(dst_base) as *mut usize,
                        );
                    }
                    dst_base += len;
                    addr = 0;
                }
            };

            match data.cur_bg.display_mode {
                BgDisplayMode::Text16 | BgDisplayMode::Text256 => unsafe {
                    if bg.control().size_key() & 1 == 0 {
                        let mut src_base = map_base;
                        let mut dst_base = 0;
                        for _ in 0..1 + (bg.control().size_key() >> 1) {
                            read_bg_slice(
                                vram,
                                src_base,
                                2 * 32 * 32,
                                data.tiles.as_mut_ptr().add(dst_base) as *mut usize,
                            );
                            src_base = (src_base + 0x800) & R::BG_VRAM_MASK;
                            dst_base += 2 * 32 * 32;
                        }
                    } else {
                        let mut src_base = map_base;
                        let mut dst_base = 0;
                        for _ in 0..1 + (bg.control().size_key() >> 1) {
                            for _ in 0..32 {
                                read_bg_slice(
                                    vram,
                                    src_base,
                                    2 * 32,
                                    data.tiles.as_mut_ptr().add(dst_base) as *mut usize,
                                );
                                read_bg_slice(
                                    vram,
                                    (src_base + 0x800) & R::BG_VRAM_MASK,
                                    2 * 32,
                                    data.tiles.as_mut_ptr().add(dst_base + 2 * 32) as *mut usize,
                                );
                                src_base += 2 * 32;
                                dst_base += 2 * 64;
                            }
                            src_base += 2 * 32 * 32;
                        }
                    }
                },

                BgDisplayMode::Affine | BgDisplayMode::ExtendedMap => {
                    let tiles_len = (data.cur_bg.size[0] as usize * data.cur_bg.size[1] as usize)
                        >> if data.cur_bg.display_mode == BgDisplayMode::Affine {
                            6
                        } else {
                            5
                        };
                    read_bg_slice_wrapping(
                        vram,
                        map_base,
                        ByteMutSlice::new(&mut data.tiles[..tiles_len]),
                    );
                }

                BgDisplayMode::ExtendedBitmap256
                | BgDisplayMode::ExtendedBitmapDirect
                | BgDisplayMode::LargeBitmap => {}
            }

            let tile_base = if R::IS_A {
                engine.control().a_tile_base() + bg.control().tile_base()
            } else {
                bg.control().tile_base()
            } & R::BG_VRAM_MASK;
            let data_base = bg.control().map_base() << 3;
            let pixels_len = data.cur_bg.size[0] as usize * data.cur_bg.size[1] as usize;

            let (base_addr, data_len) = match data.cur_bg.display_mode {
                BgDisplayMode::Text16 => (tile_base, 0x400 << 5),
                BgDisplayMode::Text256 | BgDisplayMode::ExtendedMap => (tile_base, 0x400 << 6),
                BgDisplayMode::Affine => (tile_base, 0x100 << 6),
                BgDisplayMode::ExtendedBitmap256 => (data_base, pixels_len),
                BgDisplayMode::ExtendedBitmapDirect => (data_base, pixels_len * 2),
                BgDisplayMode::LargeBitmap => (0, pixels_len),
            };
            read_bg_slice_wrapping(
                vram,
                base_addr,
                ByteMutSlice::new(&mut data.tile_bitmap_data[..data_len]),
            );

            if data.cur_bg.display_mode != BgDisplayMode::ExtendedBitmapDirect {
                unsafe {
                    if data.cur_bg.uses_ext_palettes {
                        let slot = selection.bg_index.get()
                            | if selection.bg_index.get() < 2 {
                                bg.control().bg01_ext_pal_slot() << 1
                            } else {
                                0
                            };
                        if R::IS_A {
                            vram.read_a_bg_ext_pal_slice(
                                (slot as u32) << 13,
                                0x2000,
                                data.palette.as_mut_ptr() as *mut usize,
                            );
                        } else {
                            vram.read_b_bg_ext_pal_slice(
                                (slot as u32) << 13,
                                0x2000,
                                data.palette.as_mut_ptr() as *mut usize,
                            );
                        }
                    } else {
                        let pal_base = (!R::IS_A as usize) << 10;
                        data.palette[..0x200].copy_from_slice(
                            &vram.palette.as_byte_slice()[pal_base..pal_base + 0x200],
                        );
                    }
                }
            }
        }

        let frame_data = frame_data.get_or_insert_with(Default::default);
        frame_data.bgs[0] = get_bgs_data(&emu.gpu.engine_2d_a);
        frame_data.bgs[1] = get_bgs_data(&emu.gpu.engine_2d_b);
        frame_data.selection = Some(*emu_state);
        match emu_state.engine {
            Engine2d::A => {
                copy_bg_render_data(&emu.gpu.engine_2d_a, &emu.gpu.vram, emu_state, frame_data)
            }
            Engine2d::B => {
                copy_bg_render_data(&emu.gpu.engine_2d_b, &emu.gpu.vram, emu_state, frame_data)
            }
        }
    }

    fn clear_frame_data(&mut self) {
        self.data.bgs = BgMapData::default_bgs();
        self.data.selection = None;
    }

    fn update_from_frame_data(&mut self, frame_data: &Self::FrameData, _window: &mut Window) {
        self.data.bgs = frame_data.bgs;
        self.data.selection = frame_data.selection;
        self.data.cur_bg = frame_data.cur_bg;
        let pixels_len = frame_data.cur_bg.size[0] as usize * frame_data.cur_bg.size[1] as usize;
        let (tiles_len, tile_bitmap_data_len) = match frame_data.cur_bg.display_mode {
            BgDisplayMode::Text16 => (pixels_len / 32, 0x400 << 5),
            BgDisplayMode::Text256 | BgDisplayMode::ExtendedMap => (pixels_len / 32, 0x400 << 6),
            BgDisplayMode::Affine => (pixels_len / 64, 0x100 << 6),
            BgDisplayMode::ExtendedBitmap256 | BgDisplayMode::LargeBitmap => (0, pixels_len),
            BgDisplayMode::ExtendedBitmapDirect => (0, pixels_len * 2),
        };
        self.data.tiles[..tiles_len].copy_from_slice(&frame_data.tiles[..tiles_len]);
        self.data.tile_bitmap_data[..tile_bitmap_data_len]
            .copy_from_slice(&frame_data.tile_bitmap_data[..tile_bitmap_data_len]);
        let palette_len = frame_data.palette_len() << 1;
        self.data.palette[..palette_len].copy_from_slice(&frame_data.palette[..palette_len]);
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
        window: &mut Window,
        _emu_running: bool,
        _messages: impl Messages<Self>,
    ) -> Option<Self::EmuState> {
        if ui.is_window_hovered_with_flags(WindowHoveredFlags::ROOT_AND_CHILD_WINDOWS)
            && ui.is_mouse_clicked(MouseButton::Right)
        {
            ui.open_popup("options");
        }

        let mut selection_updated = false;

        let content_width = ui.content_region_avail()[0];
        let two_widgets_total_width = content_width - style!(ui, item_spacing)[0];

        ui.set_next_item_width(two_widgets_total_width * (1.0 / 3.0));
        let mut cur_engine = self.cur_selection.engine as u8;
        selection_updated |= ui
            .slider_config("##engine", 0_u8, 1)
            .display_format(match self.cur_selection.engine {
                Engine2d::A => "Engine A",
                Engine2d::B => "Engine B",
            })
            .flags(SliderFlags::NO_INPUT)
            .build(&mut cur_engine);
        self.cur_selection.engine = match cur_engine {
            0 => Engine2d::A,
            _ => Engine2d::B,
        };

        ui.same_line();
        ui.set_next_item_width(two_widgets_total_width * (2.0 / 3.0));
        let mut cur_bg_index = self.cur_selection.bg_index.get();
        selection_updated |= ui
            .slider_config("##bg_index", 0_u8, 3)
            .display_format("BG%d")
            .flags(SliderFlags::NO_INPUT)
            .build(&mut cur_bg_index);
        self.cur_selection.bg_index = BgIndex::new(cur_bg_index);

        if selection_updated {
            self.cur_selection.use_ext_palettes = None;
            self.cur_selection.display_mode = None;
        }

        let default_bg_data = self.data.bgs[self.cur_selection.engine as usize]
            [self.cur_selection.bg_index.get() as usize];

        ui.align_text_to_frame_padding();
        ui.text("Mode:");
        ui.same_line();
        ui.set_next_item_width(
            ui.content_region_avail()[0]
                - (two_widgets_total_width * 0.5 + style!(ui, item_spacing)[0]),
        );

        static BG_DISPLAY_MODES: [Option<BgDisplayMode>; 8] = [
            None,
            Some(BgDisplayMode::Text16),
            Some(BgDisplayMode::Text256),
            Some(BgDisplayMode::Affine),
            Some(BgDisplayMode::ExtendedMap),
            Some(BgDisplayMode::ExtendedBitmap256),
            Some(BgDisplayMode::ExtendedBitmapDirect),
            Some(BgDisplayMode::LargeBitmap),
        ];

        selection_updated |= combo_value(
            ui,
            "##display_mode",
            &mut self.cur_selection.display_mode,
            &BG_DISPLAY_MODES,
            |display_mode: &Option<BgDisplayMode>| {
                let label_display_mode = |display_mode| match display_mode {
                    BgDisplayMode::Text16 => "Text, 16 colors",
                    BgDisplayMode::Text256 => "Text, 256 colors",
                    BgDisplayMode::Affine => "Affine, 256 colors",
                    BgDisplayMode::ExtendedMap => "Extended, 256 colors",
                    BgDisplayMode::ExtendedBitmap256 => "Extended bitmap, 256 colors",
                    BgDisplayMode::ExtendedBitmapDirect => "Extended bitmap, direct color",
                    BgDisplayMode::LargeBitmap => "Large bitmap, 256 colors",
                };
                match display_mode {
                    None => format!(
                        "Default ({})",
                        label_display_mode(default_bg_data.display_mode)
                    )
                    .into(),
                    Some(display_mode) => label_display_mode(*display_mode).into(),
                }
            },
        );

        ui.same_line();
        ui.text("Use ext palettes:");
        ui.same_line();
        ui.set_next_item_width(ui.content_region_avail()[0]);

        static EXT_PALETTE_SETTINGS: [Option<bool>; 3] = [None, Some(true), Some(false)];

        selection_updated |= combo_value(
            ui,
            "##ext_palettes",
            &mut self.cur_selection.use_ext_palettes,
            &EXT_PALETTE_SETTINGS,
            |use_ext_palettes: &Option<bool>| match use_ext_palettes {
                None => format!(
                    "Default ({})",
                    if default_bg_data.uses_ext_palettes {
                        "Yes"
                    } else {
                        "No"
                    },
                )
                .into(),
                Some(true) => "Yes".into(),
                Some(false) => "No".into(),
            },
        );

        let new_state = if selection_updated {
            Some(self.cur_selection)
        } else {
            None
        };

        if self.data.selection == Some(self.cur_selection) {
            ui.align_text_to_frame_padding();
            ui.text(&format!(
                "Size: {}x{}",
                self.data.cur_bg.size[0], self.data.cur_bg.size[1]
            ));
            ui.same_line();
        }

        if ui.button("Options...") {
            ui.open_popup("options");
        }

        ui.popup("options", || {
            ui.checkbox(
                "Show transparency checkerboard",
                &mut self.show_transparency_checkerboard,
            );

            ui.checkbox("Show grid lines", &mut self.show_grid_lines);
        });

        if self.data.selection != Some(self.cur_selection) {
            return new_state;
        }

        let (mut image_pos, image_size) = scale_to_fit(
            self.data.cur_bg.size[0] as f32 / self.data.cur_bg.size[1] as f32,
            ui.content_region_avail(),
        );
        image_pos[0] += style!(ui, window_padding)[0];
        image_pos[1] += ui.cursor_pos()[1];
        ui.set_cursor_pos(image_pos);
        Image::new(self.tex_id, image_size)
            .uv1(self.data.cur_bg.size.map(|size| size as f32 / 1024.0))
            .build(ui);

        if !matches!(
            self.data.cur_bg.display_mode,
            BgDisplayMode::ExtendedBitmap256
                | BgDisplayMode::ExtendedBitmapDirect
                | BgDisplayMode::LargeBitmap
        ) {
            let window_abs_pos = ui.window_pos();
            let image_abs_pos = [0, 1].map(|i| window_abs_pos[i] + image_pos[i]);
            let tiles = [
                self.data.cur_bg.size[0] as usize >> 3,
                self.data.cur_bg.size[1] as usize >> 3,
            ];
            let tile_size = [
                image_size[0] / tiles[0] as f32,
                image_size[1] / tiles[1] as f32,
            ];
            let border_color = ui.style_color(StyleColor::Border);

            if self.show_grid_lines {
                let draw_list = ui.get_window_draw_list();
                let image_abs_end_pos = [0, 1].map(|i| image_abs_pos[i] + image_size[i]);
                for x in 0..=tiles[0] {
                    let x_pos = image_abs_pos[0] + x as f32 * tile_size[0];
                    draw_list
                        .add_line(
                            [x_pos, image_abs_pos[1]],
                            [x_pos, image_abs_end_pos[1]],
                            border_color,
                        )
                        .build();
                }
                for y in 0..=tiles[1] {
                    let y_pos = image_abs_pos[1] + y as f32 * tile_size[1];
                    draw_list
                        .add_line(
                            [image_abs_pos[0], y_pos],
                            [image_abs_end_pos[0], y_pos],
                            border_color,
                        )
                        .build();
                }
            }

            if ui.is_item_hovered() {
                ui.tooltip(|| {
                    let font_size = ui.current_font_size();
                    let mouse_abs_pos = ui.io().mouse_pos;
                    let tile_pos = [0, 1]
                        .map(|i| ((mouse_abs_pos[i] - image_abs_pos[i]) / tile_size[i]) as usize);
                    let image = Image::new(self.tex_id, [font_size * 4.0, font_size * 4.0])
                        .border_col(border_color)
                        .uv0([0, 1].map(|i| tile_pos[i] as f32 / 128.0))
                        .uv1([0, 1].map(|i| (tile_pos[i] + 1) as f32 / 128.0));
                    let map_entry_index = tile_pos[1] * tiles[0] + tile_pos[0];
                    if self.data.cur_bg.display_mode == BgDisplayMode::Affine {
                        ui.text(&format!("Tile {:#04X}", self.data.tiles[map_entry_index]));
                        image.build(ui);
                    } else {
                        let tile = self.data.tiles.read_le::<u16>(map_entry_index << 1);
                        ui.text(&format!("Tile {:#05X}", tile & 0x3FF));
                        image.build(ui);
                        ui.align_text_to_frame_padding();
                        ui.text("Flip: ");
                        ui.same_line();
                        ui.checkbox("X", &mut (tile & 0x400 != 0));
                        ui.same_line_with_spacing(0.0, style!(ui, item_spacing)[0] + 4.0);
                        ui.checkbox("Y", &mut (tile & 0x800 != 0));
                        if self.data.cur_bg.display_mode == BgDisplayMode::Text16
                            || self.data.cur_bg.uses_ext_palettes
                        {
                            ui.text(&format!("Palette number: {:#03X}", tile >> 12));
                        }
                    }
                    ui.text(&format!("X: {}, Y: {}", tile_pos[0] * 8, tile_pos[1] * 8));
                });
            }
        }

        for (i, color) in self.palette_buffer[..self.data.palette_len()]
            .iter_mut()
            .enumerate()
        {
            let orig_color = self.data.palette.read_le::<u16>(i << 1);
            *color = rgb5_to_rgba8(orig_color);
        }

        let pixels_len = self.data.cur_bg.size[0] as usize * self.data.cur_bg.size[1] as usize;
        let x_shift = self.data.cur_bg.size[0].trailing_zeros();

        let transparency_colors = if self.show_transparency_checkerboard {
            0x0FFF_FFFF_u64 << 32 | 0x03FF_FFFF
        } else {
            0
        };

        unsafe {
            match self.data.cur_bg.display_mode {
                BgDisplayMode::Text16 => {
                    let tile_x_shift = x_shift - 3;
                    let tile_i_x_mask = (1 << tile_x_shift) - 1;
                    for tile_i in 0..pixels_len / 64 {
                        let tile = self
                            .data
                            .tiles
                            .read_le_aligned_unchecked::<u16>(tile_i << 1)
                            as usize;
                        let src_base = (tile & 0x3FF) << 5;
                        let dst_base =
                            (tile_i >> tile_x_shift << 10 | (tile_i & tile_i_x_mask)) << 3;
                        let pal_base = tile >> 8 & 0xF0;
                        let src_x_xor_mask = if tile & 0x400 != 0 { 7 } else { 0 };
                        let src_y_xor_mask = if tile & 0x800 != 0 { 7 } else { 0 };
                        for y in 0..8 {
                            let src_base = src_base | (y ^ src_y_xor_mask) << 2;
                            let dst_base = dst_base | y << 10;
                            for x in 0..8 {
                                let src_x = x ^ src_x_xor_mask;
                                let color_index = *self
                                    .data
                                    .tile_bitmap_data
                                    .get_unchecked(src_base | src_x >> 1)
                                    >> ((src_x & 1) << 2)
                                    & 0xF;
                                *self.pixel_buffer.get_unchecked_mut(dst_base | x) =
                                    if color_index == 0 {
                                        (transparency_colors >> ((x ^ y) << 3 & 32)) as u32
                                    } else {
                                        self.palette_buffer[pal_base | color_index as usize]
                                    };
                            }
                        }
                    }
                }

                BgDisplayMode::Text256 | BgDisplayMode::ExtendedMap => {
                    let tile_x_shift = x_shift - 3;
                    let tile_i_x_mask = (1 << tile_x_shift) - 1;
                    for tile_i in 0..pixels_len / 64 {
                        let tile = self
                            .data
                            .tiles
                            .read_le_aligned_unchecked::<u16>(tile_i << 1)
                            as usize;
                        let src_base = (tile & 0x3FF) << 6;
                        let dst_base =
                            (tile_i >> tile_x_shift << 10 | (tile_i & tile_i_x_mask)) << 3;
                        let pal_base = if self.data.cur_bg.uses_ext_palettes {
                            tile >> 4 & 0xF00
                        } else {
                            0
                        };
                        let src_x_xor_mask = if tile & 0x400 != 0 { 7 } else { 0 };
                        let src_y_xor_mask = if tile & 0x800 != 0 { 7 } else { 0 };
                        for y in 0..8 {
                            let src_base = src_base | (y ^ src_y_xor_mask) << 3;
                            let dst_base = dst_base | y << 10;
                            for x in 0..8 {
                                let color_index = *self
                                    .data
                                    .tile_bitmap_data
                                    .get_unchecked(src_base | (x ^ src_x_xor_mask));
                                *self.pixel_buffer.get_unchecked_mut(dst_base | x) =
                                    if color_index == 0 {
                                        (transparency_colors >> ((x ^ y) << 3 & 32)) as u32
                                    } else {
                                        self.palette_buffer[pal_base | color_index as usize]
                                    };
                            }
                        }
                    }
                }

                BgDisplayMode::Affine => {
                    let tile_x_shift = x_shift - 3;
                    let tile_i_x_mask = (1 << tile_x_shift) - 1;
                    for tile_i in 0..pixels_len / 64 {
                        let src_base = (self.data.tiles[tile_i] as usize) << 6;
                        let dst_base =
                            (tile_i >> tile_x_shift << 10 | (tile_i & tile_i_x_mask)) << 3;
                        for y in 0..8 {
                            let src_base = src_base | y << 3;
                            let dst_base = dst_base | y << 10;
                            for (x, (dst_color, &color_index)) in self
                                .pixel_buffer
                                .get_unchecked_mut(dst_base..dst_base + 8)
                                .iter_mut()
                                .zip(
                                    self.data
                                        .tile_bitmap_data
                                        .get_unchecked(src_base..src_base + 8),
                                )
                                .enumerate()
                            {
                                *dst_color = if color_index == 0 {
                                    (transparency_colors >> ((x ^ y) << 3 & 32)) as u32
                                } else {
                                    self.palette_buffer[color_index as usize]
                                };
                            }
                        }
                    }
                }

                BgDisplayMode::ExtendedBitmap256 | BgDisplayMode::LargeBitmap => {
                    for y in 0..self.data.cur_bg.size[1] as usize {
                        let src_base = y << x_shift;
                        let dst_base = y << 10;
                        for (x, (dst_color, &color_index)) in self
                            .pixel_buffer
                            .get_unchecked_mut(
                                dst_base..dst_base + self.data.cur_bg.size[0] as usize,
                            )
                            .iter_mut()
                            .zip(self.data.tile_bitmap_data.get_unchecked(
                                src_base..src_base + self.data.cur_bg.size[0] as usize,
                            ))
                            .enumerate()
                        {
                            *dst_color = if color_index == 0 {
                                (transparency_colors >> ((x ^ y) << 3 & 32)) as u32
                            } else {
                                self.palette_buffer[color_index as usize]
                            };
                        }
                    }
                }

                BgDisplayMode::ExtendedBitmapDirect => {
                    for y in 0..self.data.cur_bg.size[1] as usize {
                        let src_base = y << (x_shift + 1);
                        let dst_base = y << 10;
                        for x in 0..self.data.cur_bg.size[0] as usize {
                            let color = self
                                .data
                                .tile_bitmap_data
                                .read_le_aligned_unchecked::<u16>(src_base + (x << 1));
                            *self.pixel_buffer.get_unchecked_mut(dst_base + x) =
                                if color & 0x8000 == 0 {
                                    (transparency_colors >> ((x ^ y) << 3 & 32)) as u32
                                } else {
                                    rgb5_to_rgba8(color)
                                };
                        }
                    }
                }
            }
        }

        window
            .imgui
            .gfx
            .texture(self.tex_id)
            .unwrap_owned_ref()
            .set_data(
                window.gfx().device(),
                window.gfx().queue(),
                unsafe {
                    slice::from_raw_parts(self.pixel_buffer.as_ptr() as *const u8, 1024 * 1024 * 4)
                },
                imgui_wgpu::TextureSetRange {
                    width: Some(self.data.cur_bg.size[0] as u32),
                    height: Some(self.data.cur_bg.size[1] as u32),
                    ..Default::default()
                },
            );

        new_state
    }
}

impl InstanceableView for BgMaps2d {
    fn finish_preparing_frame_data<E: cpu::Engine>(_emu: &mut Emu<E>) {}
}
