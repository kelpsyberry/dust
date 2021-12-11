mod cpal;
pub use self::cpal::*;
mod interp;
pub use interp::{Interp, InterpMethod};

use core::{
    hint::spin_loop,
    sync::atomic::{AtomicUsize, Ordering},
};
use dust_core::audio::Sample;
#[cfg(feature = "xq-audio")]
use parking_lot::RwLock;
use std::sync::Arc;

const SYS_CLOCK: f64 = (1 << 25) as f64;
const ORIG_FRAME_RATE: f64 = SYS_CLOCK / (6.0 * 355.0 * 263.0);
const BASE_INPUT_SAMPLE_RATE: f64 = SYS_CLOCK / 1024.0 * 60.0 / ORIG_FRAME_RATE;

const BUFFER_CAPACITY: usize = 0x800;

#[repr(C)]
struct Buffer {
    read_pos: AtomicUsize,
    write_pos: AtomicUsize,
    data: *mut [[Sample; 2]],
}

unsafe impl Send for Buffer {}
unsafe impl Sync for Buffer {}

impl Buffer {
    fn new_arc(#[cfg(feature = "xq-audio")] xq_sample_rate_shift: u8) -> Arc<Self> {
        #[cfg(not(feature = "xq-audio"))]
        let capacity = BUFFER_CAPACITY;
        #[cfg(feature = "xq-audio")]
        let capacity = BUFFER_CAPACITY << xq_sample_rate_shift;

        Arc::new(Buffer {
            read_pos: AtomicUsize::new(capacity - 1),
            write_pos: AtomicUsize::new(0),
            data: Box::into_raw(unsafe { Box::new_zeroed_slice(capacity).assume_init() }),
        })
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        drop(unsafe { Box::from_raw(self.data) })
    }
}

#[derive(Clone)]
pub struct SenderData {
    #[cfg(feature = "xq-audio")]
    buffer_ptr: Arc<RwLock<Arc<Buffer>>>,
    #[cfg(not(feature = "xq-audio"))]
    buffer: Arc<Buffer>,
}

pub struct Sender {
    #[cfg(feature = "xq-audio")]
    buffer_ptr: Arc<RwLock<Arc<Buffer>>>,
    buffer: Arc<Buffer>,
    write_pos: usize,
    sync: bool,
}

impl Sender {
    pub fn new(data: &SenderData, sync: bool) -> Self {
        #[cfg(feature = "xq-audio")]
        let buffer = Arc::clone(&data.buffer_ptr.read());
        #[cfg(not(feature = "xq-audio"))]
        let buffer = Arc::clone(&data.buffer);
        Sender {
            #[cfg(feature = "xq-audio")]
            buffer_ptr: Arc::clone(&data.buffer_ptr),
            write_pos: buffer.write_pos.load(Ordering::Relaxed),
            buffer,
            sync,
        }
    }
}

impl dust_core::audio::Backend for Sender {
    fn handle_sample_chunk(&mut self, samples: &mut Vec<[Sample; 2]>) {
        while !samples.is_empty() {
            #[cfg(not(feature = "xq-audio"))]
            let buffer_mask = BUFFER_CAPACITY - 1;
            #[cfg(feature = "xq-audio")]
            let mut buffer_mask = {
                let buffer = self.buffer_ptr.read();
                if Arc::as_ptr(&buffer) != Arc::as_ptr(&self.buffer) {
                    self.buffer = Arc::clone(&buffer);
                    self.write_pos = self.buffer.write_pos.load(Ordering::Relaxed);
                }
                self.buffer.data.len() - 1
            };
            #[allow(unused_mut)]
            let mut len = samples.len().min((buffer_mask + 1) >> 1);

            if self.sync {
                // Wait until enough samples have been played
                while self
                    .buffer
                    .read_pos
                    .load(Ordering::Relaxed)
                    .wrapping_sub(self.write_pos)
                    & buffer_mask
                    <= len
                {
                    #[cfg(feature = "xq-audio")]
                    {
                        let buffer = self.buffer_ptr.read();
                        if Arc::as_ptr(&buffer) != Arc::as_ptr(&self.buffer) {
                            self.buffer = Arc::clone(&buffer);
                            self.write_pos = self.buffer.write_pos.load(Ordering::Relaxed);
                        }
                        buffer_mask = self.buffer.data.len() - 1;
                        len = samples.len().min((buffer_mask + 1) >> 1);
                    }
                    spin_loop();
                }
            } else {
                // Overwrite the oldest samples, attempt to move the read position to the start of the
                // oldest remaining ones
                let _ = self.buffer.read_pos.fetch_update(
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                    |read_pos| {
                        if read_pos.wrapping_sub(self.write_pos) & buffer_mask <= len {
                            Some((self.write_pos + len + 1) & buffer_mask)
                        } else {
                            None
                        }
                    },
                );
            }
            for sample in samples.drain(..len) {
                unsafe {
                    *self.buffer.data.get_unchecked_mut(self.write_pos) = sample;
                }
                self.write_pos = (self.write_pos + 1) & buffer_mask;
            }
            self.buffer
                .write_pos
                .store(self.write_pos, Ordering::Release);
        }
    }
}

