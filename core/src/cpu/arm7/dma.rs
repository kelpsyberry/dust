use super::{bus::timings::Timings, Arm7};
use crate::{
    cpu::{
        arm7::{bus, Timestamp},
        bus::DmaAccess,
        dma::{Control, Index},
        Engine, Irqs, Schedule as _,
    },
    emu::Emu,
    utils::schedule::RawTimestamp,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Timing {
    Immediate, // x
    VBlank,    // x
    DsSlot,    // x
    WiFi,      // -
    GbaSlot,   // -
    Disabled,
}

impl<E: Engine> Arm7<E> {
    pub fn set_dma_channel_control(&mut self, i: Index, value: Control) {
        let channel = &mut self.dma.channels[i.get() as usize];
        let prev_value = channel.control;
        channel.control.0 = value.0 & (0xF7E0_0000 | channel.unit_count_mask());

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

        channel.timing = match value.timing_arm7() {
            0 => Timing::Immediate,
            1 => Timing::VBlank,
            2 => Timing::DsSlot,
            _ => {
                if i.get() & 1 == 0 {
                    Timing::WiFi
                } else {
                    Timing::GbaSlot
                }
            }
        };

        let incr_shift = 1 + value.is_32_bit() as u8;
        channel.src_addr_incr = match value.src_addr_control() {
            0 => 1,
            1 => -1,
            2 => 0,
            _ => unimplemented!("Tried to use prohibited DMA src address increment mode 3"),
        } << incr_shift;
        channel.dst_addr_incr = match value.dst_addr_control() {
            1 => -1,
            2 => 0,
            _ => 1,
        } << incr_shift;

        channel.unit_count = value.0 & channel.unit_count_mask();
        if channel.unit_count == 0 {
            channel.unit_count = channel.unit_count_mask() + 1;
        }

        // TODO: Check whether the repeat bit is actually ignored for immediate DMA
        channel.repeat = value.repeat() && channel.timing != Timing::Immediate;

        if self.dma.running_channels & 1 << i.get() != 0 {
            // TODO: Add the ID of any game that triggers this to a "ludi non gratae" list
            return;
        }

        let mask = !(1 | (value.is_32_bit() as u32) << 1);
        channel.cur_src_addr = channel.src_addr & mask;
        channel.cur_dst_addr = channel.dst_addr & mask;
        channel.remaining_units = channel.unit_count;
        channel.next_access_is_nseq = true;

        if channel.timing == Timing::Immediate {
            self.start_dma_transfer::<true>(i);
        }
    }

    fn find_next_dma_channel(&mut self) {
        let trailing_zeros = self.dma.running_channels.trailing_zeros() as u8;
        if trailing_zeros < 4 {
            self.dma.cur_channel = Some(Index::new(trailing_zeros));
        } else {
            self.dma.cur_channel = None;
        }
    }

    fn start_dma_transfer<const NEED_SCHED_UPDATE: bool>(&mut self, i: Index) {
        self.dma.running_channels |= 1 << i.get();
        if let Some(cur_i) = self.dma.cur_channel {
            if cur_i < i {
                return;
            }
        }
        self.dma.cur_channel = Some(i);
        if NEED_SCHED_UPDATE {
            self.schedule.set_target_time(self.schedule.cur_time());
        }
    }

    pub(crate) fn start_dma_transfers_with_timing(&mut self, timing: Timing) {
        for i in 0..4 {
            if self.dma.channels[i as usize].timing == timing {
                self.start_dma_transfer::<false>(Index::new(i));
            }
        }
    }

    fn end_dma_transfer(&mut self, i: Index) {
        let channel = &mut self.dma.channels[i.get() as usize];
        if channel.repeat {
            if channel.control.dst_addr_control() == 3 {
                let mask = !(1 | (channel.control.is_32_bit() as u32) << 1);
                channel.cur_dst_addr = channel.dst_addr & mask;
            }
            channel.remaining_units = channel.unit_count;
            channel.next_access_is_nseq = true;
        } else {
            channel.control.set_enabled(false);
            channel.timing = Timing::Disabled;
        }
        if channel.control.fire_irq() {
            self.irqs.request_dma(i);
        }
        self.dma.running_channels &= !(1 << i.get());
        if self.dma.cur_channel == Some(i) {
            self.find_next_dma_channel();
        }
    }

    pub(in super::super) fn run_dma_transfer(emu: &mut Emu<E>, i: Index) {
        let channel = &mut emu.arm7.dma.channels[i.get() as usize];
        let src_timings = emu.arm7.bus_timings.get(channel.cur_src_addr);
        let dst_timings = emu.arm7.bus_timings.get(channel.cur_dst_addr);

        if channel.control.is_32_bit() {
            let mut seq_timing =
                Timestamp(src_timings.s32 as RawTimestamp + dst_timings.s32 as RawTimestamp);
            let mut unit_timing = if channel.next_access_is_nseq {
                Timestamp(src_timings.n32 as RawTimestamp + dst_timings.n32 as RawTimestamp)
            } else {
                seq_timing
            };

            while emu.arm7.schedule.cur_time() < emu.arm7.schedule.target_time() {
                let (src_addr, dst_addr) = {
                    let channel = &emu.arm7.dma.channels[i.get() as usize];
                    (channel.cur_src_addr, channel.cur_dst_addr)
                };

                if src_addr >> 25 != 0 {
                    emu.arm7.last_dma_words[i.get() as usize] =
                        bus::read_32::<DmaAccess, _>(emu, src_addr);
                }
                bus::write_32::<DmaAccess, _>(
                    emu,
                    dst_addr,
                    emu.arm7.last_dma_words[i.get() as usize],
                );

                emu.arm7
                    .schedule
                    .set_cur_time(emu.arm7.schedule.cur_time() + unit_timing);

                let channel = &mut emu.arm7.dma.channels[i.get() as usize];
                let prev_src_addr = channel.cur_src_addr;
                let prev_dst_addr = channel.cur_dst_addr;
                channel.cur_src_addr = channel
                    .cur_src_addr
                    .wrapping_add(channel.src_addr_incr as u32);
                channel.cur_dst_addr = channel
                    .cur_dst_addr
                    .wrapping_add(channel.dst_addr_incr as u32);

                if (channel.cur_src_addr ^ prev_src_addr)
                    | (channel.cur_dst_addr ^ prev_dst_addr) >> Timings::PAGE_SHIFT
                    != 0
                {
                    seq_timing = Timestamp(
                        emu.arm7.bus_timings.get(channel.cur_src_addr).s32 as RawTimestamp
                            + emu.arm7.bus_timings.get(channel.cur_dst_addr).s32 as RawTimestamp,
                    );
                }
                unit_timing = seq_timing;
                channel.next_access_is_nseq = false;

                channel.remaining_units -= 1;
                if channel.remaining_units == 0 {
                    emu.arm7.end_dma_transfer(i);
                    break;
                }
            }
        } else {
            let mut seq_timing =
                Timestamp(src_timings.s16 as RawTimestamp + dst_timings.s16 as RawTimestamp);
            let mut unit_timing = if channel.next_access_is_nseq {
                Timestamp(src_timings.n16 as RawTimestamp + dst_timings.n16 as RawTimestamp)
            } else {
                seq_timing
            };

            while emu.arm7.schedule.cur_time() < emu.arm7.schedule.target_time() {
                let (src_addr, dst_addr) = {
                    let channel = &emu.arm7.dma.channels[i.get() as usize];
                    (channel.cur_src_addr, channel.cur_dst_addr)
                };

                // TODO: Check if the latched word's value is right
                if src_addr >> 25 != 0 {
                    let value = bus::read_16::<DmaAccess, _>(emu, src_addr);
                    emu.arm7.last_dma_words[i.get() as usize] = value as u32 | (value as u32) << 16;
                }
                bus::write_16::<DmaAccess, _>(
                    emu,
                    dst_addr,
                    (emu.arm7.last_dma_words[i.get() as usize] >> ((dst_addr & 2) << 3)) as u16,
                );

                emu.arm7
                    .schedule
                    .set_cur_time(emu.arm7.schedule.cur_time() + unit_timing);

                let channel = &mut emu.arm7.dma.channels[i.get() as usize];
                let prev_src_addr = channel.cur_src_addr;
                let prev_dst_addr = channel.cur_dst_addr;
                channel.cur_src_addr = channel
                    .cur_src_addr
                    .wrapping_add(channel.src_addr_incr as u32);
                channel.cur_dst_addr = channel
                    .cur_dst_addr
                    .wrapping_add(channel.dst_addr_incr as u32);

                if (channel.cur_src_addr ^ prev_src_addr)
                    | (channel.cur_dst_addr ^ prev_dst_addr) >> Timings::PAGE_SHIFT
                    != 0
                {
                    seq_timing = Timestamp(
                        emu.arm7.bus_timings.get(channel.cur_src_addr).s16 as RawTimestamp
                            + emu.arm7.bus_timings.get(channel.cur_dst_addr).s16 as RawTimestamp,
                    );
                }
                unit_timing = seq_timing;
                channel.next_access_is_nseq = false;

                channel.remaining_units -= 1;
                if channel.remaining_units == 0 {
                    emu.arm7.end_dma_transfer(i);
                    break;
                }
            }
        }
    }
}
