macro_rules! forward_to_variants {
    ($ty: ident; $($variant: ident),*; $expr: expr, $f: ident $args: tt) => {
        match $expr {
            $(
                $ty::$variant(value) => value.$f $args,
            )*
        }
    }
}

macro_rules! impl_from_variants {
    ($ty: ident; $($variant: ident),*; $($variant_ty: ty),*) => {
        $(
            impl From<$variant_ty> for $ty {
                #[inline]
                fn from(other: $variant_ty) -> Self {
                    $ty::$variant(other)
                }
            }
        )*
    }
}

pub mod rom;
pub mod spi;

use crate::{
    cpu::{arm7, arm9, Engine, Schedule as _},
    emu::{Emu, Timestamp},
    utils::{schedule::RawTimestamp, zeroed_box, Bytes, Savestate},
};

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq, Savestate)]
    pub const struct AuxSpiControl(pub u16): Debug {
        pub spi_baud_rate: u8 @ 0..=1,
        pub spi_hold: bool @ 6,
        pub spi_busy: bool @ 7,
        // TODO: What's the effect of toggling the current device with bit 13 and accessing the
        //       deselected one?
        pub ds_slot_mode: bool @ 13,
        pub rom_transfer_complete_irq_enabled: bool @ 14,
        pub ds_slot_enabled: bool @ 15,
    }
}

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq, Savestate)]
    pub const struct RomControl(pub u32): Debug {
        // Applied after the command is sent (unless bit 30 is set)
        pub leading_gap_length: u16 @ 0..=12,
        pub data_key2_enabled: bool @ 13,
        pub security_enabled: bool @ 14,
        pub apply_key2_seed: bool @ 15,
        // Applied before every first word of a 512-byte block (unless bit 30 is set)
        pub first_block_byte_gap_length: u8 @ 16..=21,
        pub cmd_key2_enabled: bool @ 22,
        pub data_ready: bool @ 23,
        pub data_block_size_shift: u8 @ 24..=26,
        // 0/false: 6.7 MHz (5 cycles/word)
        // 1/true: 4.2 MHz (8 cycles/word)
        pub transfer_clock_rate: bool @ 27,
        pub gap_clks: bool @ 28,
        pub not_reset: bool @ 29,
        pub write_enabled: bool @ 30,
        pub busy: bool @ 31,
    }
}

mod bounded {
    use crate::utils::{bounded_int_lit, bounded_int_savestate};
    bounded_int_lit!(pub struct RomOutputLen(u16), max 0x4000);
    bounded_int_savestate!(RomOutputLen(u16));
    bounded_int_lit!(pub struct RomOutputPos(u16), max 0x3FFC, mask 0x3FFC);
    bounded_int_savestate!(RomOutputPos(u16));
}
pub use bounded::{RomOutputLen, RomOutputPos};

#[derive(Savestate)]
#[load(in_place_only)]
pub struct DsSlot {
    pub rom: rom::Rom,
    pub spi: spi::Spi,
    spi_control: AuxSpiControl,
    rom_control: RomControl,
    pub rom_cmd: Bytes<8>,
    arm7_access: bool,
    arm9_access: bool,
    pub rom_output_buffer: Box<Bytes<0x4000>>,
    pub rom_output_len: RomOutputLen,
    pub rom_output_pos: RomOutputPos,
    rom_data_out: u32,
    rom_clk_pulse_duration: u32,
    rom_busy: bool,
    spi_last_hold: bool,
    spi_data_out: u8,
}

impl DsSlot {
    pub(crate) fn new(
        rom: rom::Rom,
        spi: spi::Spi,
        arm7_schedule: &mut arm7::Schedule,
        arm9_schedule: &mut arm9::Schedule,
    ) -> Self {
        arm7_schedule.set_event(
            arm7::event_slots::DS_SLOT_ROM,
            arm7::Event::DsSlotRomDataReady,
        );
        arm9_schedule.set_event(
            arm9::event_slots::DS_SLOT_ROM,
            arm9::Event::DsSlotRomDataReady,
        );
        arm7_schedule.set_event(
            arm7::event_slots::DS_SLOT_SPI,
            arm7::Event::DsSlotSpiDataReady,
        );
        arm9_schedule.set_event(
            arm9::event_slots::DS_SLOT_SPI,
            arm9::Event::DsSlotSpiDataReady,
        );
        DsSlot {
            rom,
            spi,
            spi_control: AuxSpiControl(0),
            rom_control: RomControl(0),
            rom_cmd: Bytes::new([0; 8]),
            arm7_access: false,
            arm9_access: true,
            rom_output_buffer: zeroed_box(),
            rom_output_len: RomOutputLen::new(0),
            rom_output_pos: RomOutputPos::new(0),
            rom_data_out: 0,
            rom_clk_pulse_duration: 5,
            rom_busy: false,
            spi_last_hold: false,
            spi_data_out: 0,
        }
    }

