use super::{bus::timings::Timings, Arm9};
use crate::{
    cpu::{
        arm9::{bus, Timestamp},
        bus::DmaAccess,
        dma::{Control, Index},
        Engine, Irqs, Schedule as _,
    },
    emu::Emu,
    gpu::engine_3d::Engine3d,
    utils::schedule::RawTimestamp,
};

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Timing {
    Immediate,    // x
    VBlank,       // x
    HBlank,       // x
    DisplayStart, // -
    DisplayFifo,  // -
    DsSlot,       // x
    GbaSlot,      // -
    GxFifo,       // x
    Disabled,
}

impl<E: Engine> Arm9<E> {
    pub fn write_dma_channel_control(&mut self, i: Index, value: Control, engine_3d: &Engine3d) {
        let channel = &mut self.dma.channels[i.get() as usize];
        let prev_value = channel.control;
        channel.control = value;

        if !value.enabled() {
            channel.timing = Timing::Disabled;
            if prev_value.enabled() {
                // Handle self-disabling DMAs
                self.dma.running_channels &= !(1 << i.get());
                if self.dma.cur_channel == Some(i) {
                    self.find_next_dma_channel();
                    self.schedule.set_target_time(self.schedule.cur_time());
                }
            }
            return;
        }

        channel.timing = unsafe { core::mem::transmute(value.timing_arm9()) };

        let incr_shift = 1 + value.is_32_bit() as u8;
        channel.src_addr_incr = match value.src_addr_control() {
            0 => 1,
            1 => -1,
            // TODO: Confirm whether source address increment mode 3 is the same as mode 2 on the
            //       ARM9 (it is on the ARM7).
            _ => 0,
        } << incr_shift;
        channel.dst_addr_incr = match value.dst_addr_control() {
            1 => -1,
            2 => 0,
            _ => 1,
        } << incr_shift;

        channel.unit_count = value.0 & 0x1F_FFFF;
        if channel.unit_count == 0 {
            channel.unit_count = 0x20_0000;
        }

        // TODO: Check whether the repeat bit is actually ignored for immediate DMA on the ARM9 (it
        //       is on the ARM7).
        channel.repeat = value.repeat() && channel.timing != Timing::Immediate;

        if prev_value.enabled() {
            return;
        }

        let mask = !(1 | (value.is_32_bit() as u32) << 1);
        channel.cur_src_addr = channel.src_addr & mask;
        channel.cur_dst_addr = channel.dst_addr & mask;
        if channel.timing == Timing::GxFifo {
            if !value.is_32_bit() {
            #[cfg(feature = "log")]
            slog::warn!(self.logger, "GX FIFO DMA with 16-bit units");
        }
            channel.remaining_units = channel.unit_count;
        } else {
            channel.remaining_batch_units = channel.unit_count;
        }

        if channel.timing == Timing::Immediate {
            self.start_dma_transfer::<true, { Timing::Immediate }>(i);
        } else if channel.timing == Timing::GxFifo && engine_3d.gx_fifo_half_empty() {
            self.start_dma_transfer::<true, { Timing::GxFifo }>(i);
        }
    }

    #[inline]
    fn find_next_dma_channel(&mut self) {
        let trailing_zeros = self.dma.running_channels.trailing_zeros() as u8;
        if trailing_zeros < 4 {
            self.dma.cur_channel = Some(Index::new(trailing_zeros));
        } else {
            self.dma.cur_channel = None;
        }
    }

    fn start_dma_transfer<const NEED_SCHED_UPDATE: bool, const TIMING: Timing>(
        &mut self,
        i: Index,
    ) {
        let channel = &mut self.dma.channels[i.get() as usize];
        channel.next_access_is_nseq = true;
        if TIMING == Timing::GxFifo {
                channel.remaining_batch_units = channel
                    .remaining_units
                    .min(112 << channel.control.is_32_bit() as u8);
                channel.remaining_units -= channel.remaining_batch_units;
        }
        self.dma.running_channels |= 1 << i.get();
        if let Some(cur_i) = self.dma.cur_channel {
            if cur_i < i {
                return;
            }
            self.dma.channels[cur_i.get() as usize].next_access_is_nseq = true;
        }
        self.dma.cur_channel = Some(i);
        if NEED_SCHED_UPDATE {
            self.schedule.set_target_time(self.schedule.cur_time());
        }
    }

    pub(crate) fn start_dma_transfers_with_timing<const TIMING: Timing>(&mut self) {
        for i in 0..4 {
            let channel = &mut self.dma.channels[i as usize];
            if channel.timing == TIMING && self.dma.running_channels & 1 << i == 0 {
                self.start_dma_transfer::<false, TIMING>(Index::new(i));
            }
        }
    }

    fn end_or_pause_dma_transfer(&mut self, i: Index) {
        let channel = &mut self.dma.channels[i.get() as usize];
        if channel.timing != Timing::GxFifo || channel.remaining_units == 0 {
            if channel.repeat {
                if channel.control.dst_addr_control() == 3 {
                    let mask = !(1 | (channel.control.is_32_bit() as u32) << 1);
                    channel.cur_dst_addr = channel.dst_addr & mask;
                }
                if channel.timing == Timing::GxFifo {
                    channel.remaining_units = channel.unit_count;
                } else {
                    channel.remaining_batch_units = channel.unit_count;
                }
            } else {
                channel.control.set_enabled(false);
                channel.timing = Timing::Disabled;
            }
            if channel.control.fire_irq() {
                self.irqs.request_dma(i);
            }
        }
        self.dma.running_channels &= !(1 << i.get());
        if self.dma.cur_channel == Some(i) {
            self.find_next_dma_channel();
        }
    }

