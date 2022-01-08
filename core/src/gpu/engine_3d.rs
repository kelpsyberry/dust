mod io;
mod matrix;

use crate::{
    cpu::{
        self,
        arm9::{self, Arm9},
        Schedule,
    },
    emu,
    utils::{bitfield_debug, Fifo},
};
use core::mem::transmute;
use matrix::{Matrix, MatrixBuffer};

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct RenderingControl(pub u16) {
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
        pub poly_vert_ram_underflow: bool @ 13,
        pub rear_plane_bitmap_enabled: bool @ 14,
    }
}

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct GxStatus(pub u32) {
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

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct PolyVertRamLevel(pub u32) {
        pub poly_ram_level: u16 @ 0..=11,
        pub vert_ram_level: u16 @ 16..=28,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(align(8))]
struct FifoEntry {
    command: u8,
    param: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
#[allow(dead_code)] // Initialized through `transmute`
enum MatrixMode {
    Projection,
    Position,
    PositionVector,
    Texture,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Light {
    direction: [i16; 3],
    color: u16,
}

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct PolygonAttrs(pub u32) {
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

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct TextureParams(pub u32) {
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

pub struct Engine3d {
    #[cfg(feature = "log")]
    logger: slog::Logger,

    rendering_control: RenderingControl,

    gx_status: GxStatus,
    gx_fifo_irq_requested: bool,
    gx_fifo: Box<Fifo<FifoEntry, 260>>,
    gx_pipe: Fifo<FifoEntry, 4>,
    cur_packed_commands: u32,
    remaining_command_params: u8,
    command_finish_time: emu::Timestamp,

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

    vert_color: u16,
    tex_coords: [i16; 2],
    last_vtx_coords: [i16; 3],

    shininess_table_enabled: bool,
    diffuse_color: u16,
    ambient_color: u16,
    specular_color: u16,
    emission_color: u16,
    shininess_table: [u8; 128],
    lights: [Light; 4],

    // Latched on BEGIN_VTXS
    next_poly_attrs: PolygonAttrs,
    cur_poly_attrs: PolygonAttrs,
    // Latched on new completely separate polygons (not strips)
    next_tex_params: TextureParams,
    cur_tex_params: TextureParams,
    next_tex_palette_base: u16,
    cur_tex_palette_base: u16,
}

fn command_name(cmd: u8) -> &'static str {
    match cmd {
        0x00 => "NOP",
        0x10 => "MTX_MODE",
        0x11 => "MTX_PUSH",
        0x12 => "MTX_POP",
        0x13 => "MTX_STORE",
        0x14 => "MTX_RESTORE",
        0x15 => "MTX_IDENTITY",
        0x16 => "MTX_LOAD_4x4",
        0x17 => "MTX_LOAD_4x3",
        0x18 => "MTX_MULT_4x4",
        0x19 => "MTX_MULT_4x3",
        0x1A => "MTX_MULT_3x3",
        0x1B => "MTX_SCALE",
        0x1C => "MTX_TRANS",
        0x20 => "COLOR",
        0x21 => "NORMAL",
        0x22 => "TEXCOORD",
        0x23 => "VTX_16",
        0x24 => "VTX_10",
        0x25 => "VTX_XY",
        0x26 => "VTX_XZ",
        0x27 => "VTX_YZ",
        0x28 => "VTX_DIFF",
        0x29 => "POLYGON_ATTR",
        0x2A => "TEXIMAGE_PARAM",
        0x2B => "PLTT_BASE",
        0x30 => "DIF_AMB",
        0x31 => "SPE_EMI",
        0x32 => "LIGHT_VECTOR",
        0x33 => "LIGHT_COLOR",
        0x34 => "SHININESS",
        0x40 => "BEGIN_VTXS",
        0x41 => "END_VTXS",
        0x50 => "SWAP_BUFFERS",
        0x60 => "VIEWPORT",
        0x70 => "BOX_TEST",
        0x71 => "POS_TEST",
        0x72 => "VEC_TEST",
        _ => "Unknown",
    }
}

impl Engine3d {
    pub(super) fn new(
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

            rendering_control: RenderingControl(0),

            gx_status: GxStatus(0),
            gx_fifo_irq_requested: false,
            gx_fifo: Box::new(Fifo::new()),
            gx_pipe: Fifo::new(),
            cur_packed_commands: 0,
            remaining_command_params: 0,
            command_finish_time: emu::Timestamp(0),

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

            vert_color: 0,
            tex_coords: [0; 2],
            last_vtx_coords: [0; 3],
            shininess_table_enabled: false,
            diffuse_color: 0,
            ambient_color: 0,
            specular_color: 0,
            emission_color: 0,
            shininess_table: [0; 128],
            lights: [Light {
                direction: [0; 3],
                color: 0,
            }; 4],

            next_poly_attrs: PolygonAttrs(0),
            cur_poly_attrs: PolygonAttrs(0),
            next_tex_params: TextureParams(0),
            cur_tex_params: TextureParams(0),
            next_tex_palette_base: 0,
            cur_tex_palette_base: 0,
        }
    }

    #[inline]
    pub fn rendering_control(&self) -> RenderingControl {
        self.rendering_control
    }

    #[inline]
    pub fn write_rendering_control(&mut self, value: RenderingControl) {
        self.rendering_control.0 =
            (self.rendering_control.0 & 0x3000 & !value.0) | (value.0 & 0x4FFF);
    }

    #[inline]
    pub fn gx_fifo_stalled(&self) -> bool {
        self.gx_fifo.len() > 256
    }

    #[inline]
    pub fn gx_status(&self) -> GxStatus {
        self.gx_status
            .with_fifo_level(self.gx_fifo.len() as u16)
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
                .set_requested(arm9.irqs.requested().with_gx_fifo(true), &mut arm9.schedule);
        }
    }

    #[inline]
    pub fn write_gx_status(&mut self, value: GxStatus, arm9: &mut Arm9<impl cpu::Engine>) {
        self.gx_status.0 =
            (self.gx_status.0 & !0xC000_0000 & !(value.0 & 0x8000)) | (value.0 & 0xC000_0000);
        self.update_gx_fifo_irq(arm9);
    }

    #[inline]
    pub fn poly_vert_ram_level(&self) -> PolyVertRamLevel {
        // TODO
        PolyVertRamLevel(0)
            .with_poly_ram_level(123)
            .with_vert_ram_level(123)
    }

    #[inline]
    pub fn line_buffer_level(&self) -> u8 {
        // TODO
        46
    }

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

    pub(crate) fn gx_fifo_irq_requested(&self) -> bool {
        self.gx_fifo_irq_requested
    }

    pub(crate) fn gx_fifo_half_empty(&self) -> bool {
        self.gx_fifo.len() < 128
    }

    fn write_to_gx_fifo(
        &mut self,
        value: FifoEntry,
        arm9: &mut Arm9<impl cpu::Engine>,
        emu_schedule: &mut emu::Schedule,
    ) {
        if !self.gx_pipe.is_full() && self.gx_fifo.is_empty() {
            let _ = self.gx_pipe.write(value);
        } else {
            let _ = self.gx_fifo.write(value);
            match self.gx_status.fifo_irq_mode() {
                1 => self.gx_fifo_irq_requested = self.gx_fifo.len() < 128,
                2 => self.gx_fifo_irq_requested = false,
                _ => {}
            }
            if self.gx_fifo.len() == 257 {
                let cur_time = arm9.schedule.cur_time();
                if arm9::Timestamp::from(self.command_finish_time) > cur_time {
                    arm9.schedule.cancel_event(arm9::event_slots::ENGINE_3D);
                    arm9.schedule
                        .schedule_event(arm9::event_slots::GX_FIFO, cur_time);
                    emu_schedule
                        .schedule_event(emu::event_slots::ENGINE_3D, self.command_finish_time);
                }
                return;
            }
        }
        if self.command_finish_time.0 == 0 {
            self.process_next_command(arm9, emu_schedule);
        }
    }

    fn write_unpacked_command(
        &mut self,
        command: u8,
        param: u32,
        arm9: &mut Arm9<impl cpu::Engine>,
        emu_schedule: &mut emu::Schedule,
    ) {
        if self.remaining_command_params == 0 {
            self.remaining_command_params = self.params_for_command(command).saturating_sub(1);
        } else {
            self.remaining_command_params -= 1;
        }
        self.write_to_gx_fifo(FifoEntry { command, param }, arm9, emu_schedule);
    }

    fn write_packed_command(
        &mut self,
        value: u32,
        arm9: &mut Arm9<impl cpu::Engine>,
        emu_schedule: &mut emu::Schedule,
    ) {
        // TODO: "Packed commands are first decompressed and then stored in the command FIFO."
        if self.remaining_command_params == 0 {
            self.cur_packed_commands = value;
            let command = self.cur_packed_commands as u8;
            self.remaining_command_params = self.params_for_command(command);
            if self.remaining_command_params > 0 {
                return;
            }
            self.write_to_gx_fifo(FifoEntry { command, param: 0 }, arm9, emu_schedule);
        } else {
            let command = self.cur_packed_commands as u8;
            self.write_to_gx_fifo(
                FifoEntry {
                    command,
                    param: value,
                },
                arm9,
                emu_schedule,
            );
            self.remaining_command_params -= 1;
            if self.remaining_command_params > 0 {
                return;
            }
        }
        let mut cur_packed_commands = self.cur_packed_commands;
        loop {
            cur_packed_commands >>= 8;
            if cur_packed_commands == 0 {
                break;
            }
            let next_command = cur_packed_commands as u8;
            let next_command_params = self.params_for_command(next_command);
            if next_command_params > 0 {
                self.cur_packed_commands = cur_packed_commands;
                self.remaining_command_params = next_command_params;
                break;
            }
            self.write_to_gx_fifo(
                FifoEntry {
                    command: next_command,
                    param: 0,
                },
                arm9,
                emu_schedule,
            );
        }
    }

    unsafe fn read_from_gx_pipe(&mut self, arm9: &mut Arm9<impl cpu::Engine>) -> FifoEntry {
        let result = self.gx_pipe.read_unchecked();
        if self.gx_pipe.len() <= 2 {
            for _ in 0..2 {
                if let Some(entry) = self.gx_fifo.read() {
                    self.gx_pipe.write_unchecked(entry);
                    self.update_gx_fifo_irq(arm9);
                    if self.gx_fifo_half_empty() {
                        arm9.start_dma_transfers_with_timing::<{ arm9::dma::Timing::GxFifo }>();
                    }
                }
            }
        }
        result
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

    fn add_vertex(&mut self, coords: [i16; 3]) {
        self.last_vtx_coords = coords;
        if self.clip_mtx_needs_recalculation {
            self.update_clip_mtx();
        }
        println!(
            "Orig: {} {} {}",
            coords[0] as f32 / 4096.0,
            coords[1] as f32 / 4096.0,
            coords[2] as f32 / 4096.0
        );
        let coords = self.cur_clip_mtx.mul_left_vec_i16(coords).0;
        println!(
            "Transformed: {} {} {} {}",
            coords[0] as f32 / 4096.0,
            coords[1] as f32 / 4096.0,
            coords[2] as f32 / 4096.0,
            coords[3] as f32 / 4096.0,
        );
    }

    pub(crate) fn process_next_command(
        &mut self,
        arm9: &mut Arm9<impl cpu::Engine>,
        emu_schedule: &mut emu::Schedule,
    ) {
        loop {
            if self.gx_pipe.is_empty() {
                self.command_finish_time.0 = 0;
                return;
            }
            let FifoEntry {
                command,
                param: first_param,
            } = unsafe { self.gx_pipe.peek_unchecked() };
            if command == 0 {
                unsafe {
                    self.read_from_gx_pipe(arm9);
                }
                continue;
            }
            let params = self.params_for_command(command);
            if self.gx_pipe.len() + self.gx_fifo.len() < params as usize {
                self.command_finish_time.0 = 0;
                return;
            }
            unsafe {
                self.read_from_gx_pipe(arm9);
            }

            match command {
                0x10 => {
                    // MTX_MODE
                    self.mtx_mode = unsafe { transmute(first_param as u8 & 3) };
                }

                0x11 => {
                    // MTX_PUSH
                    match self.mtx_mode {
                        MatrixMode::Projection => {
                            if self.proj_stack_pointer {
                                self.gx_status.set_matrix_stack_overflow(true);
                            }
                            self.proj_stack = self.cur_proj_mtx;
                            self.proj_stack_pointer = true;
                        }

                        MatrixMode::Position | MatrixMode::PositionVector => {
                            if self.pos_vec_stack_pointer >= 31 {
                                self.gx_status.set_matrix_stack_overflow(true);
                            }
                            self.pos_vec_stack[(self.pos_vec_stack_pointer & 31) as usize] =
                                self.cur_pos_vec_mtxs;
                            self.pos_vec_stack_pointer = (self.pos_vec_stack_pointer + 1).min(63);
                        }

                        MatrixMode::Texture => self.tex_stack = self.cur_tex_mtx,
                    }
                }

                0x12 => {
                    // MTX_POP
                    match self.mtx_mode {
                        MatrixMode::Projection => {
                            self.proj_stack_pointer = false;
                            self.cur_proj_mtx = self.proj_stack;
                            self.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::Position | MatrixMode::PositionVector => {
                            self.pos_vec_stack_pointer = (self.pos_vec_stack_pointer as i8
                                - ((first_param as i8) << 2 >> 2))
                                .clamp(0, 63)
                                as u8;
                            if self.pos_vec_stack_pointer >= 31 {
                                self.gx_status.set_matrix_stack_overflow(true);
                            }
                            self.cur_pos_vec_mtxs =
                                self.pos_vec_stack[(self.pos_vec_stack_pointer & 31) as usize];
                            self.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::Texture => self.cur_tex_mtx = self.tex_stack,
                    }
                }

                0x13 => {
                    // MTX_STORE
                    match self.mtx_mode {
                        MatrixMode::Projection => self.proj_stack = self.cur_proj_mtx,

                        MatrixMode::Position | MatrixMode::PositionVector => {
                            let addr = first_param as u8 & 31;
                            if addr == 31 {
                                self.gx_status.set_matrix_stack_overflow(true);
                            }
                            self.pos_vec_stack[addr as usize] = self.cur_pos_vec_mtxs;
                        }

                        MatrixMode::Texture => self.tex_stack = self.cur_tex_mtx,
                    }
                }

                0x14 => {
                    // MTX_RESTORE
                    match self.mtx_mode {
                        MatrixMode::Projection => {
                            self.cur_proj_mtx = self.proj_stack;
                            self.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::Position | MatrixMode::PositionVector => {
                            let addr = first_param as u8 & 31;
                            if addr == 31 {
                                self.gx_status.set_matrix_stack_overflow(true);
                            }
                            self.cur_pos_vec_mtxs = self.pos_vec_stack[addr as usize];
                            self.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::Texture => self.cur_tex_mtx = self.tex_stack,
                    }
                }

                0x15 => {
                    // MTX_IDENTITY
                    match self.mtx_mode {
                        MatrixMode::Projection => {
                            self.cur_proj_mtx = Matrix::identity();
                            self.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::Position => {
                            self.cur_pos_vec_mtxs[0] = Matrix::identity();
                            self.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::PositionVector => {
                            self.cur_pos_vec_mtxs[0] = Matrix::identity();
                            self.cur_pos_vec_mtxs[1] = Matrix::identity();
                            self.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::Texture => self.cur_tex_mtx = Matrix::identity(),
                    }
                }

                0x16 => {
                    // MTX_LOAD_4x4
                    let mut contents = [0; 16];
                    contents[0] = first_param as i32;
                    for elem in &mut contents[1..] {
                        *elem = unsafe { self.read_from_gx_pipe(arm9).param as i32 };
                    }
                    self.load_matrix(Matrix::new(contents));
                }

                0x17 => {
                    // MTX_LOAD_4x3
                    let mut contents = [0; 16];
                    contents[0] = first_param as i32;
                    contents[15] = 0x1000;
                    for elem in &mut contents[1..3] {
                        *elem = unsafe { self.read_from_gx_pipe(arm9).param as i32 };
                    }
                    for elem in &mut contents[4..7] {
                        *elem = unsafe { self.read_from_gx_pipe(arm9).param as i32 };
                    }
                    for elem in &mut contents[8..11] {
                        *elem = unsafe { self.read_from_gx_pipe(arm9).param as i32 };
                    }
                    self.load_matrix(Matrix::new(contents));
                }

                0x18 => {
                    // MTX_MULT_4x4
                    let mut contents = MatrixBuffer([0; 16]);
                    contents.0[0] = first_param as i32;
                    for elem in &mut contents.0[1..] {
                        *elem = unsafe { self.read_from_gx_pipe(arm9).param as i32 };
                    }

                    match self.mtx_mode {
                        MatrixMode::Projection => {
                            self.cur_proj_mtx.mul_left_4x4(contents);
                            self.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::Position => {
                            self.cur_pos_vec_mtxs[0].mul_left_4x4(contents);
                            self.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::PositionVector => {
                            self.cur_pos_vec_mtxs[0].mul_left_4x4(contents);
                            self.cur_pos_vec_mtxs[1].mul_left_4x4(contents);
                            self.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::Texture => self.cur_tex_mtx.mul_left_4x4(contents),
                    }
                }

                0x19 => {
                    // MTX_MULT_4x3
                    let mut contents = MatrixBuffer([0; 12]);
                    contents.0[0] = first_param as i32;
                    for elem in &mut contents.0[1..] {
                        *elem = unsafe { self.read_from_gx_pipe(arm9).param as i32 };
                    }

                    match self.mtx_mode {
                        MatrixMode::Projection => {
                            self.cur_proj_mtx.mul_left_4x3(contents);
                            self.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::Position => {
                            self.cur_pos_vec_mtxs[0].mul_left_4x3(contents);
                            self.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::PositionVector => {
                            self.cur_pos_vec_mtxs[0].mul_left_4x3(contents);
                            self.cur_pos_vec_mtxs[1].mul_left_4x3(contents);
                            self.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::Texture => self.cur_tex_mtx.mul_left_4x3(contents),
                    }
                }

                0x1A => {
                    // MTX_MULT_3x3
                    let mut contents = MatrixBuffer([0; 9]);
                    contents.0[0] = first_param as i32;
                    for elem in &mut contents.0[1..] {
                        *elem = unsafe { self.read_from_gx_pipe(arm9).param as i32 };
                    }

                    match self.mtx_mode {
                        MatrixMode::Projection => {
                            self.cur_proj_mtx.mul_left_3x3(contents);
                            self.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::Position => {
                            self.cur_pos_vec_mtxs[0].mul_left_3x3(contents);
                            self.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::PositionVector => {
                            self.cur_pos_vec_mtxs[0].mul_left_3x3(contents);
                            self.cur_pos_vec_mtxs[1].mul_left_3x3(contents);
                            self.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::Texture => self.cur_tex_mtx.mul_left_3x3(contents),
                    }
                }

                0x1B => {
                    // MTX_SCALE
                    let contents = unsafe {
                        [
                            first_param as i32,
                            self.read_from_gx_pipe(arm9).param as i32,
                            self.read_from_gx_pipe(arm9).param as i32,
                        ]
                    };

                    match self.mtx_mode {
                        MatrixMode::Projection => {
                            self.cur_proj_mtx.scale(contents);
                            self.clip_mtx_needs_recalculation = true;
                        }
                        MatrixMode::Position | MatrixMode::PositionVector => {
                            self.cur_pos_vec_mtxs[0].scale(contents);
                            self.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::Texture => self.cur_tex_mtx.scale(contents),
                    }
                }
                0x1C => {
                    // MTX_TRANS
                    let contents = unsafe {
                        [
                            first_param as i32,
                            self.read_from_gx_pipe(arm9).param as i32,
                            self.read_from_gx_pipe(arm9).param as i32,
                        ]
                    };

                    match self.mtx_mode {
                        MatrixMode::Projection => {
                            self.cur_proj_mtx.translate(contents);
                            self.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::Position => {
                            self.cur_pos_vec_mtxs[0].translate(contents);
                            self.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::PositionVector => {
                            self.cur_pos_vec_mtxs[0].translate(contents);
                            self.cur_pos_vec_mtxs[1].translate(contents);
                            self.clip_mtx_needs_recalculation = true;
                        }

                        MatrixMode::Texture => self.cur_tex_mtx.translate(contents),
                    }
                }

                0x20 => {
                    // COLOR
                    self.vert_color = first_param as u16 & 0x7FFF;
                }

                // 0x21 => {} // TODO: NORMAL
                0x22 => {
                    // TEXCOORD
                    self.tex_coords = [first_param as i16, (first_param >> 16) as i16];
                }

                0x23 => {
                    // VTX_16
                    let second_param = unsafe { self.read_from_gx_pipe(arm9).param };
                    self.add_vertex([
                        first_param as i16,
                        (first_param >> 16) as i16,
                        second_param as i16,
                    ]);
                }

                0x24 => {
                    // VTX_10
                    self.add_vertex([
                        (first_param as i16) << 6,
                        ((first_param >> 10) as i16) << 6,
                        ((first_param >> 20) as i16) << 6,
                    ]);
                }
                0x25 => {
                    // VTX_XY
                    self.add_vertex([
                        first_param as i16,
                        (first_param >> 16) as i16,
                        self.last_vtx_coords[2],
                    ]);
                }
                0x26 => {
                    // VTX_XZ
                    self.add_vertex([
                        first_param as i16,
                        self.last_vtx_coords[1],
                        (first_param >> 16) as i16,
                    ]);
                }

                0x27 => {
                    // VTX_YZ
                    self.add_vertex([
                        self.last_vtx_coords[0],
                        first_param as i16,
                        (first_param >> 16) as i16,
                    ]);
                }

                0x28 => {
                    // VTX_DIFF
                    self.add_vertex([
                        self.last_vtx_coords[0].wrapping_add((first_param as i16) << 6 >> 6),
                        self.last_vtx_coords[1].wrapping_add((first_param >> 4) as i16 >> 6),
                        self.last_vtx_coords[2].wrapping_add((first_param >> 14) as i16 >> 6),
                    ]);
                }

                0x29 => {
                    // POLYGON_ATTR
                    self.next_poly_attrs = PolygonAttrs(first_param);
                }

                0x2A => {
                    // TEXIMAGE_PARAM
                    self.next_tex_params = TextureParams(first_param);
                }

                0x2B => {
                    // PLTT_BASE
                    self.next_tex_palette_base = first_param as u16 & 0xFFF;
                }

                0x30 => {
                    // DIF_AMB
                    self.diffuse_color = first_param as u16 & 0x7FFF;
                    self.ambient_color = (first_param >> 16) as u16 & 0x7FFF;
                    if first_param & 1 << 15 != 0 {
                        self.vert_color = self.diffuse_color;
                    }
                }

                0x31 => {
                    // SPE_EMI
                    self.specular_color = first_param as u16 & 0x7FFF;
                    self.emission_color = (first_param >> 16) as u16 & 0x7FFF;
                    self.shininess_table_enabled = first_param & 1 << 15 != 0;
                }

                0x32 => {
                    // LIGHT_VECTOR
                    self.lights[(first_param >> 30) as usize].direction = [
                        (first_param as i16) << 6 >> 3,
                        ((first_param >> 10) as i16) << 6 >> 3,
                        ((first_param >> 20) as i16) << 6 >> 3,
                    ];
                }

                0x33 => {
                    // LIGHT_COLOR
                    self.lights[(first_param >> 30) as usize].color = first_param as u16 & 0x7FFF;
                }

                0x34 => {
                    // SHININESS
                    self.shininess_table[0] = first_param as u8;
                    self.shininess_table[1] = (first_param >> 8) as u8;
                    self.shininess_table[2] = (first_param >> 16) as u8;
                    self.shininess_table[3] = (first_param >> 24) as u8;
                    for i in (4..128).step_by(4) {
                        let param = unsafe { self.read_from_gx_pipe(arm9).param };
                        self.shininess_table[i] = param as u8;
                        self.shininess_table[i + 1] = (param >> 8) as u8;
                        self.shininess_table[i + 2] = (param >> 16) as u8;
                        self.shininess_table[i + 3] = (param >> 24) as u8;
                    }
                }

                0x40 => {
                    // BEGIN_VTXS
                    self.cur_poly_attrs = self.next_poly_attrs;
                    // TODO
                }

                0x41 => {
                    // END_VTXS
                    // Should do nothing according to GBATEK
                }

                // 0x50 => {} // TODO: SWAP_BUFFERS

                // 0x60 => {} // TODO: VIEWPORT

                // 0x70 => {} // TODO: BOX_TEST

                // 0x71 => {} // TODO: POS_TEST

                // 0x72 => {} // TODO: VEC_TEST
                _ => {
                    #[cfg(feature = "log")]
                    slog::warn!(
                        self.logger,
                        "Unhandled command: {:#04X} ({})",
                        command,
                        command_name(command),
                    );
                    for _ in 1..params {
                        unsafe { self.read_from_gx_pipe(arm9).param };
                    }
                }
            }

            self.command_finish_time.0 =
                emu::Timestamp::from(arm9::Timestamp(arm9.schedule.cur_time().0 + 1)).0 + 10;
            if self.gx_fifo_stalled() {
                emu_schedule.schedule_event(emu::event_slots::ENGINE_3D, self.command_finish_time);
            } else {
                arm9.schedule.schedule_event(
                    arm9::event_slots::ENGINE_3D,
                    self.command_finish_time.into(),
                );
            }
            break;
        }
    }
}
