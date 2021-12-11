mod drain;
pub use drain::{Drain, DrainError};
mod console;
pub use console::Console;

use slog::{ser::Serializer, Level, OwnedKVList, Record, RecordLocation, KV};

pub type Sender = crossbeam_channel::Sender<AsyncRecord>;

enum AsyncValue {
    None,
    Unit,
    Bool(bool),
    Char(char),
    String(String),
    U64(u64),
    I64(i64),
    F32(f32),
    F64(f64),
}

struct AsyncKV(Vec<(&'static str, AsyncValue)>);

pub struct AsyncRecord {
    msg: String,
    location: RecordLocation,
    tag: String,
    level: Level,
    kv: AsyncKV,
    logger_values: OwnedKVList,
}

impl KV for AsyncKV {
    fn serialize(&self, _record: &Record, serializer: &mut dyn Serializer) -> slog::Result {
        for (key, val) in &self.0 {
            match val {
                AsyncValue::None => serializer.emit_none(key)?,
                AsyncValue::Unit => serializer.emit_unit(key)?,
                &AsyncValue::Bool(val) => serializer.emit_bool(key, val)?,
                &AsyncValue::Char(val) => serializer.emit_char(key, val)?,
                AsyncValue::String(val) => serializer.emit_str(key, val)?,
                &AsyncValue::U64(val) => serializer.emit_u64(key, val)?,
                &AsyncValue::I64(val) => serializer.emit_i64(key, val)?,
                &AsyncValue::F32(val) => serializer.emit_f32(key, val)?,
                &AsyncValue::F64(val) => serializer.emit_f64(key, val)?,
            }
        }
        Ok(())
    }
}
