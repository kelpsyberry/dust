// TODO:
// - Tracepoints
// - Host I/O with vFile, maybe using the DS cart's filesystem?
// - Extended mode, enabling R, vAttach and vRun, as well as some set packets, but only the first
//   one has a sensible interpretation
// - Non-stop mode
// - QAllow parsing

mod server;
use server::Server;

use bitflags::bitflags;
use dust_core::{
    cpu::{self, arm7, arm9, bus::DebugCpuAccess},
    emu::Emu,
};
use gdb_protocol::packet::{CheckedPacket, Kind as PacketKind};
use std::{
    cell::RefCell, collections::BTreeMap, io::Write, lazy::SyncLazy, net::ToSocketAddrs, rc::Rc,
    str,
};

enum Breakpoint {}

bitflags! {
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

#[derive(Clone, Copy, Debug)]
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

#[derive(Clone, Copy, Debug)]
enum StopCause {
    Break,
    Watchpoint(u32, u8),
    Syscall(u8),
    SwBreakpoint,
    HwBreakpoint,
    Shutdown,
}

static CRC_TABLE: SyncLazy<[u32; 256]> = SyncLazy::new(|| {
    let mut table = [0; 256];
    let mut crc = 0x8000_0000;
    let mut i = 1;
    while i < 256 {
        if crc & 0x8000_0000 == 0 {
            crc <<= 1;
        } else {
            crc = crc << 1 ^ 0x04C1_1DB7;
        }
        for v in &mut table[i..i << 1] {
            *v ^= crc;
        }
        i <<= 1;
    }
    table
});

pub struct GdbServer {
    server: Server,
    c_thread: ThreadId,
    g_thread: ThreadId,
    breakpoints: BTreeMap<u32, Breakpoint>,
    target_stopped: bool,
    waiting_for_stop: bool,
    stop_causes: Rc<RefCell<Vec<StopCause>>>,
}

fn split_once(data: &[u8], char: u8) -> (&[u8], Option<&[u8]>) {
    let split_pos = data.iter().position(|c| *c == char);
    if let Some(split_pos) = split_pos {
        (&data[..split_pos], Some(&data[split_pos + 1..]))
    } else {
        (data, None)
    }
}

impl GdbServer {
    pub fn new(addr: impl ToSocketAddrs) -> Result<Self, gdb_protocol::Error> {
        Ok(GdbServer {
            server: Server::new(addr)?,
            c_thread: ThreadId::new(1, ThreadMask::ARM9),
            g_thread: ThreadId::new(1, ThreadMask::ARM9),
            breakpoints: BTreeMap::new(),
            target_stopped: false,
            waiting_for_stop: false,
            stop_causes: Rc::new(RefCell::new(Vec::new())),
        })
    }

    #[inline]
    pub fn target_stopped(&self) -> bool {
        self.target_stopped
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
            StopCause::Watchpoint(_addr, _size) => todo!(),
            StopCause::Syscall(number) => {
                let _ = write!(buf, "T05syscall_entry:{:X}", number);
            }
            StopCause::SwBreakpoint => buf.extend_from_slice(b"T05swbreak:"),
            StopCause::HwBreakpoint => buf.extend_from_slice(b"T05hwbreak:"),
            StopCause::Shutdown => buf.extend_from_slice(b"X00"),
        }
    }

    fn detach<E: cpu::Engine>(&mut self, emu: &mut Emu<E>) {
        self.target_stopped = false;
        self.server.close();
        emu.arm7.clear_sw_breakpoints();
        // TODO: Clean up the emulator's state
    }

    pub fn emu_stopped<E: cpu::Engine>(&mut self, emu: &mut Emu<E>) {
        self.target_stopped = true;
        if self.waiting_for_stop {
            self.waiting_for_stop = false;
            self.send_stop_reason(emu);
        }
    }

    fn manually_stop<E: cpu::Engine>(&mut self, emu: &mut Emu<E>) {
        self.stop_causes.borrow_mut().push(StopCause::Break);
        self.target_stopped = true;
        if self.waiting_for_stop {
            self.waiting_for_stop = false;
            self.send_stop_reason(emu);
        }
    }

    fn send_stop_reason<E: cpu::Engine>(&mut self, _emu: &mut Emu<E>) {
        let mut stop_causes = self.stop_causes.borrow_mut();
        let stop_cause = stop_causes.get(0).copied().unwrap_or(StopCause::Break);
        stop_causes.clear();
        drop(stop_causes);
        let mut reply = Vec::new();
        self.encode_stop_reply(stop_cause, &mut reply);
        self.send_packet(reply);
    }

    fn wait_for_stop<E: cpu::Engine>(&mut self, emu: &mut Emu<E>) {
        if self.stop_causes.borrow().is_empty() {
            self.target_stopped = false;
            self.waiting_for_stop = true;
        } else {
            self.send_stop_reason(emu);
        }
    }

    fn handle_packet<E: cpu::Engine>(&mut self, emu: &mut Emu<E>, packet: &[u8]) -> bool {
        macro_rules! reply {
            () => {{
                self.send_empty_packet();
                return false;
            }};
            ($reply: expr) => {{
                self.send_packet($reply);
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

        macro_rules! parse_addr_length {
            ($args: expr, $sep: expr, $packet_name: literal) => {{
                let (addr, length) = split_once($args, $sep);
                (
                    unwrap_res!(
                        u32::from_str_radix(
                            unwrap_res!(
                                str::from_utf8(addr),
                                (concat!("Invalid unicode in ", $packet_name, " packet addr: {}"))
                            ),
                            16
                        ),
                        (concat!("Couldn't parse ", $packet_name, " packet addr: {:?}"))
                    ),
                    unwrap_res!(
                        u32::from_str_radix(
                            unwrap_res!(
                                str::from_utf8(unwrap_opt!(
                                    length,
                                    (concat!("Received invalid ", $packet_name, " packet"))
                                )),
                                (concat!(
                                    "Invalid unicode in ",
                                    $packet_name,
                                    " packet length: {}"
                                ))
                            ),
                            16
                        ),
                        (concat!("Couldn't parse ", $packet_name, " packet length: {:?}"))
                    ),
                )
            }};
        }

        macro_rules! parse_thread_id {
            ($id: expr, $packet_name: literal) => {{
                let thread_id = unwrap_res!(
                    i8::from_str_radix(
                        unwrap_res!(
                            str::from_utf8($id),
                            (concat!("Invalid unicode in ", $packet_name, " packet thread ID: {}"))
                        ),
                        16
                    ),
                    (concat!("Couldn't parse ", $packet_name, " packet thread ID: {}"))
                );
                if !(-1..=2).contains(&thread_id) {
                    reply!(b"E00".to_vec());
                }
                thread_id
            }};
        }

        let prefix = *unwrap_opt!(packet.get(0), ("Received empty packet"));
        let data = &packet[1..];

        match prefix {
            b'?' => {
                self.waiting_for_stop = true;
                self.manually_stop(emu);
                return false;
            }

            b'B' => {
                // TODO: Set breakpoint
            }

            b'c' => {
                if !data.is_empty() {
                    let _addr = unwrap_res!(
                        u32::from_str_radix(
                            unwrap_res!(
                                str::from_utf8(data),
                                ("Invalid unicode in c packet addr: {}")
                            ),
                            16
                        ),
                        ("Couldn't parse c packet addr: {:?}")
                    );
                    if self.c_thread.mask.contains(ThreadMask::ARM9) {
                        // TODO: Set r15
                    }
                    if self.c_thread.mask.contains(ThreadMask::ARM7) {
                        // TODO: Set r15
                    }
                }
                self.wait_for_stop(emu);
                return false;
            }

            b'D' => {
                self.detach(emu);
                return false;
            }

            b'g' => {
                if self.g_thread.mask.contains_multiple() {
                    reply!(b"E00".to_vec());
                }
                let mut reply = Vec::with_capacity(17 * 8);
                let regs = if self.g_thread.mask == ThreadMask::ARM9 {
                    emu.arm9.regs()
                } else {
                    emu.arm7.regs()
                };
                for reg in regs.gprs {
                    let _ = write!(reply, "{:08X}", reg.swap_bytes());
                }
                let _ = write!(reply, "{:08X}", regs.cpsr.raw().swap_bytes());
                reply!(reply);
            }

            b'G' => {
                // TODO: Write registers
            }

            b'H' => {
                let op = *unwrap_opt!(data.get(0), ("Received invalid H packet"));
                let thread_id = parse_thread_id!(&data[1..], "H");
                match op {
                    b'c' => self.c_thread = ThreadId::from_value(thread_id, self.c_thread),
                    b'g' => self.g_thread = ThreadId::from_value(thread_id, self.g_thread),
                    _ => {
                        err!(("Received unknown \"H {} {:X}\" packet", op, thread_id));
                    }
                }
                reply!(b"OK".to_vec());
            }

            b'i' => {
                // TODO: Step by cycles
            }

            b'k' => {
                self.target_stopped = false;
                self.server.close();
                emu.request_shutdown();
                return false;
            }

            b'm' => {
                let (mut addr, length) = parse_addr_length!(data, b',', "m");
                if self.g_thread.mask.contains_multiple() {
                    reply!(b"E00".to_vec());
                }
                let mut reply = Vec::with_capacity(length as usize * 2);
                for _ in 0..length {
                    let byte = if self.g_thread.mask.contains(ThreadMask::ARM9) {
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
                let (mut addr, _) = parse_addr_length!(addr_length, b',', "m");
                if let Some(bytes) = bytes {
                    for byte in bytes.array_chunks::<2>() {
                        let byte = unwrap_res!(
                            u8::from_str_radix(
                                unwrap_res!(
                                    str::from_utf8(byte),
                                    ("Invalid unicode in M packet data: {}")
                                ),
                                16
                            ),
                            ("Couldn't parse M packet data: {:?}")
                        );
                        if self.g_thread.mask.contains(ThreadMask::ARM9) {
                            arm9::bus::write_8::<DebugCpuAccess, _>(emu, addr, byte);
                        }
                        if self.g_thread.mask.contains(ThreadMask::ARM7) {
                            arm7::bus::write_8::<DebugCpuAccess, _>(emu, addr, byte);
                        }
                        addr = addr.wrapping_add(1);
                    }
                }
                reply!(b"OK".to_vec());
            }

            b'p' => {
                let reg_index = unwrap_res!(
                    u8::from_str_radix(
                        unwrap_res!(
                            str::from_utf8(data),
                            ("Invalid unicode in p packet reg index: {}")
                        ),
                        16
                    ),
                    ("Couldn't parse p packet reg index: {}")
                );
                if !(0..=16).contains(&reg_index) {
                    reply!(b"E00".to_vec());
                }

                let regs = emu.arm7.regs();
                let value = if reg_index < 16 {
                    regs.gprs[reg_index as usize]
                } else {
                    regs.cpsr.raw()
                };
                let mut reply = Vec::with_capacity(8);
                let _ = write!(reply, "{:08X}", value.swap_bytes());
                reply!(reply);
            }

            b'P' => {
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
                        let (mut addr, length) = parse_addr_length!(data, b',', "qCRC");
                        if self.g_thread.mask.contains_multiple() {
                            reply!(b"E00".to_vec());
                        }
                        let mut crc = 0xFFFF_FFFF;
                        let crc_table = &*CRC_TABLE;
                        for _ in 0..length {
                            let byte = if self.g_thread.mask.contains(ThreadMask::ARM9) {
                                arm9::bus::read_8::<DebugCpuAccess, _>(emu, addr)
                            } else {
                                arm7::bus::read_8::<DebugCpuAccess, _>(emu, addr)
                            };
                            crc = crc << 8 ^ crc_table[(((crc >> 24) as u8) ^ byte) as usize];
                            addr = addr.wrapping_add(1);
                        }
                        let mut reply = Vec::with_capacity(8);
                        let _ = write!(reply, "{:08X}", crc);
                        reply!(reply);
                    }

                    b"fThreadInfo" => reply!(b"m1,2".to_vec()),

                    b"sThreadInfo" => reply!(b"l".to_vec()),

                    b"Search" => {
                        // TODO: Search for byte pattern
                    }

                    b"Supported" => {
                        // TODO: Parse GDB features
                        reply!(b"PacketSize=1048576;qXfer:features:read+;qXfer:memory-map:read+;qXfer:threads:read+;QNonStop+;QCatchSyscalls+;QStartNoAckMode+;swbreak+;hwbreak+;vContSupported+".to_vec())
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

                        let mut args =
                            unwrap_opt!(args, ("Received invalid qXfer packet"), b"E00".to_vec())
                                .split(|c| *c == b':');
                        let object = unwrap_opt!(
                            args.next(),
                            ("Received invalid qXfer packet"),
                            b"E00".to_vec()
                        );
                        let operation = unwrap_opt!(
                            args.next(),
                            ("Received invalid qXfer packet"),
                            b"E00".to_vec()
                        );

                        if operation == b"read" {
                            let annex = unwrap_opt!(
                                args.next(),
                                ("Received invalid qXfer read packet"),
                                b"E00".to_vec()
                            );
                            let (addr, length) = parse_addr_length!(
                                unwrap_opt!(
                                    args.next(),
                                    ("Received invalid qXfer read packet"),
                                    b"E00".to_vec()
                                ),
                                b',',
                                "qXfer read"
                            );

                            match object {
                                b"features" => {
                                    if annex != b"target.xml" {
                                        err!(
                                            ("Received invalid qXfer features read packet"),
                                            b"E00".to_vec()
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
                                            b"E00".to_vec()
                                        );
                                    }
                                    // TODO: Memory map
                                }

                                b"threads" => {
                                    if annex != b"" {
                                        err!(
                                            ("Received invalid qXfer:threads:read packet"),
                                            b"E00".to_vec()
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
                        reply!(b"1".to_vec());
                    }

                    _ => {}
                }
            }

            b'Q' => {
                let (command, args) = split_once(data, b':');
                match command {
                    b"NonStop" => {
                        let enabled = match args {
                            Some(b"0") => false,
                            Some(b"1") => true,
                            _ => err!(("Received invalid QNonStop packet"), b"E00".to_vec()),
                        };
                        if enabled {
                            // TODO: Non-stop mode
                        } else {
                            reply!(b"OK".to_vec());
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
                // TODO: Single step
            }

            b't' => {
                // TODO: Search backwards in memory for pattern
            }

            b'T' => {
                let thread_id = unwrap_res!(
                    i8::from_str_radix(
                        unwrap_res!(
                            str::from_utf8(data),
                            ("Invalid unicode in T packet thread ID: {}")
                        ),
                        16
                    ),
                    ("Couldn't parse T packet thread ID: {}")
                );
                reply!(if (-1..=2).contains(&thread_id) {
                    &b"OK"[..]
                } else {
                    &b"E00"[..]
                }
                .to_vec());
            }

            b'v' => {
                let (command, args) = split_once(data, b';');
                match command {
                    b"Cont" => {
                        if let Some(args) = args {
                            for (action, thread_id) in args
                                .split(|c| *c == b';')
                                .map(|action_and_tid| split_once(action_and_tid, b':'))
                            {
                                let _thread_id =
                                    parse_thread_id!(thread_id.unwrap_or(b"-1"), "vCont");
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
                    }
                    b"Cont?" => reply!(b"vCont;c;s;t;r".to_vec()),
                    b"CtrlC" => {
                        // TODO: Pause program (non-stop mode)
                    }
                    _ => {}
                }
            }

            b'X' => {
                let (addr_length, bytes) = split_once(data, b':');
                let (mut addr, _) = parse_addr_length!(addr_length, b',', "X");
                if let Some(bytes) = bytes {
                    for &byte in bytes {
                        if self.g_thread.mask.contains(ThreadMask::ARM9) {
                            arm9::bus::write_8::<DebugCpuAccess, _>(emu, addr, byte);
                        }
                        if self.g_thread.mask.contains(ThreadMask::ARM7) {
                            arm7::bus::write_8::<DebugCpuAccess, _>(emu, addr, byte);
                        }
                        addr = addr.wrapping_add(1);
                    }
                }
                reply!(b"OK".to_vec());
            }

            b'z' => {
                // TODO: Remove breakpoint
            }

            b'Z' => {
                // TODO: Add breakpoint
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
                    eprintln!("[GDB] Couldn't receive data: {}", err);
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
                    eprintln!("[GDB] Couldn't receive data: {}", err);
                    break;
                }
            }
        }

        false
    }
}
