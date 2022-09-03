// TODO:
// - Tracepoints
// - Host I/O with vFile, maybe using the DS cart's filesystem?
// - Extended mode, enabling R, vAttach and vRun, as well as some set packets, but only the first
//   one has a sensible interpretation
// - Non-stop mode
// - QAllow parsing

mod server;
use server::Server;

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
    emu::Emu,
    utils::schedule::RawTimestamp,
};
use gdb_protocol::packet::{CheckedPacket, Kind as PacketKind};
use std::{cell::RefCell, io::Write, net::ToSocketAddrs, rc::Rc, str};

bitflags::bitflags! {
    struct ThreadMask: u8 {
        const ARM9 = 1 << 0;
        const ARM7 = 1 << 1;
    }
}

impl ThreadMask {
    pub fn contains_multiple(self) -> bool {
        self.bits() & (self.bits().wrapping_sub(1)) != 0
    }
}

#[derive(Clone, Copy)]
struct ThreadId {
    id: i8,
    mask: ThreadMask,
}

impl ThreadId {
    fn new(id: i8, mask: ThreadMask) -> Self {
        ThreadId { id, mask }
    }

    fn from_value(value: i8, default: Self) -> Self {
        match value {
            -1 => Self::new(-1, ThreadMask::all()),
            0 => default,
            1 => Self::new(1, ThreadMask::ARM9),
            _ => Self::new(2, ThreadMask::ARM7),
        }
    }
}

#[derive(Clone, Copy)]
struct Breakpoint {}

#[derive(Clone, Copy)]
struct Watchpoint {}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Core {
    Arm9,
    Arm7,
}

#[derive(Clone, Copy)]
enum StopCause {
    Break,                                        // Signal 0x00 (None)
    Syscall(u8, u8),                              // Signal 0x05 (SIGTRAP), syscall_entry
    Undefined(Core),                              // Signal 0x04 (SIGILL)
    PrefetchAbort,                                // Signal 0x0B (SIGSEGV)
    DataAbort,                                    // Signal 0x0B (SIGSEGV)
    Breakpoint(Core),                             // Signal 0x05 (SIGTRAP), hwbreak
    MemWatchpoint(Core, u32, MemWatchpointCause), // Signal 0x05 (SIGTRAP), watch/rwatch/awatch
    Shutdown,                                     // Signal 0x0F (SIGTERM)
}

pub struct GdbServer {
    server: Server,
    c_thread: ThreadId,
    g_thread: ThreadId,
    target_stopped: bool,
    pub remaining_step_cycles: RawTimestamp,
    waiting_for_stop: bool,
    sw_breakpoints: HashMap<u32, (u32, u32)>,
    stop_cause: Rc<RefCell<StopCause>>,
}

static CRC_TABLE: [u32; 256] = {
    let mut table = [0; 256];
    let mut crc = 0x8000_0000;
    let mut i = 1;
    while i < 256 {
        if crc & 0x8000_0000 == 0 {
            crc <<= 1;
        } else {
            crc = crc << 1 ^ 0x04C1_1DB7;
        }
        let mut j = i;
        while j < i << 1 {
            table[j] ^= crc;
            j += 1;
        }
        i <<= 1;
    }
    table
};

fn split_once(data: &[u8], char: u8) -> (&[u8], &[u8]) {
    if let Some(split_pos) = data.iter().position(|c| *c == char) {
        (&data[..split_pos], &data[split_pos + 1..])
    } else {
        (data, &[])
    }
}

trait IntoVec<T> {
    fn into_vec(self) -> Vec<T>;
}

impl<T> IntoVec<T> for Vec<T> {
    fn into_vec(self) -> Vec<T> {
        self
    }
}

impl<T, const LEN: usize> IntoVec<T> for &[T; LEN]
where
    T: Clone,
{
    fn into_vec(self) -> Vec<T> {
        self[..].into()
    }
}

impl GdbServer {
    pub fn new(addr: impl ToSocketAddrs) -> Result<Self, gdb_protocol::Error> {
        Ok(GdbServer {
            server: Server::new(addr)?,
            c_thread: ThreadId::new(1, ThreadMask::ARM9),
            g_thread: ThreadId::new(1, ThreadMask::ARM9),
            target_stopped: false,
            remaining_step_cycles: 0,
            waiting_for_stop: false,
            sw_breakpoints: HashMap::default(),
            stop_cause: Rc::new(RefCell::new(StopCause::Break)),
        })
    }