    #[inline]
    pub const fn spi_control(&self) -> AuxSpiControl {
        self.spi_control
    }

    #[inline]
    pub fn write_spi_control(&mut self, value: AuxSpiControl) {
        // TODO: What happens if AUXSPICNT is changed while busy?
        self.spi_control.0 = (self.spi_control.0 & 0x0080) | (value.0 & 0xE043);
    }

    #[inline]
    pub const fn rom_control(&self) -> RomControl {
        self.rom_control
    }

    pub fn write_rom_control(
        &mut self,
        value: RomControl,
        arm7_schedule: &mut arm7::Schedule,
        arm9_schedule: &mut arm9::Schedule,
    ) {
        // TODO: What happens if ROMCTRL is modified while busy? (Particularly bit31, which might or
        // might not abort the previous transfer and start a new one if set while busy)
        // TODO: What's the actual behavior if AUXSPICNT.bit15 is 0?
        self.rom_control.0 = (self.rom_control.0 & 0x8080_0000) | (value.0 & !0x0080_8000);
        self.rom_clk_pulse_duration = if self.rom_control.transfer_clock_rate() {
            8
        } else {
            5
        };
        if !self.spi_control.ds_slot_enabled() || !self.rom_control.busy() {
            return;
        }
        self.rom_control.set_data_ready(false);
        self.rom_output_pos = RomOutputPos::new(0);
        self.rom_output_len = RomOutputLen::new(match self.rom_control.data_block_size_shift() {
            0 => 0,
            7 => 4,
            shift => 0x100 << shift,
        });
        self.rom.handle_rom_command(
            self.rom_cmd.clone(),
            &mut self.rom_output_buffer,
            self.rom_output_len,
        );
        // The command itself takes 8 CLK pulses to transfer, while every data byte takes 4 pulses
        // (the DS game card slot can only transfer 8 bits on every CLK cycle)
        let mut first_word_delay = 8 + (((self.rom_output_len.get() != 0) as u16) << 2);
        if !self.rom_control.write_enabled() {
            first_word_delay += self.rom_control.leading_gap_length();
            if self.rom_output_len.get() != 0 {
                first_word_delay += self.rom_control.first_block_byte_gap_length() as u16;
            }
        }
        let first_word_delay_cycles =
            Timestamp((first_word_delay as u32 * self.rom_clk_pulse_duration) as RawTimestamp);
        if self.arm7_access {
            if self.rom_busy {
                // NOTE: Not verified on hardware, only here to avoid locking up the whole emulator.
                arm7_schedule.cancel_event(arm7::event_slots::DS_SLOT_ROM);
            }
            self.rom_busy = true;
            arm7_schedule.schedule_event(
                arm7::event_slots::DS_SLOT_ROM,
                arm7_schedule.cur_time() + arm7::Timestamp::from(first_word_delay_cycles),
            );
        } else {
            if self.rom_busy {
                // NOTE: Not verified on hardware, only here to avoid locking up the whole emulator.
                arm9_schedule.cancel_event(arm9::event_slots::DS_SLOT_ROM);
            }
            self.rom_busy = true;
            arm9_schedule.schedule_event(
                arm9::event_slots::DS_SLOT_ROM,
                arm9_schedule.cur_time() + arm9::Timestamp::from(first_word_delay_cycles),
            );
        }
    }

    pub(crate) fn handle_rom_data_ready(emu: &mut Emu<impl Engine>) {
        emu.ds_slot.rom_busy = false;
        emu.ds_slot.rom_control.set_data_ready(true);
        if emu.ds_slot.rom_output_len.get() == 0 {
            // Clear the busy bit and trigger the "end of ROM transfer" IRQ immediately (since no
            // data should be read)
            emu.ds_slot.rom_control.set_busy(false);
            if emu.ds_slot.spi_control.rom_transfer_complete_irq_enabled() {
                if emu.ds_slot.arm7_access {
                    emu.arm7.irqs.write_requested(
                        emu.arm7
                            .irqs
                            .requested()
                            .with_ds_slot_transfer_complete(true),
                        (),
                    );
                } else {
                    emu.arm9.irqs.write_requested(
                        emu.arm9
                            .irqs
                            .requested()
                            .with_ds_slot_transfer_complete(true),
                        (),
                    );
                }
            }
            emu.ds_slot.rom_data_out = 0;
        } else {
            // Postpone ending the ROM transfer to when the data word will actually be read
            emu.ds_slot.rom_data_out = emu
                .ds_slot
                .rom_output_buffer
                .read_le(emu.ds_slot.rom_output_pos.get() as usize);
            if emu.ds_slot.arm7_access {
                emu.arm7
                    .start_dma_transfers_with_timing::<{ arm7::dma::Timing::DsSlot }>();
            } else {
                emu.arm9
                    .start_dma_transfers_with_timing::<{ arm9::dma::Timing::DsSlot }>();
            }
        }
    }

