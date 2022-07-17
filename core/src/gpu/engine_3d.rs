mod io;
mod matrix;
mod vertex;
pub use vertex::{Color, InterpColor, ScreenCoords, ScreenVertex, TexCoords};
mod renderer;
pub use renderer::Renderer;

use crate::{
    cpu::{
        self,
        arm9::{self, Arm9},
        Schedule,
    },
    emu::{self, Emu},
    gpu::vram::Vram,
    utils::{
        load_slice_in_place, schedule::RawTimestamp, store_slice, zeroed_box, Fifo, Savestate, Zero,
    },
};
use core::{
    mem::{replace, transmute},
    simd::i32x4,
};
use matrix::{Matrix, MatrixBuffer};
use vertex::{ConversionScreenCoords, Vertex};

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq, Savestate)]
    pub const struct GxStatus(pub u32): Debug {
        pub test_busy: bool @ 0,
        pub box_test_result: bool @ 1,
        pub pos_vec_matrix_stack_level: u8 @ 8..12,
        pub proj_matrix_stack_level: bool @ 13,
        pub matrix_stack_busy: bool @ 14,
        pub matrix_stack_overflow: bool @ 15,
        pub fifo_level: u16 @ 16..=24,
        pub fifo_less_than_half_full: bool @ 25,
        pub fifo_empty: bool @ 26,
        pub busy: bool @ 27,
        pub fifo_irq_mode: u8 @ 30..=31,
    }
}

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq, Savestate)]
    pub const struct PolyVertRamLevel(pub u32): Debug {
        pub poly_ram_level: u16 @ 0..=11,
        pub vert_ram_level: u16 @ 16..=28,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Savestate)]
#[repr(C, align(8))]
struct FifoEntry {
    command: u8,
    param: u32,
}

unsafe impl Zero for FifoEntry {}

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq, Savestate)]
    pub const struct SwapBuffersAttrs(pub u8): Debug {
        pub translucent_auto_sort_disabled: bool @ 0,
        pub w_buffering: bool @ 1,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Savestate)]
#[repr(u8)]
#[allow(dead_code)] // Initialized through `transmute`
enum MatrixMode {
    Projection,
    Position,
    PositionVector,
    Texture,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Savestate)]
struct Light {
    direction: [i16; 3],
    half_vec: [i16; 3],
    color: i32x4,
}

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq, Savestate)]
    pub const struct PolygonAttrs(pub u32): Debug {
        pub lights_mask: u8 @ 0..=3,
        pub mode: u8 @ 4..=5,
        pub show_back: bool @ 6,
        pub show_front: bool @ 7,
        pub update_depth_for_translucent: bool @ 11,
        pub clip_far_plane: bool @ 12,
        pub always_render_1_dot: bool @ 13,
        pub depth_test_equal: bool @ 14,
        pub fog_enabled: bool @ 15,
        pub alpha: u8 @ 16..=20,
        pub id: u8 @ 24..=29,
    }
}

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq, Savestate)]
    pub const struct TextureParams(pub u32): Debug {
        pub vram_off: u16 @ 0..=15,
        pub repeat_s: bool @ 16,
        pub repeat_t: bool @ 17,
        pub flip_s: bool @ 18,
        pub flip_t: bool @ 19,
        pub size_shift_s: u8 @ 20..=22,
        pub size_shift_t: u8 @ 23..=25,
        pub format: u8 @ 26..=28,
        pub use_color_0_as_transparent: bool @ 29,
        pub coord_transform_mode: u8 @ 30..=31,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Savestate)]
#[repr(u8)]
#[allow(dead_code)] // Initialized through `transmute`
enum PrimitiveType {
    Triangles,
    Quads,
    TriangleStrip,
    QuadStrip,
}

mod bounded {
    use crate::utils::{bounded_int_lit, bounded_int_savestate};
    bounded_int_lit!(pub struct PrimVertIndex(u8), max 3);
    bounded_int_savestate!(PrimVertIndex(u8));
    bounded_int_lit!(pub struct PrimMaxVerts(u8), max 4);
    bounded_int_savestate!(PrimMaxVerts(u8));
    bounded_int_lit!(pub struct PolyVertIndex(u8), max 9);
    bounded_int_savestate!(PolyVertIndex(u8));
    bounded_int_lit!(pub struct PolyVertsLen(u8), max 10);
    bounded_int_savestate!(PolyVertsLen(u8));
    bounded_int_lit!(pub struct PolyAddr(u16), max 2047);
    bounded_int_savestate!(PolyAddr(u16));
    bounded_int_lit!(pub struct VertexAddr(u16), max 6143);
    bounded_int_savestate!(VertexAddr(u16));
}
pub use bounded::{PolyAddr, PolyVertIndex, PolyVertsLen, VertexAddr};
use bounded::{PrimMaxVerts, PrimVertIndex};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Savestate)]
#[repr(C)]
pub struct Polygon {
    pub vertices: [VertexAddr; 10],
    pub depth_values: [i32; 10],
    pub w_values: [u16; 10],
    pub top_y: u8,
    pub bot_y: u8,
    pub vertices_len: PolyVertsLen,
    pub attrs: PolygonAttrs,
    pub is_front_facing: bool,
    pub is_translucent: bool,
    pub tex_params: TextureParams,
    pub tex_palette_base: u16,
}

unsafe impl Zero for Polygon {}

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq, Savestate)]
    pub const struct RenderingControl(pub u16): Debug {
        pub texture_mapping_enabled: bool @ 0,
        pub highlight_shading_enabled: bool @ 1,
        pub alpha_test_enabled: bool @ 2,
        pub alpha_blending_enabled: bool @ 3,
        pub antialiasing_enabled: bool @ 4,
        pub edge_marking_enabled: bool @ 5,
        pub fog_only_alpha: bool @ 6,
        pub fog_enabled: bool @ 7,
        pub fog_depth_shift: u8 @ 8..=11,
        pub color_buffer_underflow: bool @ 12,
        pub poly_vert_ram_overflow: bool @ 13,
        pub rear_plane_bitmap_enabled: bool @ 14,
    }
}

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub const struct ClearControl(pub u16): Debug {
        pub alpha: u8 @ 0..=4,
        pub poly_id: u8 @ 8..=13,
    }
}

#[derive(Clone, Debug, Savestate)]
pub struct RenderingState {
    pub control: RenderingControl,

    #[load(value = "0xF")]
    #[store(skip)]
    pub texture_dirty: u8,
    #[load(value = "0x3F")]
    #[store(skip)]
    pub tex_pal_dirty: u8,

    pub alpha_test_ref: u8,

    pub clear_color: Color,
    pub clear_poly_id: u8,
    pub clear_depth: u16,
    pub clear_image_offset: [u8; 2],

    pub toon_colors: [Color; 0x20],

    pub edge_colors: [Color; 8],

    pub fog_color: Color,
    pub fog_densities: [u8; 0x20],
    pub fog_offset: u16,
    pub rear_plane_fog_enabled: bool,
}

#[derive(Savestate)]
#[load(in_place_only)]
pub struct Engine3d {
    #[cfg(feature = "log")]
    #[savestate(skip)]
    logger: slog::Logger,
    #[savestate(skip)]
    pub renderer: Box<dyn Renderer>,

    pub(super) gx_enabled: bool,
    pub(super) rendering_enabled: bool,

    gx_status: GxStatus,
    gx_fifo_irq_requested: bool,
    gx_fifo: Box<Fifo<FifoEntry, { 256 + 4 * 16 }>>,
    gx_pipe: Fifo<FifoEntry, 4>,
    cur_packed_commands: u32,
    remaining_command_params: u8,
    command_finish_time: emu::Timestamp,
    gx_fifo_stalled: bool,
    queued_mtx_stack_cmds: u16,
    queued_test_cmd_entries: u16,

    swap_buffers_attrs: SwapBuffersAttrs,

    mtx_mode: MatrixMode,
    proj_stack: Matrix,
    pos_vec_stack: [[Matrix; 2]; 32],
    tex_stack: Matrix,
    proj_stack_pointer: bool,
    pos_vec_stack_pointer: u8,
    cur_proj_mtx: Matrix,
    cur_pos_vec_mtxs: [Matrix; 2],
    cur_clip_mtx: Matrix,
    clip_mtx_needs_recalculation: bool,
    cur_tex_mtx: Matrix,
    tex_params: TextureParams,
    tex_palette_base: u16,