    #[inline]
    pub fn target_stopped(&self) -> bool {
        self.target_stopped
    }

    pub fn attach<E: cpu::Engine>(&mut self, emu: &mut Emu<E>) {
        macro_rules! set_hook {
            (
                ($fn: ident, $hook_ty: ty),
                [$(($core: ident, $core_enum: ident)),*],
                |$core_enum_ident: ident, $stop_cause_ident: ident| $hook: expr
            ) => {
                $(
                    let $stop_cause_ident = Rc::clone(&self.stop_cause);
                    let $core_enum_ident = Core::$core_enum;
                    emu.$core.$fn(Some(<$hook_ty>::new(Box::new($hook))));
                )*
            }
        }

        set_hook!(
            (set_undef_hook, UndefHook<E>),
            [(arm7, Arm7), (arm9, Arm9)],
            |core, stop_cause| move |_emu| {
                *stop_cause.borrow_mut() = StopCause::Undefined(core);
                true
            }
        );

        set_hook!(
            (set_prefetch_abort_hook, PrefetchAbortHook<E>),
            [(arm9, Arm9)],
            |_core, stop_cause| move |_emu| {
                *stop_cause.borrow_mut() = StopCause::PrefetchAbort;
                true
            }
        );

        set_hook!(
            (set_data_abort_hook, DataAbortHook<E>),
            [(arm9, Arm9)],
            |_core, stop_cause| move |_emu, _addr| {
                *stop_cause.borrow_mut() = StopCause::DataAbort;
                true
            }
        );

        set_hook!(
            (set_breakpoint_hook, BreakpointHook<E>),
            [(arm7, Arm7), (arm9, Arm9)],
            |core, stop_cause| move |_emu, _addr| {
                *stop_cause.borrow_mut() = StopCause::Breakpoint(core);
                true
            }
        );

        set_hook!(
            (set_mem_watchpoint_hook, MemWatchpointHook<E>),
            [(arm7, Arm7), (arm9, Arm9)],
            |core, stop_cause| move |_emu, addr, _size, cause| {
                *stop_cause.borrow_mut() = StopCause::MemWatchpoint(core, addr, cause);
                true
            }
        );
    }

    fn detach<E: cpu::Engine>(&mut self, emu: &mut Emu<E>) {
        self.target_stopped = false;
        self.server.close();

        emu.arm9.set_swi_hook(None);
        emu.arm9.set_undef_hook(None);
        emu.arm9.set_prefetch_abort_hook(None);
        emu.arm9.set_data_abort_hook(None);
        emu.arm9.set_breakpoint_hook(None);
        emu.arm9.set_mem_watchpoint_hook(None);
        emu.arm9.clear_breakpoints();
        emu.arm9.clear_mem_watchpoints();
        emu.arm9.stopped = false;

        emu.arm7.set_swi_hook(None);
        emu.arm7.set_undef_hook(None);
        emu.arm7.set_breakpoint_hook(None);
        emu.arm7.set_mem_watchpoint_hook(None);
        emu.arm7.clear_breakpoints();
        emu.arm7.clear_mem_watchpoints();
        emu.arm7.stopped = false;
    }

    fn send_empty_packet(&mut self) {
        let _ = self.server.send(&CheckedPacket::empty());
    }

    fn send_packet(&mut self, data: Vec<u8>) {
        self.server
            .send(&CheckedPacket::from_data(PacketKind::Packet, data))
            .unwrap();
    }

    fn send_notif(&mut self, data: Vec<u8>) {
        self.server
            .send(&CheckedPacket::from_data(PacketKind::Packet, data))
            .unwrap();
    }

