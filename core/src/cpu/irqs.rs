use super::{dma, timers, Schedule};

pub trait Irqs {
    type Schedule: Schedule;
    fn request_timer(&mut self, i: timers::Index, schedule: &mut Self::Schedule);
    fn request_dma(&mut self, i: dma::Index);
}
