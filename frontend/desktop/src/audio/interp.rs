use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InterpMethod {
    Nearest,
    Cubic,
}

impl InterpMethod {
    #[inline]
    pub fn create_interp(self) -> Box<dyn Interp> {
        match self {
            InterpMethod::Nearest => Box::new(Nearest {
                last_sample: [0.0; 2],
            }),
            InterpMethod::Cubic => Box::new(Cubic {
                hist: [[0.0; 2]; 4],
            }),
        }
    }
}

pub trait Interp: Send {
    fn push_input_sample(&mut self, sample: [f64; 2]);
    fn copy_last_input_sample(&mut self);
    fn get_output_sample(&self, fract: f64) -> [f64; 2];
}

struct Nearest {
    last_sample: [f64; 2],
}

impl Interp for Nearest {
    fn push_input_sample(&mut self, sample: [f64; 2]) {
        self.last_sample = sample;
    }
    fn copy_last_input_sample(&mut self) {}
    fn get_output_sample(&self, _fract: f64) -> [f64; 2] {
        self.last_sample
    }
}

struct Cubic {
    hist: [[f64; 2]; 4],
}

impl Interp for Cubic {
    fn push_input_sample(&mut self, sample: [f64; 2]) {
        self.hist.copy_within(1..4, 0);
        self.hist[3] = sample;
    }
    fn copy_last_input_sample(&mut self) {
        self.hist.copy_within(1..4, 0);
    }
    fn get_output_sample(&self, fract: f64) -> [f64; 2] {
        let mut result = [0.0; 2];
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
