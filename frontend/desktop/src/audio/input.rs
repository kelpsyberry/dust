mod cpal;
pub use self::cpal::*;

use super::{InterpMethod, SYS_CLOCK_RATE};
use dust_core::spi::tsc::{MicBackend, MIC_SAMPLES_PER_FRAME};
use std::{
    cell::UnsafeCell,
    sync::atomic::{AtomicUsize, Ordering},
    sync::Arc,
};

pub const OUTPUT_SAMPLE_RATE: u32 = SYS_CLOCK_RATE / 128;

struct Buffer {
    write_pos: AtomicUsize,
    data: Box<UnsafeCell<[i16; MIC_SAMPLES_PER_FRAME * 2]>>,
}

unsafe impl Send for Buffer {}
unsafe impl Sync for Buffer {}

impl Buffer {
    fn new_arc() -> Arc<Self> {
        Arc::new(Buffer {
            write_pos: AtomicUsize::new(0),
            data: unsafe { Box::new_zeroed().assume_init() },
        })
    }
}

pub struct Sender {
    buffer: Arc<Buffer>,
    write_pos: usize,
}

impl Sender {
    pub fn write_sample(&mut self, sample: i16) {
        unsafe { *(self.buffer.data.get() as *mut i16).add(self.write_pos) = sample };
        self.write_pos += 1;
        if self.write_pos >= MIC_SAMPLES_PER_FRAME {
            self.write_pos = 0;
        }
    }

    pub fn finish_writing(&mut self) {
        self.buffer
            .write_pos
            .store(self.write_pos, Ordering::Release);
    }
}

pub struct Receiver {
    buffer: Arc<Buffer>,
    frame_start_i: usize,
}

impl MicBackend for Receiver {
    fn start_frame(&mut self) {
        self.frame_start_i = self.buffer.write_pos.load(Ordering::Acquire);
    }

    fn read_frame_samples(&mut self, offset: usize, samples: &mut [i16]) {
        let buffer_data_ptr = self.buffer.data.get() as *const i16;
        let mut i = self.frame_start_i + offset;
        if i >= MIC_SAMPLES_PER_FRAME {
            i -= MIC_SAMPLES_PER_FRAME;
        }
        for sample in samples {
            *sample = unsafe { *buffer_data_ptr.add(i) };
            i += 1;
            if i >= MIC_SAMPLES_PER_FRAME {
                i = 0;
            }
        }
    }
}

pub struct Channel {
    pub input_stream: InputStream,
    pub rx: Receiver,
}

impl Channel {
    pub fn new(interp_method: InterpMethod) -> Option<Self> {
        let buffer = Buffer::new_arc();
        Some(Channel {
            input_stream: InputStream::new(
                Sender {
                    buffer: Arc::clone(&buffer),
                    write_pos: 0,
                },
                interp_method,
            )?,
            rx: Receiver {
                buffer,
                frame_start_i: 0,
            },
        })
    }
}
