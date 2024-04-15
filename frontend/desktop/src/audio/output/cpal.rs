use super::{
    super::{Interp, InterpMethod, SAMPLE_RATE_ADJUSTMENT_RATIO},
    Receiver, DEFAULT_INPUT_SAMPLE_RATE,
};
use cpal::{
    default_host,
    platform::Stream,
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Sample, SampleFormat, SupportedStreamConfigRange,
};
use std::{
    iter,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
};
#[cfg(feature = "xq-audio")]
use std::{num::NonZeroU32, sync::atomic::AtomicU64};

struct SharedData {
    volume: AtomicU32,
    #[cfg(feature = "xq-audio")]
    sample_rate_ratio: AtomicU64,
}

pub struct OutputStream {
    _stream: Stream,
    interp_method: InterpMethod,
    interp_tx: crossbeam_channel::Sender<Box<dyn Interp<2>>>,
    #[cfg(feature = "xq-audio")]
    output_sample_rate: u32,
    shared_data: Arc<SharedData>,
}

#[cfg(feature = "xq-audio")]
fn sample_rate_ratio(custom_sample_rate: Option<NonZeroU32>, output_sample_rate: u32) -> f64 {
    (match custom_sample_rate {
        Some(sample_rate) => sample_rate.get() as f64 * SAMPLE_RATE_ADJUSTMENT_RATIO,
        None => DEFAULT_INPUT_SAMPLE_RATE as f64 * SAMPLE_RATE_ADJUSTMENT_RATIO,
    }) / output_sample_rate as f64
}

impl OutputStream {
    pub(super) fn new(
        rx: Receiver,
        interp_method: InterpMethod,
        volume: f32,
        #[cfg(feature = "xq-audio")] custom_sample_rate: Option<NonZeroU32>,
    ) -> Option<Self> {
        let output_device = default_host().default_output_device()?;
        let supported_output_config = output_device
            .supported_output_configs()
            .map_err(|e| {
                error!(
                    "Audio output error",
                    "Error setting up audio output device: {e}"
                );
            })
            .ok()?
            .filter(|config| config.channels() == 2)
            .max_by(SupportedStreamConfigRange::cmp_default_heuristics)?
            .with_max_sample_rate();

        let output_sample_rate = supported_output_config.sample_rate().0;

        let (interp_tx, interp_rx) = crossbeam_channel::unbounded();
        let shared_data = Arc::new(SharedData {
            volume: AtomicU32::new(volume.to_bits()),
            #[cfg(feature = "xq-audio")]
            sample_rate_ratio: AtomicU64::new(
                sample_rate_ratio(custom_sample_rate, output_sample_rate).to_bits(),
            ),
        });

        let mut output_data = OutputData {
            rx,
            interp_rx,
            interp: interp_method.create_interp(),
            shared_data: Arc::clone(&shared_data),
            #[cfg(not(feature = "xq-audio"))]
            sample_rate_ratio: DEFAULT_INPUT_SAMPLE_RATE as f64 * SAMPLE_RATE_ADJUSTMENT_RATIO
                / output_sample_rate as f64,
            fract: 0.0,
        };

        let err_callback = |err| panic!("Error in default audio output device stream: {err}");

        macro_rules! build_output_stream {
            ($t: ty) => {
                output_device.build_output_stream(
                    &supported_output_config.config(),
                    move |data: &mut [$t], _| output_data.fill(data),
                    err_callback,
                    None,
                )
            };
        }

        let stream = match supported_output_config.sample_format() {
            SampleFormat::U8 => build_output_stream!(u8),
            SampleFormat::I8 => build_output_stream!(i8),
            SampleFormat::U16 => build_output_stream!(u16),
            SampleFormat::I16 => build_output_stream!(i16),
            SampleFormat::U32 => build_output_stream!(u32),
            SampleFormat::I32 => build_output_stream!(i32),
            SampleFormat::U64 => build_output_stream!(u64),
            SampleFormat::I64 => build_output_stream!(i64),
            SampleFormat::F32 => build_output_stream!(f32),
            SampleFormat::F64 => build_output_stream!(f64),
            _ => panic!("Unsupported audio output sample format"),
        }
        .map_err(|e| {
            error!(
                "Audio output error",
                "Error setting up audio output stream: {e}"
            );
        })
        .ok()?;
        stream
            .play()
            .map_err(|e| {
                error!(
                    "Audio output error",
                    "Error starting audio output stream: {e}"
                );
            })
            .ok()?;

        Some(OutputStream {
            _stream: stream,
            interp_method,
            interp_tx,
            #[cfg(feature = "xq-audio")]
            output_sample_rate,
            shared_data,
        })
    }

    pub fn set_interp_method(&mut self, value: InterpMethod) {
        if value == self.interp_method {
            return;
        }
        self.interp_tx
            .send(value.create_interp())
            .expect("couldn't send new interpolator to audio output thread");
    }

    #[cfg(feature = "xq-audio")]
    pub(super) fn set_custom_sample_rate(&mut self, value: Option<NonZeroU32>) {
        self.shared_data.sample_rate_ratio.store(
            sample_rate_ratio(value, self.output_sample_rate).to_bits(),
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
    interp_rx: crossbeam_channel::Receiver<Box<dyn Interp<2>>>,
    interp: Box<dyn Interp<2>>,
    shared_data: Arc<SharedData>,
    #[cfg(not(feature = "xq-audio"))]
    sample_rate_ratio: f64,
    fract: f64,
}

impl OutputData {
    fn fill<T: Sample + cpal::FromSample<f32>>(&mut self, data: &mut [T]) {
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
                        self.rx.finish_reading();
                        return;
                    }
                    let result = self.interp.get_output_sample(fract);
                    data[output_i] = T::from_sample(result[0] as f32 * volume);
                    data[output_i + 1] = T::from_sample(result[1] as f32 * volume);
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
