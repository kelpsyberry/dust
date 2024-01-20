use super::{
    Core, CoreStopCause, EmuControlFlow, GdbServer, StopCause, ThreadId, VStoppedSequenceKind,
};
use ahash::AHashSet as HashSet;
use dust_core::{
    cpu::{self, arm7, arm9, bus::DebugCpuAccess, debug::SwiHook, psr::Psr},
    emu::Emu,
};
use emu_utils::resource;
use std::rc::Rc;

fn split_once_mut(data: &mut [u8], byte: u8) -> (&mut [u8], &mut [u8]) {
    if let Some(split_pos) = data.iter().position(|c| *c == byte) {
        let (first, last) = data.split_at_mut(split_pos);
        (first, &mut last[1..])
    } else {
        (data, &mut [])
    }
}

fn split_once(data: &[u8], byte: u8) -> (&[u8], &[u8]) {
    if let Some(split_pos) = data.iter().position(|c| *c == byte) {
        (&data[..split_pos], &data[split_pos + 1..])
    } else {
        (data, &[])
    }
}

fn u32_from_ascii_hex(data: &[u8]) -> Option<u32> {
    let mut value = 0;
    if data.len() > 8 {
        return None;
    }
    for byte in data {
        let byte_value = match byte {
            b'0'..=b'9' => byte - b'0',
            b'A'..=b'F' => byte - b'A' + 10,
            b'a'..=b'f' => byte - b'a' + 10,
            _ => return None,
        };
        value = value << 4 | byte_value as u32;
    }
    Some(value)
}

fn tid_from_ascii(data: &[u8]) -> Option<Option<ThreadId>> {
    Some(match data {
        b"-1" => Some(ThreadId::All),
        b"0" => None,
        b"1" => Some(ThreadId::Arm7),
        b"2" => Some(ThreadId::Arm9),
        _ => return None,
    })
}

fn write_bool_to_ascii_hex(value: bool, data: &mut Vec<u8>) {
    data.push(b"01"[value as usize]);
}

fn write_u8_to_ascii_hex_padded(value: u8, data: &mut Vec<u8>) {
    data.push(b"0123456789ABCDEF"[(value >> 4) as usize]);
    data.push(b"0123456789ABCDEF"[(value & 0xF) as usize]);
}

fn write_u32_to_ascii_hex_padded(value: u32, data: &mut Vec<u8>) {
    for i in (0..32).step_by(4).rev() {
        data.push(b"0123456789ABCDEF"[(value >> i & 0xF) as usize]);
    }
}

