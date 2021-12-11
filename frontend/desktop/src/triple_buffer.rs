use std::{
    cell::UnsafeCell,
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
};

struct Buffers<T> {
    frame_data: [UnsafeCell<T>; 3],
    next: AtomicU8,
}

unsafe impl<T> Sync for Buffers<T> {}

pub struct Sender<T> {
    buffers: Arc<Buffers<T>>,
    i: u8,
}

impl<T> Sender<T> {
    pub fn start(&mut self) -> &mut T {
        unsafe { &mut *self.buffers.frame_data.get_unchecked(self.i as usize).get() }
    }

    pub fn finish(&mut self) {
        self.i = self.buffers.next.swap(self.i | 4, Ordering::AcqRel) & 3;
    }
}

pub struct Receiver<T> {
    buffers: Arc<Buffers<T>>,
    i: u8,
}

impl<T> Receiver<T> {
    pub fn get(&mut self) -> Result<&T, &T> {
        unsafe {
            if self.buffers.next.load(Ordering::Relaxed) & 4 != 0 {
                self.i = self.buffers.next.swap(self.i, Ordering::AcqRel) & 3;
                Ok(&*self.buffers.frame_data.get_unchecked(self.i as usize).get())
            } else {
                Err(&*self.buffers.frame_data.get_unchecked(self.i as usize).get())
            }
        }
    }
}

pub fn init<T>(frame_data: [T; 3]) -> (Sender<T>, Receiver<T>) {
    let [a, b, c] = frame_data;
    let buffers = Arc::new(Buffers {
        frame_data: [UnsafeCell::new(a), UnsafeCell::new(b), UnsafeCell::new(c)],
        next: AtomicU8::new(1),
    });
    (
        Sender {
            buffers: Arc::clone(&buffers),
            i: 0,
        },
        Receiver { buffers, i: 2 },
    )
}
