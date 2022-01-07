mod io;

use crate::{
    cpu::{
        self,
        arm9::{self, Arm9},
        Schedule,
    },
    emu,
    utils::{bitfield_debug, Fifo},
};

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

            // TODO: Process command
            for i in 1..params {
                unsafe { self.read_from_gx_pipe(arm9).param };
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
