pub mod engine_2d;
pub mod engine_3d;
pub mod vram;

use crate::{
    cpu::{arm7, arm9, Engine},
    emu::{self, event_slots, Emu, Timestamp},
    utils::{schedule::RawTimestamp, Savestate},
};
use engine_2d::Engine2d;
use engine_3d::Engine3d;
use vram::Vram;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Savestate)]
pub enum Event {
    EndHDraw,
    EndHBlank,
    FinishFrame,
}

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq, Savestate)]
    pub const struct PowerControl(pub u16): Debug {
        pub display_enabled: bool @ 0,
        pub engine_2d_a_enabled: bool @ 1,
        pub engine_3d_rendering_enabled: bool @ 2,
        pub engine_3d_geometry_enabled: bool @ 3,
        pub engine_2d_b_enabled: bool @ 9,
        pub swap_screens: bool @ 15,
    }
}

proc_bitfield::bitfield! {
    #[derive(Clone, Copy, PartialEq, Eq, Savestate)]
    pub const struct DispStatus(pub u16): Debug {
        pub vblank: bool @ 0,
        pub hblank: bool @ 1,
        pub vcount_match: bool @ 2,
        pub vblank_irq_enabled: bool @ 3,
        pub hblank_irq_enabled: bool @ 4,
        pub vcount_match_irq_enabled: bool @ 5,
        pub vcount_compare_high: u8 @ 7..=7,
        pub vcount_compare_low: u8 @ 8..=15,
    }
}

impl DispStatus {
    #[inline]
    pub const fn vcount_compare(self) -> u16 {
        self.0 >> 8 | (self.0 << 1 & 0x100)
    }
}

pub const SCREEN_WIDTH: usize = 256;
pub const SCREEN_HEIGHT: usize = 192;
pub const TOTAL_SCANLINES: usize = 263;

const DOT_CYCLES: RawTimestamp = 6;
const HDRAW_DURATION: Timestamp = Timestamp(SCREEN_WIDTH as RawTimestamp * DOT_CYCLES + 48);
const HBLANK_DURATION: Timestamp = Timestamp(99 * DOT_CYCLES - 48);

#[repr(C, align(64))]
#[derive(Clone, Copy, Savestate)]
pub struct Scanline<T, const LEN: usize = SCREEN_WIDTH>(pub [T; LEN]);

pub type Framebuffer = [[u32; SCREEN_WIDTH * SCREEN_HEIGHT]; 2];

#[derive(Savestate)]
#[load(in_place_only)]
pub struct Gpu {
    power_control: PowerControl,
    vcount: u16,
    next_vcount: Option<u16>,
    cur_scanline: u32,
    disp_status_7: DispStatus,
    vcount_compare_7: u16,
    disp_status_9: DispStatus,
    vcount_compare_9: u16,
    pub vram: Vram,
    #[savestate(skip)]
    renderer_2d: Box<dyn engine_2d::Renderer>,
    pub engine_2d_a: Engine2d<engine_2d::EngineA>,
    pub engine_2d_b: Engine2d<engine_2d::EngineB>,
    pub engine_3d: Engine3d,
}

impl Gpu {
    pub(crate) fn new(
        renderer_2d: Box<dyn engine_2d::Renderer>,
        renderer_3d_tx: Box<dyn engine_3d::RendererTx>,
        schedule: &mut arm9::Schedule,
        emu_schedule: &mut emu::Schedule,
        #[cfg(feature = "log")] logger: &slog::Logger,
    ) -> Self {
        emu_schedule.set_event(event_slots::GPU, emu::Event::Gpu(Event::EndHDraw));
        emu_schedule.schedule_event(event_slots::GPU, HDRAW_DURATION);

        let mut result = Gpu {
            power_control: PowerControl(0),
            vcount: 0,
            next_vcount: None,
            cur_scanline: 0,
            disp_status_7: DispStatus(0),
            vcount_compare_7: 0,
            disp_status_9: DispStatus(0),
            vcount_compare_9: 0,
            vram: Vram::new(renderer_2d.uses_bg_obj_vram_tracking()),
            renderer_2d,
            engine_2d_a: Engine2d::new(
                #[cfg(feature = "log")]
                logger.new(slog::o!("eng_2d" => "a")),
            ),
            engine_2d_b: Engine2d::new(
                #[cfg(feature = "log")]
                logger.new(slog::o!("eng_2d" => "b")),
            ),
            engine_3d: Engine3d::new(
                renderer_3d_tx,
                schedule,
                emu_schedule,
                #[cfg(feature = "log")]
                logger.new(slog::o!("eng_3d" => "")),
            ),
        };

        result.renderer_2d.start_scanline(
            0,
            0,
            (&mut result.engine_2d_a, &mut result.engine_2d_b),
            &mut result.vram,
        );

        result
    }

    #[inline]
    pub fn renderer_2d(&self) -> &dyn engine_2d::Renderer {
        &*self.renderer_2d
    }

