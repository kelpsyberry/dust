mod schedule;
pub use schedule::{event_slots, Event, EventSlotIndex, Schedule, Timestamp};
pub mod input;
pub mod swram;

use crate::{
    audio::{self, Audio},
    cpu::{
        self,
        arm7::{self, Arm7},
        arm9::{self, Arm9},
        Arm7Data, Arm9Data, CoreData,
    },
    ds_slot::{self, DsSlot},
    flash,
    gpu::{self, Gpu},
    ipc::Ipc,
    rtc::Rtc,
    spi,
    utils::{
        bitfield_debug, schedule::RawTimestamp, zeroed_box, BoxedByteSlice, ByteMutSlice, Bytes,
        OwnedBytesCellPtr,
    },
    Model,
};
use input::Input;
use swram::Swram;

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct LocalExMemControl(pub u16) {
        pub gba_slot_sram_access_time: u8 @ 0..=1,
        pub gba_slot_rom_1st_access_time: u8 @ 2..=3,
        pub gba_slot_rom_2nd_access_time: bool @ 4,
        pub gba_slot_phi_pin_out: u8 @ 5..=6,
    }
}

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct GlobalExMemControl(pub u16) {
        pub arm7_gba_slot_access: bool @ 7,
        pub arm7_ds_slot_access: bool @ 11,
        pub sync_main_mem: bool @ 14,
        pub arm7_main_mem_priority: bool @ 15,
    }
}

#[repr(C)]
pub struct Emu<E: cpu::Engine> {
    global_engine_data: E::GlobalData,
    pub arm7: Arm7<E>,
    pub arm9: Arm9<E>,
    main_mem: OwnedBytesCellPtr<0x40_0000>,
    pub swram: Swram,
    pub schedule: Schedule,
    global_ex_mem_control: GlobalExMemControl,
    pub ipc: Ipc,
    pub ds_slot: DsSlot,
    pub spi: spi::Controller,
    pub rtc: Rtc,
    pub gpu: Gpu,
    pub input: Input,
    pub audio: Audio,
    rcnt: u16, // TODO: Move to SIO
}

pub struct Builder {
    pub arm7_bios: Box<Bytes<{ arm7::BIOS_SIZE }>>,
    pub arm9_bios: Box<Bytes<{ arm9::BIOS_SIZE }>>,
    pub batch_duration: u32,
    pub audio_sample_chunk_size: usize,
    pub first_launch: bool,
    #[cfg(feature = "xq-audio")]
    pub audio_xq_sample_rate_shift: u8,
    #[cfg(feature = "xq-audio")]
    pub audio_xq_interp_method: audio::InterpMethod,
}

impl Builder {
    #[inline]
    pub fn new() -> Self {
        Builder {
            arm7_bios: zeroed_box(),
            arm9_bios: zeroed_box(),
            batch_duration: 64,
            audio_sample_chunk_size: audio::DEFAULT_SAMPLE_CHUNK_SIZE,
            first_launch: false,
            #[cfg(feature = "xq-audio")]
            audio_xq_sample_rate_shift: 0,
            #[cfg(feature = "xq-audio")]
            audio_xq_interp_method: audio::InterpMethod::Nearest,
        }
    }

