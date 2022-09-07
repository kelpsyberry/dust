use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InterpMethod {
    Nearest,
    Cubic,
}

impl InterpMethod {
    #[inline]
    pub fn create_interp<const CHANNELS: usize>(self) -> Box<dyn Interp<CHANNELS>> {
        match self {
            InterpMethod::Nearest => Box::new(Nearest {
                last_sample: [0.0; CHANNELS],
            }),
            InterpMethod::Cubic => Box::new(Cubic {
                hist: [[0.0; CHANNELS]; 4],
            }),
        }
    }
}

pub trait Interp<const CHANNELS: usize>: Send {
    fn push_input_sample(&mut self, sample: [f64; CHANNELS]);
    fn copy_last_input_sample(&mut self);
    fn get_output_sample(&self, fract: f64) -> [f64; CHANNELS];
}

struct Nearest<const CHANNELS: usize> {
    last_sample: [f64; CHANNELS],
}

impl<const CHANNELS: usize> Interp<CHANNELS> for Nearest<CHANNELS> {
    fn push_input_sample(&mut self, sample: [f64; CHANNELS]) {
        self.last_sample = sample;
    }
    fn copy_last_input_sample(&mut self) {}
    fn get_output_sample(&self, _fract: f64) -> [f64; CHANNELS] {
        self.last_sample
    }
}

struct Cubic<const CHANNELS: usize> {
    hist: [[f64; CHANNELS]; 4],
}

impl<const CHANNELS: usize> Interp<CHANNELS> for Cubic<CHANNELS> {
    fn push_input_sample(&mut self, sample: [f64; CHANNELS]) {
        self.hist.copy_within(1..4, 0);
        self.hist[3] = sample;
    }
    fn copy_last_input_sample(&mut self) {
        self.hist.copy_within(1..4, 0);
    }
    fn get_output_sample(&self, fract: f64) -> [f64; CHANNELS] {
        let mut result = [0.0; CHANNELS];
        for (i, result) in result.iter_mut().enumerate() {
            let a = self.hist[3][i] - self.hist[2][i] - self.hist[0][i] + self.hist[1][i];
            let b = self.hist[0][i] - self.hist[1][i] - a;
            let c = self.hist[2][i] - self.hist[0][i];
            let d = self.hist[1][i];
            *result = (((a * fract + b) * fract + c) * fract + d).clamp(-1.0, 1.0);
        }
        result
    }
}