    fn encode_stop_reply(&mut self, cause: StopCause, buf: &mut Vec<u8>) {
        match cause {
            StopCause::Break => buf.extend_from_slice(b"S00"),
            StopCause::Syscall(number, core) => {
                let _ = write!(
                    buf,
                    "T05syscall_entry:{number:X};thread:{};core:{};",
                    core as u8 + 1,
                    core as u8
                );
            }
            StopCause::Undefined(core) => {
                let _ = write!(buf, "T04thread:{};core:{};", core as u8 + 1, core as u8);
            }
            StopCause::PrefetchAbort | StopCause::DataAbort => {
                buf.extend_from_slice(b"T0Bthread:1;core:0;");
            }
            StopCause::Breakpoint(core) => {
                let _ = write!(
                    buf,
                    "T05hwbreak:;thread:{};core:{};",
                    core as u8 + 1,
                    core as u8
                );
            }
            StopCause::MemWatchpoint(core, addr, cause) => {
                let _ = write!(
                    buf,
                    "T05{}:{addr:08X};thread:{};core:{};",
                    if cause == MemWatchpointCause::Read {
                        "rwatch"
                    } else {
                        "watch"
                    },
                    core as u8 + 1,
                    core as u8
                );
            }
            StopCause::Shutdown => buf.extend_from_slice(b"X0F"),
        }
    }

    fn send_stop_reason<E: cpu::Engine>(&mut self, _emu: &mut Emu<E>) {
        let stop_cause = *self.stop_cause.borrow();
        let mut reply = Vec::new();
        self.encode_stop_reply(stop_cause, &mut reply);
        self.send_packet(reply);
    }

    pub fn emu_stopped<E: cpu::Engine>(&mut self, emu: &mut Emu<E>) {
        self.target_stopped = true;
        if self.waiting_for_stop {
            self.waiting_for_stop = false;
            self.send_stop_reason(emu);
        }
    }

    pub fn emu_shutdown<E: cpu::Engine>(&mut self, _emu: &mut Emu<E>) {
        *self.stop_cause.borrow_mut() = StopCause::Shutdown;
    }

    fn manually_stop<E: cpu::Engine>(&mut self, emu: &mut Emu<E>) {
        *self.stop_cause.borrow_mut() = StopCause::Break;
        self.target_stopped = true;
        self.waiting_for_stop = false;
        self.send_stop_reason(emu);
    }

    fn wait_for_stop(&mut self) {
        self.target_stopped = false;
        self.waiting_for_stop = true;
    }