fn binary_range_reply(data: &[u8], addr: u32, length: u32) -> Vec<u8> {
    let range = addr as usize..(addr as usize + length as usize).min(data.len());
    let mut result = Vec::with_capacity(range.len() + 1);
    result.push(if range.end == data.len() { b'l' } else { b'm' });
    result.extend_from_slice(&data[range]);
    result
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PacketError {
    Unrecognized,
    Parsing,
    InvalidParams,
    UnrecognizedParams,
    MultipleThreadsSelected,
}

#[derive(Debug)]
pub(super) enum HandlePacketError {
    GdbProtocol(gdb_protocol::Error),
    Packet(PacketError),
}

impl From<gdb_protocol::Error> for HandlePacketError {
    #[inline]
    fn from(value: gdb_protocol::Error) -> Self {
        HandlePacketError::GdbProtocol(value)
    }
}

impl From<PacketError> for HandlePacketError {
    #[inline]
    fn from(value: PacketError) -> Self {
        HandlePacketError::Packet(value)
    }
}

impl GdbServer {
    fn continued(&mut self) -> super::Result {
        if self.is_in_non_stop_mode {
            self.send_packet_slice(b"OK")
        } else {
            Ok(())
        }
    }

    pub(super) fn handle_packet<E: cpu::Engine>(
        &mut self,
        emu: &mut Emu<E>,
        packet: &mut [u8],
    ) -> Result<EmuControlFlow, HandlePacketError> {
        macro_rules! ok {
            () => {
                self.send_packet_slice(b"OK")?;
                return Ok(EmuControlFlow::Continue);
            };
        }

        macro_rules! check_not_multiple_threads_selected {
            () => {
                if self.g_thread == ThreadId::All {
                    return Err(PacketError::MultipleThreadsSelected.into());
                }
            };
        }

        macro_rules! continue_at_addr {
            ($addr: expr) => {
                if (self.c_thread.has_arm7()
                    && $addr & 3 >> emu.arm7.cpsr().thumb_state() as u8 != 0)
                    || (self.c_thread.has_arm9()
                        && $addr & 3 >> emu.arm9.cpsr().thumb_state() as u8 != 0)
                {
                    return Err(PacketError::InvalidParams.into());
                }

                if self.c_thread.has_arm7() {
                    arm7::Arm7::jump(emu, $addr);
                }
                if self.c_thread.has_arm9() {
                    arm9::Arm9::jump(emu, $addr);
                }
            };
        }

        macro_rules! parse_int {
            ($data: expr) => {
                u32_from_ascii_hex($data).ok_or(PacketError::Parsing)?
            };
        }

        let prefix = *packet.first().ok_or(PacketError::Unrecognized)?;
        let data = &mut packet[1..];

        match prefix {
            b'?' => {
                self.just_connected = false;
                let mut stop_causes = self.stop_causes.borrow_mut();
                if self.is_in_non_stop_mode {
                    stop_causes.flush();
                    let mut i = 0;
                    while i < 2 {
                        let stop_cause = stop_causes.cores[i as usize];
                        i += 1;
                        if let Some(stop_cause) = stop_cause {
                            drop(stop_causes);
                            self.send_stop_cause_packet(stop_cause)?;
                            self.cur_vstopped_sequence_kind = Some((
                                VStoppedSequenceKind::Stopped { i },
                                if i == 1 {
                                    ThreadId::Arm7
                                } else {
                                    ThreadId::Arm9
                                },
                            ));
                            return Ok(EmuControlFlow::Continue);
                        }
                    }
                    drop(stop_causes);
                    self.cur_vstopped_sequence_kind = None;
                    ok!();
                } else if !stop_causes.queue.is_empty() {
                    drop(stop_causes);
                    self.send_all_stop_stop_reply()?;
                } else {
                    return Err(PacketError::Unrecognized.into());
                }
                return Ok(EmuControlFlow::Continue);
            }

            b'B' => {
                let (addr, mode) = split_once(data, b',');
                let addr = parse_int!(addr);

                match mode {
                    b"S" => self.toggle_hw_breakpoint::<_, true>(emu, addr),
                    b"C" => self.toggle_hw_breakpoint::<_, false>(emu, addr),
                    _ => return Err(PacketError::UnrecognizedParams.into()),
                }

                ok!();
            }

            b'c' => {
                if !data.is_empty() {
                    let addr = parse_int!(data);
                    continue_at_addr!(addr);
                }

                if self.c_thread.has_arm7() {
                    emu.arm7.is_stopped = false;
                    self.is_running[0] = true;
                    self.stop_causes.borrow_mut().cores[0] = None;
                    self.remaining_step_cycles[0] = 0;
                }
                if self.c_thread.has_arm9() {
                    emu.arm9.is_stopped = false;
                    self.is_running[1] = true;
                    self.stop_causes.borrow_mut().cores[1] = None;
                    self.remaining_step_cycles[1] = 0;
                }

                self.continued()?;
                return Ok(EmuControlFlow::Continue);
            }

            b'D' => {
                self.detach(emu);
                ok!();
            }

            b'g' => {
                check_not_multiple_threads_selected!();

                let mut reply = Vec::with_capacity(17 * 8);

                let (mut regs, cpsr) = if self.g_thread == ThreadId::Arm7 {
                    (emu.arm7.regs(), emu.arm7.cpsr())
                } else {
                    (emu.arm9.regs(), emu.arm9.cpsr())
                };
                regs.gprs[15] = regs.gprs[15].wrapping_sub(8 >> cpsr.thumb_state() as u8);

                for &reg in &regs.gprs {
                    write_u32_to_ascii_hex_padded(reg.swap_bytes(), &mut reply);
                }
                write_u32_to_ascii_hex_padded(cpsr.raw().swap_bytes(), &mut reply);

                self.send_packet(reply)?;
                return Ok(EmuControlFlow::Continue);
            }

            b'G' => {
                check_not_multiple_threads_selected!();

                let regs_data = data
                    .get(1..(17 * 8) + 1)
                    .ok_or(PacketError::InvalidParams)?;

                let mut reg_values = [0; 17];
                for (i, reg_data) in regs_data.array_chunks::<8>().enumerate() {
                    reg_values[i] = parse_int!(reg_data).swap_bytes();
                }

                let is_in_thumb_state = Psr::from_raw(reg_values[16]).thumb_state();
                if reg_values[15] & 3 >> is_in_thumb_state as u8 != 0 {
                    return Err(PacketError::InvalidParams.into());
                }
                reg_values[15] = reg_values[15].wrapping_add(8 >> is_in_thumb_state as u8);

                if self.g_thread.has_arm7() {
                    let mut regs = emu.arm7.regs();
                    regs.gprs.copy_from_slice(&reg_values[..16]);
                    arm7::Arm7::set_cpsr(emu, Psr::from_raw_masked::<false>(reg_values[16]));
                    arm7::Arm7::set_regs(emu, &regs);
                }
                if self.g_thread.has_arm9() {
                    let mut regs = emu.arm9.regs();
                    regs.gprs.copy_from_slice(&reg_values[..16]);
                    arm9::Arm9::set_cpsr(emu, Psr::from_raw_masked::<true>(reg_values[16]));
                    arm9::Arm9::set_regs(emu, &regs);
                }

                ok!();
            }

            b'H' => {
                let op = data.first().ok_or(PacketError::UnrecognizedParams)?;
                let thread_id = tid_from_ascii(&data[1..]).ok_or(PacketError::Parsing)?;

                match op {
                    b'c' => self.c_thread = thread_id.unwrap_or(self.c_thread),
                    b'g' => self.g_thread = thread_id.unwrap_or(self.g_thread),
                    _ => return Err(PacketError::UnrecognizedParams.into()),
                }

                ok!();
            }

            b'i' => {
                let mut cycles = 1;
                if !data.is_empty() {
                    let (addr, cycles_str) = split_once(data, b',');
                    let addr = parse_int!(addr);
                    if !cycles_str.is_empty() {
                        cycles = parse_int!(cycles_str) as u64;
                    }

                    continue_at_addr!(addr);
                }

                if self.c_thread.has_arm7() {
                    emu.arm7.is_stopped = false;
                    self.is_running[0] = true;
                    self.stop_causes.borrow_mut().cores[0] = None;
                    self.remaining_step_cycles[0] = cycles;
                }
                if self.c_thread.has_arm9() {
                    emu.arm9.is_stopped = false;
                    self.is_running[1] = true;
                    self.stop_causes.borrow_mut().cores[1] = None;
                    self.remaining_step_cycles[1] = cycles;
                }

                self.continued()?;
                return Ok(EmuControlFlow::Continue);
            }

            b'k' => {
                self.detach(emu);
                emu.request_shutdown();
                return Ok(EmuControlFlow::Continue);
            }

            b'm' => {
                check_not_multiple_threads_selected!();

                let (addr, length) = split_once(data, b',');
                let mut addr = parse_int!(addr);
                let length = parse_int!(length);

                let mut reply = Vec::with_capacity(length as usize * 2);
                for _ in 0..length {
                    let byte = if self.g_thread == ThreadId::Arm7 {
                        arm7::bus::read_8::<DebugCpuAccess, _>(emu, addr)
                    } else {
                        arm9::bus::read_8::<DebugCpuAccess, _>(emu, addr)
                    };
                    write_u8_to_ascii_hex_padded(byte, &mut reply);
                    addr = addr.wrapping_add(1);
                }

                self.send_packet(reply)?;
                return Ok(EmuControlFlow::Continue);
            }

            b'M' => {
                let (addr_length, bytes) = split_once(data, b':');
                let (addr, length) = split_once(addr_length, b',');
                let mut addr = parse_int!(addr);
                let length = parse_int!(length);

                if bytes.len() != length as usize * 2 {
                    return Err(PacketError::InvalidParams.into());
                }
                if bytes.iter().any(|b| !b"0123456789ABCDEabcde".contains(b)) {
                    return Err(PacketError::InvalidParams.into());
                }

                for byte in bytes.array_chunks::<2>() {
                    let byte = u32_from_ascii_hex(byte).unwrap() as u8;
                    if self.g_thread.has_arm7() {
                        arm7::bus::write_8::<DebugCpuAccess, _>(emu, addr, byte);
                    }
                    if self.g_thread.has_arm9() {
                        arm9::bus::write_8::<DebugCpuAccess, _>(emu, addr, byte);
                    }
                    addr = addr.wrapping_add(1);
                }

                ok!();
            }

            b'p' => {
                check_not_multiple_threads_selected!();

                let reg_index = parse_int!(data);
                if !(0..17).contains(&reg_index) {
                    return Err(PacketError::UnrecognizedParams.into());
                }

                let cpsr = if self.g_thread == ThreadId::Arm7 {
                    emu.arm7.cpsr()
                } else {
                    emu.arm9.cpsr()
                };
                let value = if reg_index < 16 {
                    let regs = if self.g_thread == ThreadId::Arm7 {
                        emu.arm7.regs()
                    } else {
                        emu.arm9.regs()
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
                write_u32_to_ascii_hex_padded(value.swap_bytes(), &mut reply);

                self.send_packet(reply)?;
                return Ok(EmuControlFlow::Continue);
            }

            b'P' => {
                let (reg_index, value) = split_once(data, b'=');
                let reg_index = parse_int!(reg_index);
                let value = parse_int!(value);
                if !(0..17).contains(&reg_index) {
                    return Err(PacketError::UnrecognizedParams.into());
                }

                if reg_index < 16 {
                    if reg_index == 15
                        && ((self.g_thread.has_arm7()
                            && value & 3 >> emu.arm7.cpsr().thumb_state() as u8 != 0)
                            || (self.g_thread.has_arm9()
                                && value & 3 >> emu.arm9.cpsr().thumb_state() as u8 != 0))
                    {
                        return Err(PacketError::InvalidParams.into());
                    }

                    if self.g_thread.has_arm7() {
                        let mut value = value;
                        if reg_index == 15 {
                            value = value.wrapping_add(8 >> emu.arm7.cpsr().thumb_state() as u8);
                        }
                        let mut regs = emu.arm7.regs();
                        regs.gprs[reg_index as usize] = value;
                        arm7::Arm7::set_regs(emu, &regs);
                    }
                    if self.g_thread.has_arm9() {
                        let mut value = value;
                        if reg_index == 15 {
                            value = value.wrapping_add(8 >> emu.arm9.cpsr().thumb_state() as u8);
                        }
                        let mut regs = emu.arm9.regs();
                        regs.gprs[reg_index as usize] = value;
                        arm9::Arm9::set_regs(emu, &regs);
                    }
                } else {
                    let is_in_thumb_state = Psr::from_raw(value).thumb_state();
                    if self.g_thread.has_arm7() {
                        let jump_addr = emu
                            .arm7
                            .r15()
                            .wrapping_sub(8 >> emu.arm7.cpsr().thumb_state() as u8);
                        arm7::Arm7::set_cpsr(emu, Psr::from_raw_masked::<false>(value));
                        arm7::Arm7::jump(emu, jump_addr | is_in_thumb_state as u32);
                    }
                    if self.g_thread.has_arm9() {
                        let jump_addr = emu
                            .arm9
                            .r15()
                            .wrapping_sub(8 >> emu.arm9.cpsr().thumb_state() as u8);
                        arm9::Arm9::set_cpsr(emu, Psr::from_raw_masked::<true>(value));
                        arm9::Arm9::jump(emu, jump_addr | is_in_thumb_state as u32);
                    }
                }

                ok!();
            }

            b'q' => {
                if data.starts_with(b"L") {
                    if data.len() != 11 {
                        return Err(PacketError::UnrecognizedParams.into());
                    }
                    let _start_flag = parse_int!(&data[0..1]);
                    let thread_count = parse_int!(&data[1..3]);
                    let next_thread = parse_int!(&data[3..11]);

                    let start_thread = next_thread.min(2) as usize;
                    let thread_count = (thread_count as usize).min(2 - start_thread);
                    let end_thread = start_thread + thread_count;
                    let done = end_thread == 2;

                    let mut reply = Vec::with_capacity(11);
                    write_u8_to_ascii_hex_padded(thread_count as u8, &mut reply);
                    write_bool_to_ascii_hex(done, &mut reply);
                    write_u32_to_ascii_hex_padded(next_thread, &mut reply);
                    for &thread_id in &[1, 2][start_thread..end_thread] {
                        write_u32_to_ascii_hex_padded(thread_id, &mut reply);
                    }

                    self.send_packet(reply)?;
                    return Ok(EmuControlFlow::Continue);
                }

                let (command, args) = split_once(data, b':');
                match command {
                    b"C" => {
                        self.send_packet_slice(match self.g_thread {
                            ThreadId::All => b"QC-1",
                            ThreadId::Arm7 => b"QC1",
                            ThreadId::Arm9 => b"QC2",
                        })?;
                        return Ok(EmuControlFlow::Continue);
                    }

                    b"CRC" => {
                        check_not_multiple_threads_selected!();

                        let (addr, length) = split_once(args, b',');
                        let mut addr = parse_int!(addr);
                        let length = parse_int!(length);

                        let mut crc = 0xFFFF_FFFF;
                        for _ in 0..length {
                            let byte = if self.g_thread == ThreadId::Arm7 {
                                arm7::bus::read_8::<DebugCpuAccess, _>(emu, addr)
                            } else {
                                arm9::bus::read_8::<DebugCpuAccess, _>(emu, addr)
                            };
                            crc = crc << 8 ^ CRC_TABLE[(((crc >> 24) as u8) ^ byte) as usize];
                            addr = addr.wrapping_add(1);
                        }
                        let mut reply = Vec::with_capacity(8);
                        write_u32_to_ascii_hex_padded(crc, &mut reply);

                        self.send_packet(reply)?;
                        return Ok(EmuControlFlow::Continue);
                    }

                    b"fThreadInfo" => {
                        self.send_packet_slice(b"m1,2")?;
                        return Ok(EmuControlFlow::Continue);
                    }

                    b"sThreadInfo" => {
                        self.send_packet_slice(b"l")?;
                        return Ok(EmuControlFlow::Continue);
                    }

                    b"Search" => {
                        // TODO: Search for byte pattern
                    }

                    b"Supported" => {
                        // TODO: Parse GDB features
                        self.send_packet_slice(
                            b"PacketSize=100000;qXfer:features:read+;qXfer:threads:read+;QNonStop+;\
                             QCatchSyscalls+;QStartNoAckMode+;swbreak+;hwbreak+;vContSupported+",
                        )?;
                        return Ok(EmuControlFlow::Continue);
                    }

                    b"Xfer" => {
                        let mut args = args.split(|c| *c == b':');
                        let object = args.next().ok_or(PacketError::Parsing)?;
                        let operation = args.next().ok_or(PacketError::Parsing)?;
                        let annex = args.next().ok_or(PacketError::Parsing)?;
                        let (addr, length) =
                            split_once(args.next().ok_or(PacketError::Parsing)?, b',');
                        if args.next().is_some() {
                            return Err(PacketError::UnrecognizedParams.into());
                        }
                        let addr = parse_int!(addr);
                        let length = parse_int!(length);

                        if operation != b"read" {
                            return Err(PacketError::UnrecognizedParams.into());
                        }

                        match object {
                            b"features" => {
                                if annex != b"target.xml" {
                                    return Err(PacketError::InvalidParams.into());
                                }
                                self.send_packet(binary_range_reply(
                                    resource!("specs/target.xml", "gdb/target.xml"),
                                    addr,
                                    length,
                                ))?;
                                return Ok(EmuControlFlow::Continue);
                            }

                            b"threads" => {
                                if !annex.is_empty() {
                                    return Err(PacketError::InvalidParams.into());
                                }
                                self.send_packet(binary_range_reply(
                                    resource!("specs/threads.xml", "gdb/threads.xml"),
                                    addr,
                                    length,
                                ))?;
                                return Ok(EmuControlFlow::Continue);
                            }

                            _ => {
                                return Err(PacketError::UnrecognizedParams.into());
                            }
                        }
                    }

                    b"Attached" => {
                        self.send_packet_slice(b"1")?;
                        return Ok(EmuControlFlow::Continue);
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
                            _ => return Err(PacketError::UnrecognizedParams.into()),
                        };
                        if enabled {
                            self.is_in_non_stop_mode = true;
                        } else if self.is_in_non_stop_mode || self.just_connected {
                            self.is_in_non_stop_mode = false;
                            self.cur_vstopped_sequence_kind = None;
                            {
                                let mut stop_causes = self.stop_causes.borrow_mut();
                                stop_causes.queue.clear();
                            }
                            self.manually_stop(emu);
                            self.all_stop_stop_all_cores(emu);
                        }
                        ok!();
                    }

                    b"CatchSyscalls" => {
                        let (enabled, syscalls) = split_once(args, b';');
                        let enabled = match enabled {
                            b"0" => false,
                            b"1" => true,
                            _ => return Err(PacketError::UnrecognizedParams.into()),
                        };

                        if enabled {
                            let mut watched_syscalls = HashSet::new();
                            for syscall in syscalls.split(|v| *v == b';') {
                                let syscall = parse_int!(syscall);
                                if syscall > u8::MAX as u32 {
                                    return Err(PacketError::InvalidParams.into());
                                }
                                watched_syscalls.insert(syscall as u8);
                            }

                            macro_rules! set_hook {
                                (
                                    |
                                        $core_enum_ident: ident,
                                        $stop_causes_ident: ident,
                                        $watched_syscalls_ident: ident
                                    | $hook: expr
                                ) => {
                                    let $stop_causes_ident = Rc::clone(&self.stop_causes);
                                    let $core_enum_ident = Core::Arm7;
                                    let $watched_syscalls_ident = watched_syscalls.clone();
                                    emu.arm7.set_swi_hook(Some(SwiHook::<E>::new($hook)));

                                    let $stop_causes_ident = Rc::clone(&self.stop_causes);
                                    let $core_enum_ident = Core::Arm9;
                                    let $watched_syscalls_ident = watched_syscalls;
                                    emu.arm9.set_swi_hook(Some(SwiHook::<E>::new($hook)));
                                };
                            }

                            set_hook!(|core, stop_causes, watched_syscalls| {
                                if watched_syscalls.is_empty() {
                                    Box::new(move |_emu, number| {
                                        stop_causes.borrow_mut().push(StopCause::CoreStopped(
                                            CoreStopCause::Syscall(number),
                                            core,
                                        ));
                                        true
                                    })
                                } else {
                                    Box::new(move |_emu, number| {
                                        if watched_syscalls.contains(&number) {
                                            stop_causes.borrow_mut().push(StopCause::CoreStopped(
                                                CoreStopCause::Syscall(number),
                                                core,
                                            ));
                                            true
                                        } else {
                                            false
                                        }
                                    })
                                }
                            });

                            ok!();
                        } else {
                            emu.arm7.set_swi_hook(None);
                            emu.arm9.set_swi_hook(None);
                            ok!();
                        }
                    }

                    b"StartNoAckMode" => {
                        self.send_packet_slice(b"OK")?;
                        self.server.set_no_ack_mode();
                        return Ok(EmuControlFlow::Continue);
                    }

                    _ => {}
                }
            }

            b'r' => {
                self.detach(emu);
                return Ok(EmuControlFlow::Reset);
            }

            b's' => {
                // TODO: This should step by one instruction, not one cycle

                if !data.is_empty() {
                    let addr = parse_int!(data);
                    continue_at_addr!(addr);
                }

                if self.c_thread.has_arm7() {
                    emu.arm7.is_stopped = false;
                    self.is_running[0] = true;
                    self.stop_causes.borrow_mut().cores[0] = None;
                    self.remaining_step_cycles[0] = 1;
                }
                if self.c_thread.has_arm9() {
                    emu.arm9.is_stopped = false;
                    self.is_running[1] = true;
                    self.stop_causes.borrow_mut().cores[1] = None;
                    self.remaining_step_cycles[1] = 1;
                }

                self.continued()?;
                return Ok(EmuControlFlow::Continue);
            }

            b't' => {
                // TODO: Search backwards in memory for pattern(?)
            }

            b'T' => {
                if matches!(
                    tid_from_ascii(data).ok_or(PacketError::Parsing)?,
                    None | Some(ThreadId::All)
                ) {
                    return Err(PacketError::InvalidParams.into());
                }
                ok!();
            }

            b'v' => {
                let (command, args) = split_once(data, b';');
                match command {
                    b"Cont" => {
                        let mut applied = [false; 2];
                        let mut continued = false;

                        for (action, thread_id) in args
                            .split(|c| *c == b';')
                            .map(|action_and_tid| split_once(action_and_tid, b':'))
                        {
                            let thread_id = if thread_id.is_empty() {
                                ThreadId::All
                            } else {
                                tid_from_ascii(thread_id)
                                    .ok_or(PacketError::Parsing)?
                                    .ok_or(PacketError::InvalidParams)?
                            };

                            let mut apply = [
                                !applied[0] && thread_id.has_arm7(),
                                !applied[1] && thread_id.has_arm9(),
                            ];
                            applied[0] |= apply[0];
                            applied[1] |= apply[1];

                            // TODO: Step while in range + actually step by one instruction
                            match action {
                                b"c" => {
                                    apply[0] &= !self.is_running[0];
                                    apply[1] &= !self.is_running[1];
                                    if apply[0] {
                                        emu.arm7.is_stopped = false;
                                        self.is_running[0] = true;
                                        self.stop_causes.borrow_mut().cores[0] = None;
                                        self.remaining_step_cycles[0] = 0;
                                    }
                                    if apply[1] {
                                        emu.arm9.is_stopped = false;
                                        self.is_running[1] = true;
                                        self.stop_causes.borrow_mut().cores[1] = None;
                                        self.remaining_step_cycles[1] = 0;
                                    }
                                    continued |= apply[0] || apply[1];
                                }
                                b"s" | b"r" => {
                                    apply[0] &= !self.is_running[0];
                                    apply[1] &= !self.is_running[1];
                                    if apply[0] {
                                        emu.arm7.is_stopped = false;
                                        self.is_running[0] = true;
                                        self.stop_causes.borrow_mut().cores[0] = None;
                                        self.remaining_step_cycles[0] = 1;
                                    }
                                    if apply[1] {
                                        emu.arm9.is_stopped = false;
                                        self.is_running[1] = true;
                                        self.stop_causes.borrow_mut().cores[1] = None;
                                        self.remaining_step_cycles[1] = 1;
                                    }
                                    continued |= apply[0] || apply[1];
                                }
                                b"t" => {
                                    if self.is_in_non_stop_mode {
                                        apply[0] &= self.is_running[0];
                                        apply[1] &= self.is_running[1];
                                        if apply[0] {
                                            emu.arm7.is_stopped = true;
                                            self.stop_causes.borrow_mut().push(
                                                StopCause::CoreStopped(
                                                    CoreStopCause::Break,
                                                    Core::Arm7,
                                                ),
                                            );
                                        }
                                        if apply[1] {
                                            emu.arm9.is_stopped = true;
                                            self.stop_causes.borrow_mut().push(
                                                StopCause::CoreStopped(
                                                    CoreStopCause::Break,
                                                    Core::Arm9,
                                                ),
                                            );
                                        }
                                    }
                                }
                                _ => {
                                    return Err(PacketError::UnrecognizedParams.into());
                                }
                            }
                        }

                        if !self.is_in_non_stop_mode && continued {
                            self.continued()?;
                            return Ok(EmuControlFlow::Continue);
                        } else {
                            ok!();
                        }
                    }

                    b"Cont?" => {
                        self.send_packet_slice(b"vCont;c;s;t;r")?;
                        return Ok(EmuControlFlow::Continue);
                    }

                    b"CtrlC" => {
                        if self.is_in_non_stop_mode {
                            self.manually_stop(emu);
                            self.poll_stop_causes(emu)?;
                            ok!();
                        }
                    }

                    b"Stopped" => {
                        if let Some((sequence_kind, ack_thread_id)) =
                            self.cur_vstopped_sequence_kind
                        {
                            if ack_thread_id.has_arm7() {
                                self.is_running[0] = false;
                            }
                            if ack_thread_id.has_arm9() {
                                self.is_running[1] = false;
                            }

                            match sequence_kind {
                                VStoppedSequenceKind::New => {
                                    let stop_cause = self.stop_causes.borrow_mut().pop();
                                    if let Some(stop_cause) = stop_cause {
                                        self.send_stop_cause_packet(stop_cause)?;
                                        self.cur_vstopped_sequence_kind = Some((
                                            VStoppedSequenceKind::New,
                                            stop_cause.thread_id().unwrap_or(ThreadId::All),
                                        ));
                                        return Ok(EmuControlFlow::Continue);
                                    } else {
                                        self.cur_vstopped_sequence_kind = None;
                                        ok!();
                                    }
                                }
                                VStoppedSequenceKind::Stopped { mut i } => {
                                    let stop_causes = self.stop_causes.borrow_mut();
                                    while i < 2 {
                                        let stop_cause = stop_causes.cores[i as usize];
                                        i += 1;
                                        if let Some(stop_cause) = stop_cause {
                                            drop(stop_causes);
                                            self.send_stop_cause_packet(stop_cause)?;
                                            self.cur_vstopped_sequence_kind = Some((
                                                VStoppedSequenceKind::Stopped { i },
                                                if i == 1 {
                                                    ThreadId::Arm7
                                                } else {
                                                    ThreadId::Arm9
                                                },
                                            ));
                                            return Ok(EmuControlFlow::Continue);
                                        }
                                    }
                                    drop(stop_causes);
                                    self.cur_vstopped_sequence_kind = None;
                                    ok!();
                                }
                            }
                        }
                    }

                    _ => {}
                }
            }

            b'X' => {
                let (addr_length, bytes) = split_once_mut(data, b':');
                let (addr, length) = split_once(addr_length, b',');
                let mut addr = parse_int!(addr);
                let length = parse_int!(length);

                let bytes = {
                    let mut src_i = 0;
                    let mut dst_i = 0;
                    while src_i < bytes.len() {
                        bytes[dst_i] = if bytes[src_i] == b'}' {
                            src_i += 1;
                            bytes.get(src_i).ok_or(PacketError::Parsing)? ^ 0x20
                        } else {
                            bytes[src_i]
                        };
                        src_i += 1;
                        dst_i += 1;
                    }
                    &bytes[..dst_i]
                };

                if bytes.len() != length as usize {
                    return Err(PacketError::InvalidParams.into());
                }

                for &byte in bytes {
                    if self.g_thread.has_arm7() {
                        arm7::bus::write_8::<DebugCpuAccess, _>(emu, addr, byte);
                    }
                    if self.g_thread.has_arm9() {
                        arm9::bus::write_8::<DebugCpuAccess, _>(emu, addr, byte);
                    }
                    addr = addr.wrapping_add(1);
                }

                ok!();
            }

            b'z' => {
                let (ty, addr_kind) = split_once(data, b',');
                let (addr, kind) = split_once(addr_kind, b',');
                let ty = parse_int!(ty);
                let addr = parse_int!(addr);
                let kind = parse_int!(kind);

                if kind > u8::MAX as u32 {
                    return Err(PacketError::InvalidParams.into());
                }
                if (2..5).contains(&ty) && (!kind.is_power_of_two() || addr & (kind - 1) != 0) {
                    return Err(PacketError::InvalidParams.into());
                }

                match ty {
                    0 => {
                        if !matches!(kind, 2 | 4) {
                            return Err(PacketError::UnrecognizedParams.into());
                        }
                        self.toggle_sw_breakpoint::<_, false>(emu, addr, kind == 2);
                        ok!();
                    }

                    1 => {
                        self.toggle_hw_breakpoint::<_, false>(emu, addr);
                        ok!();
                    }

                    2 => {
                        self.toggle_watchpoint::<_, false, true, false>(emu, addr, kind as u8);
                        ok!();
                    }

                    3 => {
                        self.toggle_watchpoint::<_, true, false, false>(emu, addr, kind as u8);
                        ok!();
                    }

                    4 => {
                        self.toggle_watchpoint::<_, true, true, false>(emu, addr, kind as u8);
                        ok!();
                    }

                    _ => {}
                }
            }

            b'Z' => {
                let (ty, addr_kind) = split_once(data, b',');
                let (addr, kind) = split_once(addr_kind, b',');
                let ty = parse_int!(ty);
                let addr = parse_int!(addr);
                let kind = parse_int!(kind);

                if kind > u8::MAX as u32 {
                    return Err(PacketError::InvalidParams.into());
                }
                if (2..5).contains(&ty) && (!kind.is_power_of_two() || addr & (kind - 1) != 0) {
                    return Err(PacketError::InvalidParams.into());
                }

                match ty {
                    0 => {
                        if !matches!(kind, 2 | 4) {
                            return Err(PacketError::UnrecognizedParams.into());
                        }
                        self.toggle_sw_breakpoint::<_, true>(emu, addr, kind == 2);
                        ok!();
                    }

                    1 => {
                        self.toggle_hw_breakpoint::<_, true>(emu, addr);
                        ok!();
                    }

                    2 => {
                        self.toggle_watchpoint::<_, false, true, true>(emu, addr, kind as u8);
                        ok!();
                    }

                    3 => {
                        self.toggle_watchpoint::<_, true, false, true>(emu, addr, kind as u8);
                        ok!();
                    }

                    4 => {
                        self.toggle_watchpoint::<_, true, true, true>(emu, addr, kind as u8);
                        ok!();
                    }

                    _ => {}
                }
            }

            _ => {}
        }

        Err(PacketError::Unrecognized.into())
    }
}
