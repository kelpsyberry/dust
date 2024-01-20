// TODO:
// - Tracepoints
// - QAllow parsing

mod server;
use server::Server;
mod packets;

use ahash::AHashMap as HashMap;
use dust_core::{
    cpu::{
        self, arm7, arm9,
        bus::DebugCpuAccess,
        debug::{
            BreakpointHook, DataAbortHook, MemWatchpointHook, MemWatchpointRwMask,
            MemWatchpointTriggerCause as MemWatchpointCause, PrefetchAbortHook, UndefHook,
        },
    },
    emu::{CoreMask, Emu},
    utils::schedule::RawTimestamp,
};
use gdb_protocol::packet::{CheckedPacket, Kind as PacketKind};
use packets::{HandlePacketError, PacketError};
use std::{
    cell::RefCell, collections::VecDeque, io::Write, net::ToSocketAddrs, rc::Rc,
    result::Result as StdResult,
};

type Result<T = ()> = StdResult<T, gdb_protocol::Error>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ThreadId {
    All,
    Arm7,
    Arm9,
}

impl ThreadId {
    fn has_arm7(&self) -> bool {
        matches!(self, ThreadId::All | ThreadId::Arm7)
    }

    fn has_arm9(&self) -> bool {
        matches!(self, ThreadId::All | ThreadId::Arm9)
    }

