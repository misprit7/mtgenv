//! Tiny stderr + file logger so the bridge's activity is readable without
//! capturing a TTY. All subsystems route their `log()` here; if a log file was
//! initialized, every line is also appended there (flushed immediately) so it can
//! be tailed live while iterating against the client.

use std::fs::OpenOptions;
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

static SINK: OnceLock<Option<Mutex<std::fs::File>>> = OnceLock::new();

/// Initialize file logging at `path` (append mode). First call wins; later calls
/// are ignored. If the file can't be opened, logging falls back to stderr only.
pub fn init(path: &str) {
    let file = OpenOptions::new().create(true).append(true).open(path).ok();
    let _ = SINK.set(file.map(Mutex::new));
}

/// UTC HH:MM:SS timestamp (dependency-free).
fn stamp() -> String {
    let s = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    format!("{:02}:{:02}:{:02}", (s / 3600) % 24, (s / 60) % 60, s % 60)
}

/// Log one line, tagged with the subsystem `prefix`, to stderr and (if
/// initialized) the log file.
pub fn log(prefix: &str, msg: &str) {
    let line = format!("{} [{}] {}", stamp(), prefix, msg);
    eprintln!("{line}");
    if let Some(Some(file)) = SINK.get() {
        if let Ok(mut f) = file.lock() {
            let _ = writeln!(f, "{line}");
            let _ = f.flush();
        }
    }
}
