use gdb_protocol::{
    packet::{CheckedPacket, Kind as PacketKind},
    parser::Parser,
};
#[cfg(target_family = "unix")]
use std::os::unix::io::AsRawFd;
#[cfg(target_family = "windows")]
use std::os::windows::io::AsRawSocket;
use std::{
    io::{self, BufRead, BufReader, ErrorKind, Write},
    net::{TcpListener, TcpStream, ToSocketAddrs},
    thread,
};

struct RunningState {
    reader: BufReader<TcpStream>,
    writer: TcpStream,
    parser: Parser,
    check_for_break: bool,
    no_ack_mode: bool,
    has_queued_break: bool,
    recv_queue: Vec<CheckedPacket>,
}

impl RunningState {
    fn fill(&mut self) -> Result<(), gdb_protocol::Error> {
        loop {
            let buf = match self.reader.fill_buf() {
                Ok(buf) => buf,
                Err(err) => {
                    if err.kind() == ErrorKind::WouldBlock {
                        self.reader.buffer()
                    } else {
                        return Err(err.into());
                    }
                }
            };
            if buf.is_empty() {
                return Ok(());
            }

            if self.check_for_break {
                if buf.starts_with(&[0xFF, 0xF3]) {
                    // Process TELNET BREAK sequence (plus 'g' if present)
                    let len = 2 + (buf.get(2) == Some(&b'g')) as usize;
                    self.reader.consume(len);
                    self.has_queued_break = true;
                    continue;
                } else if buf.first() == Some(&0x03) {
                    // Process Ctrl-C
                    self.reader.consume(1);
                    self.has_queued_break = true;
                    continue;
                }
            }

            let (read, packet) = self.parser.feed(buf)?;
            self.reader.consume(read);
            self.check_for_break = packet.is_some();
            let packet = match packet {
                Some(packet) => packet,
                None => return Ok(()),
            };
            let kind = packet.kind;
            let checked = packet.check();

            if kind == PacketKind::Packet && !self.no_ack_mode {
                loop {
                    match self
                        .writer
                        .write_all(if checked.is_some() { b"+" } else { b"-" })
                    {
                        Ok(_) => break,
                        Err(err) => {
                            if err.kind() == ErrorKind::WouldBlock {
                                thread::yield_now()
                            } else {
                                return Err(err.into());
                            }
                        }
                    }
                }
            }

            if let Some(checked) = checked {
                self.recv_queue.push(checked);
            }
        }
    }

    fn try_recv_break(&mut self) -> Result<bool, gdb_protocol::Error> {
        self.fill()?;
        let received = self.has_queued_break;
        self.has_queued_break = false;
        Ok(received)
    }

    fn try_recv_packet(&mut self) -> Result<Option<CheckedPacket>, gdb_protocol::Error> {
        self.fill()?;
        Ok(if self.recv_queue.is_empty() {
            None
        } else {
            Some(self.recv_queue.remove(0))
        })
    }

    fn send(&mut self, packet: &CheckedPacket) -> Result<(), gdb_protocol::Error> {
        'send_packet: loop {
            loop {
                match packet
                    .encode(&mut self.writer)
                    .and_then(|_| self.writer.flush())
                {
                    Ok(_) => break,
                    Err(err) => {
                        if err.kind() == ErrorKind::WouldBlock {
                            thread::yield_now();
                        } else {
                            return Err(err.into());
                        }
                    }
                }
            }
            break if self.no_ack_mode {
                Ok(())
            } else {
                loop {
                    let buf = match self.reader.fill_buf() {
                        Ok(buf) => buf,
                        Err(err) => {
                            if err.kind() == ErrorKind::WouldBlock {
                                self.reader.buffer()
                            } else {
                                return Err(err.into());
                            }
                        }
                    };
                    match buf.first() {
                        Some(b'+') => {
                            self.reader.consume(1);
                        }
                        Some(b'-') => {
                            self.reader.consume(1);
                            if packet.is_valid() {
                                continue 'send_packet;
                            } else {
                                return Err(gdb_protocol::Error::InvalidChecksum);
                            }
                        }
                        Some(_) => {}
                        None => {
                            thread::yield_now();
                            continue;
                        }
                    }
                    break Ok(());
                }
            };
        }
    }
}

enum State {
    Listening,
    Running(RunningState),
}

pub struct Server {
    listener: TcpListener,
    state: State,
}

impl Server {
    pub fn new(addr: impl ToSocketAddrs) -> Result<Self, gdb_protocol::Error> {
        let listener = TcpListener::bind(addr)?;
        listener.set_nonblocking(true)?;
        unsafe {
            #[cfg(target_family = "unix")]
            libc::setsockopt(
                listener.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_KEEPALIVE,
                &1_u32 as *const _ as *const libc::c_void,
                4,
            );
            #[cfg(target_family = "windows")]
            libc::setsockopt(
                listener.as_raw_socket() as libc::SOCKET,
                0xFFFF, // SOL_SOCKET
                0x0008, // SO_KEEPALIVE
                &true as *const _ as *const libc::c_char,
                1,
            );
        }
        Ok(Server {
            listener,
            state: State::Listening,
        })
    }

    pub fn is_running(&self) -> bool {
        matches!(self.state, State::Running(_))
    }

    pub fn poll_listener(&mut self) -> bool {
        if matches!(self.state, State::Running(_)) {
            unreachable!();
        }

        if let Ok((reader, writer)) = self
            .listener
            .accept()
            .and_then(|(writer, _)| Ok((BufReader::new(writer.try_clone()?), writer)))
        {
            self.state = State::Running(RunningState {
                reader,
                writer,
                parser: Parser::default(),
                check_for_break: true,
                no_ack_mode: false,
                has_queued_break: false,
                recv_queue: Vec::new(),
            });
            true
        } else {
            false
        }
    }

    pub fn close(&mut self) {
        self.state = State::Listening;
    }

    pub fn set_no_ack_mode(&mut self) {
        if let State::Running(state) = &mut self.state {
            state.no_ack_mode = true;
        }
    }

    pub fn try_recv_break(&mut self) -> Result<bool, gdb_protocol::Error> {
        match &mut self.state {
            State::Running(state) => state.try_recv_break(),
            _ => Err(gdb_protocol::Error::IoError(io::Error::new(
                io::ErrorKind::NotConnected,
                "Not connected",
            ))),
        }
    }

    pub fn try_recv_packet(&mut self) -> Result<Option<CheckedPacket>, gdb_protocol::Error> {
        match &mut self.state {
            State::Running(state) => state.try_recv_packet(),
            _ => Err(gdb_protocol::Error::IoError(io::Error::new(
                io::ErrorKind::NotConnected,
                "Not connected",
            ))),
        }
    }

    pub fn send(&mut self, packet: &CheckedPacket) -> Result<(), gdb_protocol::Error> {
        match &mut self.state {
            State::Running(state) => state.send(packet),
            _ => Ok(()),
        }
    }
}
