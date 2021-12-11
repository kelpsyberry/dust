use super::{Interp, Receiver, BASE_INPUT_SAMPLE_RATE};
#[cfg(feature = "xq-audio")]
use core::sync::atomic::AtomicU64;
use core::{
    iter,
    sync::atomic::{AtomicU32, Ordering},
};
use cpal::{
    default_host,
    platform::Stream,
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Sample, SampleFormat,
};
use std::sync::Arc;

struct SharedData {
    volume: AtomicU32,
    #[cfg(feature = "xq-audio")]
    sample_rate_ratio: AtomicU64,
}

pub struct OutputStream {
    _stream: Stream,
    interp_tx: crossbeam_channel::Sender<Box<dyn Interp>>,
    #[cfg(feature = "xq-audio")]
    base_sample_rate_ratio: f64,
    shared_data: Arc<SharedData>,
}

impl OutputStream {
    pub(super) fn new(
        rx: Receiver,
        interp: Box<dyn Interp>,
        volume: f32,
        #[cfg(feature = "xq-audio")] xq_sample_rate_shift: u8,
    ) -> Option<Self> {
        let output_device = default_host().default_output_device()?;
        let supported_output_config = output_device
            .supported_output_configs()
            .ok()?
            .find(|config| config.channels() == 2)?
            .with_max_sample_rate();

        let output_sample_rate = supported_output_config.sample_rate().0 as f64;
        let base_sample_rate_ratio = BASE_INPUT_SAMPLE_RATE / output_sample_rate;

        let (interp_tx, interp_rx) = crossbeam_channel::unbounded();
        let shared_data = Arc::new(SharedData {
            volume: AtomicU32::new(volume.to_bits()),
            #[cfg(feature = "xq-audio")]
            sample_rate_ratio: AtomicU64::new(
                (base_sample_rate_ratio * (1 << xq_sample_rate_shift) as f64).to_bits(),
            ),
        });

        let mut output_data = OutputData {
            rx,
            interp_rx,
            interp,
            shared_data: Arc::clone(&shared_data),
            #[cfg(not(feature = "xq-audio"))]
            sample_rate_ratio: base_sample_rate_ratio,
            fract: 0.0,
        };

        let err_callback = |err| panic!("Error in default audio output device stream: {}", err);
        let stream = match supported_output_config.sample_format() {
            SampleFormat::U16 => output_device.build_output_stream(
                &supported_output_config.config(),
                move |data: &mut [u16], _| output_data.fill(data),
                err_callback,
            ),
            SampleFormat::I16 => output_device.build_output_stream(
                &supported_output_config.config(),
                move |data: &mut [i16], _| output_data.fill(data),
                err_callback,
            ),
            SampleFormat::F32 => output_device.build_output_stream(
                &supported_output_config.config(),
                move |data: &mut [f32], _| output_data.fill(data),
                err_callback,
            ),
        }
        .ok()?;
        stream.play().expect("Couldn't start audio output stream");

        Some(OutputStream {
            _stream: stream,
            interp_tx,
            #[cfg(feature = "xq-audio")]
            base_sample_rate_ratio,
            shared_data,
        })
    }

    pub fn set_interp(&mut self, interp: Box<dyn Interp>) {
        self.interp_tx
            .send(interp)
            .expect("Couldn't send new interpolator to audio thread");
    }

    #[cfg(feature = "xq-audio")]
    pub fn set_xq_input_sample_rate_shift(&mut self, value: u8) {
        self.shared_data.sample_rate_ratio.store(
            (self.base_sample_rate_ratio * (1 << value) as f64).to_bits(),
            Ordering::Relaxed,
        );
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.shared_data
            .volume
            .store(volume.to_bits(), Ordering::Relaxed);
    }
}

struct OutputData {
    rx: Receiver,
    interp_rx: crossbeam_channel::Receiver<Box<dyn Interp>>,
    interp: Box<dyn Interp>,
    shared_data: Arc<SharedData>,
    #[cfg(not(feature = "xq-audio"))]
    sample_rate_ratio: f64,
    fract: f64,
}

impl OutputData {
    fn fill<T: Sample>(&mut self, data: &mut [T]) {
        if let Some(interp) = self.interp_rx.try_iter().last() {
            self.interp = interp;
        }

        let mut fract = self.fract;
        let mut output_i = 0;
        let mut volume = f32::from_bits(self.shared_data.volume.load(Ordering::Relaxed));
        volume *= volume;
        #[cfg(feature = "xq-audio")]
        let sample_rate_ratio =
            f64::from_bits(self.shared_data.sample_rate_ratio.load(Ordering::Relaxed));
        #[cfg(not(feature = "xq-audio"))]
        let sample_rate_ratio = self.sample_rate_ratio;

        let max_input_samples =
            (((data.len()) >> 1) as f64 * sample_rate_ratio + fract).ceil() as usize;

        macro_rules! push_output_samples {
            () => {
                while fract < 1.0 {
                    if output_i >= data.len() {
                        self.fract = fract;
                        return;
                    }
                    let result = self.interp.get_output_sample(fract);
                    data[output_i] = T::from(&(result[0] as f32 * volume));
                    data[output_i + 1] = T::from(&(result[1] as f32 * volume));
                    fract += sample_rate_ratio;
                    output_i += 2;
                }
                fract -= 1.0;
            };
        }

        self.rx.start_reading();

        for input_sample in iter::from_fn(|| self.rx.read_sample()).take(max_input_samples) {
            self.interp.push_input_sample(input_sample);
            push_output_samples!();
        }

        loop {
            self.interp.copy_last_input_sample();
            push_output_samples!();
        }
    }
}
