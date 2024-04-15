use crate::config::{Config, LoggingKind, Setting};
use slog::Drain;

pub enum Log {
    Imgui {
        console: Box<slog_imgui::console::Console>,
        rx: slog_imgui::async_drain::Receiver,
        console_opened: bool,
        logger: slog::Logger,
    },
    Term(slog::Logger),
}

impl Log {
    pub fn new(config: &Config) -> Self {
        match config.logging_kind.get() {
            LoggingKind::Imgui => {
                let (drain_data, rx) = slog_imgui::async_drain::init();
                let mut builder = slog_imgui::console::Builder::new();
                builder.history_capacity = *config.imgui_log_history_capacity.get() as usize;
                let console = builder.build();
                let logger = slog::Logger::root(
                    slog_imgui::async_drain::Drain::new(drain_data).fuse(),
                    slog::o!(),
                );
                Log::Imgui {
                    console: Box::new(console),
                    rx,
                    console_opened: false,
                    logger,
                }
            }

            LoggingKind::Term => {
                let decorator = slog_term::TermDecorator::new().stdout().build();
                let drain = slog_term::CompactFormat::new(decorator)
                    .use_custom_timestamp(|_: &mut dyn std::io::Write| Ok(()))
                    .build()
                    .fuse();
                Log::Term(slog::Logger::root(
                    slog_async::Async::new(drain)
                        .overflow_strategy(slog_async::OverflowStrategy::Block)
                        .thread_name("async logger".to_owned())
                        .build()
                        .fuse(),
                    slog::o!(),
                ))
            }
        }
    }

    pub fn is_imgui(&self) -> bool {
        matches!(self, Log::Imgui { .. })
    }

    pub fn logger(&self) -> &slog::Logger {
        let (Log::Imgui { logger, .. } | Log::Term(logger)) = self;
        logger
    }

    #[must_use]
    pub fn update(&mut self, config: &Config) -> bool {
        match self {
            Log::Imgui { console, .. } => {
                if *config.logging_kind.get() == LoggingKind::Imgui {
                    if config_changed!(config, imgui_log_history_capacity) {
                        console.history_capacity =
                            *config.imgui_log_history_capacity.get() as usize;
                    }
                    return false;
                }
            }
            Log::Term(..) => {
                if *config.logging_kind.get() == LoggingKind::Term {
                    return false;
                }
            }
        }
        *self = Self::new(config);
        true
    }

    pub fn draw(&mut self, ui: &imgui::Ui, font: imgui::FontId) {
        if let Log::Imgui {
            console,
            rx,
            console_opened,
            ..
        } = self
        {
            let _ = console.process_async(rx.try_iter());
            if *console_opened {
                let _window_padding = ui.push_style_var(imgui::StyleVar::WindowPadding([6.0; 2]));
                console.draw_window(ui, Some(font), 0.0, 2.0, console_opened);
            }
        }
    }
}
