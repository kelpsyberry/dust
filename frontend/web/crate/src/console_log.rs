use slog::*;
use std::{
    cell::RefCell,
    fmt,
    panic::{RefUnwindSafe, UnwindSafe},
    result,
};
use wasm_bindgen::JsValue;
use web_sys::console;

pub struct Console {
    history: RefCell<Vec<(String, String)>>,
}

// The program is single threaded, this should be safe
unsafe impl Sync for Console {}
impl RefUnwindSafe for Console {}
impl UnwindSafe for Console {}

impl Drain for Console {
    type Ok = ();
    type Err = Never;

    fn log(&self, record: &Record, values: &OwnedKVList) -> result::Result<Self::Ok, Self::Err> {
        let mut message = String::new();
        {
            let mut history_ref = self.history.borrow_mut();
            let mut serializer = CompactFormatSerializer::new(&mut *history_ref);
            let _ = values.serialize(record, &mut serializer);
            serializer.finish()
        };
        let msg = format!("{}", record.msg());
        message += &msg;
        {
            let mut serializer = Serializer::new(&mut message, !msg.is_empty());
            let _ = record.kv().serialize(record, &mut serializer);
        }
        (level_to_fn(record.level()))(&message.as_str().into());
        Ok(())
    }
}

impl Console {
    pub fn new() -> Self {
        Console {
            history: RefCell::new(vec![]),
        }
    }
}

impl Drop for Console {
    fn drop(&mut self) {
        for _ in 0..self.history.borrow().len() {
            console::group_end();
        }
    }
}

struct Serializer<'a> {
    buffer: &'a mut String,
    comma_needed: bool,
}

impl<'a> Serializer<'a> {
    fn new(buffer: &'a mut String, comma_needed: bool) -> Self {
        Serializer {
            buffer,
            comma_needed,
        }
    }

    fn maybe_print_comma(&mut self) {
        if self.comma_needed {
            *self.buffer += ", ";
        }
        self.comma_needed = true;
    }
}

macro_rules! s {
    ($s:expr, $k:expr, $v:expr) => {
        $s.maybe_print_comma();
        *$s.buffer += &format!("{}: {}", $k, $v);
    };
}

impl<'a> slog::ser::Serializer for Serializer<'a> {
    fn emit_none(&mut self, key: Key) -> slog::Result {
        s!(self, key, "None");
        Ok(())
    }
    fn emit_unit(&mut self, key: Key) -> slog::Result {
        s!(self, key, "()");
        Ok(())
    }
    fn emit_bool(&mut self, key: Key, val: bool) -> slog::Result {
        s!(self, key, val);
        Ok(())
    }
    fn emit_char(&mut self, key: Key, val: char) -> slog::Result {
        s!(self, key, val);
        Ok(())
    }
    fn emit_usize(&mut self, key: Key, val: usize) -> slog::Result {
        s!(self, key, val);
        Ok(())
    }
    fn emit_isize(&mut self, key: Key, val: isize) -> slog::Result {
        s!(self, key, val);
        Ok(())
    }
    fn emit_u8(&mut self, key: Key, val: u8) -> slog::Result {
        s!(self, key, val);
        Ok(())
    }
    fn emit_i8(&mut self, key: Key, val: i8) -> slog::Result {
        s!(self, key, val);
        Ok(())
    }
    fn emit_u16(&mut self, key: Key, val: u16) -> slog::Result {
        s!(self, key, val);
        Ok(())
    }
    fn emit_i16(&mut self, key: Key, val: i16) -> slog::Result {
        s!(self, key, val);
        Ok(())
    }
    fn emit_u32(&mut self, key: Key, val: u32) -> slog::Result {
        s!(self, key, val);
        Ok(())
    }
    fn emit_i32(&mut self, key: Key, val: i32) -> slog::Result {
        s!(self, key, val);
        Ok(())
    }
    fn emit_f32(&mut self, key: Key, val: f32) -> slog::Result {
        s!(self, key, val);
        Ok(())
    }
    fn emit_u64(&mut self, key: Key, val: u64) -> slog::Result {
        s!(self, key, val);
        Ok(())
    }
    fn emit_i64(&mut self, key: Key, val: i64) -> slog::Result {
        s!(self, key, val);
        Ok(())
    }
    fn emit_f64(&mut self, key: Key, val: f64) -> slog::Result {
        s!(self, key, val);
        Ok(())
    }
    fn emit_str(&mut self, key: Key, val: &str) -> slog::Result {
        s!(self, key, val);
        Ok(())
    }
    fn emit_arguments(&mut self, key: Key, val: &fmt::Arguments) -> slog::Result {
        s!(self, key, val);
        Ok(())
    }
}