    #[allow(clippy::too_many_arguments)]
    /// # Errors
    /// - [`CreationError::SizeNotPowerOfTwo`](flash::CreationError::SizeNotPowerOfTwo): the given
    ///   firmware image's size is not a power of two.
    pub fn build<E: cpu::Engine>(
        self,
        model: Model,
        firmware_contents: BoxedByteSlice,
        ds_rom: Box<dyn ds_slot::rom::Rom>,
        ds_spi: Box<dyn ds_slot::spi::SpiDevice>,
        direct_boot: bool,
        engine: E,
        audio_backend: Box<dyn audio::Backend>,
        #[cfg(feature = "log")] logger: &slog::Logger,
    ) -> Result<Emu<E>, flash::CreationError> {
        let (global_engine_data, arm7_engine_data, arm9_engine_data) = engine.into_data();
        let mut arm7 = Arm7::new(
            arm7_engine_data,
            self.arm7_bios,
            #[cfg(feature = "log")]
            logger.new(slog::o!("cpu" => "arm7")),
        );
        let mut arm9 = Arm9::new(
            arm9_engine_data,
            self.arm9_bios,
            #[cfg(feature = "log")]
            logger.new(slog::o!("cpu" => "arm9")),
        );
        let mut global_schedule = Schedule::new(Timestamp(self.batch_duration as RawTimestamp));
        let mut emu = Emu {
            global_engine_data,
            main_mem: OwnedBytesCellPtr::new_zeroed(),
            swram: Swram::new(),
            global_ex_mem_control: GlobalExMemControl(0x6000),
            ipc: Ipc::new(),
            ds_slot: DsSlot::new(ds_rom, ds_spi, &mut arm7.schedule, &mut arm9.schedule),
            spi: spi::Controller::new(
                firmware_contents,
                model,
                &mut arm7.schedule,
                &mut global_schedule,
                #[cfg(feature = "log")]
                logger.new(slog::o!("spi" => "")),
            )?,
            rtc: Rtc::new(
                self.first_launch,
                #[cfg(feature = "log")]
                logger.new(slog::o!("rtc" => "")),
            ),
            gpu: Gpu::new(
                &mut global_schedule,
                #[cfg(feature = "log")]
                &logger.new(slog::o!("gpu" => "")),
            ),
            input: Input(0x007F_03FF),
            audio: Audio::new(
                audio_backend,
                &mut arm7.schedule,
                self.audio_sample_chunk_size,
                #[cfg(feature = "xq-audio")]
                self.audio_xq_sample_rate_shift,
                #[cfg(feature = "xq-audio")]
                self.audio_xq_interp_method,
                #[cfg(feature = "log")]
                logger.new(slog::o!("audio" => "")),
            ),
            rcnt: 0,
            schedule: global_schedule,
            arm7,
            arm9,
        };
        Arm7::setup(&mut emu);
        Arm9::setup(&mut emu);
        emu.ds_slot.rom.setup(direct_boot, emu.arm7.bios());
        emu.swram.recalc(&mut emu.arm7, &mut emu.arm9);
        E::Arm7Data::setup(&mut emu);
        E::Arm9Data::setup(&mut emu);
        if direct_boot {
            emu.setup_direct_boot();
        }
        Ok(emu)
    }
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

impl<E: cpu::Engine> Emu<E> {
    fn setup_direct_boot(&mut self) {
        // TODO: More accurate direct boot
        let mut header = Bytes::new([0; 0x170]);
        self.ds_slot.rom.read(0, header.as_byte_mut_slice());
        let chip_id = self.ds_slot.rom.chip_id();
        self.main_mem.write_le(0x3F_F800, chip_id);
        self.main_mem.write_le(0x3F_F804, chip_id);
        self.main_mem
            .write_le(0x3F_F808, header.read_le::<u16>(0x15E));
        self.main_mem
            .write_le(0x3F_F80A, header.read_le::<u16>(0x6C));
        self.main_mem.write_le(0x3F_F80C, 0_u32);
        self.main_mem.write_le(0x3F_F810, 0xFFFF_u16);
        self.main_mem.write_le(0x3F_F850, 0x5835_u16);
        self.main_mem.write_le(0x3F_F880, 7_u32);
        self.main_mem.write_le(0x3F_F884, 6_u32);
        self.main_mem.write_le(0x3F_FC00, chip_id);
        self.main_mem.write_le(0x3F_FC04, chip_id);
        self.main_mem
            .write_le(0x3F_FC08, header.read_le::<u16>(0x15E));
        self.main_mem
            .write_le(0x3F_FC0A, header.read_le::<u16>(0x6C));
        self.main_mem.write_le(0x3F_FC0C, 0_u32);
        self.main_mem.write_le(0x3F_FC10, 0x5835_u16);
        self.main_mem.write_le(0x3F_FC40, 1_u16);
        unsafe {
            self.main_mem.as_byte_mut_slice()[0x3F_FE00..0x3F_FF70].copy_from_slice(&header[..]);
        };
        self.arm7.wram.write_le(0xF980, 0xFBDD_37BB_u32);
        self.arm7.set_bios_prot(0x1204);
        self.arm7.set_post_boot_flag(true);
        self.arm9.set_post_boot_flag(arm9::PostBootFlag(1));
        self.audio.set_bias(0x200);
        Arm9::set_cp15_dtcm_control(self, arm9::cp15::TcmControl(0x0300_000A));
        Arm9::set_cp15_itcm_control(self, arm9::cp15::TcmControl(0x20));
        Arm9::set_cp15_control(self, arm9::cp15::Control(0x0005_2078));
        self.swram
            .set_control(swram::Control(3), &mut self.arm7, &mut self.arm9);
        let arm9_rom_offset = header.read_le::<u32>(0x20);
        let arm9_entry_addr = header.read_le::<u32>(0x24);
        let arm9_ram_addr = header.read_le::<u32>(0x28);
        let arm9_size = header.read_le::<u32>(0x2C);
        let arm7_rom_offset = header.read_le::<u32>(0x30);
        let arm7_entry_addr = header.read_le::<u32>(0x34);
        let arm7_ram_addr = header.read_le::<u32>(0x38);
        let arm7_size = header.read_le::<u32>(0x3C);
        let mut arm7_loaded_data = BoxedByteSlice::new_zeroed(arm7_size as usize);
        self.ds_slot.rom.read(
            arm7_rom_offset,
            ByteMutSlice::new(&mut arm7_loaded_data[..]),
        );
        E::Arm7Data::setup_direct_boot(
            self,
            arm7_entry_addr,
            (arm7_loaded_data.as_byte_slice(), arm7_ram_addr),
        );
        let mut arm9_loaded_data = BoxedByteSlice::new_zeroed(arm9_size as usize);
        self.ds_slot.rom.read(
            arm9_rom_offset,
            ByteMutSlice::new(&mut arm9_loaded_data[..]),
        );
        E::Arm9Data::setup_direct_boot(
            self,
            arm9_entry_addr,
            (arm9_loaded_data.as_byte_slice(), arm9_ram_addr),
        );
    }

