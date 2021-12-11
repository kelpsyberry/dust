use super::{AsyncKV, AsyncRecord, AsyncValue};
use core::fmt::{self, Write as _};
use crossbeam_channel::{Receiver, Sender};
use imgui::*;
use slog::{ser::Serializer, BorrowedKV, Key, Level, Record, RecordStatic, KV};

#[derive(Clone)]
enum HistoryNode {
    Leaf(Level, String),
    Group(String),
}

struct LoggerValuesSerializer<'a> {
    logger_values_history: &'a mut Vec<(String, String)>,
    history: &'a mut Vec<(f32, HistoryNode)>,
    buf: &'a mut Vec<(String, String)>,
}

impl<'a> LoggerValuesSerializer<'a> {
    fn finish(&mut self) -> usize {
        let mut indent = 0;
        for buf in self.buf.drain(..).rev() {
            enum HistoryAction {
                None,
                Push,
                Change,
            }
            let action = if let Some(prev) = self.logger_values_history.get(indent) {
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
                    self.logger_values_history.push(buf);
                    let (k, v) = &self.logger_values_history[indent];
                    let group_str = format!("{}: {}", k, v);
                    self.history
                        .push((indent as f32, HistoryNode::Group(group_str)));
                }
                HistoryAction::Change => {
                    self.logger_values_history.truncate(indent);
                    self.logger_values_history.push(buf);
                    let (k, v) = &self.logger_values_history[indent];
                    let group_str = format!("{}: {}", k, v);
                    self.history
                        .push((indent as f32, HistoryNode::Group(group_str)));
                }
            }
            indent += 1;
        }
        if indent == 0 {
            self.logger_values_history.clear();
        }
        indent
    }
}

macro_rules! emit(
	($s:expr, $k:expr, $v:expr) => {{
	    let k = format!("{}", $k);
	    let v = format!("{}", $v);
		$s.buf.push((k, v));
        Ok(())
	}};
);

impl<'a> Serializer for LoggerValuesSerializer<'a> {
    #[inline]
    fn emit_none(&mut self, key: Key) -> slog::Result {
        emit!(self, key, "None")
    }
    #[inline]
    fn emit_unit(&mut self, key: Key) -> slog::Result {
        emit!(self, key, "()")
    }
    #[inline]
    fn emit_bool(&mut self, key: Key, val: bool) -> slog::Result {
        emit!(self, key, val)
    }
    #[inline]
    fn emit_char(&mut self, key: Key, val: char) -> slog::Result {
        emit!(self, key, val)
    }
    #[inline]
    fn emit_usize(&mut self, key: Key, val: usize) -> slog::Result {
        emit!(self, key, val)
    }
    #[inline]
    fn emit_isize(&mut self, key: Key, val: isize) -> slog::Result {
        emit!(self, key, val)
    }
    #[inline]
    fn emit_u8(&mut self, key: Key, val: u8) -> slog::Result {
        emit!(self, key, val)
    }
    #[inline]
    fn emit_i8(&mut self, key: Key, val: i8) -> slog::Result {
        emit!(self, key, val)
    }
    #[inline]
    fn emit_u16(&mut self, key: Key, val: u16) -> slog::Result {
        emit!(self, key, val)
    }
    #[inline]
    fn emit_i16(&mut self, key: Key, val: i16) -> slog::Result {
        emit!(self, key, val)
    }
    #[inline]
    fn emit_u32(&mut self, key: Key, val: u32) -> slog::Result {
        emit!(self, key, val)
    }
    #[inline]
    fn emit_i32(&mut self, key: Key, val: i32) -> slog::Result {
        emit!(self, key, val)
    }
    #[inline]
    fn emit_f32(&mut self, key: Key, val: f32) -> slog::Result {
        emit!(self, key, val)
    }
    #[inline]
    fn emit_u64(&mut self, key: Key, val: u64) -> slog::Result {
        emit!(self, key, val)
    }
    #[inline]
    fn emit_i64(&mut self, key: Key, val: i64) -> slog::Result {
        emit!(self, key, val)
    }
    #[inline]
    fn emit_f64(&mut self, key: Key, val: f64) -> slog::Result {
        emit!(self, key, val)
    }
    #[inline]
    fn emit_str(&mut self, key: Key, val: &str) -> slog::Result {
        emit!(self, key, val)
    }
    #[inline]
    fn emit_arguments(&mut self, key: Key, val: &fmt::Arguments) -> slog::Result {
        emit!(self, key, val)
    }
}

