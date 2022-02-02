use dust_core::audio::Sample;
use js_sys::{Float32Array, Function};

pub struct Backend {
    callback: Function,
}

impl Backend {
    pub fn new(callback: Function) -> Self {
        Backend { callback }
    }
}

impl dust_core::audio::Backend for Backend {
    fn handle_sample_chunk(&mut self, samples: &mut Vec<[Sample; 2]>) {
        let mut l_buf = Vec::with_capacity(samples.len());
        let mut r_buf = Vec::with_capacity(samples.len());
        for [l, r] in samples.drain(..) {
            l_buf.push(l as f32 * (1.0 / 512.0) - 1.0);
            r_buf.push(r as f32 * (1.0 / 512.0) - 1.0);
        }
        let _ = self.callback.call2(
            &wasm_bindgen::JsValue::UNDEFINED,
            &Float32Array::from(&l_buf[..]),
            &Float32Array::from(&r_buf[..]),
        );
    }
}
