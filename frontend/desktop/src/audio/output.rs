mod cpal;
pub use self::cpal::*;

use super::{InterpMethod, SYS_CLOCK_RATE};
use dust_core::audio::OutputSample;
use parking_lot::Mutex;
#[cfg(feature = "xq-audio")]
use parking_lot::RwLock;
#[cfg(feature = "xq-audio")]
use std::num::NonZeroU32;
use std::{
    marker::PhantomData,
    sync::atomic::{AtomicUsize, Ordering},
    sync::Arc,
    thread::{self, Thread},
};

pub const DEFAULT_INPUT_SAMPLE_RATE: u32 = SYS_CLOCK_RATE >> 10;

const BUFFER_BASE_CAPACITY: usize = 0x800;

struct Buffer {
    read_pos: AtomicUsize,
    write_pos: AtomicUsize,
    data: *mut [[OutputSample; 2]],
    thread: Mutex<Thread>,
}

unsafe impl Send for Buffer {}
unsafe impl Sync for Buffer {}

impl Buffer {
    fn new_arc(
        thread: Thread,
        #[cfg(feature = "xq-audio")] custom_sample_rate: Option<NonZeroU32>,
    ) -> Arc<Self> {
        #[cfg(not(feature = "xq-audio"))]
        let capacity = BUFFER_BASE_CAPACITY;
        #[cfg(feature = "xq-audio")]
        let capacity = match custom_sample_rate {
            Some(sample_rate) => {
                BUFFER_BASE_CAPACITY
                    * ((sample_rate.get() / DEFAULT_INPUT_SAMPLE_RATE) as usize).next_power_of_two()
            }
            None => BUFFER_BASE_CAPACITY,
        };

        Arc::new(Buffer {
            read_pos: AtomicUsize::new(capacity - 1),
            write_pos: AtomicUsize::new(0),
            data: Box::into_raw(unsafe { Box::new_zeroed_slice(capacity).assume_init() }),
            thread: Mutex::new(thread),
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
    _not_send: PhantomData<*const ()>,
}

impl Sender {
    pub fn new(data: &SenderData, sync: bool) -> Self {
        #[cfg(feature = "xq-audio")]
        let buffer = Arc::clone(&data.buffer_ptr.read());
        #[cfg(not(feature = "xq-audio"))]
        let buffer = Arc::clone(&data.buffer);
        *buffer.thread.lock() = thread::current();
        Sender {
            #[cfg(feature = "xq-audio")]
            buffer_ptr: Arc::clone(&data.buffer_ptr),
            write_pos: buffer.write_pos.load(Ordering::Relaxed),
            buffer,
            sync,
            _not_send: PhantomData,
        }
    }
}

impl dust_core::audio::Backend for Sender {
    fn handle_sample_chunk(&mut self, samples: &mut Vec<[OutputSample; 2]>) {
        while !samples.is_empty() {
            #[cfg(not(feature = "xq-audio"))]
            let buffer_mask = BUFFER_BASE_CAPACITY - 1;
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
                            buffer_mask = self.buffer.data.len() - 1;
                            len = samples.len().min((buffer_mask + 1) >> 1);
                            continue;
                        }
                    }
                    thread::park();
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
        let buffer_mask = BUFFER_BASE_CAPACITY - 1;
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

    fn finish_reading(&mut self) {
        self.buffer.thread.lock().unpark();
    }
}

pub struct Channel {
    pub tx_data: SenderData,
    pub output_stream: OutputStream,
}

impl Channel {
    pub fn new(
        interp_method: InterpMethod,
        volume: f32,
        #[cfg(feature = "xq-audio")] custom_sample_rate: Option<NonZeroU32>,
    ) -> Option<Self> {
        let buffer = Buffer::new_arc(
            thread::current(),
            #[cfg(feature = "xq-audio")]
            custom_sample_rate,
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
                interp_method,
                volume,
                #[cfg(feature = "xq-audio")]
                custom_sample_rate,
            )?,
        })
    }

    #[cfg(feature = "xq-audio")]
    pub fn set_custom_sample_rate(&mut self, custom_sample_rate: Option<NonZeroU32>) {
        let mut buffer = self.tx_data.buffer_ptr.write();
        let new_buffer = Buffer::new_arc(buffer.thread.lock().clone(), custom_sample_rate);
        *buffer = new_buffer;
        self.output_stream
            .set_custom_sample_rate(custom_sample_rate);
    }
}