    viewport: [u8; 4],

    vert_color: Color,
    vert_normal: [i16; 3],
    tex_coords: TexCoords,
    transformed_tex_coords: TexCoords,
    last_vtx_coords: [i16; 3],

    shininess_table_enabled: bool,
    diffuse_color: i32x4,
    ambient_color: i32x4,
    specular_color: i32x4,
    emission_color: i32x4,
    shininess_table: [u8; 128],
    lights: [Light; 4],

    // Latched on BEGIN_VTXS
    next_poly_attrs: PolygonAttrs,
    cur_poly_attrs: PolygonAttrs,

    cur_prim_type: PrimitiveType,
    cur_prim_verts: [Vertex; 4],
    last_strip_prim_vert_indices: [VertexAddr; 2],
    cur_prim_max_verts: PrimMaxVerts,
    cur_prim_vert_index: PrimVertIndex,
    cur_strip_prim_is_odd: bool,
    connect_to_last_strip_prim: bool,

    vert_ram_level: u16,
    poly_ram_level: u16,
    #[load(
        with_in_place = "load_slice_in_place(&mut vert_ram[..*vert_ram_level as usize], save)?"
    )]
    #[store(with = "store_slice(&mut vert_ram[..*vert_ram_level as usize], save)?")]
    vert_ram: Box<[ScreenVertex; 6144]>,
    #[load(
        with_in_place = "load_slice_in_place(&mut poly_ram[..*poly_ram_level as usize], save)?"
    )]
    #[store(with = "store_slice(&mut poly_ram[..*poly_ram_level as usize], save)?")]
    poly_ram: Box<[Polygon; 2048]>,

    rendering_state: RenderingState,
}

fn decode_rgb_5(value: u16, alpha: u8) -> Color {
    Color::from_array([
        value as u8 & 0x1F,
        (value >> 5) as u8 & 0x1F,
        (value >> 10) as u8 & 0x1F,
        alpha,
    ])
}

fn rgb_5_to_6(value: Color) -> Color {
    value << Color::splat(1) | (value + Color::splat(0x1F)) >> Color::splat(5)
}

impl Engine3d {
    pub(super) fn new(
        renderer: Box<dyn Renderer>,
        schedule: &mut arm9::Schedule,
        emu_schedule: &mut emu::Schedule,
        #[cfg(feature = "log")] logger: slog::Logger,
    ) -> Self {
        schedule.set_event(arm9::event_slots::GX_FIFO, arm9::Event::GxFifoStall);
        schedule.set_event(
            arm9::event_slots::ENGINE_3D,
            arm9::Event::Engine3dCommandFinished,
        );
        emu_schedule.set_event(
            emu::event_slots::ENGINE_3D,
            emu::Event::Engine3dCommandFinished,
        );

        Engine3d {
            #[cfg(feature = "log")]
            logger,
            renderer,

            gx_enabled: false,
            rendering_enabled: false,

            gx_status: GxStatus(0),

            gx_fifo_irq_requested: false,
            gx_fifo: Box::new(Fifo::new()),
            gx_pipe: Fifo::new(),
            cur_packed_commands: 0,
            remaining_command_params: 0,
            command_finish_time: emu::Timestamp(0),
            gx_fifo_stalled: false,
            queued_mtx_stack_cmds: 0,
            queued_test_cmd_entries: 0,

            swap_buffers_attrs: SwapBuffersAttrs(0),

            mtx_mode: MatrixMode::Projection,
            proj_stack: Matrix::zero(),
            pos_vec_stack: [[Matrix::zero(); 2]; 32],
            tex_stack: Matrix::zero(),
            proj_stack_pointer: false,
            pos_vec_stack_pointer: 0,
            cur_proj_mtx: Matrix::zero(),
            cur_pos_vec_mtxs: [Matrix::zero(), Matrix::zero()],
            cur_clip_mtx: Matrix::zero(),
            clip_mtx_needs_recalculation: false,
            cur_tex_mtx: Matrix::zero(),
            tex_params: TextureParams(0),
            tex_palette_base: 0,

            viewport: [0; 4],

            vert_color: Color::splat(0),
            vert_normal: [0; 3],
            tex_coords: TexCoords::splat(0),
            transformed_tex_coords: TexCoords::splat(0),
            last_vtx_coords: [0; 3],
            shininess_table_enabled: false,
            diffuse_color: i32x4::splat(0),
            ambient_color: i32x4::splat(0),
            specular_color: i32x4::splat(0),
            emission_color: i32x4::splat(0),
            shininess_table: [0; 128],
            lights: [Light {
                direction: [0; 3],
                half_vec: [0, 0, -0x100],
                color: i32x4::splat(0),
            }; 4],

            next_poly_attrs: PolygonAttrs(0),
            cur_poly_attrs: PolygonAttrs(0),

            cur_prim_type: PrimitiveType::Triangles,
            cur_prim_verts: [Vertex::new(); 4],
            last_strip_prim_vert_indices: [VertexAddr::new(0); 2],
            cur_prim_max_verts: PrimMaxVerts::new(0),
            cur_prim_vert_index: PrimVertIndex::new(0),
            cur_strip_prim_is_odd: false,
            connect_to_last_strip_prim: false,

            vert_ram_level: 0,
            poly_ram_level: 0,
            vert_ram: zeroed_box(),
            poly_ram: zeroed_box(),

            rendering_state: RenderingState {
                control: RenderingControl(0),

                texture_dirty: 0xF,
                tex_pal_dirty: 0x3F,

                alpha_test_ref: 0,

                clear_color: Color::splat(0),
                clear_poly_id: 0,
                clear_depth: 0,
                clear_image_offset: [0; 2],

                toon_colors: [Color::splat(0); 0x20],

                edge_colors: [Color::splat(0); 8],

                fog_color: Color::splat(0),
                fog_densities: [0; 0x20],
                fog_offset: 0,
                rear_plane_fog_enabled: false,
            },
        }
    }

    #[inline]
    pub fn gx_fifo_stalled(&self) -> bool {
        self.gx_fifo_stalled
    }

    #[inline]
    pub fn gx_status(&self) -> GxStatus {
        self.gx_status
            .with_proj_matrix_stack_level(self.proj_stack_pointer)
            .with_pos_vec_matrix_stack_level(self.pos_vec_stack_pointer)
            .with_fifo_level(self.gx_fifo.len().min(256) as u16)
            .with_fifo_less_than_half_full(self.gx_fifo.len() < 128)
            .with_fifo_empty(self.gx_fifo.is_empty())
    }

    fn update_gx_fifo_irq(&mut self, arm9: &mut Arm9<impl cpu::Engine>) {
        self.gx_fifo_irq_requested = match self.gx_status.fifo_irq_mode() {
            1 => self.gx_fifo.len() < 128,
            2 => self.gx_fifo.is_empty(),
            _ => false,
        };
        if self.gx_fifo_irq_requested {
            arm9.irqs
                .write_requested(arm9.irqs.requested().with_gx_fifo(true), &mut arm9.schedule);
        }
    }

    #[inline]
    pub fn write_gx_status(&mut self, value: GxStatus, arm9: &mut Arm9<impl cpu::Engine>) {
        self.gx_status.0 =
            (self.gx_status.0 & !0xC000_0000 & !(value.0 & 0x8000)) | (value.0 & 0xC000_0000);
        self.update_gx_fifo_irq(arm9);
    }

    pub fn vert_ram_level(&self) -> u16 {
        self.vert_ram_level
    }

    pub fn vert_ram(&self) -> &[ScreenVertex; 6144] {
        &self.vert_ram
    }

    pub fn poly_ram_level(&self) -> u16 {
        self.poly_ram_level
    }

    pub fn poly_ram(&self) -> &[Polygon; 2048] {
        &self.poly_ram
    }

