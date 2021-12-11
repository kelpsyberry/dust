use super::{AsyncKV, AsyncRecord, AsyncValue};
use crossbeam_channel::Sender;
use slog::{ser::Serializer, Key, OwnedKVList, Record, KV};
use std::fmt;

struct ToSendSerializer(AsyncKV);

impl Serializer for ToSendSerializer {
    #[inline]
    fn emit_none(&mut self, key: Key) -> slog::Result {
        self.0 .0.push((key, AsyncValue::None));
        Ok(())
    }
    #[inline]
    fn emit_unit(&mut self, key: Key) -> slog::Result {
        self.0 .0.push((key, AsyncValue::Unit));
        Ok(())
    }
    #[inline]
    fn emit_bool(&mut self, key: Key, val: bool) -> slog::Result {
        self.0 .0.push((key, AsyncValue::Bool(val)));
        Ok(())
    }
    #[inline]
    fn emit_char(&mut self, key: Key, val: char) -> slog::Result {
        self.0 .0.push((key, AsyncValue::Char(val)));
        Ok(())
    }
    #[inline]
    fn emit_str(&mut self, key: Key, val: &str) -> slog::Result {
        self.0 .0.push((key, AsyncValue::String(val.to_string())));
        Ok(())
    }
    #[inline]
    fn emit_usize(&mut self, key: Key, val: usize) -> slog::Result {
        self.0 .0.push((key, AsyncValue::U64(val as u64)));
        Ok(())
    }
    #[inline]
    fn emit_isize(&mut self, key: Key, val: isize) -> slog::Result {
        self.0 .0.push((key, AsyncValue::I64(val as i64)));
        Ok(())
    }
    #[inline]
    fn emit_u8(&mut self, key: Key, val: u8) -> slog::Result {
        self.0 .0.push((key, AsyncValue::U64(val as u64)));
        Ok(())
    }
    #[inline]
    fn emit_i8(&mut self, key: Key, val: i8) -> slog::Result {
        self.0 .0.push((key, AsyncValue::I64(val as i64)));
        Ok(())
    }
    #[inline]
    fn emit_u16(&mut self, key: Key, val: u16) -> slog::Result {
        self.0 .0.push((key, AsyncValue::U64(val as u64)));
        Ok(())
    }
    #[inline]
    fn emit_i16(&mut self, key: Key, val: i16) -> slog::Result {
        self.0 .0.push((key, AsyncValue::I64(val as i64)));
        Ok(())
    }
    #[inline]
    fn emit_u32(&mut self, key: Key, val: u32) -> slog::Result {
        self.0 .0.push((key, AsyncValue::U64(val as u64)));
        Ok(())
    }
    #[inline]
    fn emit_i32(&mut self, key: Key, val: i32) -> slog::Result {
        self.0 .0.push((key, AsyncValue::I64(val as i64)));
        Ok(())
    }
    #[inline]
    fn emit_u64(&mut self, key: Key, val: u64) -> slog::Result {
        self.0 .0.push((key, AsyncValue::U64(val)));
        Ok(())
    }
    #[inline]
    fn emit_i64(&mut self, key: Key, val: i64) -> slog::Result {
        self.0 .0.push((key, AsyncValue::I64(val)));
        Ok(())
    }
    #[inline]
    fn emit_f32(&mut self, key: Key, val: f32) -> slog::Result {
        self.0 .0.push((key, AsyncValue::F32(val)));
        Ok(())
    }
    #[inline]
    fn emit_f64(&mut self, key: Key, val: f64) -> slog::Result {
        self.0 .0.push((key, AsyncValue::F64(val)));
        Ok(())
    }
    #[inline]
    fn emit_arguments(&mut self, key: Key, val: &fmt::Arguments) -> slog::Result {
        self.0 .0.push((key, AsyncValue::String(fmt::format(*val))));
        Ok(())
    }
}

#[derive(Debug)]
pub enum DrainError {
    Serialization(slog::Error),
    Send,
}

pub struct Drain {
    tx: Sender<AsyncRecord>,
}

impl Drain {
    #[inline]
    pub fn new(tx: Sender<AsyncRecord>) -> Self {
        Drain { tx }
    }
}

impl slog::Drain for Drain {
    type Ok = ();
    type Err = DrainError;

    fn log(&self, record: &Record, logger_values: &OwnedKVList) -> Result<Self::Ok, Self::Err> {
        let mut ser = ToSendSerializer(AsyncKV(Vec::new()));
        record
            .kv()
            .serialize(record, &mut ser)
            .map_err(DrainError::Serialization)?;
        self.tx
            .send(AsyncRecord {
                msg: fmt::format(*record.msg()),
                location: *record.location(),
                tag: record.tag().to_string(),
                level: record.level(),
                kv: ser.0,
                logger_values: logger_values.clone(),
            })
            .map_err(|_| DrainError::Send)
    }
}