struct Receiver {
    #[cfg(feature = "xq-audio")]
    buffer_ptr: Arc<RwLock<Arc<Buffer>>>,
    buffer: Arc<Buffer>,
}

impl Receiver {
    fn start_reading(&mut self) {
        #[cfg(feature = "xq-audio")]
        {
            let buffer = self.buffer_ptr.read();
            if Arc::as_ptr(&buffer) != Arc::as_ptr(&self.buffer) {
                self.buffer = Arc::clone(&buffer);
            }
        }
    }

    fn read_sample(&mut self) -> Option<[f64; 2]> {
        #[cfg(not(feature = "xq-audio"))]
        let buffer_mask = BUFFER_CAPACITY - 1;
        #[cfg(feature = "xq-audio")]
        let buffer_mask = self.buffer.data.len() - 1;

        if let Ok(read_pos) =
            self.buffer
                .read_pos
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |read_pos| {
                    let new = (read_pos + 1) & buffer_mask;
                    if new == self.buffer.write_pos.load(Ordering::Acquire) {
                        None
                    } else {
                        Some(new)
                    }
                })
        {
            let result = unsafe { &*self.buffer.data.get_unchecked_mut(read_pos) };
            #[cfg(not(feature = "xq-audio"))]
            {
                Some([
                    result[0] as f64 * (1.0 / 512.0) - 1.0,
                    result[1] as f64 * (1.0 / 512.0) - 1.0,
                ])
            }
            #[cfg(feature = "xq-audio")]
            {
                Some([result[0] as f64, result[1] as f64])
            }
        } else {
            None
        }
    }
}

pub struct Channel {
    pub tx_data: SenderData,
    pub output_stream: OutputStream,
}

impl Channel {
    #[cfg(feature = "xq-audio")]
    pub fn set_xq_sample_rate_shift(&mut self, shift: u8) {
        let buffer = Buffer::new_arc(shift);
        *self.tx_data.buffer_ptr.write() = buffer;
        self.output_stream.set_xq_input_sample_rate_shift(shift);
    }
}

pub fn channel(
    interp_method: InterpMethod,
    volume: f32,
    #[cfg(feature = "xq-audio")] xq_sample_rate_shift: u8,
) -> Option<Channel> {
    let buffer = Buffer::new_arc(
        #[cfg(feature = "xq-audio")]
        xq_sample_rate_shift,
    );
    #[cfg(feature = "xq-audio")]
    let buffer_ptr = Arc::new(RwLock::new(Arc::clone(&buffer)));
    Some(Channel {
        tx_data: SenderData {
            #[cfg(feature = "xq-audio")]
            buffer_ptr: Arc::clone(&buffer_ptr),
            #[cfg(not(feature = "xq-audio"))]
            buffer: Arc::clone(&buffer),
        },
        output_stream: OutputStream::new(
            Receiver {
                #[cfg(feature = "xq-audio")]
                buffer_ptr,
                buffer,
            },
            interp_method.create_interp(),
            volume,
            #[cfg(feature = "xq-audio")]
            xq_sample_rate_shift,
        )?,
    })
}
