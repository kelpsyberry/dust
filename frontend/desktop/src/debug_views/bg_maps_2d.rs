use super::{
    common::rgb5_to_rgba8, BaseView, FrameDataSlot, FrameView, FrameViewMessages,
    InstanceableFrameViewEmuState, InstanceableView,
};
use crate::ui::{
    utils::{add2, combo_value, scale_to_fit, sub2, sub2s},
    window::Window,
};
use dust_core::{
    cpu,
    emu::Emu,
    gpu::{
        engine_2d::{self, BgIndex, Role},
        vram::Vram,
    },
    utils::{mem_prelude::*, zeroed_box},
};
use imgui::{Image, MouseButton, SliderFlags, StyleColor, TextureId, WindowHoveredFlags};
use rfd::FileDialog;
use std::{fs::File, io, slice};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Engine2d {
    A,
    B,
}

impl AsRef<str> for Engine2d {
    fn as_ref(&self) -> &str {
        match self {
            Engine2d::A => "Engine A",
            Engine2d::B => "Engine B",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BgMode {
    Text16,
    Text256,
    Affine,
    ExtendedMap,
    ExtendedBitmap256,
    ExtendedBitmapDirect,
    LargeBitmap,
}

impl AsRef<str> for BgMode {
    fn as_ref(&self) -> &str {
        match self {
            BgMode::Text16 => "Text, 16 colors",
            BgMode::Text256 => "Text, 256 colors",
            BgMode::Affine => "Affine, 256 colors",
            BgMode::ExtendedMap => "Ext, 256 colors",
            BgMode::ExtendedBitmap256 => "Ext bitmap, 256 colors",
            BgMode::ExtendedBitmapDirect => "Ext bitmap, direct color",
            BgMode::LargeBitmap => "Large bitmap, 256 colors",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BgResolvedFetchMode {
    Text16,
    Text256 { uses_ext_pal: bool },
    Affine,
    ExtendedMap { uses_ext_pal: bool },
    ExtendedBitmap256,
    ExtendedBitmapDirect,
    LargeBitmap,
}

impl BgResolvedFetchMode {
    fn uses_ext_pal(self) -> Option<bool> {
        match self {
            BgResolvedFetchMode::Text256 { uses_ext_pal }
            | BgResolvedFetchMode::ExtendedMap { uses_ext_pal } => Some(uses_ext_pal),
            _ => None,
        }
    }

    fn palette_size(self) -> usize {
        match self {
            BgResolvedFetchMode::ExtendedBitmapDirect => 0,
            BgResolvedFetchMode::Text256 { uses_ext_pal: true }
            | BgResolvedFetchMode::ExtendedMap { uses_ext_pal: true } => 0x1000,
            _ => 0x100,
        }
    }

    fn tiles_sqrt_len(self) -> usize {
        match self {
            BgResolvedFetchMode::Text16
            | BgResolvedFetchMode::Text256 { .. }
            | BgResolvedFetchMode::ExtendedMap { .. } => 0x20,
            BgResolvedFetchMode::Affine => 0x10,
            _ => 0,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BgFetchMode {
    Text16,
    Text256 { uses_ext_pal: Option<bool> },
    Affine,
    ExtendedMap { uses_ext_pal: Option<bool> },
    ExtendedBitmap256,
    ExtendedBitmapDirect,
    LargeBitmap,
}

impl BgFetchMode {
    fn resolve(self, default: BgResolvedFetchMode) -> BgResolvedFetchMode {
        let default_uses_ext_pal = match default {
            BgResolvedFetchMode::Text256 { uses_ext_pal }
            | BgResolvedFetchMode::ExtendedMap { uses_ext_pal } => uses_ext_pal,
            _ => false,
        };
        match self {
            BgFetchMode::Text16 => BgResolvedFetchMode::Text16,
            BgFetchMode::Text256 { uses_ext_pal } => BgResolvedFetchMode::Text256 {
                uses_ext_pal: uses_ext_pal.unwrap_or(default_uses_ext_pal),
            },
            BgFetchMode::Affine => BgResolvedFetchMode::Affine,
            BgFetchMode::ExtendedMap { uses_ext_pal } => BgResolvedFetchMode::ExtendedMap {
                uses_ext_pal: uses_ext_pal.unwrap_or(default_uses_ext_pal),
            },
            BgFetchMode::ExtendedBitmap256 => BgResolvedFetchMode::ExtendedBitmap256,
            BgFetchMode::ExtendedBitmapDirect => BgResolvedFetchMode::ExtendedBitmapDirect,
            BgFetchMode::LargeBitmap => BgResolvedFetchMode::LargeBitmap,
        }
    }

    fn allows_pal_index(self, default: BgResolvedFetchMode) -> bool {
        match self {
            BgFetchMode::Text16 => true,
            BgFetchMode::Text256 { uses_ext_pal } => uses_ext_pal
                .unwrap_or(default == BgResolvedFetchMode::Text256 { uses_ext_pal: true }),
            BgFetchMode::ExtendedMap { uses_ext_pal } => uses_ext_pal
                .unwrap_or(default == BgResolvedFetchMode::ExtendedMap { uses_ext_pal: true }),
            _ => false,
        }
    }

    fn pal_index_changed(&mut self) {
        match self {
            BgFetchMode::Text256 { uses_ext_pal } | BgFetchMode::ExtendedMap { uses_ext_pal } => {
                *uses_ext_pal = Some(true);
            }
            _ => {}
        }
    }

    fn uses_ext_pal_mut(&mut self) -> Option<&mut Option<bool>> {
        match self {
            BgFetchMode::Text256 { uses_ext_pal } | BgFetchMode::ExtendedMap { uses_ext_pal } => {
                Some(uses_ext_pal)
            }
            _ => None,
        }
    }

    fn has_tiles(self) -> bool {
        matches!(
            self,
            BgFetchMode::Text16
                | BgFetchMode::Text256 { .. }
                | BgFetchMode::Affine
                | BgFetchMode::ExtendedMap { .. }
        )
    }
}

impl From<BgResolvedFetchMode> for BgMode {
    fn from(value: BgResolvedFetchMode) -> Self {
        match value {
            BgResolvedFetchMode::Text16 => BgMode::Text16,
            BgResolvedFetchMode::Text256 { .. } => BgMode::Text256,
            BgResolvedFetchMode::Affine => BgMode::Affine,
            BgResolvedFetchMode::ExtendedMap { .. } => BgMode::ExtendedMap,
            BgResolvedFetchMode::ExtendedBitmap256 => BgMode::ExtendedBitmap256,
            BgResolvedFetchMode::ExtendedBitmapDirect => BgMode::ExtendedBitmapDirect,
            BgResolvedFetchMode::LargeBitmap => BgMode::LargeBitmap,
        }
    }
}

impl From<BgFetchMode> for BgMode {
    fn from(value: BgFetchMode) -> Self {
        match value {
            BgFetchMode::Text16 => BgMode::Text16,
            BgFetchMode::Text256 { .. } => BgMode::Text256,
            BgFetchMode::Affine => BgMode::Affine,
            BgFetchMode::ExtendedMap { .. } => BgMode::ExtendedMap,
            BgFetchMode::ExtendedBitmap256 => BgMode::ExtendedBitmap256,
            BgFetchMode::ExtendedBitmapDirect => BgMode::ExtendedBitmapDirect,
            BgFetchMode::LargeBitmap => BgMode::LargeBitmap,
        }
    }
}

impl From<BgMode> for BgFetchMode {
    fn from(value: BgMode) -> Self {
        match value {
            BgMode::Text16 => BgFetchMode::Text16,
            BgMode::Text256 => BgFetchMode::Text256 { uses_ext_pal: None },
            BgMode::Affine => BgFetchMode::Affine,
            BgMode::ExtendedMap => BgFetchMode::ExtendedMap { uses_ext_pal: None },
            BgMode::ExtendedBitmap256 => BgFetchMode::ExtendedBitmap256,
            BgMode::ExtendedBitmapDirect => BgFetchMode::ExtendedBitmapDirect,
            BgMode::LargeBitmap => BgFetchMode::LargeBitmap,
        }
    }
}

impl From<BgResolvedFetchMode> for BgFetchMode {
    fn from(value: BgResolvedFetchMode) -> Self {
        match value {
            BgResolvedFetchMode::Text16 => BgFetchMode::Text16,
            BgResolvedFetchMode::Text256 { .. } => BgFetchMode::Text256 { uses_ext_pal: None },
            BgResolvedFetchMode::Affine => BgFetchMode::Affine,
            BgResolvedFetchMode::ExtendedMap { .. } => {
                BgFetchMode::ExtendedMap { uses_ext_pal: None }
            }
            BgResolvedFetchMode::ExtendedBitmap256 => BgFetchMode::ExtendedBitmap256,
            BgResolvedFetchMode::ExtendedBitmapDirect => BgFetchMode::ExtendedBitmapDirect,
            BgResolvedFetchMode::LargeBitmap => BgFetchMode::LargeBitmap,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    engine: Engine2d,
    bg_index: BgIndex,
}

impl Selection {
    fn to_default_filename(self, mode: BgResolvedFetchMode, show_tiles: bool) -> String {
        let engine = match self.engine {
            Engine2d::A => "a",
            Engine2d::B => "b",
        };
        let bg_index = self.bg_index.get();
        match mode {
            BgResolvedFetchMode::Text16 => format!(
                "{}_{engine}_{bg_index}_text16",
                if show_tiles { "tiles" } else { "bg" }
            ),
            BgResolvedFetchMode::Text256 { .. } => format!(
                "{}_{engine}_{bg_index}_text256",
                if show_tiles { "tiles" } else { "bg" }
            ),
            BgResolvedFetchMode::Affine => format!(
                "{}_{engine}_{bg_index}_affine",
                if show_tiles { "tiles" } else { "bg" }
            ),
            BgResolvedFetchMode::ExtendedMap { .. } => format!(
                "{}_{engine}_{bg_index}_extmap",
                if show_tiles { "tiles" } else { "bg" }
            ),
            BgResolvedFetchMode::ExtendedBitmap256 => {
                format!("bitmap_{engine}_{bg_index}_extbitmap256")
            }
            BgResolvedFetchMode::ExtendedBitmapDirect => {
                format!("bitmap_{engine}_{bg_index}_extbitmapdirect")
            }
            BgResolvedFetchMode::LargeBitmap => format!("bitmap_{engine}_{bg_index}_largebitmap"),
        }
    }
}

#[derive(Clone, Copy)]
struct BgData {
    mode: BgResolvedFetchMode,
    size: [u16; 2],
}

impl BgData {
    fn map_and_tiles_bitmap_size(&self) -> (usize, usize) {
        let pixels_len = self.size[0] as usize * self.size[1] as usize;
        match self.mode {
            BgResolvedFetchMode::Text16 => (pixels_len / 32, 0x400 << 5),
            BgResolvedFetchMode::Text256 { .. } | BgResolvedFetchMode::ExtendedMap { .. } => {
                (pixels_len / 32, 0x400 << 6)
            }
            BgResolvedFetchMode::Affine => (pixels_len / 64, 0x100 << 6),
            BgResolvedFetchMode::ExtendedBitmap256 | BgResolvedFetchMode::LargeBitmap => {
                (0, pixels_len)
            }
            BgResolvedFetchMode::ExtendedBitmapDirect => (0, pixels_len << 1),
        }
    }
}

#[derive(Clone)]
pub struct BgsData {
    bg_defaults: [[BgData; 4]; 2],

    selection: (Selection, Option<BgFetchMode>),
    bg: BgData,

    map: Box<Bytes<{ 2 * 128 * 128 }>>,
    tiles_bitmap: Box<Bytes<{ 1024 * 512 }>>,
    palette: Box<Bytes<0x2000>>,
}

impl BgsData {
    const DEFAULT_BG_DEFAULT: BgData = BgData {
        mode: BgResolvedFetchMode::Text16,
        size: [128; 2],
    };
}

impl Default for BgsData {
    fn default() -> Self {
        BgsData {
            bg_defaults: [[Self::DEFAULT_BG_DEFAULT; 4]; 2],

            selection: (
                Selection {
                    engine: Engine2d::A,
                    bg_index: BgIndex::new(0),
                },
                None,
            ),
            bg: BgData {
                mode: BgResolvedFetchMode::Text16,
                size: [128; 2],
            },

            map: zeroed_box(),
            tiles_bitmap: zeroed_box(),
            palette: zeroed_box(),
        }
    }
}

pub struct EmuState {
    selection: (Selection, Option<BgFetchMode>),
}

impl super::FrameViewEmuState for EmuState {
    type InitData = (Selection, Option<BgFetchMode>);
    type Message = (Selection, Option<BgFetchMode>);
    type FrameData = BgsData;

    fn new<E: cpu::Engine>(selection: Self::InitData, _visible: bool, _emu: &mut Emu<E>) -> Self {
        EmuState { selection }
    }

    fn handle_message<E: cpu::Engine>(&mut self, selection: Self::Message, _emu: &mut Emu<E>) {
        self.selection = selection;
    }

    fn prepare_frame_data<'a, E: cpu::Engine, S: FrameDataSlot<'a, Self::FrameData>>(
        &mut self,
        emu: &mut Emu<E>,
        frame_data: S,
    ) {
        fn bg_size(bg: &engine_2d::Bg, mode: BgResolvedFetchMode) -> [u16; 2] {
            match mode {
                BgResolvedFetchMode::Text16 | BgResolvedFetchMode::Text256 { .. } => {
                    match bg.control().size_key() {
                        0 => [256, 256],
                        1 => [512, 256],
                        2 => [256, 512],
                        _ => [512, 512],
                    }
                }
                BgResolvedFetchMode::Affine | BgResolvedFetchMode::ExtendedMap { .. } => {
                    [128 << bg.control().size_key(); 2]
                }
                BgResolvedFetchMode::ExtendedBitmap256
                | BgResolvedFetchMode::ExtendedBitmapDirect => match bg.control().size_key() {
                    0 => [128, 128],
                    1 => [256, 256],
                    2 => [512, 256],
                    _ => [256, 512],
                },
                BgResolvedFetchMode::LargeBitmap => match bg.control().size_key() {
                    0 => [512, 1024],
                    1 => [1024, 512],
                    2 => [512, 256],
                    _ => [256, 512],
                },
            }
        }

        fn get_bg_defaults<R: Role>(engine: &engine_2d::Engine2d<R>) -> [BgData; 4] {
            [0, 1, 2, 3].map(|i| {
                let bg = &engine.bgs[i];

                let text = if bg.control().use_256_colors() {
                    BgResolvedFetchMode::Text256 {
                        uses_ext_pal: engine.control().bg_ext_pal_enabled(),
                    }
                } else {
                    BgResolvedFetchMode::Text16
                };

                let extended = if bg.control().use_bitmap_extended_bg() {
                    if bg.control().use_direct_color_extended_bg() {
                        BgResolvedFetchMode::ExtendedBitmapDirect
                    } else {
                        BgResolvedFetchMode::ExtendedBitmap256
                    }
                } else {
                    BgResolvedFetchMode::ExtendedMap {
                        uses_ext_pal: engine.control().bg_ext_pal_enabled(),
                    }
                };

                let mode = match i {
                    0 => text,
                    1 => text,
                    2 => match engine.control().bg_mode() {
                        0..=1 | 3 | 7 => text,
                        2 | 4 => BgResolvedFetchMode::Affine,
                        5 => extended,
                        _ => BgResolvedFetchMode::LargeBitmap,
                    },
                    _ => match engine.control().bg_mode() {
                        0 | 6..=7 => text,
                        1..=2 => BgResolvedFetchMode::Affine,
                        _ => extended,
                    },
                };

                BgData {
                    mode,
                    size: bg_size(bg, mode),
                }
            })
        }

        fn read_bg_slice_wrapping<R: Role>(vram: &Vram, mut addr: u32, result: &mut [u8]) {
            let mut dst_base = 0;
            while dst_base != result.len() {
                let len = ((R::BG_VRAM_MASK + 1 - addr) as usize).min(result.len() - dst_base);
                unsafe {
                    (if R::IS_A {
                        Vram::read_a_bg_slice::<usize>
                    } else {
                        Vram::read_b_bg_slice::<usize>
                    })(
                        vram,
                        addr,
                        len,
                        result.as_mut_ptr().add(dst_base).cast::<usize>(),
                    );
                }
                dst_base += len;
                addr = 0;
            }
        }

        fn copy_bg_render_data<R: Role>(
            engine: &engine_2d::Engine2d<R>,
            vram: &Vram,
            (selection, mode): (Selection, Option<BgFetchMode>),
            data: &mut BgsData,
        ) {
            let all_bg_defaults = get_bg_defaults(engine);
            let bg_defaults = all_bg_defaults[selection.bg_index.get() as usize];
            let bg = &engine.bgs[selection.bg_index.get() as usize];

            data.bg_defaults[selection.engine as usize] = all_bg_defaults;
            data.bg = match mode {
                Some(mode) => {
                    let mode = mode.resolve(bg_defaults.mode);
                    BgData {
                        mode,
                        size: bg_size(bg, mode),
                    }
                }
                None => bg_defaults,
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
            let read_bg_slice_wrapping = read_bg_slice_wrapping::<R>;

            // Fetch tiles
            match data.bg.mode {
                BgResolvedFetchMode::Text16 | BgResolvedFetchMode::Text256 { .. } => unsafe {
                    if bg.control().size_key() & 1 == 0 {
                        let mut src_base = map_base;
                        let mut dst_base = 0;
                        for _ in 0..1 + (bg.control().size_key() >> 1) {
                            read_bg_slice(
                                vram,
                                src_base,
                                0x800,
                                data.map.as_mut_ptr().add(dst_base).cast::<usize>(),
                            );
                            src_base = (src_base + 0x800) & R::BG_VRAM_MASK;
                            dst_base += 0x800;
                        }
                    } else {
                        let mut src_base = map_base;
                        let mut dst_base = 0;
                        for _ in 0..1 + (bg.control().size_key() >> 1) {
                            for _ in 0..32 {
                                read_bg_slice(
                                    vram,
                                    src_base,
                                    0x40,
                                    data.map.as_mut_ptr().add(dst_base).cast::<usize>(),
                                );
                                read_bg_slice(
                                    vram,
                                    (src_base + 0x800) & R::BG_VRAM_MASK,
                                    0x40,
                                    data.map.as_mut_ptr().add(dst_base + 0x40).cast::<usize>(),
                                );
                                src_base += 0x40;
                                dst_base += 0x80;
                            }
                            src_base = (src_base + 0x800) & R::BG_VRAM_MASK;
                        }
                    }
                },

                BgResolvedFetchMode::Affine | BgResolvedFetchMode::ExtendedMap { .. } => {
                    let map_size = (data.bg.size[0] as usize * data.bg.size[1] as usize)
                        >> if data.bg.mode == BgResolvedFetchMode::Affine {
                            6
                        } else {
                            5
                        };
                    read_bg_slice_wrapping(vram, map_base, &mut data.map[..map_size]);
                }

                BgResolvedFetchMode::ExtendedBitmap256
                | BgResolvedFetchMode::ExtendedBitmapDirect
                | BgResolvedFetchMode::LargeBitmap => {}
            }

            let tile_base = if R::IS_A {
                engine.control().a_tile_base() + bg.control().tile_base()
            } else {
                bg.control().tile_base()
            } & R::BG_VRAM_MASK;
            let bitmap_base = bg.control().map_base() << 3;
            let pixels_len = data.bg.size[0] as usize * data.bg.size[1] as usize;

            let (base_addr, tiles_bitmap_size) = match data.bg.mode {
                BgResolvedFetchMode::Text16 => (tile_base, 0x400 << 5),
                BgResolvedFetchMode::Text256 { .. } | BgResolvedFetchMode::ExtendedMap { .. } => {
                    (tile_base, 0x400 << 6)
                }
                BgResolvedFetchMode::Affine => (tile_base, 0x100 << 6),
                BgResolvedFetchMode::ExtendedBitmap256 => (bitmap_base, pixels_len),
                BgResolvedFetchMode::ExtendedBitmapDirect => (bitmap_base, pixels_len * 2),
                BgResolvedFetchMode::LargeBitmap => (0, pixels_len),
            };
            read_bg_slice_wrapping(vram, base_addr, &mut data.tiles_bitmap[..tiles_bitmap_size]);

            unsafe {
                match data.bg.mode {
                    BgResolvedFetchMode::ExtendedBitmapDirect => {}
                    BgResolvedFetchMode::Text256 { uses_ext_pal: true }
                    | BgResolvedFetchMode::ExtendedMap { uses_ext_pal: true } => {
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
                                data.palette.as_mut_ptr().cast::<usize>(),
                            );
                        } else {
                            vram.read_b_bg_ext_pal_slice(
                                (slot as u32) << 13,
                                0x2000,
                                data.palette.as_mut_ptr().cast::<usize>(),
                            );
                        }
                    }
                    _ => {
                        let pal_base = (!R::IS_A as usize) << 10;
                        data.palette[..0x200]
                            .copy_from_slice(&vram.palette.as_arr()[pal_base..pal_base + 0x200]);
                    }
                }
            }
        }

        let frame_data = frame_data.get_or_insert_with(Default::default);
        frame_data.selection = self.selection;
        match self.selection.0.engine {
            Engine2d::A => copy_bg_render_data(
                &emu.gpu.engine_2d_a,
                &emu.gpu.vram,
                self.selection,
                frame_data,
            ),
            Engine2d::B => copy_bg_render_data(
                &emu.gpu.engine_2d_b,
                &emu.gpu.vram,
                self.selection,
                frame_data,
            ),
        }
    }
}

impl InstanceableFrameViewEmuState for EmuState {}

struct DisplayOptions {
    show_tiles: bool,
    pal_index: u8,
}

pub struct BgMaps2d {
    tex_id: TextureId,
    transparency_tex_id: TextureId,
    show_transparency_checkerboard: bool,
    show_grid_lines_tiles: bool,
    show_grid_lines_bitmap: bool,

    palette_buffer: Box<[u32; 0x1000]>,
    pixel_buffer: Box<[u32; 1024 * 1024]>,

    selection: (Selection, Option<BgFetchMode>, DisplayOptions),
    data: Option<BgsData>,
}

impl BaseView for BgMaps2d {
    const MENU_NAME: &'static str = "2D BG maps";
}

const BORDER_WIDTH: f32 = 1.0;

impl FrameView for BgMaps2d {
    type EmuState = EmuState;

    fn new(window: &mut Window) -> Self {
        let tex_id = window.imgui_gfx.create_and_add_owned_texture(
            Some("BG map".into()),
            imgui_wgpu::TextureDescriptor {
                width: 1024,
                height: 1024,
                format: wgpu::TextureFormat::Rgba8Unorm,
                ..Default::default()
            },
            imgui_wgpu::SamplerDescriptor {
                mag_filter: wgpu::FilterMode::Nearest,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            },
        );

        let transparency_tex_id = {
            let tex = window.imgui_gfx.create_owned_texture(
                Some("BG map transparency checkerboard".into()),
                imgui_wgpu::TextureDescriptor {
                    width: 1024,
                    height: 1024,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    ..Default::default()
                },
                imgui_wgpu::SamplerDescriptor {
                    mag_filter: wgpu::FilterMode::Nearest,
                    min_filter: wgpu::FilterMode::Linear,
                    ..Default::default()
                },
            );

            let transparency_colors = 0x0FFF_FFFF_u64 << 32 | 0x03FF_FFFF;
            let mut data = Vec::with_capacity(1024 * 1024 * 4);
            for y in 0..1024 {
                for x in 0..1024 {
                    data.extend_from_slice(
                        &((transparency_colors >> ((x ^ y) << 3 & 32)) as u32).to_le_bytes(),
                    );
                }
            }
            tex.set_data(
                window.gfx_device(),
                window.gfx_queue(),
                &data,
                Default::default(),
            );

            window
                .imgui_gfx
                .add_texture(imgui_wgpu::Texture::Owned(tex))
        };

        BgMaps2d {
            tex_id,
            transparency_tex_id,
            show_transparency_checkerboard: true,
            show_grid_lines_tiles: true,
            show_grid_lines_bitmap: false,

            palette_buffer: zeroed_box(),
            pixel_buffer: zeroed_box(),

            selection: (
                Selection {
                    engine: Engine2d::A,
                    bg_index: BgIndex::new(0),
                },
                None,
                DisplayOptions {
                    show_tiles: false,
                    pal_index: 0,
                },
            ),
            data: None,
        }
    }

    fn destroy(self, window: &mut Window) {
        window.imgui_gfx.remove_texture(self.tex_id);
    }

    fn emu_state(&self) -> <Self::EmuState as super::FrameViewEmuState>::InitData {
        (self.selection.0, self.selection.1.map(Into::into))
    }

    fn update_from_frame_data(
        &mut self,
        frame_data: &<Self::EmuState as super::FrameViewEmuState>::FrameData,
        _window: &mut Window,
    ) {
        if let Some(data) = &mut self.data {
            data.bg_defaults = frame_data.bg_defaults;
            data.selection = frame_data.selection;
            data.bg = frame_data.bg;

            let (map_size, tiles_bitmap_size) = frame_data.bg.map_and_tiles_bitmap_size();
            data.map[..map_size].copy_from_slice(&frame_data.map[..map_size]);
            data.tiles_bitmap[..tiles_bitmap_size]
                .copy_from_slice(&frame_data.tiles_bitmap[..tiles_bitmap_size]);
            let palette_size = frame_data.bg.mode.palette_size() << 1;
            data.palette[..palette_size].copy_from_slice(&frame_data.palette[..palette_size]);
        } else {
            self.data = Some(frame_data.clone());
        }
    }

    fn draw(
        &mut self,
        ui: &imgui::Ui,
        window: &mut Window,
        mut messages: impl FrameViewMessages<Self>,
    ) {
        if ui.is_window_hovered_with_flags(WindowHoveredFlags::ROOT_AND_CHILD_WINDOWS)
            && ui.is_mouse_clicked(MouseButton::Right)
        {
            ui.open_popup("options");
        }

        let content_width = ui.content_region_avail()[0];

        let mut selection_updated = false;

        let three_width = content_width - 2.0 * style!(ui, item_spacing)[0];

        {
            ui.set_next_item_width(three_width * (1.0 / 3.0));
            let mut engine = self.selection.0.engine as u8;
            selection_updated |= ui
                .slider_config("##engine", 0_u8, 1)
                .display_format(self.selection.0.engine.as_ref())
                .flags(SliderFlags::NO_INPUT)
                .build(&mut engine);
            self.selection.0.engine = match engine {
                0 => Engine2d::A,
                _ => Engine2d::B,
            };

            ui.same_line();
            ui.set_next_item_width(three_width * (1.0 / 3.0));
            let mut bg_index = self.selection.0.bg_index.get();
            selection_updated |= ui
                .slider_config("##bg_index", 0_u8, 3)
                .display_format("BG%d")
                .flags(SliderFlags::NO_INPUT)
                .build(&mut bg_index);
            self.selection.0.bg_index = BgIndex::new(bg_index);
        }

        if selection_updated {
            self.selection.1 = None;
        }

        let default_bg_data = self
            .data
            .as_ref()
            .map(|data| {
                data.bg_defaults[self.selection.0.engine as usize]
                    [self.selection.0.bg_index.get() as usize]
            })
            .unwrap_or(BgsData::DEFAULT_BG_DEFAULT);

        ui.same_line();
        ui.set_next_item_width(three_width * (1.0 / 3.0));
        {
            static BG_MODES: [Option<BgMode>; 8] = [
                None,
                Some(BgMode::Text16),
                Some(BgMode::Text256),
                Some(BgMode::Affine),
                Some(BgMode::ExtendedMap),
                Some(BgMode::ExtendedBitmap256),
                Some(BgMode::ExtendedBitmapDirect),
                Some(BgMode::LargeBitmap),
            ];

            let mut mode = self.selection.1.map(Into::into);
            if combo_value(ui, "##mode", &mut mode, &BG_MODES, |mode| match mode {
                None => format!("Default ({})", BgMode::from(default_bg_data.mode).as_ref()).into(),
                Some(mode) => mode.as_ref().into(),
            }) {
                self.selection.1 = mode.map(Into::into);
                selection_updated = true;
            }
        }

        let mut fetch_mode = self
            .selection
            .1
            .unwrap_or_else(|| default_bg_data.mode.into());
        let has_tiles = fetch_mode.has_tiles();

        if selection_updated {
            self.selection.2.show_tiles = false;
            self.selection.2.pal_index = 0;
        }

        'display_mode_options: {
            let uses_ext_pal = fetch_mode.uses_ext_pal_mut();

            let count = uses_ext_pal.is_some() as u8 + has_tiles as u8;
            if count == 0 {
                break 'display_mode_options;
            }

            let select_width =
                (content_width - (count - 1) as f32 * style!(ui, item_spacing)[0]) / count as f32;

            let mut mode_updated = false;

            if let Some(uses_ext_pal) = uses_ext_pal {
                static EXT_PAL_SETTINGS: [Option<bool>; 3] = [None, Some(false), Some(true)];
                static LABELS: [&str; 2] = ["Standard palette", "Extended palette"];

                ui.set_next_item_width(select_width);
                mode_updated |= combo_value(
                    ui,
                    "##uses_ext_pals",
                    uses_ext_pal,
                    &EXT_PAL_SETTINGS,
                    |uses_ext_pals| match uses_ext_pals {
                        None => format!(
                            "Default ({})",
                            LABELS[default_bg_data.mode.uses_ext_pal().unwrap_or(false) as usize]
                        )
                        .into(),
                        Some(value) => LABELS[*value as usize].into(),
                    },
                );
                ui.same_line();
            }

            if has_tiles {
                ui.set_next_item_width(select_width);
                let mut show_tiles = self.selection.2.show_tiles as u8;
                mode_updated |= ui
                    .slider_config("##show_tiles", 0_u8, 1)
                    .display_format(if self.selection.2.show_tiles {
                        "Tileset"
                    } else {
                        "Tilemap"
                    })
                    .flags(SliderFlags::NO_INPUT)
                    .build(&mut show_tiles);
                self.selection.2.show_tiles = show_tiles != 0;
                ui.same_line();
            }

            if mode_updated {
                self.selection.1 = Some(fetch_mode);
                selection_updated = true;
            }

            ui.new_line();
        }

        let updated_data = self
            .data
            .as_ref()
            .filter(|data| data.selection == (self.selection.0, self.selection.1));

        let options_export_width = ui.calc_text_size("Options...")[0]
            + style!(ui, item_spacing)[0]
            + 4.0 * style!(ui, frame_padding)[0]
            + ui.calc_text_size("Export...")[0];
        if let Some(data) = updated_data {
            if self.selection.2.show_tiles {
                if fetch_mode.allows_pal_index(data.bg.mode) {
                    ui.set_next_item_width(
                        ui.content_region_avail()[0]
                            - style!(ui, item_spacing)[0]
                            - options_export_width,
                    );
                    if ui
                        .slider_config("##pal_index", 0_u8, 15)
                        .display_format("Palette %d")
                        .flags(SliderFlags::NO_INPUT)
                        .build(&mut self.selection.2.pal_index)
                    {
                        fetch_mode.pal_index_changed();
                        self.selection.1 = Some(fetch_mode);
                        selection_updated = true;
                    }
                    ui.same_line();
                }
            } else {
                ui.align_text_to_frame_padding();
                ui.text(format!("Size: {}x{}", data.bg.size[0], data.bg.size[1]));
                ui.same_line();
            }
        }

        if selection_updated {
            messages.push((self.selection.0, self.selection.1));
        }

        ui.set_cursor_pos([
            ui.content_region_max()[0] - options_export_width,
            ui.cursor_pos()[1],
        ]);

        if ui.button("Options...") {
            ui.open_popup("options");
        }

        ui.popup("options", || {
            ui.checkbox(
                "Show transparency checkerboard",
                &mut self.show_transparency_checkerboard,
            );

            ui.checkbox("Show tile grid lines", &mut self.show_grid_lines_tiles);

            ui.checkbox("Show bitmap grid lines", &mut self.show_grid_lines_bitmap);
        });

        ui.same_line();
        let mut export_requested = false;
        ui.enabled(updated_data.is_some(), || {
            export_requested = ui.button("Export...");
        });

        let Some(data) = updated_data else {
            ui.text("Loading...");
            return;
        };

        let tiles_per_row = data.bg.mode.tiles_sqrt_len();
        let image_pixels = if self.selection.2.show_tiles {
            [tiles_per_row << 3; 2]
        } else {
            [data.bg.size[0] as usize, data.bg.size[1] as usize]
        };

        let (mut image_pos, image_size) = scale_to_fit(
            image_pixels[0] as f32 / image_pixels[1] as f32,
            ui.content_region_avail(),
        );
        image_pos[0] += style!(ui, window_padding)[0];
        image_pos[1] += ui.cursor_pos()[1];

        if self.show_transparency_checkerboard {
            ui.set_cursor_pos(image_pos);
            Image::new(self.transparency_tex_id, image_size)
                .uv1(image_pixels.map(|size| size as f32 / 1024.0))
                .build(ui);
        }
        ui.set_cursor_pos(image_pos);
        Image::new(self.tex_id, image_size)
            .uv1(image_pixels.map(|size| size as f32 / 1024.0))
            .build(ui);

        let show_grid_lines = if has_tiles {
            self.show_grid_lines_tiles
        } else {
            self.show_grid_lines_bitmap
        };
        let show_zoom_tooltip = ui.is_item_hovered();

        if show_grid_lines || show_zoom_tooltip {
            let window_screen_pos = ui.window_pos();
            let image_screen_pos = add2(window_screen_pos, image_pos);

            let grid_size = [image_pixels[0] >> 3, image_pixels[1] >> 3];
            let cell_size = [
                image_size[0] / grid_size[0] as f32,
                image_size[1] / grid_size[1] as f32,
            ];
            let border_color = ui.style_color(StyleColor::Border);

            if show_grid_lines {
                let start_screen_pos = sub2s(image_screen_pos, 0.5 * BORDER_WIDTH);
                let end_screen_pos = add2(start_screen_pos, image_size);

                let draw_list = ui.get_window_draw_list();
                for x in 0..=grid_size[0] {
                    let x_pos = start_screen_pos[0] + x as f32 * cell_size[0];
                    draw_list
                        .add_line(
                            [x_pos, start_screen_pos[1]],
                            [x_pos, end_screen_pos[1]],
                            border_color,
                        )
                        .thickness(BORDER_WIDTH)
                        .build();
                }
                for y in 0..=grid_size[1] {
                    let y_pos = start_screen_pos[1] + y as f32 * cell_size[1];
                    draw_list
                        .add_line(
                            [start_screen_pos[0], y_pos],
                            [end_screen_pos[0], y_pos],
                            border_color,
                        )
                        .thickness(BORDER_WIDTH)
                        .build();
                }
            }

            if show_zoom_tooltip {
                ui.tooltip(|| {
                    let tooltip_image_size = [ui.current_font_size() * 4.0; 2];
                    let mouse_pos = sub2(ui.io().mouse_pos, image_screen_pos);

                    if self.show_transparency_checkerboard {
                        let image_pos = ui.cursor_screen_pos();
                        Image::new(self.transparency_tex_id, tooltip_image_size)
                            .uv1([1.0 / 128.0; 2])
                            .build(ui);
                        ui.set_cursor_screen_pos(image_pos);
                    }

                    if has_tiles {
                        let cell_pos = [
                            (mouse_pos[0] / cell_size[0]) as usize,
                            (mouse_pos[1] / cell_size[1]) as usize,
                        ];

                        Image::new(self.tex_id, tooltip_image_size)
                            .border_col(border_color)
                            .uv0(cell_pos.map(|pos| pos as f32 / 128.0))
                            .uv1(cell_pos.map(|pos| (pos + 1) as f32 / 128.0))
                            .build(ui);

                        let cell_index = cell_pos[1] * grid_size[0] + cell_pos[0];
                        if self.selection.2.show_tiles {
                            ui.text(format!(
                                "Tile {cell_index:#0width$X}",
                                width = 4 + (data.bg.mode != BgResolvedFetchMode::Affine) as usize
                            ));
                        } else {
                            if data.bg.mode == BgResolvedFetchMode::Affine {
                                ui.text(format!("Tile {:#04X}", data.map[cell_index]));
                            } else {
                                let tile = data.map.read_le::<u16>(cell_index << 1);
                                ui.text(format!("Tile {:#05X}", tile & 0x3FF));
                                ui.align_text_to_frame_padding();
                                ui.text("Flip: ");
                                ui.same_line();
                                ui.checkbox("X", &mut (tile & 0x400 != 0));
                                ui.same_line_with_spacing(0.0, style!(ui, item_spacing)[0] + 4.0);
                                ui.checkbox("Y", &mut (tile & 0x800 != 0));
                                if data.bg.mode == BgResolvedFetchMode::Text16
                                    || data.bg.mode.uses_ext_pal() == Some(true)
                                {
                                    ui.text(format!("Palette number: {:#03X}", tile >> 12));
                                }
                            }
                            ui.text(format!("X: {}, Y: {}", cell_pos[0] * 8, cell_pos[1] * 8));
                        }
                    } else {
                        let pixel_pos = [
                            ((mouse_pos[0] / image_size[0] * image_pixels[0] as f32) as usize)
                                .saturating_sub(4)
                                .min(image_pixels[0] - 8),
                            ((mouse_pos[1] / image_size[1] * image_pixels[1] as f32) as usize)
                                .saturating_sub(4)
                                .min(image_pixels[1] - 8),
                        ];

                        Image::new(self.tex_id, tooltip_image_size)
                            .border_col(border_color)
                            .uv0(pixel_pos.map(|pos| pos as f32 / 1024.0))
                            .uv1(pixel_pos.map(|pos| (pos + 8) as f32 / 1024.0))
                            .build(ui);

                        ui.text(format!("X: {}, Y: {}", pixel_pos[0], pixel_pos[1]));
                    }
                });
            }
        }

        for (i, color) in self.palette_buffer[..data.bg.mode.palette_size()]
            .iter_mut()
            .enumerate()
        {
            let orig_color = data.palette.read_le::<u16>(i << 1);
            *color = rgb5_to_rgba8(orig_color);
        }

        let pixels_len = image_pixels[0] * image_pixels[1];

        unsafe {
            match data.bg.mode {
                BgResolvedFetchMode::Text16 => {
                    if self.selection.2.show_tiles {
                        for tile in 0..0x400 {
                            let src_base = tile << 5;
                            let dst_base =
                                ((tile / tiles_per_row) << 10 | (tile % tiles_per_row)) << 3;
                            let pal_base = (self.selection.2.pal_index << 4) as usize;
                            for y in 0..8 {
                                let src_base = src_base | y << 2;
                                let dst_base = dst_base | y << 10;
                                let pixels = data.tiles_bitmap.read_le_unchecked::<u32>(src_base);
                                for x in 0..8 {
                                    let color_index = pixels >> (x << 2) & 0xF;
                                    *self.pixel_buffer.get_unchecked_mut(dst_base | x) =
                                        if color_index == 0 {
                                            0
                                        } else {
                                            self.palette_buffer[pal_base | color_index as usize]
                                        };
                                }
                            }
                        }
                    } else {
                        let x_shift = data.bg.size[0].trailing_zeros();
                        let tile_x_shift = x_shift - 3;
                        let tile_i_x_mask = (1 << tile_x_shift) - 1;
                        for tile_i in 0..pixels_len / 64 {
                            let tile = data.map.read_le_unchecked::<u16>(tile_i << 1) as usize;
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
                                    let color_index =
                                        data.tiles_bitmap.read_unchecked(src_base | src_x >> 1)
                                            >> ((src_x & 1) << 2)
                                            & 0xF;
                                    *self.pixel_buffer.get_unchecked_mut(dst_base | x) =
                                        if color_index == 0 {
                                            0
                                        } else {
                                            self.palette_buffer[pal_base | color_index as usize]
                                        };
                                }
                            }
                        }
                    }
                }

                BgResolvedFetchMode::Text256 { uses_ext_pal }
                | BgResolvedFetchMode::ExtendedMap { uses_ext_pal } => {
                    if self.selection.2.show_tiles {
                        for tile in 0..0x400 {
                            let src_base = tile << 6;
                            let dst_base =
                                ((tile / tiles_per_row) << 10 | (tile % tiles_per_row)) << 3;
                            let pal_base = if uses_ext_pal {
                                (self.selection.2.pal_index as usize & 0xF) << 8
                            } else {
                                0
                            };
                            for y in 0..8 {
                                let src_base = src_base | y << 3;
                                let dst_base = dst_base | y << 10;
                                for (dst_color, &color_index) in self
                                    .pixel_buffer
                                    .get_unchecked_mut(dst_base..dst_base + 8)
                                    .iter_mut()
                                    .zip(data.tiles_bitmap.get_unchecked(src_base..src_base + 8))
                                {
                                    *dst_color = if color_index == 0 {
                                        0
                                    } else {
                                        self.palette_buffer[pal_base | color_index as usize]
                                    };
                                }
                            }
                        }
                    } else {
                        let x_shift = data.bg.size[0].trailing_zeros();
                        let tile_x_shift = x_shift - 3;
                        let tile_i_x_mask = (1 << tile_x_shift) - 1;
                        for tile_i in 0..pixels_len / 64 {
                            let tile = data.map.read_le_unchecked::<u16>(tile_i << 1) as usize;
                            let src_base = (tile & 0x3FF) << 6;
                            let dst_base =
                                (tile_i >> tile_x_shift << 10 | (tile_i & tile_i_x_mask)) << 3;
                            let pal_base = if uses_ext_pal { tile >> 4 & 0xF00 } else { 0 };
                            let src_x_xor_mask = if tile & 0x400 != 0 { 7 } else { 0 };
                            let src_y_xor_mask = if tile & 0x800 != 0 { 7 } else { 0 };
                            for y in 0..8 {
                                let src_base = src_base | (y ^ src_y_xor_mask) << 3;
                                let dst_base = dst_base | y << 10;
                                for x in 0..8 {
                                    let color_index = data
                                        .tiles_bitmap
                                        .read_unchecked(src_base | (x ^ src_x_xor_mask));
                                    *self.pixel_buffer.get_unchecked_mut(dst_base | x) =
                                        if color_index == 0 {
                                            0
                                        } else {
                                            self.palette_buffer[pal_base | color_index as usize]
                                        };
                                }
                            }
                        }
                    }
                }

                BgResolvedFetchMode::Affine => {
                    if self.selection.2.show_tiles {
                        for tile in 0..0x100 {
                            let src_base = tile << 6;
                            let dst_base =
                                ((tile / tiles_per_row) << 10 | (tile % tiles_per_row)) << 3;
                            for y in 0..8 {
                                let src_base = src_base | y << 3;
                                let dst_base = dst_base | y << 10;
                                for (dst_color, &color_index) in self
                                    .pixel_buffer
                                    .get_unchecked_mut(dst_base..dst_base + 8)
                                    .iter_mut()
                                    .zip(data.tiles_bitmap.get_unchecked(src_base..src_base + 8))
                                {
                                    *dst_color = if color_index == 0 {
                                        0
                                    } else {
                                        self.palette_buffer[color_index as usize]
                                    };
                                }
                            }
                        }
                    } else {
                        let x_shift = data.bg.size[0].trailing_zeros();
                        let tile_x_shift = x_shift - 3;
                        let tile_i_x_mask = (1 << tile_x_shift) - 1;
                        for tile_i in 0..pixels_len / 64 {
                            let src_base = (data.map[tile_i] as usize) << 6;
                            let dst_base =
                                (tile_i >> tile_x_shift << 10 | (tile_i & tile_i_x_mask)) << 3;
                            for y in 0..8 {
                                let src_base = src_base | y << 3;
                                let dst_base = dst_base | y << 10;
                                for (dst_color, &color_index) in self
                                    .pixel_buffer
                                    .get_unchecked_mut(dst_base..dst_base + 8)
                                    .iter_mut()
                                    .zip(data.tiles_bitmap.get_unchecked(src_base..src_base + 8))
                                {
                                    *dst_color = if color_index == 0 {
                                        0
                                    } else {
                                        self.palette_buffer[color_index as usize]
                                    };
                                }
                            }
                        }
                    }
                }

                BgResolvedFetchMode::ExtendedBitmap256 | BgResolvedFetchMode::LargeBitmap => {
                    let x_shift = data.bg.size[0].trailing_zeros();
                    for y in 0..data.bg.size[1] as usize {
                        let src_base = y << x_shift;
                        let dst_base = y << 10;
                        for (dst_color, &color_index) in self
                            .pixel_buffer
                            .get_unchecked_mut(dst_base..dst_base + data.bg.size[0] as usize)
                            .iter_mut()
                            .zip(
                                data.tiles_bitmap
                                    .get_unchecked(src_base..src_base + data.bg.size[0] as usize),
                            )
                        {
                            *dst_color = if color_index == 0 {
                                0
                            } else {
                                self.palette_buffer[color_index as usize]
                            };
                        }
                    }
                }

                BgResolvedFetchMode::ExtendedBitmapDirect => {
                    let x_shift = data.bg.size[0].trailing_zeros();
                    for y in 0..data.bg.size[1] as usize {
                        let src_base = y << (x_shift + 1);
                        let dst_base = y << 10;
                        for x in 0..data.bg.size[0] as usize {
                            let color = data
                                .tiles_bitmap
                                .read_le_unchecked::<u16>(src_base + (x << 1));
                            *self.pixel_buffer.get_unchecked_mut(dst_base + x) =
                                if color & 0x8000 == 0 {
                                    0
                                } else {
                                    rgb5_to_rgba8(color)
                                };
                        }
                    }
                }
            }
        }

        window
            .imgui_gfx
            .texture(self.tex_id)
            .unwrap_owned_ref()
            .set_data(
                window.gfx_device(),
                window.gfx_queue(),
                unsafe {
                    slice::from_raw_parts(self.pixel_buffer.as_ptr() as *const u8, 1024 * 1024 * 4)
                },
                imgui_wgpu::TextureSetRange {
                    width: Some(image_pixels[0] as u32),
                    height: Some(image_pixels[1] as u32),
                    ..Default::default()
                },
            );

        if export_requested {
            if let Some(dst_path) = FileDialog::new()
                .add_filter("PNG image", &["png"])
                .set_file_name(
                    self.selection
                        .0
                        .to_default_filename(data.bg.mode, self.selection.2.show_tiles),
                )
                .save_file()
            {
                if let Err(err) = (|| -> io::Result<()> {
                    let [width, height] = image_pixels;

                    let file = File::create(&dst_path)?;
                    let mut encoder = png::Encoder::new(file, width as u32, height as u32);
                    encoder.set_color(png::ColorType::Rgba);
                    encoder.set_depth(png::BitDepth::Eight);
                    encoder.set_srgb(png::SrgbRenderingIntent::Perceptual);
                    let mut writer = encoder.write_header()?;

                    let mut data = Vec::with_capacity(4 * pixels_len);
                    for y in 0..height {
                        let y_base = y * 1024;
                        for pixel in &self.pixel_buffer[y_base..y_base + width] {
                            data.extend_from_slice(&pixel.to_le_bytes())
                        }
                    }
                    writer.write_image_data(&data)?;

                    writer.finish()?;
                    Ok(())
                })() {
                    error!(
                        "Export error",
                        "Couldn't complete export to `{}`: {err}",
                        dst_path.display()
                    );
                }
            }
        }
    }
}

impl InstanceableView for BgMaps2d {}
