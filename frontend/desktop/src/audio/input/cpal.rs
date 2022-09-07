use super::{
    super::{Interp, InterpMethod, SAMPLE_RATE_ADJUSTMENT_RATIO},
    Sender, OUTPUT_SAMPLE_RATE,
};
use cpal::{
    default_host,
    platform::Stream,
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Sample, SampleFormat, SupportedStreamConfigRange,
};

pub struct InputStream {
    _stream: Stream,
    interp_method: InterpMethod,
    interp_tx: crossbeam_channel::Sender<Box<dyn Interp<1>>>,
}

impl InputStream {
    pub(super) fn new(tx: Sender, interp_method: InterpMethod) -> Option<Self> {
        let input_device = default_host().default_input_device()?;
        let supported_input_config = input_device
            .supported_input_configs()
            .ok()?
            .max_by(SupportedStreamConfigRange::cmp_default_heuristics)?
            .with_max_sample_rate();

        let input_sample_rate = supported_input_config.sample_rate().0;

        let (interp_tx, interp_rx) = crossbeam_channel::unbounded();

        let mut input_data = InputData {
            tx,
            interp_rx,
            interp: interp_method.create_interp(),
            channels: supported_input_config.channels(),
            sample_rate_ratio: input_sample_rate as f64
                / (OUTPUT_SAMPLE_RATE as f64 * SAMPLE_RATE_ADJUSTMENT_RATIO),
            fract: 0.0,
        };

        let err_callback = |err| panic!("Error in default audio output device stream: {err}");
        let stream = match supported_input_config.sample_format() {
            SampleFormat::U16 => input_device.build_input_stream(
                &supported_input_config.config(),
                move |data: &[u16], _| input_data.fill(data),
                err_callback,
            ),
            SampleFormat::I16 => input_device.build_input_stream(
                &supported_input_config.config(),
                move |data: &[i16], _| input_data.fill(data),
                err_callback,
            ),
            SampleFormat::F32 => input_device.build_input_stream(
                &supported_input_config.config(),
                move |data: &[f32], _| input_data.fill(data),
                err_callback,
            ),
        }
        .ok()?;
        stream.play().expect("Couldn't start audio output stream");

        Some(InputStream {
            _stream: stream,
            interp_method,
            interp_tx,
        })
    }

    pub fn set_interp_method(&mut self, value: InterpMethod) {
        if value == self.interp_method {
            return;
        }
        self.interp_tx
            .send(value.create_interp())
            .expect("Couldn't send new interpolator to audio input thread");
    }
}

struct InputData {
    tx: Sender,
    interp_rx: crossbeam_channel::Receiver<Box<dyn Interp<1>>>,
    interp: Box<dyn Interp<1>>,
    channels: u16,
    sample_rate_ratio: f64,
    fract: f64,
}

impl InputData {
    fn fill<T: Sample>(&mut self, data: &[T]) {
        if let Some(interp) = self.interp_rx.try_iter().last() {
            self.interp = interp;
        }

        let mut fract = self.fract;
        for input_samples in data.chunks(self.channels as usize) {
            let mut input_sample = 0.0;
            for sample in input_samples {
                input_sample += sample.to_f32() as f64;
            }
            self.interp
                .push_input_sample([input_sample / self.channels as f64]);
            while fract < 1.0 {
                let [result] = self.interp.get_output_sample(fract);
                self.tx
                    .write_sample((result * 32768.0).clamp(-32768.0, 32767.0) as i16);
                fract += self.sample_rate_ratio;
            }
            fract -= 1.0;
        }
        self.fract = fract;
        self.tx.finish_writing();
    }
}