    #[inline]
    pub fn set_renderer_2d<E: Engine>(
        &mut self,
        renderer: Box<dyn engine_2d::Renderer>,
        arm9: &mut arm9::Arm9<E>,
    ) {
        self.renderer_2d = renderer;
        self.vram
            .set_vram_tracking(self.renderer_2d.uses_bg_obj_vram_tracking(), arm9);

        self.renderer_2d.start_scanline(
            0,
            0,
            (&mut self.engine_2d_a, &mut self.engine_2d_b),
            &mut self.vram,
        );
    }

    #[inline]
    pub fn into_renderers(self) -> (Box<dyn engine_2d::Renderer>, Box<dyn engine_3d::RendererTx>) {
        (self.renderer_2d, self.engine_3d.renderer_tx)
    }

    #[inline]
    pub fn power_control(&self) -> PowerControl {
        self.power_control
    }

    #[inline]
    pub fn write_power_control(&mut self, value: PowerControl) {
        // TODO: What to do with bit 0? The current handling code is just a guess
        if !value.display_enabled() {
            self.disp_status_7 = self
                .disp_status_7
                .with_hblank(false)
                .with_vblank(false)
                .with_vcount_match(false);
            self.disp_status_9 = self
                .disp_status_9
                .with_hblank(false)
                .with_vblank(false)
                .with_vcount_match(false);
        }

        self.power_control.0 = value.0 & 0x820F;

        self.engine_2d_a.is_enabled = value.engine_2d_a_enabled();
        self.engine_2d_a.is_on_lower_screen = !value.swap_screens();
        self.engine_2d_b.is_enabled = value.engine_2d_b_enabled();
        self.engine_2d_b.is_on_lower_screen = value.swap_screens();

        self.engine_2d_a.engine_3d_enabled = value.engine_3d_rendering_enabled();
        self.engine_3d.gx_enabled = value.engine_3d_geometry_enabled();
        self.engine_3d.rendering_enabled = value.engine_3d_rendering_enabled();
    }

    #[inline]
    pub fn vcount(&self) -> u16 {
        self.vcount
    }

    #[inline]
    pub fn write_vcount(&mut self, value: u16) {
        // VCOUNT writes are allowed on the DS, though according to melonDS, its value doesn't
        // control where rendered scanlines end up, but just which BG scanlines the engines think
        // they're rendering, while still rendering OBJs in the same place, as well as modifying
        // timing to match up with the new value (ending frames earlier or later).
        // Also, VCOUNT writes are delayed until the next scanline.
        // TODO: How does this interact with the 3D engine? It seems to stop rendering and repeat
        // the last two finished scanlines until the end of the frame.
        self.next_vcount = Some(value);
    }

    #[inline]
    pub fn cur_scanline(&self) -> u32 {
        self.cur_scanline
    }

    #[inline]
    pub fn disp_status_7(&self) -> DispStatus {
        self.disp_status_7
    }

    #[inline]
    pub fn vcount_compare_7(&self) -> u16 {
        self.vcount_compare_7
    }

    #[inline]
    pub fn write_disp_status_7(&mut self, value: DispStatus) {
        self.disp_status_7.0 = (self.disp_status_7.0 & 7) | (value.0 & 0xFFB8);
        self.vcount_compare_7 = value.vcount_compare();
    }

    #[inline]
    pub fn disp_status_9(&self) -> DispStatus {
        self.disp_status_9
    }

    #[inline]
    pub fn vcount_compare_9(&self) -> u16 {
        self.vcount_compare_9
    }

    #[inline]
    pub fn write_disp_status_9(&mut self, value: DispStatus) {
        self.disp_status_9.0 = (self.disp_status_9.0 & 7) | (value.0 & 0xFFB8);
        self.vcount_compare_9 = value.vcount_compare();
    }

    pub(crate) fn end_hdraw(emu: &mut Emu<impl Engine>, time: Timestamp) {
        if emu.gpu.power_control.display_enabled() {
            emu.gpu.disp_status_7.set_hblank(true);
            if emu.gpu.disp_status_7.hblank_irq_enabled() {
                emu.arm7
                    .irqs
                    .write_requested(emu.arm7.irqs.requested().with_hblank(true), ());
            }
            emu.gpu.disp_status_9.set_hblank(true);
            if emu.gpu.disp_status_9.hblank_irq_enabled() {
                emu.arm9
                    .irqs
                    .write_requested(emu.arm9.irqs.requested().with_hblank(true), ());
            }
        }

        if emu.gpu.vcount < SCREEN_HEIGHT as u16 {
            if emu.gpu.power_control.display_enabled() {
                emu.arm9
                    .start_dma_transfers_with_timing::<{ arm9::dma::Timing::HBlank }>();
            }
            if emu.gpu.cur_scanline < SCREEN_HEIGHT as u32 {
                emu.gpu.renderer_2d.finish_scanline(
                    emu.gpu.cur_scanline as u8,
                    emu.gpu.vcount as u8,
                    (&mut emu.gpu.engine_2d_a, &mut emu.gpu.engine_2d_b),
                    &mut emu.gpu.vram,
                );
            }
        } else if emu.gpu.vcount == (TOTAL_SCANLINES - 1) as u16 {
            // Render scanline 0 OBJs
            emu.gpu.renderer_2d.start_prerendering_objs(
                (&mut emu.gpu.engine_2d_a, &mut emu.gpu.engine_2d_b),
                &mut emu.gpu.vram,
            );
        }

        emu.schedule.set_event(
            event_slots::GPU,
            emu::Event::Gpu(if emu.gpu.vcount == (TOTAL_SCANLINES - 1) as u16 {
                Event::FinishFrame
            } else {
                Event::EndHBlank
            }),
        );
        emu.schedule
            .schedule_event(event_slots::GPU, time + HBLANK_DURATION);
    }

