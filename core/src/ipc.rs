use crate::{
    cpu::{arm7, arm9},
    utils::{bitfield_debug, Fifo},
};

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct Sync(pub u16) {
        pub recv: u8 @ 0..=3,
        pub send: u8 @ 8..=11,
        pub send_irq: bool @ 13,
        pub irq_enabled: bool @ 14,
    }
}

bitfield_debug! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct FifoControl(pub u16) {
        pub send_fifo_empty: bool @ 0,
        pub send_fifo_full: bool @ 1,
        pub send_fifo_empty_irq_enabled: bool @ 2,
        pub clear_send_fifo: bool @ 3,
        pub recv_fifo_empty: bool @ 8,
        pub recv_fifo_full: bool @ 9,
        pub recv_fifo_not_empty_irq_enabled: bool @ 10,
        pub error: bool @ 14,
        pub fifos_enabled: bool @ 15,
    }
}

pub struct Ipc {
    sync_7: Sync,
    fifo_control_7: FifoControl,
    send_fifo_7: Fifo<u32, 16>,
    last_word_received_from_arm7: u32,
    sync_9: Sync,
    fifo_control_9: FifoControl,
    send_fifo_9: Fifo<u32, 16>,
    last_word_received_from_arm9: u32,
}

impl Ipc {
    pub(crate) fn new() -> Self {
        Ipc {
            sync_7: Sync(0),
            fifo_control_7: FifoControl(0x0101),
            send_fifo_7: Fifo::new(),
            last_word_received_from_arm7: 0,
            sync_9: Sync(0),
            fifo_control_9: FifoControl(0x0101),
            send_fifo_9: Fifo::new(),
            last_word_received_from_arm9: 0,
        }
    }

    #[inline]
    pub const fn sync_7(&self) -> Sync {
        self.sync_7
    }

    pub fn write_sync_7(&mut self, value: Sync, arm9_irqs: &mut arm9::Irqs) {
        self.sync_7.0 = (self.sync_7.0 & 0x000F) | (value.0 & 0x4F00);
        self.sync_9.0 = (self.sync_9.0 & 0x4F00) | (value.0 >> 8 & 0xF);
        if value.send_irq() && self.sync_9.irq_enabled() {
            arm9_irqs.write_requested(arm9_irqs.requested().with_ipc_sync(true), ());
        }
    }

    #[inline]
    pub const fn sync_9(&self) -> Sync {
        self.sync_9
    }

    pub fn write_sync_9(&mut self, value: Sync, arm7_irqs: &mut arm7::Irqs) {
        self.sync_9.0 = (self.sync_9.0 & 0x000F) | (value.0 & 0x4F00);
        self.sync_7.0 = (self.sync_7.0 & 0x4F00) | (value.0 >> 8 & 0xF);
        if value.send_irq() && self.sync_7.irq_enabled() {
            arm7_irqs.write_requested(arm7_irqs.requested().with_ipc_sync(true), ());
        }
    }

    #[inline]
    pub const fn fifo_control_7(&self) -> FifoControl {
        self.fifo_control_7
    }

    pub fn write_fifo_control_7(
        &mut self,
        value: FifoControl,
        arm7_irqs: &mut arm7::Irqs,
        arm7_schedule: &mut arm7::Schedule,
    ) {
        let prev_value = self.fifo_control_7;
        if value.clear_send_fifo() {
            self.send_fifo_7.clear();
            self.fifo_control_7 = self
                .fifo_control_7
                .with_send_fifo_empty(true)
                .with_send_fifo_full(false);
            self.fifo_control_9 = self
                .fifo_control_9
                .with_recv_fifo_empty(true)
                .with_recv_fifo_full(false);
            self.last_word_received_from_arm7 = 0;
        }
        self.fifo_control_7.0 =
            ((self.fifo_control_7.0 & 0x4303) | (value.0 & 0x8404)) & !(value.0 & 0x4000);
        if value.send_fifo_empty_irq_enabled()
            && self.fifo_control_7.send_fifo_empty()
            && (!prev_value.send_fifo_empty_irq_enabled() || !prev_value.send_fifo_empty())
        {
            arm7_irqs.write_requested(
                arm7_irqs.requested().with_ipc_send_fifo_empty(true),
                &mut *arm7_schedule,
            );
        }
        if value.recv_fifo_not_empty_irq_enabled()
            && !prev_value.recv_fifo_empty()
            && !prev_value.recv_fifo_not_empty_irq_enabled()
        {
            arm7_irqs.write_requested(
                arm7_irqs.requested().with_ipc_recv_fifo_not_empty(true),
                arm7_schedule,
            );
        }
    }

    pub fn send_7(&mut self, value: u32, arm9_irqs: &mut arm9::Irqs) {
        if !self.fifo_control_7.fifos_enabled() {
            return;
        }
        let was_empty = self.send_fifo_7.is_empty();
        if self.send_fifo_7.write(value).is_none() {
            self.fifo_control_7.set_error(true);
            return;
        }
        self.fifo_control_7 = self
            .fifo_control_7
            .with_send_fifo_empty(false)
            .with_send_fifo_full(self.send_fifo_7.is_full());
        self.fifo_control_9 = self
            .fifo_control_9
            .with_recv_fifo_empty(false)
            .with_recv_fifo_full(self.send_fifo_7.is_full());
        if self.fifo_control_9.recv_fifo_not_empty_irq_enabled() && was_empty {
            arm9_irqs.write_requested(arm9_irqs.requested().with_ipc_recv_fifo_not_empty(true), ());
        }
    }