    #[inline]
    pub fn poly_vert_ram_level(&self) -> PolyVertRamLevel {
        PolyVertRamLevel(0)
            .with_poly_ram_level(self.poly_ram_level)
            .with_vert_ram_level(self.vert_ram_level)
    }

    #[inline]
    pub fn line_buffer_level(&self) -> u8 {
        // TODO
        if self.rendering_enabled {
            46
        } else {
            0
        }
    }

    #[inline]
    pub fn rendering_state(&self) -> &RenderingState {
        &self.rendering_state
    }

    #[inline]
    pub fn rendering_control(&self) -> RenderingControl {
        self.rendering_state.control
    }

    #[inline]
    pub fn write_rendering_control(&mut self, value: RenderingControl) {
        self.rendering_state.control.0 =
            (self.rendering_state.control.0 & 0x3000 & !value.0) | (value.0 & 0x4FFF);
    }

    pub(super) fn set_texture_dirty(&mut self, slot_mask: u8) {
        self.rendering_state.texture_dirty |= slot_mask;
    }

    pub(super) fn set_tex_pal_dirty(&mut self, slot_mask: u8) {
        self.rendering_state.tex_pal_dirty |= slot_mask;
    }

    pub(crate) fn gx_fifo_irq_requested(&self) -> bool {
        self.gx_fifo_irq_requested
    }

    pub(crate) fn gx_fifo_half_empty(&self) -> bool {
        self.gx_fifo.len() < 128
    }

    #[allow(clippy::match_same_arms)]
    fn params_for_command(&self, command: u8) -> u8 {
        match command {
            0x00 | 0x11 | 0x15 | 0x41 => 0,
            0x10 | 0x12 | 0x13 | 0x14 | 0x20 | 0x21 | 0x22 | 0x24 | 0x25 | 0x26 | 0x27 | 0x28
            | 0x29 | 0x2A | 0x2B | 0x30 | 0x31 | 0x32 | 0x33 | 0x40 | 0x50 | 0x60 | 0x72 => 1,
            0x23 | 0x71 => 2,
            0x1B | 0x1C | 0x70 => 3,
            0x1A => 9,
            0x17 | 0x19 => 12,
            0x16 | 0x18 => 16,
            0x34 => 32,
            _ => {
                #[cfg(feature = "log")]
                slog::warn!(self.logger, "Unknown command: {:#04X}", command);
                0
            }
        }
    }

    fn write_to_gx_fifo(emu: &mut Emu<impl cpu::Engine>, value: FifoEntry) {
        match value.command {
            0x11 | 0x12 => {
                emu.gpu.engine_3d.queued_mtx_stack_cmds += 1;
                emu.gpu.engine_3d.gx_status.set_matrix_stack_busy(true);
            }
            0x70..=0x72 => {
                emu.gpu.engine_3d.queued_test_cmd_entries += 1;
                emu.gpu.engine_3d.gx_status.set_test_busy(true);
            }
            _ => {}
        }

        if !emu.gpu.engine_3d.gx_pipe.is_full() && emu.gpu.engine_3d.gx_fifo.is_empty() {
            let _ = emu.gpu.engine_3d.gx_pipe.write(value);
        } else {
            let _ = emu.gpu.engine_3d.gx_fifo.write(value);
            match emu.gpu.engine_3d.gx_status.fifo_irq_mode() {
                1 => {
                    emu.gpu.engine_3d.gx_fifo_irq_requested = emu.gpu.engine_3d.gx_fifo.len() < 128;
                }
                2 => emu.gpu.engine_3d.gx_fifo_irq_requested = false,
                _ => {}
            }
            if emu.gpu.engine_3d.gx_fifo.len() > 256 {
                if !emu.gpu.engine_3d.gx_fifo_stalled {
                    emu.gpu.engine_3d.gx_fifo_stalled = true;
                    let cur_time = emu.arm9.schedule.cur_time();
                    if arm9::Timestamp::from(emu.gpu.engine_3d.command_finish_time) > cur_time
                        && !emu.gpu.engine_3d.swap_buffers_waiting()
                    {
                        emu.arm9.schedule.cancel_event(arm9::event_slots::ENGINE_3D);
                        emu.schedule.schedule_event(
                            emu::event_slots::ENGINE_3D,
                            emu.gpu.engine_3d.command_finish_time,
                        );
                    }
                    emu.arm9
                        .schedule
                        .schedule_event(arm9::event_slots::GX_FIFO, cur_time);
                }
                return;
            }
        }
        if emu.gpu.engine_3d.command_finish_time.0 == 0 {
            Self::process_next_command(emu);
        }
    }

    fn write_unpacked_command(emu: &mut Emu<impl cpu::Engine>, command: u8, param: u32) {
        if emu.gpu.engine_3d.remaining_command_params == 0 {
            emu.gpu.engine_3d.remaining_command_params = emu
                .gpu
                .engine_3d
                .params_for_command(command)
                .saturating_sub(1);
        } else {
            emu.gpu.engine_3d.remaining_command_params -= 1;
        }
        Self::write_to_gx_fifo(emu, FifoEntry { command, param });
    }

    fn write_packed_command(emu: &mut Emu<impl cpu::Engine>, value: u32) {
        // TODO: "Packed commands are first decompressed and then stored in the command FIFO."
        if emu.gpu.engine_3d.remaining_command_params == 0 {
            emu.gpu.engine_3d.cur_packed_commands = value;
            let command = emu.gpu.engine_3d.cur_packed_commands as u8;
            emu.gpu.engine_3d.remaining_command_params =
                emu.gpu.engine_3d.params_for_command(command);
            if emu.gpu.engine_3d.remaining_command_params > 0 {
                return;
            }
            Self::write_to_gx_fifo(emu, FifoEntry { command, param: 0 });
        } else {
            let command = emu.gpu.engine_3d.cur_packed_commands as u8;
            Self::write_to_gx_fifo(
                emu,
                FifoEntry {
                    command,
                    param: value,
                },
            );
            emu.gpu.engine_3d.remaining_command_params -= 1;
            if emu.gpu.engine_3d.remaining_command_params > 0 {
                return;
            }
        }
        let mut cur_packed_commands = emu.gpu.engine_3d.cur_packed_commands;
        loop {
            cur_packed_commands >>= 8;
            if cur_packed_commands == 0 {
                break;
            }
            let next_command = cur_packed_commands as u8;
            let next_command_params = emu.gpu.engine_3d.params_for_command(next_command);
            if next_command_params > 0 {
                emu.gpu.engine_3d.cur_packed_commands = cur_packed_commands;
                emu.gpu.engine_3d.remaining_command_params = next_command_params;
                break;
            }
            Self::write_to_gx_fifo(
                emu,
                FifoEntry {
                    command: next_command,
                    param: 0,
                },
            );
        }
    }

    fn refill_gx_pipe(&mut self, arm9: &mut Arm9<impl cpu::Engine>, empty: usize) {
        if self.gx_pipe.len() > 2 {
            return;
        }
        for _ in (self.gx_pipe.len()..4 - empty).take(self.gx_fifo.len()) {
            unsafe {
                self.gx_pipe.write_unchecked(self.gx_fifo.read_unchecked());
            }
        }
        self.update_gx_fifo_irq(arm9);
        if self.gx_fifo_half_empty() {
            arm9.start_dma_transfers_with_timing::<{ arm9::dma::Timing::GxFifo }>();
        }
    }

    fn update_clip_mtx(&mut self) {
        self.clip_mtx_needs_recalculation = false;
        self.cur_clip_mtx = self.cur_pos_vec_mtxs[0] * self.cur_proj_mtx;
    }

    fn load_matrix(&mut self, matrix: Matrix) {
        match self.mtx_mode {
            MatrixMode::Projection => {
                self.cur_proj_mtx = matrix;
                self.clip_mtx_needs_recalculation = true;
            }

            MatrixMode::Position => {
                self.cur_pos_vec_mtxs[0] = matrix;
                self.clip_mtx_needs_recalculation = true;
            }

            MatrixMode::PositionVector => {
                self.cur_pos_vec_mtxs[0] = matrix;
                self.cur_pos_vec_mtxs[1] = matrix;
                self.clip_mtx_needs_recalculation = true;
            }

            MatrixMode::Texture => self.cur_tex_mtx = matrix,
        }
    }

