use crate::config::LoggingKind;
use slog::Drain;

pub struct ImguiLogData {
    pub console: slog_imgui::console::Console,
    pub rx: slog_imgui::async_drain::Receiver,
    pub drain_data: slog_imgui::async_drain::DrainData,
    pub console_opened: bool,
}

pub fn init(
    imgui_log_data: &mut Option<ImguiLogData>,
    kind: LoggingKind,
    imgui_log_history_capacity: u32,
) -> slog::Logger {
    match kind {
        LoggingKind::Imgui => {
            let drain_data = if let Some(log_data) = imgui_log_data {
                log_data.drain_data.clone()
            } else {
                let (drain_data, rx) = slog_imgui::async_drain::init();
                let mut builder = slog_imgui::console::Builder::new();
                builder.history_capacity = imgui_log_history_capacity as usize;
                let console = builder.build();
                *imgui_log_data = Some(ImguiLogData {
                    console,
                    rx,
                    drain_data: drain_data.clone(),
                    console_opened: false,
                });
                drain_data
            };
            slog::Logger::root(
                slog_imgui::async_drain::Drain::new(drain_data).fuse(),
                slog::o!(),
            )
        }

        LoggingKind::Term => {
            *imgui_log_data = None;
            let decorator = slog_term::TermDecorator::new().stdout().build();
            let drain = slog_term::CompactFormat::new(decorator)
                .use_custom_timestamp(|_: &mut dyn std::io::Write| Ok(()))
                .build()
                .fuse();
            slog::Logger::root(
                slog_async::Async::new(drain)
                    .overflow_strategy(slog_async::OverflowStrategy::Block)
                    .thread_name("async logger".to_string())
                    .build()
                    .fuse(),
                slog::o!(),
            )
        }
    }
}
