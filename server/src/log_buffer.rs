use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

const MAX_ENTRIES: usize = 500;

pub struct LogEntry {
    pub elapsed_ms: f64,
    pub level: log::Level,
    pub target: String,
    pub message: String,
}

pub type LogBuffer = Arc<Mutex<VecDeque<LogEntry>>>;

struct OverlayLogger {
    buffer: LogBuffer,
    inner: env_logger::Logger,
    start: std::time::Instant,
}

impl log::Log for OverlayLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        self.inner.enabled(metadata)
    }

    fn log(&self, record: &log::Record) {
        if !self.inner.enabled(record.metadata()) {
            return;
        }
        self.inner.log(record);
        let entry = LogEntry {
            elapsed_ms: self.start.elapsed().as_secs_f64() * 1000.0,
            level: record.level(),
            target: record.target().to_string(),
            message: record.args().to_string(),
        };
        if let Ok(mut buf) = self.buffer.lock() {
            buf.push_back(entry);
            if buf.len() > MAX_ENTRIES {
                buf.pop_front();
            }
        }
    }

    fn flush(&self) {
        self.inner.flush();
    }
}

/// Install a composited logger that forwards to `env_logger` for stderr output
/// and also writes every record into the returned ring buffer.
pub fn install(inner: env_logger::Logger, start: std::time::Instant) -> LogBuffer {
    let max_level = inner.filter();
    let buffer: LogBuffer = Arc::new(Mutex::new(VecDeque::with_capacity(MAX_ENTRIES)));
    let logger = OverlayLogger {
        buffer: buffer.clone(),
        inner,
        start,
    };
    log::set_boxed_logger(Box::new(logger)).expect("logger already set");
    log::set_max_level(max_level);
    buffer
}