    fn apply_lighting(&mut self) {
        let normal = self.cur_pos_vec_mtxs[1]
            .mul_left_vec3_zero::<i16, i32, 12>(self.vert_normal)
            .to_array();
        let normal = [normal[0], normal[1], normal[2]];
        let mut color = self.emission_color;
        for (i, light) in self.lights.iter().enumerate() {
            if self.cur_poly_attrs.lights_mask() & 1 << i == 0 {
                continue;
            }

            let diffuse_level = ((-light
                .direction
                .iter()
                .zip(normal.iter())
                .fold(0_i32, |acc, (a, b)| {
                    acc.wrapping_add((*a as i32).wrapping_mul(*b))
                }))
                >> 9)
                .max(0);

            let mut shininess_level = ((-light
                .half_vec
                .iter()
                .zip(normal.iter())
                .fold(0_i32, |acc, (a, b)| {
                    acc.wrapping_add((*a as i32).wrapping_mul(*b))
                }))
                >> 9)
                .max(0);
            shininess_level = (shininess_level * shininess_level) >> 9;

            if self.shininess_table_enabled {
                shininess_level =
                    self.shininess_table[(shininess_level >> 2).min(0x7F) as usize] as i32;
            }

            color += ((self.diffuse_color * light.color * i32x4::splat(diffuse_level))
                >> i32x4::splat(14))
                + ((self.specular_color * light.color * i32x4::splat(shininess_level))
                    >> i32x4::splat(14))
                + ((self.ambient_color * light.color) >> i32x4::splat(5));
        }
        self.vert_color = rgb_5_to_6(color.min(i32x4::splat(0x1F)).cast());
    }

    fn add_vert(&mut self, coords: [i16; 3]) {
        if self.poly_ram_level as usize == self.poly_ram.len() {
            self.rendering_state
                .control
                .set_poly_vert_ram_overflow(true);
            return;
        }

        self.last_vtx_coords = coords;

        if self.clip_mtx_needs_recalculation {
            self.update_clip_mtx();
        }

        let transformed_coords = self.cur_clip_mtx.mul_left_vec3::<i16, i32>(coords);

        if self.tex_params.coord_transform_mode() == 3 {
            let [u, v, ..] = self
                .cur_tex_mtx
                .mul_left_vec3_zero::<i16, i16, 24>(coords)
                .to_array();
            self.transformed_tex_coords = self.tex_coords + TexCoords::from_array([u, v]);
        }

        self.cur_prim_verts[self.cur_prim_vert_index.get() as usize] = Vertex {
            coords: transformed_coords,
            uv: self.transformed_tex_coords,
            color: self.vert_color,
        };

        let new_vert_index = self.cur_prim_vert_index.get() + 1;
        if new_vert_index < self.cur_prim_max_verts.get() {
            self.cur_prim_vert_index = PrimVertIndex::new(new_vert_index);
            return;
        }

        if self.cur_prim_type == PrimitiveType::QuadStrip {
            self.cur_prim_verts.swap(2, 3);
        }

        self.clip_and_submit_polygon();

        match self.cur_prim_type {
            PrimitiveType::Triangles | PrimitiveType::Quads => {
                self.cur_prim_vert_index = PrimVertIndex::new(0);
            }

            PrimitiveType::TriangleStrip => {
                self.cur_prim_verts[self.cur_strip_prim_is_odd as usize] = self.cur_prim_verts[2];
                self.cur_prim_vert_index = PrimVertIndex::new(2);
                self.cur_strip_prim_is_odd = !self.cur_strip_prim_is_odd;
            }

            PrimitiveType::QuadStrip => {
                self.cur_prim_verts.copy_within(2.., 0);
                self.cur_prim_verts.swap(0, 1);
                self.cur_prim_vert_index = PrimVertIndex::new(2);
            }
        }
    }