    #[inline]
    pub const fn peek_rom_data(&self) -> u32 {
        self.rom_data_out
    }

    pub(crate) fn read_rom_data_arm7(
        &mut self,
        irqs: &mut arm7::Irqs,
        schedule: &mut arm7::Schedule,
    ) -> u32 {
        if self.rom_control.data_ready() {
            self.rom_control.set_data_ready(false);
            let new_rom_output_pos = self.rom_output_pos.get() + 4;
            if new_rom_output_pos < self.rom_output_len.get() {
                self.rom_output_pos = RomOutputPos::new(new_rom_output_pos);
                let mut word_delay = 4;
                if !self.rom_control.write_enabled() && new_rom_output_pos & 0x1FF == 0 {
                    word_delay += self.rom_control.first_block_byte_gap_length();
                }
                let target = schedule.cur_time()
                    + arm7::Timestamp::from(Timestamp(
                        (word_delay as u32 * self.rom_clk_pulse_duration) as RawTimestamp,
                    ));
                schedule.schedule_event(arm7::event_slots::DS_SLOT_ROM, target);
            } else {
                self.rom_control.set_busy(false);
                if self.spi_control.rom_transfer_complete_irq_enabled() {
                    irqs.write_requested(
                        irqs.requested().with_ds_slot_transfer_complete(true),
                        schedule,
                    );
                }
            }
        }
        self.rom_data_out
    }

    pub(crate) fn read_rom_data_arm9(
        &mut self,
        irqs: &mut arm9::Irqs,
        schedule: &mut arm9::Schedule,
    ) -> u32 {
        if self.rom_control.data_ready() {
            self.rom_control.set_data_ready(false);
            let new_rom_output_pos = self.rom_output_pos.get() + 4;
            if new_rom_output_pos < self.rom_output_len.get() {
                self.rom_output_pos = RomOutputPos::new(new_rom_output_pos);
                let mut word_delay = 4;
                if !self.rom_control.write_enabled() && new_rom_output_pos & 0x1FF == 0 {
                    word_delay += self.rom_control.first_block_byte_gap_length();
                }
                let target = schedule.cur_time()
                    + arm9::Timestamp::from(Timestamp(
                        (word_delay as u32 * self.rom_clk_pulse_duration) as RawTimestamp,
                    ));
                schedule.schedule_event(arm9::event_slots::DS_SLOT_ROM, target);
            } else {
                self.rom_control.set_busy(false);
                if self.spi_control.rom_transfer_complete_irq_enabled() {
                    irqs.write_requested(
                        irqs.requested().with_ds_slot_transfer_complete(true),
                        schedule,
                    );
                }
            }
        }
        self.rom_data_out
    }

    #[inline]
    pub const fn spi_data_out(&self) -> u8 {
        // TODO: What's the response while busy?
        self.spi_data_out
    }

    pub fn write_spi_data(
        &mut self,
        value: u8,
        arm7_schedule: &mut arm7::Schedule,
        arm9_schedule: &mut arm9::Schedule,
    ) {
        if self.spi_control.spi_busy() {
            // TODO: What's supposed to happen if AUXSPIDATA is written while busy?
            return;
        }
        let first = !self.spi_last_hold;
        self.spi_last_hold = self.spi_control.spi_hold();
        let last = !self.spi_last_hold;
        self.spi_data_out = self.spi.write_data(value, first, last);
        // 8 bits at 33 / (8 << baud_rate) MHz (each bit takes 8 << baud_rate cycles to be
        // transferred)
        let byte_delay_cycles = Timestamp(64 << self.spi_control.spi_baud_rate());
        if self.arm7_access {
            arm7_schedule.schedule_event(
                arm7::event_slots::DS_SLOT_SPI,
                arm7_schedule.cur_time() + arm7::Timestamp::from(byte_delay_cycles),
            );
        } else {
            arm9_schedule.schedule_event(
                arm9::event_slots::DS_SLOT_SPI,
                arm9_schedule.cur_time() + arm9::Timestamp::from(byte_delay_cycles),
            );
        }
        self.spi_control.set_spi_busy(true);
    }

    pub(crate) fn handle_spi_data_ready(&mut self) {
        self.spi_control.set_spi_busy(false);
    }

    #[inline]
    pub const fn arm7_access(&self) -> bool {
        self.arm7_access
    }

    #[inline]
    pub const fn arm9_access(&self) -> bool {
        self.arm9_access
    }

    #[inline]
    pub(crate) fn update_access(&mut self, arm7_access: bool) {
        self.arm7_access = arm7_access;
        self.arm9_access = !arm7_access;
    }
}
