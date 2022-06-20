mod schedule;
pub use schedule::{
    event_slots, Event, EventSlotIndex, Schedule, Timestamp, DEFAULT_BATCH_DURATION,
};
pub mod input;
pub mod swram;

use crate::{
    audio::{self, Audio},
    cpu::{
        self,
        arm7::{self, Arm7},
        arm9::{self, Arm9},
        bus::CpuAccess,
        Arm7Data, Arm9Data, CoreData, Schedule as _,
    },
    ds_slot::{self, DsSlot},
    flash::Flash,
    gpu::{self, engine_3d::Engine3d, Gpu},
    ipc::Ipc,
    rtc::{self, Rtc},
    spi,
    utils::{
        bitfield_debug, schedule::RawTimestamp, BoxedByteSlice, ByteMutSlice, Bytes,
        OwnedBytesCellPtr,
    },
    Model,
};
#[cfg(feature = "xq-audio")]
use core::num::NonZeroU32;
use input::Input;
use swram::Swram;

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct LocalExMemControl(pub u8) {
        pub gba_slot_sram_access_time: u8 @ 0..=1,
        pub gba_slot_rom_1st_access_time: u8 @ 2..=3,
        pub gba_slot_rom_2nd_access_time: bool @ 4,
        pub gba_slot_phi_pin_out: u8 @ 5..=6,
    }
}

impl LocalExMemControl {
    pub(crate) fn gba_rom_halfword(self, addr: u32) -> u16 {
        let value = (addr >> 1) as u16;
        match self.gba_slot_rom_1st_access_time() {
            0 => value | 0xFE08,
            1 | 2 => value,
            _ => 0xFFFF,
        }
    }

    pub(crate) fn gba_rom_word(self, addr: u32) -> u32 {
        let value = (addr >> 1 & 0xFFFF) | (addr >> 1 | 1) << 16;
        match self.gba_slot_rom_1st_access_time() {
            0 => value | 0xFE08_FE08,
            1 | 2 => value,
            _ => 0xFFFF_FFFF,
        }
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

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct AudioWifiPowerControl(pub u8) {
        pub speaker_enabled: bool @ 0,
        pub wifi_enabled: bool @ 1,
    }
}

pub struct Emu<E: cpu::Engine> {
    #[allow(dead_code)]
    pub(crate) global_engine_data: E::GlobalData,
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
    pub audio_wifi_power_control: AudioWifiPowerControl,
    pub audio: Audio,
    rcnt: u16, // TODO: Move to SIO
}

pub struct Builder {
    #[cfg(feature = "log")]
    logger: slog::Logger,

    pub arm7_bios: Box<Bytes<{ arm7::BIOS_SIZE }>>,
    pub arm9_bios: Box<Bytes<{ arm9::BIOS_BUFFER_SIZE }>>,
    pub firmware: Flash,
    pub ds_rom: ds_slot::rom::Rom,
    pub ds_spi: ds_slot::spi::Spi,
    pub audio_backend: Box<dyn audio::Backend>,
    pub rtc_backend: Box<dyn rtc::Backend>,
    pub renderer_3d: Box<dyn gpu::engine_3d::Renderer>,

    pub model: Model,
    pub direct_boot: bool,
    pub batch_duration: u32,
    pub first_launch: bool,
    pub audio_sample_chunk_size: usize,
    #[cfg(feature = "xq-audio")]
    pub audio_custom_sample_rate: Option<NonZeroU32>,
    #[cfg(feature = "xq-audio")]
    pub audio_channel_interp_method: audio::ChannelInterpMethod,
}

impl Builder {
    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        arm7_bios: Box<Bytes<{ arm7::BIOS_SIZE }>>,
        arm9_bios: Box<Bytes<{ arm9::BIOS_BUFFER_SIZE }>>,
        firmware: Flash,
        ds_rom: ds_slot::rom::Rom,
        ds_spi: ds_slot::spi::Spi,
        audio_backend: Box<dyn audio::Backend>,
        rtc_backend: Box<dyn rtc::Backend>,
        renderer_3d: Box<dyn gpu::engine_3d::Renderer>,
        #[cfg(feature = "log")] logger: slog::Logger,
    ) -> Self {
        Builder {
            #[cfg(feature = "log")]
            logger,

            arm7_bios,
            arm9_bios,
            firmware,
            ds_rom,
            ds_spi,
            audio_backend,
            rtc_backend,
            renderer_3d,

            model: Model::Ds,
            direct_boot: true,
            batch_duration: DEFAULT_BATCH_DURATION,
            first_launch: false,
            audio_sample_chunk_size: audio::DEFAULT_OUTPUT_SAMPLE_CHUNK_SIZE,
            #[cfg(feature = "xq-audio")]
            audio_custom_sample_rate: None,
            #[cfg(feature = "xq-audio")]
            audio_channel_interp_method: audio::ChannelInterpMethod::Nearest,
        }
    }