    fn clip_and_submit_polygon(&mut self) {
        // TODO:
        // - Check whether </> or <=/>= should be used for the frustum checks
        // - Check what happens for vertices where the divisor ends up being 0
        // - Maybe use the Cohen-Sutherland algorithm? It'd basically be the same but without
        //   grouping passes, and instead running until there are no points outside the frustum

        let mut clipped_verts_len = self.cur_prim_max_verts.get() as usize;

        let is_front_facing = vertex::front_facing(
            &self.cur_prim_verts[0],
            &self.cur_prim_verts[1],
            &self.cur_prim_verts[2],
        );
        let not_culled = if is_front_facing {
            self.cur_poly_attrs.show_front()
        } else {
            self.cur_poly_attrs.show_back()
        };
        if !not_culled {
            self.connect_to_last_strip_prim = false;
            return;
        }

        // If the last polygon wasn't clipped, then the shared vertices won't need clipping either
        let shared_verts = (self.connect_to_last_strip_prim as usize) << 1;

        macro_rules! interpolate {
            (
                $axis_i: expr,
                $output: expr,
                ($vert: expr, $coord: expr, $w: expr, $sign: expr),
                $other: expr,
                |$other_coord: ident, $other_w: ident|
                ($compare: expr, $numer: expr, $coord_diff: expr,),
            ) => {
                let other = $other;
                let $other_coord = other.coords[$axis_i] as i64;
                let $other_w = other.coords[3] as i64;
                if $compare {
                    // For the positive side of the frustum:
                    //          w0 - x0
                    // t = -----------------
                    //     x1 - x0 - w1 + w0
                    // for the negative side:
                    //          w0 + x0
                    // t = -----------------
                    //     x0 - x1 - w1 + w0
                    // Both can be summed up by:
                    //           w0 ∓ x0                  $numer
                    // t = --------------------- = ---------------------
                    //     ±(x1 - x0) - w1 + w0    $coord_diff - w1 + w0
                    let denom = $coord_diff + $w - $other_w;
                    #[allow(clippy::neg_multiply)]
                    if denom != 0 {
                        let mut vert = $vert.interpolate($other, $numer, denom);
                        vert.coords[$axis_i] = $sign * vert.coords[3];
                        $output[clipped_verts_len] = vert;
                        clipped_verts_len += 1;
                    }
                }
            };
        }

        macro_rules! run_pass {
            ($axis_i: expr, $clip_far: expr, $input: expr => $output: expr) => {
                let input_len = replace(&mut clipped_verts_len, shared_verts);
                for (i, vert) in $input[..input_len].iter().enumerate().skip(shared_verts) {
                    let coord = vert.coords[$axis_i] as i64;
                    let w = vert.coords[3] as i64;
                    if coord > w {
                        if !$clip_far {
                            return;
                        }
                        self.connect_to_last_strip_prim = false;
                        interpolate!(
                            $axis_i,
                            $output,
                            (vert, coord, w, 1),
                            &$input[if i == 0 { input_len - 1 } else { i - 1 }],
                            |other_coord, other_w| (
                                other_coord <= other_w,
                                w - coord,
                                other_coord - coord,
                            ),
                        );
                        interpolate!(
                            $axis_i,
                            $output,
                            (vert, coord, w, 1),
                            &$input[if i + 1 == input_len { 0 } else { i + 1 }],
                            |other_coord, other_w| (
                                other_coord <= other_w,
                                w - coord,
                                other_coord - coord,
                            ),
                        );
                    } else if coord < -w {
                        self.connect_to_last_strip_prim = false;
                        interpolate!(
                            $axis_i,
                            $output,
                            (vert, coord, w, -1),
                            &$input[if i == 0 { input_len - 1 } else { i - 1 }],
                            |other_coord, other_w| (
                                other_coord >= -other_w,
                                w + coord,
                                coord - other_coord,
                            ),
                        );
                        interpolate!(
                            $axis_i,
                            $output,
                            (vert, coord, w, -1),
                            &$input[if i + 1 == input_len { 0 } else { i + 1 }],
                            |other_coord, other_w| (
                                other_coord >= -other_w,
                                w + coord,
                                coord - other_coord,
                            ),
                        );
                    } else {
                        $output[clipped_verts_len] = *vert;
                        clipped_verts_len += 1;
                    }
                }
                if clipped_verts_len == 0 {
                    return;
                }
            };
        }

        let connect_to_last_strip_prim = replace(
            &mut self.connect_to_last_strip_prim,
            matches!(
                self.cur_prim_type,
                PrimitiveType::TriangleStrip | PrimitiveType::QuadStrip
            ),
        );
        let [mut buffer_0, mut buffer_1] = [[Vertex::new(); 10]; 2];
        buffer_0[..shared_verts].copy_from_slice(&self.cur_prim_verts[..shared_verts]);
        buffer_1[..shared_verts].copy_from_slice(&self.cur_prim_verts[..shared_verts]);
        run_pass!(2, self.cur_poly_attrs.clip_far_plane(), self.cur_prim_verts => buffer_0);
        run_pass!(1, true, buffer_0 => buffer_1);
        run_pass!(0, true, buffer_1 => buffer_0);

        if self.vert_ram_level as usize > self.vert_ram.len() - (clipped_verts_len - shared_verts) {
            self.rendering_state
                .control
                .set_poly_vert_ram_overflow(true);
            return;
        }

        let mut poly = &mut self.poly_ram[self.poly_ram_level as usize];
        self.poly_ram_level += 1;
        poly.vertices_len = PolyVertsLen::new(clipped_verts_len as u8);
        poly.tex_palette_base = self.tex_palette_base;
        poly.tex_params = self.tex_params;
        poly.attrs = self.cur_poly_attrs;
        poly.is_front_facing = is_front_facing;
        poly.is_translucent = matches!(poly.attrs.alpha(), 1..=30)
            || (matches!(poly.attrs.mode(), 0 | 2) && matches!(poly.tex_params.format(), 1 | 6));

        if connect_to_last_strip_prim {
            poly.vertices[..2].copy_from_slice(&self.last_strip_prim_vert_indices);
        }

        let mut top_y = 0xFF;
        let mut bot_y = 0;

        let viewport_origin = ConversionScreenCoords::from_array([
            self.viewport[0] as i64,
            191_u8.wrapping_sub(self.viewport[3]) as i64,
        ]);
        let viewport_size = ConversionScreenCoords::from_array([
            (self.viewport[2] as i64 - self.viewport[0] as i64 + 1) & 0x1FF,
            self.viewport[3]
                .wrapping_sub(self.viewport[1])
                .wrapping_add(1) as i64,
        ]);

        for (vert, vert_addr) in buffer_0[shared_verts..clipped_verts_len]
            .iter_mut()
            .zip(&mut poly.vertices[shared_verts..clipped_verts_len])
        {
            vert.coords[3] &= 0x00FF_FFFF;
            let w = vert.coords[3];
            let coords = if w == 0 {
                // TODO: What should actually happen for W == 0?
                ScreenCoords::splat(0)
            } else {
                let w_2 = ConversionScreenCoords::splat(w as i64);
                ((ConversionScreenCoords::from_array([
                    vert.coords[0] as i64,
                    -vert.coords[1] as i64,
                ]) + w_2)
                    * viewport_size
                    / (w_2 << ConversionScreenCoords::splat(1))
                    + viewport_origin)
                    .cast::<u16>()
                    & ScreenCoords::from_array([0x1FF, 0xFF])
            };
            let y = coords[1] as u8;
            top_y = top_y.min(y);
            bot_y = bot_y.max(y);
            self.vert_ram[self.vert_ram_level as usize] = ScreenVertex {
                coords,
                uv: vert.uv,
                color: vert.color.cast() << InterpColor::splat(3)
                    | vert.color.cast() >> InterpColor::splat(3),
            };
            *vert_addr = VertexAddr::new(self.vert_ram_level);
            self.vert_ram_level += 1;
        }

        for &vert_addr in &poly.vertices[..shared_verts] {
            let y = self.vert_ram[vert_addr.get() as usize].coords[1] as u8;
            top_y = top_y.min(y);
            bot_y = bot_y.max(y);
        }

        poly.top_y = top_y;
        poly.bot_y = bot_y;

        let mut leading_zeros = 32;

        for (i, vert) in buffer_0[..clipped_verts_len].iter().enumerate() {
            let w = vert.coords[3];
            leading_zeros = leading_zeros.min(w.leading_zeros());
            poly.depth_values[i] = if self.swap_buffers_attrs.w_buffering() {
                w
            } else if w != 0 {
                (((((vert.coords[2] as i64) << 14) / w as i64) + 0x3FFF) << 9) as i32 & 0xFF_FFFF
            } else {
                // TODO: What should this value be? This is using 0 as (z << 14) / w
                0x7F_FE00
            };
        }

        leading_zeros &= !3;
        if leading_zeros >= 16 {
            let shift = leading_zeros - 16;
            for (i, vert) in buffer_0[..clipped_verts_len].iter().enumerate() {
                poly.w_values[i] = (vert.coords[3] << shift) as u16;
            }
        } else {
            let shift = 16 - leading_zeros;
            for (i, vert) in buffer_0[..clipped_verts_len].iter().enumerate() {
                poly.w_values[i] = (vert.coords[3] >> shift) as u16;
            }
        }

        if self.connect_to_last_strip_prim {
            match self.cur_prim_type {
                PrimitiveType::TriangleStrip => {
                    self.last_strip_prim_vert_indices = if self.cur_strip_prim_is_odd {
                        [poly.vertices[0], poly.vertices[2]]
                    } else {
                        [poly.vertices[2], poly.vertices[1]]
                    };
                }

                PrimitiveType::QuadStrip => {
                    self.last_strip_prim_vert_indices = [poly.vertices[3], poly.vertices[2]];
                }

                _ => {}
            }
        }
    }

    pub(super) fn swap_buffers_waiting(&self) -> bool {
        self.command_finish_time.0 == RawTimestamp::MAX
    }

    pub(super) fn swap_buffers_missed(&mut self, vram: &Vram) {
        if self.gx_enabled && self.rendering_enabled {
            unsafe {
                self.renderer.repeat_last_frame(
                    &*vram.texture.as_bytes_ptr(),
                    &*vram.tex_pal.as_bytes_ptr(),
                    &self.rendering_state,
                );
            }
            self.rendering_state.texture_dirty = 0;
            self.rendering_state.tex_pal_dirty = 0;
        }
    }

    pub(super) fn swap_buffers(emu: &mut Emu<impl cpu::Engine>) {
        if emu.gpu.engine_3d.rendering_enabled {
            // According to melonDS, the sort order is determined by these things, in order of
            // decreasing priority:
            // - Being translucent/opaque (opaque polygons always come first, GBATEK says this too)
            // - Bottom Y (lower first)
            // - Top Y (lower first)
            // - Submit order (thus needing a stable sort)
            if emu
                .gpu
                .engine_3d
                .swap_buffers_attrs
                .translucent_auto_sort_disabled()
            {
                emu.gpu.engine_3d.poly_ram[..emu.gpu.engine_3d.poly_ram_level as usize]
                    .sort_by_key(|poly| {
                        if poly.is_translucent {
                            0x1_0000
                        } else {
                            (poly.bot_y as u32) << 8 | poly.top_y as u32
                        }
                    });
            } else {
                emu.gpu.engine_3d.poly_ram[..emu.gpu.engine_3d.poly_ram_level as usize]
                    .sort_by_key(|poly| {
                        (poly.is_translucent as u32) << 16
                            | (poly.bot_y as u32) << 8
                            | poly.top_y as u32
                    });
            }
            unsafe {
                emu.gpu.engine_3d.renderer.swap_buffers(
                    &*emu.gpu.vram.texture.as_bytes_ptr(),
                    &*emu.gpu.vram.tex_pal.as_bytes_ptr(),
                    &emu.gpu.engine_3d.vert_ram[..emu.gpu.engine_3d.vert_ram_level as usize],
                    &emu.gpu.engine_3d.poly_ram[..emu.gpu.engine_3d.poly_ram_level as usize],
                    &emu.gpu.engine_3d.rendering_state,
                    emu.gpu.engine_3d.swap_buffers_attrs.w_buffering(),
                );
            }
            emu.gpu.engine_3d.rendering_state.texture_dirty = 0;
            emu.gpu.engine_3d.rendering_state.tex_pal_dirty = 0;
        }
        emu.gpu.engine_3d.vert_ram_level = 0;
        emu.gpu.engine_3d.poly_ram_level = 0;
        Self::process_next_command(emu);
    }