    pub(in super::super) fn run_dma_transfer(emu: &mut Emu<E>, i: Index) {
        let channel = &mut emu.arm9.dma.channels[i.get() as usize];
        let src_timings = emu.arm9.bus_timings.get(channel.cur_src_addr);
        let dst_timings = emu.arm9.bus_timings.get(channel.cur_dst_addr);

        if channel.control.is_32_bit() {
            let mut seq_timing = Timestamp(
                src_timings.s32_data as RawTimestamp + dst_timings.s32_data as RawTimestamp,
            );
            let mut unit_timing = if channel.next_access_is_nseq {
                Timestamp(
                    src_timings.n32_data as RawTimestamp + dst_timings.n32_data as RawTimestamp,
                )
            } else {
                seq_timing
            };

            while emu.arm9.schedule.cur_time() < emu.arm9.schedule.target_time() {
                let (src_addr, dst_addr) = {
                    let channel = &emu.arm9.dma.channels[i.get() as usize];
                    (channel.cur_src_addr, channel.cur_dst_addr)
                };

                let value = bus::read_32::<DmaAccess, _, false>(emu, src_addr);
                bus::write_32::<DmaAccess, _>(emu, dst_addr, value);

                emu.arm9
                    .schedule
                    .set_cur_time(emu.arm9.schedule.cur_time() + unit_timing);

                let channel = &mut emu.arm9.dma.channels[i.get() as usize];
                let prev_src_addr = channel.cur_src_addr;
                let prev_dst_addr = channel.cur_dst_addr;
                channel.cur_src_addr = channel
                    .cur_src_addr
                    .wrapping_add(channel.src_addr_incr as u32);
                channel.cur_dst_addr = channel
                    .cur_dst_addr
                    .wrapping_add(channel.dst_addr_incr as u32);

                if ((channel.cur_src_addr ^ prev_src_addr) | (channel.cur_dst_addr ^ prev_dst_addr))
                    >> Timings::PAGE_SHIFT
                    != 0
                {
                    seq_timing = Timestamp(
                        emu.arm9.bus_timings.get(channel.cur_src_addr).s32_data as RawTimestamp
                            + emu.arm9.bus_timings.get(channel.cur_dst_addr).s32_data
                                as RawTimestamp,
                    );
                }
                unit_timing = seq_timing;
                channel.next_access_is_nseq = false;

                channel.remaining_batch_units -= 1;
                if channel.remaining_batch_units == 0 {
                    if channel.timing == Timing::GxFifo
                        && emu.gpu.engine_3d.gx_fifo_half_empty()
                        && channel.remaining_units != 0
                    {
                        channel.remaining_batch_units = channel.remaining_units.min(112);
                        channel.remaining_units -= channel.remaining_batch_units;
                    } else {
                        emu.arm9.end_or_pause_dma_transfer(i);
                        break;
                    }
                }
            }
        } else {
            let mut seq_timing = Timestamp(
                src_timings.s16_data as RawTimestamp + dst_timings.s16_data as RawTimestamp,
            );
            let mut unit_timing = if channel.next_access_is_nseq {
                Timestamp(
                    src_timings.n16_data as RawTimestamp + dst_timings.n16_data as RawTimestamp,
                )
            } else {
                seq_timing
            };

            while emu.arm9.schedule.cur_time() < emu.arm9.schedule.target_time() {
                let (src_addr, dst_addr) = {
                    let channel = &emu.arm9.dma.channels[i.get() as usize];
                    (channel.cur_src_addr, channel.cur_dst_addr)
                };

                let value = bus::read_16::<DmaAccess, _>(emu, src_addr);
                bus::write_16::<DmaAccess, _>(emu, dst_addr, value);

                emu.arm9
                    .schedule
                    .set_cur_time(emu.arm9.schedule.cur_time() + unit_timing);

                let channel = &mut emu.arm9.dma.channels[i.get() as usize];
                let prev_src_addr = channel.cur_src_addr;
                let prev_dst_addr = channel.cur_dst_addr;
                channel.cur_src_addr = channel
                    .cur_src_addr
                    .wrapping_add(channel.src_addr_incr as u32);
                channel.cur_dst_addr = channel
                    .cur_dst_addr
                    .wrapping_add(channel.dst_addr_incr as u32);

                if ((channel.cur_src_addr ^ prev_src_addr) | (channel.cur_dst_addr ^ prev_dst_addr))
                    >> Timings::PAGE_SHIFT
                    != 0
                {
                    seq_timing = Timestamp(
                        emu.arm9.bus_timings.get(channel.cur_src_addr).s16_data as RawTimestamp
                            + emu.arm9.bus_timings.get(channel.cur_dst_addr).s16_data
                                as RawTimestamp,
                    );
                }
                unit_timing = seq_timing;
                channel.next_access_is_nseq = false;

                channel.remaining_batch_units -= 1;
                if channel.remaining_batch_units == 0 {
                    if channel.timing == Timing::GxFifo
                        && emu.gpu.engine_3d.gx_fifo_half_empty()
                        && channel.remaining_units != 0
                    {
                        channel.remaining_batch_units = channel.remaining_units.min(224);
                        channel.remaining_units -= channel.remaining_batch_units;
                    } else {
                        emu.arm9.end_or_pause_dma_transfer(i);
                        break;
                    }
                }
            }
        }
    }
}