    pub(crate) fn end_hblank(emu: &mut Emu<impl Engine>, time: Timestamp) {
        emu.gpu.cur_scanline = emu.gpu.cur_scanline.wrapping_add(1);
        emu.gpu.vcount = emu
            .gpu
            .next_vcount
            .unwrap_or_else(|| emu.gpu.vcount.wrapping_add(1));
        emu.gpu.next_vcount = None;

        if emu.gpu.vcount == TOTAL_SCANLINES as u16 {
            emu.gpu.vcount = 0;
            emu.gpu.cur_scanline = 0;
            emu.gpu.engine_2d_a.end_vblank();
            emu.gpu.engine_2d_b.end_vblank();
        }

        emu.gpu.engine_2d_a.update_windows(emu.gpu.vcount as u8);
        emu.gpu.engine_2d_b.update_windows(emu.gpu.vcount as u8);

        if emu.gpu.power_control.display_enabled() {
            emu.gpu.disp_status_7.set_hblank(false);
            emu.gpu.disp_status_9.set_hblank(false);
            if emu.gpu.vcount == emu.gpu.vcount_compare_7 {
                emu.gpu.disp_status_7.set_vcount_match(true);
                if emu.gpu.disp_status_7.vcount_match_irq_enabled() {
                    emu.arm7
                        .irqs
                        .write_requested(emu.arm7.irqs.requested().with_vcount_match(true), ());
                }
            } else {
                emu.gpu.disp_status_7.set_vcount_match(false);
            }
            if emu.gpu.vcount == emu.gpu.vcount_compare_9 {
                emu.gpu.disp_status_9.set_vcount_match(true);
                if emu.gpu.disp_status_9.vcount_match_irq_enabled() {
                    emu.arm9
                        .irqs
                        .write_requested(emu.arm9.irqs.requested().with_vcount_match(true), ());
                }
            } else {
                emu.gpu.disp_status_9.set_vcount_match(false);
            }
        }
        if emu.gpu.vcount < SCREEN_HEIGHT as u16 {
            if emu.gpu.cur_scanline < SCREEN_HEIGHT as u32 {
                emu.gpu.renderer_2d.start_scanline(
                    emu.gpu.cur_scanline as u8,
                    emu.gpu.vcount as u8,
                    (&mut emu.gpu.engine_2d_a, &mut emu.gpu.engine_2d_b),
                    &mut emu.gpu.vram,
                );
            }
        } else if emu.gpu.vcount == SCREEN_HEIGHT as u16 {
            // Unlock the 3D engine if it was waiting for VBlank
            if emu.gpu.engine_3d.swap_buffers_waiting() {
                Engine3d::swap_buffers(emu);
            } else {
                emu.gpu.engine_3d.swap_buffers_missed();
            }
            emu.gpu.engine_2d_a.start_vblank();
            emu.gpu.engine_2d_b.start_vblank();
            if emu.gpu.power_control.display_enabled() {
                emu.gpu.disp_status_7.set_vblank(true);
                if emu.gpu.disp_status_7.vblank_irq_enabled() {
                    emu.arm7
                        .irqs
                        .write_requested(emu.arm7.irqs.requested().with_vblank(true), ());
                }
                emu.gpu.disp_status_9.set_vblank(true);
                if emu.gpu.disp_status_9.vblank_irq_enabled() {
                    emu.arm9
                        .irqs
                        .write_requested(emu.arm9.irqs.requested().with_vblank(true), ());
                }
                emu.arm7
                    .start_dma_transfers_with_timing::<{ arm7::dma::Timing::VBlank }>();
                emu.arm9
                    .start_dma_transfers_with_timing::<{ arm9::dma::Timing::VBlank }>();
            }
        } else if emu.gpu.vcount == (TOTAL_SCANLINES - 48) as u16 {
            emu.gpu.engine_3d.start_rendering(&emu.gpu.vram);
        } else if emu.gpu.vcount == (TOTAL_SCANLINES - 1) as u16
            && emu.gpu.power_control.display_enabled()
        {
            // The VBlank flag gets cleared one scanline earlier than the actual VBlank end.
            emu.gpu.disp_status_7.set_vblank(false);
            emu.gpu.disp_status_9.set_vblank(false);
        }
        emu.schedule
            .set_event(event_slots::GPU, emu::Event::Gpu(Event::EndHDraw));
        emu.schedule
            .schedule_event(event_slots::GPU, time + HDRAW_DURATION);
    }
}
