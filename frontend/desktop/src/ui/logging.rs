use super::imgui_log;
use crate::config::LoggingKind;
use slog::Drain;

pub fn init(
    imgui_log: &mut Option<(imgui_log::Console, imgui_log::Sender, bool)>,
    kind: LoggingKind,
    imgui_log_history_capacity: usize,
) -> slog::Logger {
    match kind {
        LoggingKind::Imgui => {
            let logger_tx = if let Some((_, logger_tx, _)) = imgui_log {
                logger_tx.clone()
            } else {
                let (log_console, logger_tx) =
                    imgui_log::Console::new(true, imgui_log_history_capacity);
                *imgui_log = Some((log_console, logger_tx.clone(), false));
                logger_tx
            };
            slog::Logger::root(imgui_log::Drain::new(logger_tx).fuse(), slog::o!())
        }

        LoggingKind::Term => {
            *imgui_log = None;
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