    pub fn build<E: cpu::Engine>(self, engine: E) -> Emu<E> {
        let (global_engine_data, arm7_engine_data, arm9_engine_data) = engine.into_data();
        let mut arm7 = Arm7::new(
            arm7_engine_data,
            self.arm7_bios.into(),
            #[cfg(feature = "log")]
            self.logger.new(slog::o!("cpu" => "arm7")),
        );
        let mut arm9 = Arm9::new(
            arm9_engine_data,
            self.arm9_bios.into(),
            #[cfg(feature = "log")]
            self.logger.new(slog::o!("cpu" => "arm9")),
        );
        let mut global_schedule = Schedule::new(Timestamp(self.batch_duration as RawTimestamp));
        let mut emu = Emu {
            global_engine_data,
            main_mem: OwnedBytesCellPtr::new_zeroed(),
            swram: Swram::new(),
            global_ex_mem_control: GlobalExMemControl(0x6000),
            ipc: Ipc::new(),
            ds_slot: DsSlot::new(
                self.ds_rom,
                self.ds_spi,
                &mut arm7.schedule,
                &mut arm9.schedule,
            ),
            spi: spi::Controller::new(
                self.firmware,
                self.model,
                &mut arm7.schedule,
                &mut global_schedule,
                #[cfg(feature = "log")]
                self.logger.new(slog::o!("spi" => "")),
            ),
            rtc: Rtc::new(
                self.rtc_backend,
                self.first_launch,
                #[cfg(feature = "log")]
                self.logger.new(slog::o!("rtc" => "")),
            ),
            gpu: Gpu::new(
                self.renderer_3d,
                &mut arm9.schedule,
                &mut global_schedule,
                #[cfg(feature = "log")]
                &self.logger.new(slog::o!("gpu" => "")),
            ),
            input: Input::new(),
            audio_wifi_power_control: AudioWifiPowerControl(0),
            audio: Audio::new(
                self.audio_backend,
                &mut arm7.schedule,
                self.audio_sample_chunk_size,
                #[cfg(feature = "xq-audio")]
                self.audio_custom_sample_rate,
                #[cfg(feature = "xq-audio")]
                self.audio_channel_interp_method,
                #[cfg(feature = "log")]
                self.logger.new(slog::o!("audio" => "")),
            ),
            rcnt: 0,
            schedule: global_schedule,
            arm7,
            arm9,
        };
        Arm7::setup(&mut emu);
        Arm9::setup(&mut emu);
        emu.ds_slot.rom.setup(self.direct_boot);
        emu.swram.recalc(&mut emu.arm7, &mut emu.arm9);
        E::Arm7Data::setup(&mut emu);
        E::Arm9Data::setup(&mut emu);
        if self.direct_boot {
            emu.setup_direct_boot();
        }
        emu
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunOutput {
    FrameFinished,
    Shutdown,
    #[cfg(feature = "debugger-hooks")]
    StoppedByDebugHook,
    #[cfg(feature = "debugger-hooks")]
    CyclesOver,
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
        self.arm7.write_bios_prot(0x1204);
        self.arm7.set_post_boot_flag(true);
        self.arm9.set_post_boot_flag(arm9::PostBootFlag(1));
        self.audio.write_bias(0x200);
        Arm9::write_cp15_dtcm_control(self, arm9::cp15::TcmControl(0x0300_000A));
        Arm9::write_cp15_itcm_control(self, arm9::cp15::TcmControl(0x20));
        Arm9::write_cp15_control(self, arm9::cp15::Control(0x0005_2078));
        self.swram
            .write_control(swram::Control(3), &mut self.arm7, &mut self.arm9);
        self.gpu.write_power_control(gpu::PowerControl(0x820F));

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
        E::Arm7Data::setup_direct_boot(self, arm7_entry_addr);
        for (&byte, addr) in arm7_loaded_data.iter().zip(arm7_ram_addr..) {
            arm7::bus::write_8::<CpuAccess, _>(self, addr, byte);
        }

        let mut arm9_loaded_data = BoxedByteSlice::new_zeroed(arm9_size as usize);
        self.ds_slot.rom.read(
            arm9_rom_offset,
            ByteMutSlice::new(&mut arm9_loaded_data[..]),
        );
        for (&byte, addr) in arm9_loaded_data.iter().zip(arm9_ram_addr..) {
            arm9::bus::write_8::<CpuAccess, _>(self, addr, byte);
        }
        E::Arm9Data::setup_direct_boot(self, arm9_entry_addr);
    }

    #[inline]
    pub fn main_mem(&self) -> &OwnedBytesCellPtr<0x40_0000> {
        &self.main_mem
    }

    #[inline]
    pub fn global_ex_mem_control(&self) -> GlobalExMemControl {
        self.global_ex_mem_control
    }

    #[inline]
    pub fn write_global_ex_mem_control(&mut self, value: GlobalExMemControl) {
        self.global_ex_mem_control.0 = (value.0 & 0x8880) | 0x6000;
        self.ds_slot.update_access(value.arm7_ds_slot_access());
    }

    #[inline]
    pub fn audio_wifi_power_control(&self) -> AudioWifiPowerControl {
        self.audio_wifi_power_control
    }

    #[inline]
    pub fn write_audio_wifi_power_control(&mut self, value: AudioWifiPowerControl) {
        self.audio_wifi_power_control.0 = value.0 & 3;
    }

    #[inline]
    pub fn rcnt(&self) -> u16 {
        self.rcnt
    }

    #[inline]
    pub fn write_rcnt(&mut self, value: u16) {
        self.rcnt = value & 0xC1FF;
    }

    #[inline]
    pub fn request_shutdown(&mut self) {
        self.spi
            .power
            .request_shutdown(&mut self.arm7.schedule, &mut self.schedule);
    }
}

macro_rules! run {
    ($emu: expr, $engine: ty $(, $cycles: expr)?) => {
        let mut batch_end_time = $emu.schedule.batch_end_time();
        $(
            #[cfg(feature = "debugger-hooks")]
            {
                batch_end_time = batch_end_time.min($emu.schedule.cur_time() + Timestamp(*$cycles));
                *$cycles -= batch_end_time.0 - $emu.schedule.cur_time().0;
            }
        )*
        if $emu.gpu.engine_3d.gx_fifo_stalled() {
            <$engine>::Arm9Data::run_stalled_until($emu, batch_end_time.into());
            <$engine>::Arm7Data::run_stalled_until($emu, batch_end_time.into());
        } else {
            macro_rules! run_core {
                ($core: expr, $engine_data: ty, $run: expr) => {
                    $core.schedule.set_cur_time_after($emu.schedule.cur_time().into());
                    #[cfg(feature = "debugger-hooks")]
                    let stopped = $core.stopped;
                    #[cfg(not(feature = "debugger-hooks"))]
                    let stopped = false;
                    if stopped {
                        $core.schedule.set_cur_time(batch_end_time.into());
                    } else {
                        <$engine_data>::run_until($emu, batch_end_time.into());
                        $run
                    }
                };
            }
            run_core!($emu.arm9, <$engine>::Arm9Data, {
                batch_end_time = batch_end_time.min(Timestamp::from($emu.arm9.schedule.cur_time()));
            });
            run_core!($emu.arm7, <$engine>::Arm7Data, {
                #[cfg(feature = "debugger-hooks")]
                {
                    batch_end_time =
                        batch_end_time.min(Timestamp::from($emu.arm7.schedule.cur_time()));
                }
            });
        }
        $emu.schedule.set_cur_time(batch_end_time);
        while let Some((event, time)) = $emu.schedule.pop_pending_event() {
            match event {
                Event::Gpu(event) => match event {
                    gpu::Event::EndHDraw => Gpu::end_hdraw($emu, time),
                    gpu::Event::EndHBlank => Gpu::end_hblank($emu, time),
                    gpu::Event::FinishFrame => {
                        Gpu::end_hblank($emu, time);
                        return RunOutput::FrameFinished;
                    }
                },
                Event::Shutdown => {
                    return RunOutput::Shutdown;
                }
                Event::Engine3dCommandFinished => Engine3d::process_next_command($emu),
            }
        }
        #[cfg(feature = "debugger-hooks")]
        if $emu.arm7.stopped_by_debug_hook || $emu.arm9.stopped_by_debug_hook {
            return RunOutput::StoppedByDebugHook;
        }
    };
}

impl<E: cpu::Engine> Emu<E> {
    #[cfg(feature = "debugger-hooks")]
    #[inline(never)]
    fn run_for_cycles(&mut self, cycles: &mut RawTimestamp) -> RunOutput {
        loop {
            run!(self, E, cycles);
            if *cycles == 0 {
                return RunOutput::CyclesOver;
            }
        }
    }

    #[inline(never)]
    pub fn run(
        &mut self,
        #[cfg(feature = "debugger-hooks")] cycles: &mut RawTimestamp,
    ) -> RunOutput {
        #[cfg(feature = "debugger-hooks")]
        {
            self.arm7.stopped_by_debug_hook = false;
            self.arm9.stopped_by_debug_hook = false;
            if *cycles != 0 {
                return self.run_for_cycles(cycles);
            }
        }
        loop {
            run!(self, E);
        }
    }
}