    #[inline]
    pub fn peek_7(&self) -> u32 {
        self.send_fifo_9
            .peek()
            .unwrap_or(self.last_word_received_from_arm9)
    }

    pub fn recv_7(&mut self, arm9_irqs: &mut arm9::Irqs) -> u32 {
        if self.fifo_control_7.fifos_enabled() {
            if let Some(value) = self.send_fifo_9.read() {
                self.fifo_control_7 = self
                    .fifo_control_7
                    .with_recv_fifo_full(false)
                    .with_recv_fifo_empty(self.send_fifo_9.is_empty());
                self.fifo_control_9 = self
                    .fifo_control_9
                    .with_send_fifo_full(false)
                    .with_send_fifo_empty(self.send_fifo_9.is_empty());
                if self.fifo_control_9.send_fifo_empty_irq_enabled() && self.send_fifo_9.is_empty()
                {
                    arm9_irqs
                        .write_requested(arm9_irqs.requested().with_ipc_send_fifo_empty(true), ());
                }
                self.last_word_received_from_arm9 = value;
                value
            } else {
                self.fifo_control_7.set_error(true);
                self.last_word_received_from_arm9
            }
        } else {
            self.send_fifo_9
                .peek()
                .unwrap_or(self.last_word_received_from_arm9)
        }
    }

    #[inline]
    pub const fn fifo_control_9(&self) -> FifoControl {
        self.fifo_control_9
    }

    pub fn write_fifo_control_9(
        &mut self,
        value: FifoControl,
        arm9_irqs: &mut arm9::Irqs,
        arm9_schedule: &mut arm9::Schedule,
    ) {
        if value.clear_send_fifo() {
            self.send_fifo_9.clear();
            self.fifo_control_9 = self
                .fifo_control_9
                .with_send_fifo_empty(true)
                .with_send_fifo_full(false);
            self.fifo_control_7 = self
                .fifo_control_7
                .with_recv_fifo_empty(true)
                .with_recv_fifo_full(false);
            self.last_word_received_from_arm9 = 0;
        }
        let prev_value = self.fifo_control_9;
        self.fifo_control_9.0 =
            ((self.fifo_control_9.0 & 0x4303) | (value.0 & 0x8404)) & !(value.0 & 0x4000);
        if value.send_fifo_empty_irq_enabled()
            && self.fifo_control_9.send_fifo_empty()
            && (!prev_value.send_fifo_empty_irq_enabled() || !prev_value.send_fifo_empty())
        {
            arm9_irqs.write_requested(
                arm9_irqs.requested().with_ipc_send_fifo_empty(true),
                &mut *arm9_schedule,
            );
        }
        if value.recv_fifo_not_empty_irq_enabled()
            && !prev_value.recv_fifo_empty()
            && !prev_value.recv_fifo_not_empty_irq_enabled()
        {
            arm9_irqs.write_requested(
                arm9_irqs.requested().with_ipc_recv_fifo_not_empty(true),
                arm9_schedule,
            );
        }
    }

    pub fn send_9(&mut self, value: u32, arm7_irqs: &mut arm7::Irqs) {
        if !self.fifo_control_9.fifos_enabled() {
            return;
        }
        let was_empty = self.send_fifo_9.is_empty();
        if self.send_fifo_9.write(value).is_none() {
            self.fifo_control_9.set_error(true);
            return;
        }
        self.fifo_control_9 = self
            .fifo_control_9
            .with_send_fifo_empty(false)
            .with_send_fifo_full(self.send_fifo_9.is_full());
        self.fifo_control_7 = self
            .fifo_control_7
            .with_recv_fifo_empty(false)
            .with_recv_fifo_full(self.send_fifo_9.is_full());
        if self.fifo_control_7.recv_fifo_not_empty_irq_enabled() && was_empty {
            arm7_irqs.write_requested(arm7_irqs.requested().with_ipc_recv_fifo_not_empty(true), ());
        }
    }

    #[inline]
    pub fn peek_9(&self) -> u32 {
        self.send_fifo_7
            .peek()
            .unwrap_or(self.last_word_received_from_arm7)
    }

    pub fn recv_9(&mut self, arm7_irqs: &mut arm7::Irqs) -> u32 {
        if self.fifo_control_9.fifos_enabled() {
            if let Some(value) = self.send_fifo_7.read() {
                self.fifo_control_9 = self
                    .fifo_control_9
                    .with_recv_fifo_full(false)
                    .with_recv_fifo_empty(self.send_fifo_7.is_empty());
                self.fifo_control_7 = self
                    .fifo_control_7
                    .with_send_fifo_full(false)
                    .with_send_fifo_empty(self.send_fifo_7.is_empty());
                if self.fifo_control_7.send_fifo_empty_irq_enabled() && self.send_fifo_7.is_empty()
                {
                    arm7_irqs
                        .write_requested(arm7_irqs.requested().with_ipc_send_fifo_empty(true), ());
                }
                self.last_word_received_from_arm7 = value;
                value
            } else {
                self.fifo_control_9.set_error(true);
                self.last_word_received_from_arm7
            }
        } else {
            self.send_fifo_7
                .peek()
                .unwrap_or(self.last_word_received_from_arm7)
        }
    }
}