struct AsyncKVSerializer<'a> {
    buffer: &'a mut String,
    comma_needed: bool,
}

impl<'a> AsyncKVSerializer<'a> {
    fn serialize(&mut self, kv: AsyncKV) {
        for (key, val) in kv.0 {
            if self.comma_needed {
                self.buffer.push_str(", ");
            }
            self.comma_needed = true;
            self.buffer.push_str(key);
            self.buffer.push_str(": ");
            match val {
                AsyncValue::None => self.buffer.push_str("(None)"),
                AsyncValue::Unit => self.buffer.push_str("()"),
                AsyncValue::Bool(val) => {
                    let _ = write!(self.buffer, "{}", val);
                }
                AsyncValue::Char(val) => self.buffer.push(val),
                AsyncValue::String(val) => self.buffer.push_str(&val),
                AsyncValue::U64(val) => {
                    let _ = write!(self.buffer, "{}", val);
                }
                AsyncValue::I64(val) => {
                    let _ = write!(self.buffer, "{}", val);
                }
                AsyncValue::F32(val) => {
                    let _ = write!(self.buffer, "{}", val);
                }
                AsyncValue::F64(val) => {
                    let _ = write!(self.buffer, "{}", val);
                }
            }
        }
    }
}

pub struct Console {
    rx: Receiver<AsyncRecord>,
    logger_values_history: Vec<(String, String)>,
    history: Vec<(f32, HistoryNode)>,
    filtered_history: Vec<(f32, HistoryNode)>,
    lvs_buf: Vec<(String, String)>,
    pub filter: String,
    pub lock_to_bottom: bool,
}

impl Console {
    #[inline]
    pub fn new(lock_to_bottom: bool) -> (Self, Sender<AsyncRecord>) {
        let (tx, rx) = crossbeam_channel::unbounded();
        (
            Console {
                rx,
                logger_values_history: Vec::new(),
                history: Vec::new(),
                filtered_history: Vec::new(),
                lvs_buf: Vec::new(),
                filter: String::new(),
                lock_to_bottom,
            },
            tx,
        )
    }

    #[inline]
    pub fn clear(&mut self) {
        self.logger_values_history.clear();
        self.history.clear();
    }

    fn filter_history_groups(&mut self) {
        let mut empty_groups_start = 0;
        let mut cur_indent = 0.0;
        let mut i = 0;
        while i < self.filtered_history.len() {
            let indent = self.filtered_history[i].0;
            if indent <= cur_indent && empty_groups_start != i {
                let start_i = match self.filtered_history[empty_groups_start..i]
                    .iter()
                    .enumerate()
                    .rev()
                    .find(|(_, (group_indent, _))| *group_indent < indent)
                {
                    Some((i, _)) => empty_groups_start + i + 1,
                    None => empty_groups_start,
                };
                self.filtered_history.drain(start_i..i);
                i = start_i;
            }
            let node = &self.filtered_history[i].1;
            if let HistoryNode::Group(content) = node {
                if content.contains(&self.filter) {
                    empty_groups_start = i + 1;
                }
            } else {
                empty_groups_start = i + 1;
            }
            cur_indent = indent;
            i += 1;
        }
        self.filtered_history.drain(empty_groups_start..);
    }

    fn collapse_history_groups(&mut self) {
        let mut i = 0;
        let mut min_indent = -1.0;
        loop {
            let mut found_groups = false;
            let mut cur_indent = f32::INFINITY;
            let mut cur_content = None;
            while i < self.filtered_history.len() {
                let (indent, node) = &self.filtered_history[i];
                let indent = *indent;
                if let HistoryNode::Group(content) = node {
                    if indent == cur_indent {
                        if Some(content) == cur_content.as_ref() {
                            self.filtered_history.remove(i);
                            continue;
                        } else {
                            cur_content = Some(content.clone());
                        }
                    } else if indent > min_indent {
                        if indent < cur_indent {
                            found_groups = true;
                            cur_indent = indent;
                            cur_content = Some(content.clone());
                        }
                    } else {
                        cur_indent = f32::INFINITY;
                        cur_content = None;
                    }
                }
                i += 1;
            }
            if !found_groups {
                break;
            }
            min_indent = cur_indent;
        }
    }