    pub(crate) fn process_next_command(emu: &mut Emu<impl cpu::Engine>) {
        loop {
            if emu.gpu.engine_3d.gx_pipe.is_empty() {
                break;
            }

            macro_rules! read_from_gx_pipe {
                () => {
                    emu.gpu.engine_3d.gx_pipe.read_unchecked()
                };
                (
                    $len: literal,
                    $iter: expr,
                    |$elem_ident: ident, $entry_ident: ident| $f: expr
                ) => {
                    let mut iter = $iter.into_iter();
                    let pipe_len = emu.gpu.engine_3d.gx_pipe.len();
                    if pipe_len >= $len {
                        for $elem_ident in iter {
                            let $entry_ident = emu.gpu.engine_3d.gx_pipe.read_unchecked();
                            $f
                        }
                    } else {
                        for $elem_ident in Iterator::take(&mut iter, pipe_len) {
                            let $entry_ident = emu.gpu.engine_3d.gx_pipe.read_unchecked();
                            $f
                        }
                        for $elem_ident in iter {
                            let $entry_ident = emu.gpu.engine_3d.gx_fifo.read_unchecked();
                            $f
                        }
                    }
                };
            }

            let FifoEntry {
                command,
                param: first_param,
            } = unsafe { emu.gpu.engine_3d.gx_pipe.peek_unchecked() };

            if command == 0 {
                unsafe {
                    read_from_gx_pipe!();
                }
                emu.gpu.engine_3d.refill_gx_pipe(&mut emu.arm9, 0);
                continue;
            }

            let params = emu.gpu.engine_3d.params_for_command(command);

            if emu.gpu.engine_3d.gx_pipe.len() + emu.gpu.engine_3d.gx_fifo.len() < params as usize {
                break;
            }

            emu.gpu.engine_3d.gx_status.set_busy(true);
            let prev_gx_pipe_len = emu.gpu.engine_3d.gx_pipe.len();

            unsafe {
                read_from_gx_pipe!();
            }

            macro_rules! dequeue_mtx_stack_cmd {
                () => {
                    emu.gpu.engine_3d.queued_mtx_stack_cmds -= 1;
                    if emu.gpu.engine_3d.queued_mtx_stack_cmds == 0 {
                        emu.gpu.engine_3d.gx_status.set_matrix_stack_busy(false);
                    }
                };
            }

            macro_rules! dequeue_test_cmd_entries {
                ($num: expr) => {
                    emu.gpu.engine_3d.queued_test_cmd_entries -= $num;
                    if emu.gpu.engine_3d.queued_test_cmd_entries == 0 {
                        emu.gpu.engine_3d.gx_status.set_test_busy(false);
                    }
                };
            }

            #[allow(clippy::match_same_arms)]
            match command {
                0x10 => {
                    // MTX_MODE
                    emu.gpu.engine_3d.mtx_mode = unsafe { transmute(first_param as u8 & 3) };
                }

                0x11 => {
                    // MTX_PUSH
                    match emu.gpu.engine_3d.mtx_mode {
                        MatrixMode::Projection => {
                            if emu.gpu.engine_3d.proj_stack_pointer {
                                emu.gpu.engine_3d.gx_status.set_matrix_stack_overflow(true);
                            }
                            emu.gpu.engine_3d.proj_stack = emu.gpu.engine_3d.cur_proj_mtx;
                            emu.gpu.engine_3d.proj_stack_pointer = true;
                        }

                        MatrixMode::Position | MatrixMode::PositionVector => {
                            if emu.gpu.engine_3d.pos_vec_stack_pointer >= 31 {
                                emu.gpu.engine_3d.gx_status.set_matrix_stack_overflow(true);
                            }
                            emu.gpu.engine_3d.pos_vec_stack
                                [(emu.gpu.engine_3d.pos_vec_stack_pointer & 31) as usize] =
                                emu.gpu.engine_3d.cur_pos_vec_mtxs;
                            emu.gpu.engine_3d.pos_vec_stack_pointer =
                                (emu.gpu.engine_3d.pos_vec_stack_pointer + 1).min(63);
                        }

                        MatrixMode::Texture => {
                            emu.gpu.engine_3d.tex_stack = emu.gpu.engine_3d.cur_tex_mtx;
                        }
                    }
                    dequeue_mtx_stack_cmd!();
                }

                0x12 => {
                    // MTX_POP
                    match emu.gpu.engine_3d.mtx_mode {
                        MatrixMode::Projection => {
                            emu.gpu.engine_3d.proj_stack_pointer = false;
                            emu.gpu.engine_3d.cur_proj_mtx = emu.gpu.engine_3d.proj_stack;
                            emu.gpu.engine_3d.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::Position | MatrixMode::PositionVector => {
                            emu.gpu.engine_3d.pos_vec_stack_pointer =
                                (emu.gpu.engine_3d.pos_vec_stack_pointer as i8
                                    - ((first_param as i8) << 2 >> 2))
                                    .clamp(0, 63) as u8;
                            if emu.gpu.engine_3d.pos_vec_stack_pointer >= 31 {
                                emu.gpu.engine_3d.gx_status.set_matrix_stack_overflow(true);
                            }
                            emu.gpu.engine_3d.cur_pos_vec_mtxs = emu.gpu.engine_3d.pos_vec_stack
                                [(emu.gpu.engine_3d.pos_vec_stack_pointer & 31) as usize];
                            emu.gpu.engine_3d.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::Texture => {
                            emu.gpu.engine_3d.cur_tex_mtx = emu.gpu.engine_3d.tex_stack;
                        }
                    }
                    dequeue_mtx_stack_cmd!();
                }

                0x13 => {
                    // MTX_STORE
                    match emu.gpu.engine_3d.mtx_mode {
                        MatrixMode::Projection => {
                            emu.gpu.engine_3d.proj_stack = emu.gpu.engine_3d.cur_proj_mtx;
                        }

                        MatrixMode::Position | MatrixMode::PositionVector => {
                            let addr = first_param as u8 & 31;
                            if addr == 31 {
                                emu.gpu.engine_3d.gx_status.set_matrix_stack_overflow(true);
                            }
                            emu.gpu.engine_3d.pos_vec_stack[addr as usize] =
                                emu.gpu.engine_3d.cur_pos_vec_mtxs;
                        }

                        MatrixMode::Texture => {
                            emu.gpu.engine_3d.tex_stack = emu.gpu.engine_3d.cur_tex_mtx;
                        }
                    }
                }

                0x14 => {
                    // MTX_RESTORE
                    match emu.gpu.engine_3d.mtx_mode {
                        MatrixMode::Projection => {
                            emu.gpu.engine_3d.cur_proj_mtx = emu.gpu.engine_3d.proj_stack;
                            emu.gpu.engine_3d.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::Position | MatrixMode::PositionVector => {
                            let addr = first_param as u8 & 31;
                            if addr == 31 {
                                emu.gpu.engine_3d.gx_status.set_matrix_stack_overflow(true);
                            }
                            emu.gpu.engine_3d.cur_pos_vec_mtxs =
                                emu.gpu.engine_3d.pos_vec_stack[addr as usize];
                            emu.gpu.engine_3d.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::Texture => {
                            emu.gpu.engine_3d.cur_tex_mtx = emu.gpu.engine_3d.tex_stack;
                        }
                    }
                }

                0x15 => {
                    // MTX_IDENTITY
                    match emu.gpu.engine_3d.mtx_mode {
                        MatrixMode::Projection => {
                            emu.gpu.engine_3d.cur_proj_mtx = Matrix::identity();
                            emu.gpu.engine_3d.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::Position => {
                            emu.gpu.engine_3d.cur_pos_vec_mtxs[0] = Matrix::identity();
                            emu.gpu.engine_3d.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::PositionVector => {
                            emu.gpu.engine_3d.cur_pos_vec_mtxs[0] = Matrix::identity();
                            emu.gpu.engine_3d.cur_pos_vec_mtxs[1] = Matrix::identity();
                            emu.gpu.engine_3d.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::Texture => emu.gpu.engine_3d.cur_tex_mtx = Matrix::identity(),
                    }
                }

                0x16 => {
                    // MTX_LOAD_4x4
                    let mut contents = MatrixBuffer([0; 16]);
                    contents.0[0] = first_param as i32;
                    unsafe {
                        read_from_gx_pipe!(15, &mut contents.0[1..], |elem, entry| *elem =
                            entry.param as i32);
                    }
                    emu.gpu.engine_3d.load_matrix(Matrix::new(contents));
                }

                0x17 => {
                    // MTX_LOAD_4x3
                    let mut contents = MatrixBuffer([0; 16]);
                    contents.0[0] = first_param as i32;
                    contents.0[15] = 0x1000;
                    unsafe {
                        read_from_gx_pipe!(
                            11,
                            [1, 2, 4, 5, 6, 8, 9, 10, 12, 13, 14],
                            |i, entry| contents.0[i] = entry.param as i32
                        );
                    }
                    emu.gpu.engine_3d.load_matrix(Matrix::new(contents));
                }

                0x18 => {
                    // MTX_MULT_4x4
                    let mut contents = MatrixBuffer([0; 16]);
                    contents.0[0] = first_param as i32;
                    unsafe {
                        read_from_gx_pipe!(15, &mut contents.0[1..], |elem, entry| *elem =
                            entry.param as i32);
                    }

                    match emu.gpu.engine_3d.mtx_mode {
                        MatrixMode::Projection => {
                            emu.gpu.engine_3d.cur_proj_mtx.mul_left_4x4(contents);
                            emu.gpu.engine_3d.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::Position => {
                            emu.gpu.engine_3d.cur_pos_vec_mtxs[0].mul_left_4x4(contents);
                            emu.gpu.engine_3d.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::PositionVector => {
                            emu.gpu.engine_3d.cur_pos_vec_mtxs[0].mul_left_4x4(contents);
                            emu.gpu.engine_3d.cur_pos_vec_mtxs[1].mul_left_4x4(contents);
                            emu.gpu.engine_3d.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::Texture => emu.gpu.engine_3d.cur_tex_mtx.mul_left_4x4(contents),
                    }
                }

                0x19 => {
                    // MTX_MULT_4x3
                    let mut contents = MatrixBuffer([0; 12]);
                    contents.0[0] = first_param as i32;
                    unsafe {
                        read_from_gx_pipe!(11, &mut contents.0[1..], |elem, entry| *elem =
                            entry.param as i32);
                    }

                    match emu.gpu.engine_3d.mtx_mode {
                        MatrixMode::Projection => {
                            emu.gpu.engine_3d.cur_proj_mtx.mul_left_4x3(contents);
                            emu.gpu.engine_3d.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::Position => {
                            emu.gpu.engine_3d.cur_pos_vec_mtxs[0].mul_left_4x3(contents);
                            emu.gpu.engine_3d.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::PositionVector => {
                            emu.gpu.engine_3d.cur_pos_vec_mtxs[0].mul_left_4x3(contents);
                            emu.gpu.engine_3d.cur_pos_vec_mtxs[1].mul_left_4x3(contents);
                            emu.gpu.engine_3d.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::Texture => emu.gpu.engine_3d.cur_tex_mtx.mul_left_4x3(contents),
                    }
                }

                0x1A => {
                    // MTX_MULT_3x3
                    let mut contents = MatrixBuffer([0; 9]);
                    contents.0[0] = first_param as i32;
                    unsafe {
                        read_from_gx_pipe!(8, &mut contents.0[1..], |elem, entry| *elem =
                            entry.param as i32);
                    }

                    match emu.gpu.engine_3d.mtx_mode {
                        MatrixMode::Projection => {
                            emu.gpu.engine_3d.cur_proj_mtx.mul_left_3x3(contents);
                            emu.gpu.engine_3d.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::Position => {
                            emu.gpu.engine_3d.cur_pos_vec_mtxs[0].mul_left_3x3(contents);
                            emu.gpu.engine_3d.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::PositionVector => {
                            emu.gpu.engine_3d.cur_pos_vec_mtxs[0].mul_left_3x3(contents);
                            emu.gpu.engine_3d.cur_pos_vec_mtxs[1].mul_left_3x3(contents);
                            emu.gpu.engine_3d.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::Texture => emu.gpu.engine_3d.cur_tex_mtx.mul_left_3x3(contents),
                    }
                }

                0x1B => {
                    // MTX_SCALE
                    let mut contents = [first_param as i32, 0, 0];
                    unsafe {
                        read_from_gx_pipe!(2, &mut contents[1..], |elem, entry| *elem =
                            entry.param as i32);
                    }

                    match emu.gpu.engine_3d.mtx_mode {
                        MatrixMode::Projection => {
                            emu.gpu.engine_3d.cur_proj_mtx.scale(contents);
                            emu.gpu.engine_3d.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::Position | MatrixMode::PositionVector => {
                            emu.gpu.engine_3d.cur_pos_vec_mtxs[0].scale(contents);
                            emu.gpu.engine_3d.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::Texture => emu.gpu.engine_3d.cur_tex_mtx.scale(contents),
                    }
                }
                0x1C => {
                    // MTX_TRANS
                    let mut contents = [first_param as i32, 0, 0];
                    unsafe {
                        read_from_gx_pipe!(2, &mut contents[1..], |elem, entry| *elem =
                            entry.param as i32);
                    }

                    match emu.gpu.engine_3d.mtx_mode {
                        MatrixMode::Projection => {
                            emu.gpu.engine_3d.cur_proj_mtx.translate(contents);
                            emu.gpu.engine_3d.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::Position => {
                            emu.gpu.engine_3d.cur_pos_vec_mtxs[0].translate(contents);
                            emu.gpu.engine_3d.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::PositionVector => {
                            emu.gpu.engine_3d.cur_pos_vec_mtxs[0].translate(contents);
                            emu.gpu.engine_3d.cur_pos_vec_mtxs[1].translate(contents);
                            emu.gpu.engine_3d.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::Texture => emu.gpu.engine_3d.cur_tex_mtx.translate(contents),
                    }
                }

                0x20 => {
                    // COLOR
                    emu.gpu.engine_3d.vert_color = rgb_5_to_6(decode_rgb_5(first_param as u16, 0));
                }

                0x21 => {
                    // NORMAL
                    emu.gpu.engine_3d.vert_normal = [
                        (first_param as i16) << 6 >> 6,
                        (first_param >> 4) as i16 >> 6,
                        (first_param >> 14) as i16 >> 6,
                    ];

                    if emu.gpu.engine_3d.tex_params.coord_transform_mode() == 2 {
                        let [u, v, ..] = emu
                            .gpu
                            .engine_3d
                            .cur_tex_mtx
                            .mul_left_vec3_zero::<i16, i16, 21>(emu.gpu.engine_3d.vert_normal)
                            .to_array();
                        emu.gpu.engine_3d.transformed_tex_coords =
                            emu.gpu.engine_3d.tex_coords + TexCoords::from_array([u, v]);
                    }

                    emu.gpu.engine_3d.apply_lighting();
                }

                0x22 => {
                    // TEXCOORD
                    emu.gpu.engine_3d.tex_coords =
                        TexCoords::from_array([first_param as i16, (first_param >> 16) as i16]);

                    match emu.gpu.engine_3d.tex_params.coord_transform_mode() {
                        0 => {
                            emu.gpu.engine_3d.transformed_tex_coords = emu.gpu.engine_3d.tex_coords;
                        }
                        1 => {
                            let [u, v, ..] = emu
                                .gpu
                                .engine_3d
                                .cur_tex_mtx
                                .mul_left_vec2_one_one::<i16, i16>(emu.gpu.engine_3d.tex_coords)
                                .to_array();
                            emu.gpu.engine_3d.transformed_tex_coords =
                                TexCoords::from_array([u, v]);
                        }
                        _ => {}
                    }
                }

                0x23 => {
                    // VTX_16
                    let second_param = unsafe { read_from_gx_pipe!() }.param;
                    emu.gpu.engine_3d.add_vert([
                        first_param as i16,
                        (first_param >> 16) as i16,
                        second_param as i16,
                    ]);
                }

                0x24 => {
                    // VTX_10
                    emu.gpu.engine_3d.add_vert([
                        (first_param as i16) << 6,
                        ((first_param >> 10) as i16) << 6,
                        ((first_param >> 20) as i16) << 6,
                    ]);
                }
                0x25 => {
                    // VTX_XY
                    emu.gpu.engine_3d.add_vert([
                        first_param as i16,
                        (first_param >> 16) as i16,
                        emu.gpu.engine_3d.last_vtx_coords[2],
                    ]);
                }
                0x26 => {
                    // VTX_XZ
                    emu.gpu.engine_3d.add_vert([
                        first_param as i16,
                        emu.gpu.engine_3d.last_vtx_coords[1],
                        (first_param >> 16) as i16,
                    ]);
                }

                0x27 => {
                    // VTX_YZ
                    emu.gpu.engine_3d.add_vert([
                        emu.gpu.engine_3d.last_vtx_coords[0],
                        first_param as i16,
                        (first_param >> 16) as i16,
                    ]);
                }

                0x28 => {
                    // VTX_DIFF
                    emu.gpu.engine_3d.add_vert([
                        emu.gpu.engine_3d.last_vtx_coords[0]
                            .wrapping_add((first_param as i16) << 6 >> 6),
                        emu.gpu.engine_3d.last_vtx_coords[1]
                            .wrapping_add((first_param >> 4) as i16 >> 6),
                        emu.gpu.engine_3d.last_vtx_coords[2]
                            .wrapping_add((first_param >> 14) as i16 >> 6),
                    ]);
                }

                0x29 => {
                    // POLYGON_ATTR
                    emu.gpu.engine_3d.next_poly_attrs = PolygonAttrs(first_param);
                }

                0x2A => {
                    // TEXIMAGE_PARAM
                    emu.gpu.engine_3d.tex_params = TextureParams(first_param);
                }

                0x2B => {
                    // PLTT_BASE
                    emu.gpu.engine_3d.tex_palette_base = first_param as u16 & 0x1FFF;
                }

                0x30 => {
                    // DIF_AMB
                    let diffuse_color = decode_rgb_5(first_param as u16, 0);
                    emu.gpu.engine_3d.diffuse_color = diffuse_color.cast();
                    emu.gpu.engine_3d.ambient_color =
                        decode_rgb_5((first_param >> 16) as u16, 0).cast();
                    if first_param & 1 << 15 != 0 {
                        emu.gpu.engine_3d.vert_color = rgb_5_to_6(diffuse_color);
                    }
                }

                0x31 => {
                    // SPE_EMI
                    emu.gpu.engine_3d.specular_color = decode_rgb_5(first_param as u16, 0).cast();
                    emu.gpu.engine_3d.emission_color =
                        decode_rgb_5((first_param >> 16) as u16, 0).cast();
                    emu.gpu.engine_3d.shininess_table_enabled = first_param & 1 << 15 != 0;
                }

                0x32 => {
                    // LIGHT_VECTOR
                    let transformed = emu.gpu.engine_3d.cur_pos_vec_mtxs[1]
                        .mul_left_vec3_zero::<i16, i16, 12>([
                            (first_param as i16) << 6 >> 6,
                            (first_param >> 4) as i16 >> 6,
                            (first_param >> 14) as i16 >> 6,
                        ])
                        .to_array();
                    let light = &mut emu.gpu.engine_3d.lights[(first_param >> 30) as usize];
                    light.direction = [transformed[0], transformed[1], transformed[2]];
                    light.half_vec = [
                        transformed[0] >> 1,
                        transformed[1] >> 1,
                        (transformed[2] - 0x200) >> 1,
                    ];
                }

                0x33 => {
                    // LIGHT_COLOR
                    emu.gpu.engine_3d.lights[(first_param >> 30) as usize].color =
                        decode_rgb_5(first_param as u16, 0).cast();
                }

                0x34 => {
                    // SHININESS
                    emu.gpu.engine_3d.shininess_table[0] = first_param as u8;
                    emu.gpu.engine_3d.shininess_table[1] = (first_param >> 8) as u8;
                    emu.gpu.engine_3d.shininess_table[2] = (first_param >> 16) as u8;
                    emu.gpu.engine_3d.shininess_table[3] = (first_param >> 24) as u8;
                    unsafe {
                        read_from_gx_pipe!(31, (4..128).step_by(4), |i, entry| {
                            emu.gpu.engine_3d.shininess_table[i] = entry.param as u8;
                            emu.gpu.engine_3d.shininess_table[i + 1] = (entry.param >> 8) as u8;
                            emu.gpu.engine_3d.shininess_table[i + 2] = (entry.param >> 16) as u8;
                            emu.gpu.engine_3d.shininess_table[i + 3] = (entry.param >> 24) as u8;
                        });
                    }
                }

                0x40 => {
                    // BEGIN_VTXS
                    emu.gpu.engine_3d.cur_poly_attrs = emu.gpu.engine_3d.next_poly_attrs;
                    emu.gpu.engine_3d.cur_prim_type = unsafe { transmute(first_param as u8 & 3) };
                    emu.gpu.engine_3d.cur_prim_vert_index = PrimVertIndex::new(0);
                    emu.gpu.engine_3d.cur_prim_max_verts = match emu.gpu.engine_3d.cur_prim_type {
                        PrimitiveType::Triangles | PrimitiveType::TriangleStrip => {
                            PrimMaxVerts::new(3)
                        }
                        PrimitiveType::Quads | PrimitiveType::QuadStrip => PrimMaxVerts::new(4),
                    };
                    emu.gpu.engine_3d.cur_strip_prim_is_odd = false;
                    emu.gpu.engine_3d.connect_to_last_strip_prim = false;
                }

                0x41 => {
                    // END_VTXS
                    // Should do nothing according to GBATEK
                }

                0x50 => {
                    // SWAP_BUFFERS
                    emu.gpu.engine_3d.swap_buffers_attrs = SwapBuffersAttrs(first_param as u8);
                    // Gets unlocked by the GPU when VBlank starts
                    emu.gpu.engine_3d.command_finish_time.0 = RawTimestamp::MAX;
                    return;
                }

                0x60 => {
                    // VIEWPORT
                    for i in 0..4 {
                        emu.gpu.engine_3d.viewport[i] = (first_param >> (i << 3)) as u8;
                    }
                }

                0x70 => {
                    // TODO: BOX_TEST
                    emu.gpu.engine_3d.gx_status.set_box_test_result(true);
                    for _ in 1..3 {
                        unsafe {
                            read_from_gx_pipe!();
                        }
                    }
                    dequeue_test_cmd_entries!(3);
                }

                0x71 => {
                    // TODO: POS_TEST
                    for _ in 1..2 {
                        unsafe {
                            read_from_gx_pipe!();
                        }
                    }
                    dequeue_test_cmd_entries!(2);
                }

                0x72 => {
                    // TODO: VEC_TEST
                    dequeue_test_cmd_entries!(1);
                }

                _ => {}
            }

            emu.gpu.engine_3d.refill_gx_pipe(
                &mut emu.arm9,
                (prev_gx_pipe_len ^ params.max(1) as usize) & 1,
            );

            emu.gpu.engine_3d.command_finish_time.0 =
                emu::Timestamp::from(arm9::Timestamp(emu.arm9.schedule.cur_time().0 + 1)).0 + 10;
            emu.gpu.engine_3d.gx_fifo_stalled &= emu.gpu.engine_3d.gx_fifo.len() > 256;
            if emu.gpu.engine_3d.gx_fifo_stalled() {
                emu.schedule.schedule_event(
                    emu::event_slots::ENGINE_3D,
                    emu.gpu.engine_3d.command_finish_time,
                );
            } else {
                emu.arm9.schedule.schedule_event(
                    arm9::event_slots::ENGINE_3D,
                    emu.gpu.engine_3d.command_finish_time.into(),
                );
            }
            return;
        }

        emu.gpu.engine_3d.gx_status.set_busy(false);
        emu.gpu.engine_3d.command_finish_time.0 = 0;
    }
}