    fn to_core(self) -> Option<Core> {
        match self {
            ThreadId::All => None,
            ThreadId::Arm7 => Some(Core::Arm7),
            ThreadId::Arm9 => Some(Core::Arm9),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Core {
    Arm7,
    Arm9,
}

impl Core {
    fn other(&self) -> Core {
        match self {
            Core::Arm7 => Core::Arm9,
            Core::Arm9 => Core::Arm7,
        }
    }

    fn to_thread_id(self) -> ThreadId {
        match self {
            Core::Arm7 => ThreadId::Arm7,
            Core::Arm9 => ThreadId::Arm9,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CoreStopCause {
    Break,                                  // Signal 0x00
    CyclesOver,                             // Signal 0x00
    Syscall(u8),                            // Signal 0x05 (SIGTRAP), syscall_entry
    Undefined,                              // Signal 0x04 (SIGILL)
    PrefetchAbort,                          // Signal 0x0B (SIGSEGV)
    DataAbort,                              // Signal 0x0B (SIGSEGV)
    SwBreakpoint,                           // Signal 0x05 (SIGTRAP), swbreak
    HwBreakpoint,                           // Signal 0x05 (SIGTRAP), hwbreak
    MemWatchpoint(u32, MemWatchpointCause), // Signal 0x05 (SIGTRAP), watch/rwatch/awatch
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StopCause {
    CoreStopped(CoreStopCause, Core),
    Shutdown, // Signal 0x00
}

impl StopCause {
    fn thread_id(&self) -> Option<ThreadId> {
        match self {
            StopCause::CoreStopped(_, core) => Some(core.to_thread_id()),
            _ => None,
        }
    }
}

struct StopCauses {
    queue: VecDeque<StopCause>,
    cores: [Option<StopCause>; 2],
}

impl StopCauses {
    fn push(&mut self, cause: StopCause) {
        self.queue.push_back(cause);
    }

    fn pop(&mut self) -> Option<StopCause> {
        let cause = self.queue.pop_front()?;
        match cause {
            StopCause::CoreStopped(_, core) => {
                self.cores[core as usize] = Some(cause);
            }
            _ => {
                self.cores = [Some(cause); 2];
            }
        }
        Some(cause)
    }

    fn flush(&mut self) {
        while self.pop().is_some() {}
    }
}

#[allow(clippy::unusual_byte_groupings)]
const ARM_SWBREAK: u32 = 0xE60_6DB5_0;
const THUMB_SWBREAK: u16 = 0xBBD6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VStoppedSequenceKind {
    New,
    Stopped { i: u8 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmuControlFlow {
    Continue,
    Reset,
}

pub struct GdbServer {
    #[cfg(feature = "logging")]
    logger: slog::Logger,
    server: Server,
    c_thread: ThreadId,
    g_thread: ThreadId,
    is_running: [bool; 2],
    pub remaining_step_cycles: [RawTimestamp; 2],
    is_in_non_stop_mode: bool,
    cur_vstopped_sequence_kind: Option<(VStoppedSequenceKind, ThreadId)>,
    just_connected: bool,
    sw_breakpoints: Rc<RefCell<HashMap<u32, (u32, u32)>>>,
    stop_causes: Rc<RefCell<StopCauses>>,
}

macro_rules! set_hook {
    (
        $self: ident, $emu: ident,
        ($fn: ident, $hook_ty: ty),
        [$(($core: ident, $core_enum: ident)),*],
        |$core_enum_ident: ident, $stop_causes_ident: ident| $hook: expr
    ) => {
        $(
            let $stop_causes_ident = Rc::clone(&$self.stop_causes);
            let $core_enum_ident = Core::$core_enum;
            $emu.$core.$fn(Some(<$hook_ty>::new(Box::new($hook))));
        )*
    }
}

impl GdbServer {
    pub fn new(
        addr: impl ToSocketAddrs,
        #[cfg(feature = "logging")] logger: slog::Logger,
    ) -> Result<Self> {
        Ok(GdbServer {
            #[cfg(feature = "logging")]
            logger,
            server: Server::new(addr)?,
            c_thread: ThreadId::Arm7,
            g_thread: ThreadId::Arm7,
            is_running: [true; 2],
            remaining_step_cycles: [0; 2],
            is_in_non_stop_mode: false,
            cur_vstopped_sequence_kind: None,
            just_connected: false,
            sw_breakpoints: Rc::new(RefCell::new(HashMap::default())),
            stop_causes: Rc::new(RefCell::new(StopCauses {
                queue: VecDeque::new(),
                cores: [None, None],
            })),
        })
    }

    #[inline]
    pub fn is_running(&self) -> bool {
        self.is_running[0] || self.is_running[1] || self.is_in_non_stop_mode
    }

    pub fn attach<E: cpu::Engine>(&mut self, emu: &mut Emu<E>) {
        set_hook!(
            self,
            emu,
            (set_undef_hook, UndefHook<E>),
            [(arm7, Arm7), (arm9, Arm9)],
            |core, stop_causes| move |_emu, instr, thumb| {
                let is_swbreak = if thumb {
                    instr as u16 == THUMB_SWBREAK
                } else {
                    instr == ARM_SWBREAK
                };
                stop_causes.borrow_mut().push(StopCause::CoreStopped(
                    if is_swbreak {
                        CoreStopCause::SwBreakpoint
                    } else {
                        CoreStopCause::Undefined
                    },
                    core,
                ));
                true
            }
        );

        set_hook!(
            self,
            emu,
            (set_prefetch_abort_hook, PrefetchAbortHook<E>),
            [(arm9, Arm9)],
            |_core, stop_causes| move |_emu| {
                stop_causes.borrow_mut().push(StopCause::CoreStopped(
                    CoreStopCause::PrefetchAbort,
                    Core::Arm9,
                ));
                true
            }
        );

        set_hook!(
            self,
            emu,
            (set_data_abort_hook, DataAbortHook<E>),
            [(arm9, Arm9)],
            |_core, stop_causes| move |_emu, _addr| {
                stop_causes
                    .borrow_mut()
                    .push(StopCause::CoreStopped(CoreStopCause::DataAbort, Core::Arm9));
                true
            }
        );

        set_hook!(
            self,
            emu,
            (set_breakpoint_hook, BreakpointHook<E>),
            [(arm7, Arm7), (arm9, Arm9)],
            |core, stop_causes| move |_emu, _addr| {
                stop_causes
                    .borrow_mut()
                    .push(StopCause::CoreStopped(CoreStopCause::HwBreakpoint, core));
                true
            }
        );

        set_hook!(
            self,
            emu,
            (set_mem_watchpoint_hook, MemWatchpointHook<E>),
            [(arm7, Arm7), (arm9, Arm9)],
            |core, stop_causes| move |_emu, addr, _size, cause| {
                stop_causes.borrow_mut().push(StopCause::CoreStopped(
                    CoreStopCause::MemWatchpoint(addr, cause),
                    core,
                ));
                true
            }
        );
    }

    pub fn detach<E: cpu::Engine>(&mut self, emu: &mut Emu<E>) {
        self.server.close();
        self.c_thread = ThreadId::Arm7;
        self.g_thread = ThreadId::Arm7;
        self.is_running = [true; 2];
        self.remaining_step_cycles = [0; 2];
        self.is_in_non_stop_mode = false;
        self.cur_vstopped_sequence_kind = None;
        self.just_connected = false;
        self.sw_breakpoints.borrow_mut().clear();
        {
            let mut stop_causes = self.stop_causes.borrow_mut();
            stop_causes.queue.clear();
            stop_causes.cores = [None; 2];
        }

        emu.arm7.set_swi_hook(None);
        emu.arm7.set_undef_hook(None);
        emu.arm7.set_breakpoint_hook(None);
        emu.arm7.set_mem_watchpoint_hook(None);
        emu.arm7.clear_breakpoints();
        emu.arm7.clear_mem_watchpoints();
        emu.arm7.is_stopped = false;

        emu.arm9.set_swi_hook(None);
        emu.arm9.set_undef_hook(None);
        emu.arm9.set_prefetch_abort_hook(None);
        emu.arm9.set_data_abort_hook(None);
        emu.arm9.set_breakpoint_hook(None);
        emu.arm9.set_mem_watchpoint_hook(None);
        emu.arm9.clear_breakpoints();
        emu.arm9.clear_mem_watchpoints();
        emu.arm9.is_stopped = false;
    }

    fn send_empty_packet(&mut self) -> Result {
        self.server.send(&CheckedPacket::empty())
    }

    fn send_packet(&mut self, data: Vec<u8>) -> Result {
        self.server
            .send(&CheckedPacket::from_data(PacketKind::Packet, data))
    }

    fn send_packet_slice(&mut self, data: &[u8]) -> Result {
        self.send_packet(data.to_vec())
    }

    fn send_notif(&mut self, data: Vec<u8>) -> Result {
        self.server
            .send(&CheckedPacket::from_data(PacketKind::Notification, data))
    }

    fn toggle_sw_breakpoint<E: cpu::Engine, const SET: bool>(
        &mut self,
        emu: &mut Emu<E>,
        addr: u32,
        is_thumb: bool,
    ) {
        let mut sw_breakpoints = self.sw_breakpoints.borrow_mut();
        if SET {
            if !sw_breakpoints.contains_key(&addr) {
                macro_rules! write {
                    ($core: ident$(, $code: expr)?) => {
                        if is_thumb {
                            let prev =
                                $core::bus::read_16::<DebugCpuAccess, _>(emu, addr)
                                    as u32;
                            $core::bus::write_16::<DebugCpuAccess, _>(
                                emu, addr, THUMB_SWBREAK,
                            );
                            prev
                        } else {
                            let prev =
                                $core::bus::read_32::<DebugCpuAccess, _$(, $code)*>(emu, addr);
                            $core::bus::write_32::<DebugCpuAccess, _>(
                                emu, addr, ARM_SWBREAK,
                            );
                            prev
                        }
                    };
                }
                sw_breakpoints.insert(
                    addr,
                    (
                        // NB: The order is important (if the area is available to both cores, the
                        // ARM7 value has to be saved first and restored last)
                        write!(arm7),
                        write!(arm9, false),
                    ),
                );
            }
        } else if let Some((arm7, arm9)) = sw_breakpoints.remove(&addr) {
            macro_rules! write {
                ($core: ident) => {
                    if is_thumb {
                        $core::bus::write_16::<DebugCpuAccess, _>(emu, addr, $core as u16);
                    } else {
                        $core::bus::write_32::<DebugCpuAccess, _>(emu, addr, $core);
                    }
                };
            }
            // NB: The order is important (see previous comment)
            write!(arm9);
            write!(arm7);
        }
    }

    fn toggle_hw_breakpoint<E: cpu::Engine, const SET: bool>(
        &mut self,
        emu: &mut Emu<E>,
        addr: u32,
    ) {
        if SET {
            emu.arm7.add_breakpoint(addr);
            emu.arm9.add_breakpoint(addr);
        } else {
            emu.arm7.remove_breakpoint(addr);
            emu.arm9.remove_breakpoint(addr);
        }
    }

    fn toggle_watchpoint<E: cpu::Engine, const READ: bool, const WRITE: bool, const SET: bool>(
        &mut self,
        emu: &mut Emu<E>,
        addr: u32,
        size: u8,
    ) {
        let mut mask = MemWatchpointRwMask::empty();
        if READ {
            mask |= MemWatchpointRwMask::READ;
        }
        if WRITE {
            mask |= MemWatchpointRwMask::WRITE;
        }
        if SET {
            emu.arm7.add_mem_watchpoint(addr, size, mask);
            emu.arm9.add_mem_watchpoint(addr, size, mask);
        } else {
            emu.arm7.remove_mem_watchpoint(addr, size, mask);
            emu.arm9.remove_mem_watchpoint(addr, size, mask);
        }
    }

    fn encode_stop_reply(&mut self, cause: StopCause, buf: &mut Vec<u8>) {
        match cause {
            StopCause::CoreStopped(cause, core) => {
                match cause {
                    CoreStopCause::Break | CoreStopCause::CyclesOver => {
                        buf.extend_from_slice(b"T00");
                    }
                    CoreStopCause::Syscall(number) => {
                        let _ = write!(buf, "T05syscall_entry:{number:X};",);
                    }
                    CoreStopCause::Undefined => {
                        buf.extend_from_slice(b"T04");
                    }
                    CoreStopCause::PrefetchAbort | CoreStopCause::DataAbort => {
                        buf.extend_from_slice(b"T0B");
                    }
                    CoreStopCause::SwBreakpoint => {
                        buf.extend_from_slice(b"T05hwbreak:;");
                    }
                    CoreStopCause::HwBreakpoint => {
                        buf.extend_from_slice(b"T05hwbreak:;");
                    }
                    CoreStopCause::MemWatchpoint(addr, cause) => {
                        let _ = write!(
                            buf,
                            "T05{}:{addr:08X};",
                            if cause == MemWatchpointCause::Read {
                                "rwatch"
                            } else {
                                "watch"
                            },
                        );
                    }
                }
                let _ = write!(
                    buf,
                    "thread:{:02X};core:{:02X};",
                    core as u8 + 1,
                    core as u8
                );
            }
            StopCause::Shutdown => {
                buf.extend_from_slice(b"X00");
            }
        }
    }

    fn send_stop_cause_packet(&mut self, stop_cause: StopCause) -> Result {
        let mut data = Vec::new();
        self.encode_stop_reply(stop_cause, &mut data);
        self.send_packet(data)
    }

    fn send_stop_cause_notif(&mut self, stop_cause: StopCause) -> Result {
        let mut data = b"Stop:".to_vec();
        self.encode_stop_reply(stop_cause, &mut data);
        self.send_notif(data)
    }

    pub fn emu_shutdown(&mut self) {
        if !self.server.is_running() {
            return;
        };
        let mut stop_causes = self.stop_causes.borrow_mut();
        stop_causes.queue.push_back(StopCause::Shutdown);
    }

    pub fn cycles_over<E: cpu::Engine>(&mut self, emu: &mut Emu<E>, core_mask: CoreMask) {
        if !self.server.is_running() {
            return;
        };
        let mut stop_causes = self.stop_causes.borrow_mut();
        if core_mask.contains(CoreMask::ARM7) {
            emu.arm7.is_stopped = true;
            stop_causes.push(StopCause::CoreStopped(
                CoreStopCause::CyclesOver,
                Core::Arm7,
            ));
        }
        if core_mask.contains(CoreMask::ARM9) {
            emu.arm9.is_stopped = true;
            stop_causes.push(StopCause::CoreStopped(
                CoreStopCause::CyclesOver,
                Core::Arm9,
            ));
        }
    }

    fn manually_stop<E: cpu::Engine>(&mut self, emu: &mut Emu<E>) {
        let was_stopped = [emu.arm7.is_stopped, emu.arm9.is_stopped];
        emu.arm7.is_stopped = true;
        emu.arm9.is_stopped = true;
        let mut stop_causes = self.stop_causes.borrow_mut();
        let core = self.g_thread.to_core().unwrap_or(Core::Arm7);
        for core in [core, core.other()] {
            if !was_stopped[core as usize] {
                stop_causes.push(StopCause::CoreStopped(CoreStopCause::Break, core));
            }
        }
    }

    fn all_stop_stop_all_cores<E: cpu::Engine>(&mut self, emu: &mut Emu<E>) {
        self.is_running = [false; 2];
        emu.arm7.is_stopped = true;
        emu.arm9.is_stopped = true;
    }

    fn send_all_stop_stop_reply(&mut self) -> Result {
        // NOTE: Excluding rare conflicts, the stop cause queue should only have one
        // entry. If a conflict happens, every stop cause other than the first to get
        // fired will be dropped.
        let stop_cause = {
            let mut stop_causes = self.stop_causes.borrow_mut();
            let stop_cause = if stop_causes.queue.contains(&StopCause::Shutdown) {
                StopCause::Shutdown
            } else {
                stop_causes.pop().unwrap()
            };
            stop_causes.flush();
            stop_cause
        };
        self.g_thread = stop_cause.thread_id().unwrap_or(ThreadId::Arm7);
        self.send_stop_cause_packet(stop_cause)
    }

    fn poll_stop_causes<E: cpu::Engine>(&mut self, emu: &mut Emu<E>) -> Result {
        if self.just_connected || self.cur_vstopped_sequence_kind.is_some() {
            return Ok(());
        }
        let mut stop_causes = self.stop_causes.borrow_mut();
        if !stop_causes.queue.is_empty() {
            if self.is_in_non_stop_mode {
                let stop_cause = stop_causes.pop().unwrap();
                drop(stop_causes);
                self.send_stop_cause_notif(stop_cause)?;
                self.cur_vstopped_sequence_kind = Some((
                    VStoppedSequenceKind::New,
                    stop_cause.thread_id().unwrap_or(ThreadId::All),
                ));
            } else {
                drop(stop_causes);
                self.send_all_stop_stop_reply()?;
                self.all_stop_stop_all_cores(emu);
            }
        }
        Ok(())
    }

    fn poll_inner<E: cpu::Engine>(&mut self, emu: &mut Emu<E>) -> Result<EmuControlFlow> {
        // Check for a connection to be present
        if !self.server.is_running() {
            if self.server.poll_listener() {
                self.attach(emu);
                self.just_connected = true;
            } else {
                return Ok(EmuControlFlow::Continue);
            }
        }

        // Process all-stop mode break sequences while running (otherwise just remove queued ones)
        if self.server.try_recv_break()?
            && !self.is_in_non_stop_mode
            && self.is_running()
            && self.stop_causes.borrow().queue.is_empty()
            && !self.just_connected
        {
            self.manually_stop(emu);
        }

        // Send packets/notifications queued stop causes (if not in a `vStopped` sequence)
        self.poll_stop_causes(emu)?;

        // In all-stop mode, don't process packets while running
        if !self.is_in_non_stop_mode && self.is_running() && !self.just_connected {
            return Ok(EmuControlFlow::Continue);
        }

        // Process packets as long as the server doesn't detach
        while self.server.is_running() {
            match self.server.try_recv_packet()? {
                Some(packet) => {
                    let mut packet = packet.invalidate_check();

                    if packet.kind != PacketKind::Packet {
                        #[cfg(feature = "logging")]
                        slog::warn!(self.logger, "Received unknown notification packet");
                        continue;
                    }

                    match self.handle_packet(emu, &mut packet.data) {
                        Ok(EmuControlFlow::Reset) => return Ok(EmuControlFlow::Reset),

                        Ok(EmuControlFlow::Continue) => {}

                        Err(HandlePacketError::Packet(e)) => {
                            #[cfg(feature = "logging")]
                            let packet_str =
                                std::str::from_utf8(&packet.data).unwrap_or("<invalid UTF-8>");
                            match e {
                                PacketError::Unrecognized => {
                                    if packet.data != b"vMustReplyEmpty" {
                                        #[cfg(feature = "logging")]
                                        slog::warn!(
                                            self.logger,
                                            "Received unrecognized packet: {packet_str}",
                                        );
                                    }
                                    self.send_empty_packet()?;
                                }
                                PacketError::Parsing => {
                                    #[cfg(feature = "logging")]
                                    slog::warn!(
                                        self.logger,
                                        "Couldn't parse packet parameters: {packet_str}",
                                    );
                                    self.send_empty_packet()?;
                                }
                                PacketError::InvalidParams => {
                                    #[cfg(feature = "logging")]
                                    slog::warn!(
                                        self.logger,
                                        "Invalid packet parameters: {packet_str}"
                                    );
                                    self.send_packet_slice(b"E00")?;
                                }
                                PacketError::UnrecognizedParams => {
                                    #[cfg(feature = "logging")]
                                    slog::warn!(
                                        self.logger,
                                        "Unrecognized packet parameters: {packet_str}",
                                    );
                                    self.send_empty_packet()?;
                                }
                                PacketError::MultipleThreadsSelected => {
                                    #[cfg(feature = "logging")]
                                    slog::warn!(
                                        self.logger,
                                        "Multiple threads selected for packet: {packet_str}",
                                    );
                                    self.send_packet_slice(b"E00")?;
                                }
                            }
                        }

                        Err(HandlePacketError::GdbProtocol(e)) => return Err(e),
                    }
                }

                None => break,
            }
        }

        Ok(EmuControlFlow::Continue)
    }

    pub fn poll<E: cpu::Engine>(&mut self, emu: &mut Emu<E>) -> EmuControlFlow {
        match self.poll_inner(emu) {
            Ok(control_flow) => control_flow,
            Err(err) => {
                error!(
                    "GDB server error",
                    "Terminating GDB server due to protocol error: {err}",
                );
                self.detach(emu);
                EmuControlFlow::Continue
            }
        }
    }
}