    pub fn render_window(&mut self, ui: &Ui, font: Option<imgui::FontId>, opened: &mut bool) {
        imgui::Window::new("Log").opened(opened).build(ui, || {
            let style = ui.clone_style();

            ui.checkbox("Lock", &mut self.lock_to_bottom);

            ui.same_line_with_spacing(0.0, 16.0);
            let clear_button_width = ui.calc_text_size("Clear")[0] + style.frame_padding[0] * 2.0;
            ui.set_next_item_width(ui.content_region_avail()[0] - clear_button_width - 16.0);

            let prev_filter = self.filter.clone();
            if ui.input_text("", &mut self.filter).build() {
                if self.filter.is_empty() {
                    self.filtered_history.clear();
                } else {
                    if !prev_filter.is_empty() && self.filter.contains(&prev_filter) {
                        self.filtered_history.retain(|(_, node)| match node {
                            HistoryNode::Group(_) => true,
                            HistoryNode::Leaf(_, content) => content.contains(&self.filter),
                        })
                    } else {
                        self.filtered_history.clear();
                        self.filtered_history.extend(
                            self.history
                                .iter()
                                .filter(|(_, node)| match node {
                                    HistoryNode::Group(_) => true,
                                    HistoryNode::Leaf(_, content) => content.contains(&self.filter),
                                })
                                .cloned(),
                        );
                    }
                    self.filter_history_groups();
                    self.collapse_history_groups();
                }
            }

            ui.same_line_with_spacing(0.0, 16.0);
            if ui.button_with_size("Clear", [clear_button_width, 0.0]) {
                self.clear();
            }

            ui.dummy([0.0, 6.0]);
            ui.separator();
            ui.dummy([0.0, 6.0]);

            imgui::ChildWindow::new("log_contents").build(ui, || {
                let _font_token = font.map(|font| ui.push_font(font));
                self.render(ui);
            });
        });
    }

    pub fn render(&mut self, ui: &Ui) {
        for record in self.rx.try_iter() {
            let indent = {
                let mut ser = LoggerValuesSerializer {
                    logger_values_history: &mut self.logger_values_history,
                    history: &mut self.history,
                    buf: &mut self.lvs_buf,
                };
                let _ = record.logger_values.serialize(
                    &Record::new(
                        &RecordStatic {
                            location: &record.location,
                            level: record.level,
                            tag: &record.tag,
                        },
                        &format_args!("{}", record.msg),
                        BorrowedKV(&record.kv),
                    ),
                    &mut ser,
                );
                ser.finish()
            };
            let mut msg = record.msg;
            {
                let mut ser = AsyncKVSerializer {
                    comma_needed: !msg.is_empty(),
                    buffer: &mut msg,
                };
                ser.serialize(record.kv);
                self.history
                    .push((indent as f32, HistoryNode::Leaf(record.level, msg)));
            }
        }

        let line_height = ui.text_line_height();
        if self.lock_to_bottom {
            ui.set_scroll_y(ui.scroll_max_y());
        }

        let history = if self.filter.is_empty() {
            &self.history
        } else {
            &self.filtered_history
        };

        let start_i = ((ui.scroll_y() / line_height).floor() as usize).min(history.len());
        let end_i = (((ui.scroll_y() + ui.window_size()[1]) / line_height).ceil() as usize)
            .min(history.len());
        let style = ui.clone_style();
        ui.dummy([0.0, start_i as f32 * line_height]);
        let mut last_indent = 0.0;
        for (indent, node) in &history[start_i..end_i] {
            let off = *indent - last_indent;
            if off != 0.0 {
                ui.indent_by(off * style.indent_spacing);
            }
            last_indent = *indent;
            match node {
                HistoryNode::Group(msg) => {
                    ui.text(msg);
                }
                HistoryNode::Leaf(level, msg) => {
                    ui.text_colored(
                        match *level {
                            Level::Critical => [0.75, 0., 0., 1.],
                            Level::Error => [1., 0., 0., 1.],
                            Level::Warning => [0.9, 0.9, 0., 1.],
                            Level::Info => [1., 1., 1., 1.],
                            Level::Debug => [0., 0.87, 1., 1.],
                            Level::Trace => [0.75, 0.75, 0.75, 1.],
                        },
                        msg,
                    );
                    if ui.is_item_clicked_with_button(MouseButton::Right) {
                        ui.set_clipboard_text(msg);
                    }
                }
            }
        }
        ui.dummy([0.0, (history.len() - end_i) as f32 * line_height]);
    }
}