    fn toggle_breakpoint<E: cpu::Engine, const SET: bool>(&mut self, emu: &mut Emu<E>, addr: u32) {
        if self.g_thread.mask.contains(ThreadMask::ARM9) {
            if SET {
                emu.arm9.add_breakpoint(addr);
            } else {
                emu.arm9.remove_breakpoint(addr);
            }
        }
        if self.g_thread.mask.contains(ThreadMask::ARM7) {
            if SET {
                emu.arm7.add_breakpoint(addr);
            } else {
                emu.arm7.remove_breakpoint(addr);
            }
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
        if self.g_thread.mask.contains(ThreadMask::ARM9) {
            if SET {
                emu.arm9.add_mem_watchpoint(addr, size, mask);
            } else {
                emu.arm9.remove_mem_watchpoint(addr, size, mask);
            }
        }
        if self.g_thread.mask.contains(ThreadMask::ARM7) {
            if SET {
                emu.arm7.add_mem_watchpoint(addr, size, mask);
            } else {
                emu.arm7.remove_mem_watchpoint(addr, size, mask);
            }
        }
    }

    fn handle_packet<E: cpu::Engine>(&mut self, emu: &mut Emu<E>, packet: &[u8]) -> bool {
        macro_rules! reply {
            () => {{
                self.send_empty_packet();
                return false;
            }};
            ($reply: expr) => {{
                self.send_packet($reply.into_vec());
                return false;
            }};
        }

        macro_rules! err {
            (($log: expr$(, $($args: tt)*)?) $(, $reply: expr)?) => {{
                eprintln!(concat!("[GDB] ", $log)$(, $($args)*)*);
                reply!($($reply)*);
            }};
        }

        macro_rules! unwrap_res {
            ($value: expr, ($log: expr$(, $($args: tt)*)?) $(, $reply: expr)?) => {
                match $value {
                    Ok(value) => value,
                    Err(err) => {
                        err!(($log, err$(, $($args)*)*) $(, $reply)*);
                    }
                }
            };
        }

        macro_rules! unwrap_opt {
            ($value: expr, ($log: expr$(, $($args: tt)*)?) $(, $reply: expr)?) => {
                match $value {
                    Some(value) => value,
                    None => {
                        err!(($log$(, $($args)*)*) $(, $reply)*);
                    }
                }
            };
        }

        macro_rules! parse_int {
            ($data: expr, $ty: ty, $name: literal, $packet_name: literal) => {{
                unwrap_res!(
                    <$ty>::from_str_radix(
                        unwrap_res!(
                            str::from_utf8($data),
                            (concat!(
                                "Invalid unicode in ",
                                $packet_name,
                                " packet ",
                                $name,
                                ": {}"
                            ))
                        ),
                        16
                    ),
                    (concat!("Couldn't parse ", $packet_name, " packet ", $name, ": {}"))
                )
            }};
        }

        macro_rules! parse_addr_length {
            ($args: expr, $packet_name: literal) => {{
                let (addr, length) = split_once($args, b',');
                (
                    parse_int!(addr, u32, "addr", $packet_name),
                    parse_int!(length, u32, "length", $packet_name),
                )
            }};
        }

        macro_rules! parse_addr_kind {
            ($args: expr, $packet_name: literal) => {{
                let (addr, kind) = split_once($args, b',');
                let addr = parse_int!(addr, u32, "addr", $packet_name);
                let kind = parse_int!(kind, u8, "kind", $packet_name);
                if !(2..=4).contains(&kind) {
                    err!(
                        (
                            concat!(
                                "Received invalid ",
                                $packet_name,
                                " packet breakpoint kind: {}"
                            ),
                            kind
                        ),
                        b"E00"
                    );
                }
                (addr, kind)
            }};
        }

        macro_rules! parse_thread_id {
            ($id: expr, $packet_name: literal) => {{
                let thread_id = parse_int!($id, i8, "thread ID", $packet_name);
                if !(-1..=2).contains(&thread_id) {
                    err!(
                        (
                            concat!("Received invalid ", $packet_name, " packet thread ID: {}"),
                            thread_id
                        ),
                        b"E00"
                    );
                }
                thread_id
            }};
        }

        macro_rules! parse_reg_index {
            ($index: expr, $packet_name: literal) => {{
                let reg_index = parse_int!($index, u8, "reg index", $packet_name);
                if !(0..=16).contains(&reg_index) {
                    err!(
                        (
                            concat!("Received invalid ", $packet_name, " packet reg index: {}"),
                            reg_index
                        ),
                        b"E00"
                    );
                }
                reg_index
            }};
        }

        macro_rules! check_not_multiple_threads {
            ($packet_name: literal) => {
                if self.g_thread.mask.contains_multiple() {
                    err!(
                        (concat!(
                            "Received invalid ",
                            $packet_name,
                            " with multiple threads selected"
                        )),
                        b"E00"
                    )
                }
            };
        }

        let prefix = *unwrap_opt!(packet.get(0), ("Received empty packet"));
        let data = &packet[1..];

        match prefix {
            b'?' => {
                self.manually_stop(emu);
                return false;
            }

            b'B' => {
                let (addr, mode) = split_once(data, b',');
                let addr = parse_int!(addr, u32, "addr", "B");
                match mode {
                    b"S" => self.toggle_breakpoint::<_, true>(emu, addr),
                    b"C" => self.toggle_breakpoint::<_, false>(emu, addr),
                    _ => err!(
                        (
                            "Received invalid B packet mode: {}",
                            str::from_utf8(mode).unwrap_or("<invalid UTF-8>")
                        ),
                        b"E00"
                    ),
                }
                reply!(b"OK");
            }

            b'c' => {
                if !data.is_empty() {
                    let _addr = parse_int!(data, u32, "addr", "c");
                    if self.c_thread.mask.contains(ThreadMask::ARM9) {
                        // TODO: Set r15
                    }
                    if self.c_thread.mask.contains(ThreadMask::ARM7) {
                        // TODO: Set r15
                    }
                }
                self.wait_for_stop();
                return false;
            }

            b'D' => {
                self.detach(emu);
                return false;
            }

            b'g' => {
                check_not_multiple_threads!("g");
                let mut reply = Vec::with_capacity(17 * 8);
                let (mut regs, cpsr) = if self.g_thread.mask == ThreadMask::ARM9 {
                    (emu.arm9.regs(), emu.arm9.cpsr())
                } else {
                    (emu.arm7.regs(), emu.arm7.cpsr())
                };
                regs.gprs[15] = regs.gprs[15].wrapping_sub(8 >> cpsr.thumb_state() as u8);
                for reg in regs.gprs {
                    let _ = write!(reply, "{:08X}", reg.swap_bytes());
                }
                let _ = write!(reply, "{:08X}", cpsr.raw().swap_bytes());
                reply!(reply);
            }

            b'G' => {
                // TODO: Write registers
            }

            b'H' => {
                let op = *unwrap_opt!(data.get(0), ("Received invalid H packet"));
                let thread_id = parse_thread_id!(&data[1..], "H");
                match op {
                    b'c' => {
                        self.c_thread = ThreadId::from_value(thread_id, self.c_thread);
                        reply!(b"OK");
                    }
                    b'g' => {
                        self.g_thread = ThreadId::from_value(thread_id, self.g_thread);
                        reply!(b"OK");
                    }
                    _ => {}
                }
            }

            b'i' => {
                self.remaining_step_cycles = 1;
                if !data.is_empty() {
                    let (addr, cycles) = split_once(data, b',');
                    let _addr = parse_int!(addr, u32, "addr", "s");
                    if self.c_thread.mask.contains(ThreadMask::ARM9) {
                        // TODO: Set r15
                    }
                    if self.c_thread.mask.contains(ThreadMask::ARM7) {
                        // TODO: Set r15
                    }
                    if !cycles.is_empty() {
                        let cycles = parse_int!(cycles, RawTimestamp, "cycles", "s");
                        self.remaining_step_cycles = cycles;
                    }
                }
                self.wait_for_stop();
                return false;
            }

            b'k' => {
                self.detach(emu);
                emu.request_shutdown();
                return false;
            }

            b'm' => {
                let (mut addr, length) = parse_addr_length!(data, "m");
                check_not_multiple_threads!("m");
                let mut reply = Vec::with_capacity(length as usize * 2);
                for _ in 0..length {
                    let byte = if self.g_thread.mask == ThreadMask::ARM9 {
                        arm9::bus::read_8::<DebugCpuAccess, _>(emu, addr)
                    } else {
                        arm7::bus::read_8::<DebugCpuAccess, _>(emu, addr)
                    };
                    let _ = write!(reply, "{:02X}", byte);
                    addr = addr.wrapping_add(1);
                }
                reply!(reply);
            }

            b'M' => {
                let (addr_length, bytes) = split_once(data, b':');
                let (mut addr, _) = parse_addr_length!(addr_length, "m");
                for byte in bytes.array_chunks::<2>() {
                    let byte = parse_int!(byte, u8, "data", "M");
                    if self.g_thread.mask.contains(ThreadMask::ARM9) {
                        arm9::bus::write_8::<DebugCpuAccess, _>(emu, addr, byte);
                    }
                    if self.g_thread.mask.contains(ThreadMask::ARM7) {
                        arm7::bus::write_8::<DebugCpuAccess, _>(emu, addr, byte);
                    }
                    addr = addr.wrapping_add(1);
                }
                reply!(b"OK");
            }

            b'p' => {
                let reg_index = parse_reg_index!(data, "p");
                check_not_multiple_threads!("p");

                let cpsr = if self.g_thread.mask == ThreadMask::ARM9 {
                    emu.arm9.cpsr()
                } else {
                    emu.arm7.cpsr()
                };
                let value = if reg_index < 16 {
                    let regs = if self.g_thread.mask == ThreadMask::ARM9 {
                        emu.arm9.regs()
                    } else {
                        emu.arm7.regs()
                    };
                    if reg_index == 15 {
                        regs.gprs[15].wrapping_sub(8 >> cpsr.thumb_state() as u8)
                    } else {
                        regs.gprs[reg_index as usize]
                    }
                } else {
                    cpsr.raw()
                };

                let mut reply = Vec::with_capacity(8);
                let _ = write!(reply, "{:08X}", value.swap_bytes());
                reply!(reply);
            }

            b'P' => {
                let _reg_index = parse_reg_index!(data, "P");
                // TODO: Write register
            }

            b'q' => {
                let (command, args) = split_once(data, b':');
                match command {
                    b"C" => {
                        let mut reply = b"QC".to_vec();
                        let _ = write!(reply, "{}", self.g_thread.id);
                        reply!(reply);
                    }

                    b"CRC" => {
                        let (mut addr, length) = parse_addr_length!(data, "qCRC");
                        check_not_multiple_threads!("qCRC");
                        let mut crc = 0xFFFF_FFFF;
                        for _ in 0..length {
                            let byte = if self.g_thread.mask == ThreadMask::ARM9 {
                                arm9::bus::read_8::<DebugCpuAccess, _>(emu, addr)
                            } else {
                                arm7::bus::read_8::<DebugCpuAccess, _>(emu, addr)
                            };
                            crc = crc << 8 ^ CRC_TABLE[(((crc >> 24) as u8) ^ byte) as usize];
                            addr = addr.wrapping_add(1);
                        }
                        let mut reply = Vec::with_capacity(8);
                        let _ = write!(reply, "{:08X}", crc);
                        reply!(reply);
                    }

                    b"fThreadInfo" => reply!(b"m1,2"),

                    b"sThreadInfo" => reply!(b"l"),

                    b"Search" => {
                        // TODO: Search for byte pattern
                    }

                    b"Supported" => {
                        // TODO: Parse GDB features
                        reply!(b"PacketSize=1048576;qXfer:features:read+;qXfer:memory-map:read+;qXfer:threads:read+;QNonStop+;QCatchSyscalls+;QStartNoAckMode+;swbreak-;hwbreak+;vContSupported+")
                    }

                    b"ThreadExtraInfo" => {
                        reply!();
                    }

                    b"Xfer" => {
                        fn send_binary_range(data: &[u8], addr: u32, length: u32) -> Vec<u8> {
                            let range =
                                addr as usize..(addr as usize + length as usize).min(data.len());
                            let mut response = Vec::with_capacity(range.len() + 1);
                            response.push(if range.end == data.len() { b'l' } else { b'm' });
                            response.extend_from_slice(&data[range]);
                            response
                        }

                        let mut args = args.split(|c| *c == b':');
                        let object =
                            unwrap_opt!(args.next(), ("Received invalid qXfer packet"), b"E00");
                        let operation =
                            unwrap_opt!(args.next(), ("Received invalid qXfer packet"), b"E00");

                        if operation == b"read" {
                            let annex = unwrap_opt!(
                                args.next(),
                                ("Received invalid qXfer read packet"),
                                b"E00"
                            );
                            let (addr, length) = parse_addr_length!(
                                unwrap_opt!(
                                    args.next(),
                                    ("Received invalid qXfer read packet"),
                                    b"E00"
                                ),
                                "qXfer read"
                            );

                            match object {
                                b"features" => {
                                    if annex != b"target.xml" {
                                        err!(
                                            ("Received invalid qXfer features read packet"),
                                            b"E00"
                                        );
                                    }
                                    reply!(send_binary_range(
                                        include_bytes!("gdb_server/target.xml"),
                                        addr,
                                        length
                                    ));
                                }

                                b"memory-map" => {
                                    if annex != b"" {
                                        err!(
                                            ("Received invalid qXfer:memory-map:read packet"),
                                            b"E00"
                                        );
                                    }
                                    // TODO: Memory map
                                }

                                b"threads" => {
                                    if annex != b"" {
                                        err!(
                                            ("Received invalid qXfer:threads:read packet"),
                                            b"E00"
                                        );
                                    }
                                    reply!(send_binary_range(
                                        include_bytes!("gdb_server/threads.xml"),
                                        addr,
                                        length
                                    ));
                                }
                                _ => {}
                            }
                        }
                    }

                    b"Attached" => {
                        reply!(b"1");
                    }

                    _ => {}
                }
            }

            b'Q' => {
                let (command, args) = split_once(data, b':');
                match command {
                    b"NonStop" => {
                        let enabled = match args {
                            b"0" => false,
                            b"1" => true,
                            _ => err!(("Received invalid QNonStop packet"), b"E00"),
                        };
                        if enabled {
                            // TODO: Non-stop mode
                        } else {
                            reply!(b"OK");
                        }
                    }

                    b"CatchSyscalls" => {
                        // TODO: Catch SWIs
                    }

                    b"StartNoAckMode" => {
                        self.send_packet(b"OK".to_vec());
                        self.server.set_no_ack_mode();
                        return false;
                    }

                    _ => {}
                }
            }

            b'r' => {
                return true;
            }

            b's' => {
                // TODO: This should step by one instruction, not one cycle
                if !data.is_empty() {
                    let _addr = parse_int!(data, u32, "addr", "s");
                    if self.c_thread.mask.contains(ThreadMask::ARM9) {
                        // TODO: Set r15
                    }
                    if self.c_thread.mask.contains(ThreadMask::ARM7) {
                        // TODO: Set r15
                    }
                }
                self.remaining_step_cycles = 1;
                self.wait_for_stop();
                return false;
            }

            b't' => {
                // TODO: Search backwards in memory for pattern
            }

            b'T' => {
                let _thread_id = parse_thread_id!(data, "T");
                reply!(b"OK");
            }

            b'v' => {
                let (command, args) = split_once(data, b';');
                match command {
                    b"Cont" => {
                        for (action, thread_id) in args
                            .split(|c| *c == b';')
                            .map(|action_and_tid| split_once(action_and_tid, b':'))
                        {
                            let _thread_id = if thread_id.is_empty() {
                                -1
                            } else {
                                parse_thread_id!(thread_id, "vCont")
                            };
                            match action {
                                b"c" => {
                                    // TODO: Continue
                                }
                                b"s" => {
                                    // TODO: Single step
                                }
                                b"t" => {
                                    // TODO: Stop thread
                                }
                                b"r" => {
                                    // TODO: Step while in range
                                }
                                _ => {}
                            }
                        }
                    }

                    b"Cont?" => reply!(b"vCont;c;s;t;r"),

                    b"CtrlC" => {
                        // TODO: Pause program (non-stop mode)
                    }

                    _ => {}
                }
            }

            b'X' => {
                let (addr_length, bytes) = split_once(data, b':');
                let (mut addr, _) = parse_addr_length!(addr_length, "X");
                for &byte in bytes {
                    if self.g_thread.mask.contains(ThreadMask::ARM9) {
                        arm9::bus::write_8::<DebugCpuAccess, _>(emu, addr, byte);
                    }
                    if self.g_thread.mask.contains(ThreadMask::ARM7) {
                        arm7::bus::write_8::<DebugCpuAccess, _>(emu, addr, byte);
                    }
                    addr = addr.wrapping_add(1);
                }
                reply!(b"OK");
            }

            b'z' => {
                let (ty, addr_kind) = split_once(data, b',');
                let ty = parse_int!(ty, u8, "type", "z");
                let (addr, kind) = parse_addr_kind!(addr_kind, "z");
                match ty {
                    0 => {
                        if matches!(kind, 2 | 4) {
                            if let Some((arm9, arm7)) = self.sw_breakpoints.remove(&addr) {
                                macro_rules! write {
                                    ($core: ident) => {
                                        if kind == 2 {
                                            $core::bus::write_16::<DebugCpuAccess, _>(
                                                emu,
                                                addr,
                                                $core as u16,
                                            );
                                        } else {
                                            $core::bus::write_32::<DebugCpuAccess, _>(
                                                emu, addr, $core,
                                            );
                                        }
                                    };
                                }
                                if self.g_thread.mask.contains(ThreadMask::ARM9) {
                                    write!(arm9);
                                }
                                if self.g_thread.mask.contains(ThreadMask::ARM7) {
                                    write!(arm7);
                                }
                                reply!(b"OK");
                            }
                        }
                    }
                    1 => {
                        self.toggle_breakpoint::<_, false>(emu, addr);
                        reply!(b"OK");
                    }
                    2 => {
                        if kind.is_power_of_two() && addr & (kind - 1) as u32 == 0 {
                            self.toggle_watchpoint::<_, false, true, false>(emu, addr, kind);
                            reply!(b"OK");
                        }
                    }
                    3 => {
                        if kind.is_power_of_two() && addr & (kind - 1) as u32 == 0 {
                            self.toggle_watchpoint::<_, true, false, false>(emu, addr, kind);
                            reply!(b"OK");
                        }
                    }
                    4 => {
                        if kind.is_power_of_two() && addr & (kind - 1) as u32 == 0 {
                            self.toggle_watchpoint::<_, true, true, false>(emu, addr, kind);
                            reply!(b"OK");
                        }
                    }
                    _ => {}
                }
            }

            b'Z' => {
                let (ty, addr_kind) = split_once(data, b',');
                let ty = parse_int!(ty, u8, "type", "Z");
                let (addr, kind) = parse_addr_kind!(addr_kind, "Z");
                match ty {
                    0 => {
                        const ARM_UDF: u32 = 0xE7FF_FFFF;
                        const THUMB_UDF: u16 = 0xDEFF;
                        if matches!(kind, 2 | 4) && !self.sw_breakpoints.contains_key(&addr) {
                            macro_rules! write {
                                    ($core: ident$(, $code: expr)?) => {
                                        if kind == 2 {
                                            let prev =
                                                $core::bus::read_16::<DebugCpuAccess, _>(emu, addr)
                                                    as u32;
                                            $core::bus::write_16::<DebugCpuAccess, _>(
                                                emu, addr, THUMB_UDF,
                                            );
                                            prev
                                        } else {
                                            let prev =
                                                $core::bus::read_32::<DebugCpuAccess, _$(, $code)*>(emu, addr);
                                            $core::bus::write_32::<DebugCpuAccess, _>(
                                                emu, addr, ARM_UDF,
                                            );
                                            prev
                                        }
                                    };
                                }
                            self.sw_breakpoints.insert(
                                addr,
                                (
                                    if self.g_thread.mask.contains(ThreadMask::ARM9) {
                                        write!(arm9, false)
                                    } else {
                                        0
                                    },
                                    if self.g_thread.mask.contains(ThreadMask::ARM7) {
                                        write!(arm7)
                                    } else {
                                        0
                                    },
                                ),
                            );
                            reply!(b"OK");
                        }
                    }
                    1 => {
                        self.toggle_breakpoint::<_, true>(emu, addr);
                        reply!(b"OK");
                    }
                    2 => {
                        if kind.is_power_of_two() && addr & (kind - 1) as u32 == 0 {
                            self.toggle_watchpoint::<_, false, true, true>(emu, addr, kind);
                            reply!(b"OK");
                        }
                    }
                    3 => {
                        if kind.is_power_of_two() && addr & (kind - 1) as u32 == 0 {
                            self.toggle_watchpoint::<_, true, false, true>(emu, addr, kind);
                            reply!(b"OK");
                        }
                    }
                    4 => {
                        if kind.is_power_of_two() && addr & (kind - 1) as u32 == 0 {
                            self.toggle_watchpoint::<_, true, true, true>(emu, addr, kind);
                            reply!(b"OK");
                        }
                    }
                    _ => {}
                }
            }

            _ => {}
        }

        if packet != b"vMustReplyEmpty" {
            eprintln!(
                "[GDB] received unknown packet: {}",
                str::from_utf8(packet).unwrap_or("<invalid UTF-8>")
            );
        }
        self.send_empty_packet();
        false
    }

    pub fn poll<E: cpu::Engine>(&mut self, emu: &mut Emu<E>) -> bool {
        if !self.server.is_running() && !self.server.poll_listener() {
            return false;
        }

        if self.waiting_for_stop
            && match self.server.try_recv_break() {
                Ok(received) => {
                    if received {
                        self.manually_stop(emu);
                    }
                    !received
                }
                Err(err) => {
                    eprintln!("[GDB] Couldn't receive data: {err}");
                    self.detach(emu);
                    false
                }
            }
        {
            return false;
        }

        while self.server.is_running() {
            match self.server.try_recv_packet() {
                Ok(Some(packet)) => {
                    if packet.kind != PacketKind::Packet {
                        eprintln!("[GDB] Received unknown notification");
                        continue;
                    }
                    if self.handle_packet(emu, &packet.invalidate_check().data) {
                        return true;
                    }
                }

                Ok(None) => break,

                Err(err) => {
                    eprintln!("[GDB] Couldn't receive data: {err}");
                    self.detach(emu);
                    break;
                }
            }
        }

        false
    }
}