    #[inline(never)]
    pub fn run_frame(&mut self) -> bool {
        loop {
            let batch_end_time = self.schedule.batch_end_time();
            E::Arm7Data::run_until(self, batch_end_time.into());
            E::Arm9Data::run_until(self, batch_end_time.into());
            self.schedule.set_cur_time(batch_end_time);
            while let Some((event, time)) = self.schedule.pop_pending_event() {
                match event {
                    Event::Gpu(event) => match event {
                        gpu::Event::EndHDraw => Gpu::end_hdraw(self, time),
                        gpu::Event::EndHBlank => Gpu::end_hblank(self, time),
                        gpu::Event::FinishFrame => {
                            Gpu::end_hblank(self, time);
                            return true;
                        }
                    },
                    Event::Shutdown => {
                        return false;
                    }
                }
            }
        }
    }
}

impl<E: cpu::Engine> Emu<E> {
    #[inline]
    pub fn main_mem(&self) -> &OwnedBytesCellPtr<0x40_0000> {
        &self.main_mem
    }

    #[inline]
    pub fn global_ex_mem_control(&self) -> GlobalExMemControl {
        self.global_ex_mem_control
    }

    #[inline]
    pub fn set_global_ex_mem_control(&mut self, value: GlobalExMemControl) {
        self.global_ex_mem_control.0 = (value.0 & 0xC88) | 0x400;
        self.ds_slot.update_access(value.arm7_ds_slot_access());
    }

    #[inline]
    pub fn rcnt(&self) -> u16 {
        self.rcnt
    }

    #[inline]
    pub fn set_rcnt(&mut self, value: u16) {
        self.rcnt = value & 0xC1FF;
    }
}