struct CompactFormatSerializer<'a> {
    history: &'a mut Vec<(String, String)>,
    buf: Vec<(String, String)>,
}

impl<'a> CompactFormatSerializer<'a> {
    fn new(history: &'a mut Vec<(String, String)>) -> Self {
        CompactFormatSerializer {
            history,
            buf: vec![],
        }
    }

    fn finish(&mut self) {
        let mut indent = 0;
        for buf in self.buf.drain(..).rev() {
            enum HistoryAction {
                None,
                Push,
                Change,
            }
            let action = if let Some(prev) = self.history.get(indent) {
                if *prev != buf {
                    HistoryAction::Change
                } else {
                    HistoryAction::None
                }
            } else {
                HistoryAction::Push
            };
            match action {
                HistoryAction::None => {}
                HistoryAction::Push => {
                    self.history.push(buf);
                    let (k, v) = &self.history[indent];
                    let group_str = format!("{}: {}", k, v);
                    console::group_1(&group_str.as_str().into());
                }
                HistoryAction::Change => {
                    for _ in indent..self.history.len() {
                        console::group_end();
                    }
                    self.history.truncate(indent);
                    self.history.push(buf);
                    let (k, v) = &self.history[indent];
                    let group_str = format!("{}: {}", k, v);
                    console::group_1(&group_str.as_str().into());
                }
            }
            indent += 1;
        }
        if indent == 0 {
            for _ in 0..self.history.len() {
                console::group_end();
            }
            self.history.clear();
        }
    }
}

macro_rules! cs(
	($s:expr, $k:expr, $v:expr) => {
	    let k = format!("{}", $k);
	    let v = format!("{}", $v);
		$s.buf.push((k, v));
	};
);

impl<'a> slog::ser::Serializer for CompactFormatSerializer<'a> {
    fn emit_none(&mut self, key: Key) -> slog::Result {
        cs!(self, key, "None");
        Ok(())
    }
    fn emit_unit(&mut self, key: Key) -> slog::Result {
        cs!(self, key, "()");
        Ok(())
    }
    fn emit_bool(&mut self, key: Key, val: bool) -> slog::Result {
        cs!(self, key, val);
        Ok(())
    }
    fn emit_char(&mut self, key: Key, val: char) -> slog::Result {
        cs!(self, key, val);
        Ok(())
    }
    fn emit_usize(&mut self, key: Key, val: usize) -> slog::Result {
        cs!(self, key, val);
        Ok(())
    }
    fn emit_isize(&mut self, key: Key, val: isize) -> slog::Result {
        cs!(self, key, val);
        Ok(())
    }
    fn emit_u8(&mut self, key: Key, val: u8) -> slog::Result {
        cs!(self, key, val);
        Ok(())
    }
    fn emit_i8(&mut self, key: Key, val: i8) -> slog::Result {
        cs!(self, key, val);
        Ok(())
    }
    fn emit_u16(&mut self, key: Key, val: u16) -> slog::Result {
        cs!(self, key, val);
        Ok(())
    }
    fn emit_i16(&mut self, key: Key, val: i16) -> slog::Result {
        cs!(self, key, val);
        Ok(())
    }
    fn emit_u32(&mut self, key: Key, val: u32) -> slog::Result {
        cs!(self, key, val);
        Ok(())
    }
    fn emit_i32(&mut self, key: Key, val: i32) -> slog::Result {
        cs!(self, key, val);
        Ok(())
    }
    fn emit_f32(&mut self, key: Key, val: f32) -> slog::Result {
        cs!(self, key, val);
        Ok(())
    }
    fn emit_u64(&mut self, key: Key, val: u64) -> slog::Result {
        cs!(self, key, val);
        Ok(())
    }
    fn emit_i64(&mut self, key: Key, val: i64) -> slog::Result {
        cs!(self, key, val);
        Ok(())
    }
    fn emit_f64(&mut self, key: Key, val: f64) -> slog::Result {
        cs!(self, key, val);
        Ok(())
    }
    fn emit_str(&mut self, key: Key, val: &str) -> slog::Result {
        cs!(self, key, val);
        Ok(())
    }
    fn emit_arguments(&mut self, key: Key, val: &fmt::Arguments) -> slog::Result {
        cs!(self, key, val);
        Ok(())
    }
}

fn level_to_fn(level: slog::Level) -> fn(&JsValue) {
    match level {
        Level::Critical => console::error_1,
        Level::Error => console::error_1,
        Level::Warning => console::warn_1,
        Level::Info => console::info_1,
        Level::Debug => console::log_1,
        Level::Trace => console::debug_1,
    }
}
