//! Small rotating debug log, off by default (`log = true` or `rord --debug`).
//! When off, `log!` costs one uncontended mutex check.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

/// Rotate to `rord.log.1` when the log reaches this size.
const MAX_SIZE: u64 = 1024 * 1024;

struct Log {
    path: PathBuf,
    file: File,
    size: u64,
}

static LOG: Mutex<Option<Log>> = Mutex::new(None);

/// Turn logging on or off. Returns the log path when on.
pub fn set_enabled(on: bool) -> Option<PathBuf> {
    let mut guard = LOG.lock().unwrap_or_else(|e| e.into_inner());
    if !on {
        *guard = None;
        return None;
    }
    if let Some(log) = guard.as_ref() {
        return Some(log.path.clone());
    }
    let path = ror_core::config::log_path()?;
    let log = open(path)?;
    let path = log.path.clone();
    *guard = Some(log);
    Some(path)
}

fn open(path: PathBuf) -> Option<Log> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).ok()?;
    }
    let file = OpenOptions::new().create(true).append(true).open(&path).ok()?;
    let size = file.metadata().map(|m| m.len()).unwrap_or(0);
    Some(Log { path, file, size })
}

pub fn write(args: std::fmt::Arguments<'_>) {
    let mut guard = LOG.lock().unwrap_or_else(|e| e.into_inner());
    let Some(log) = guard.as_mut() else { return };

    if log.size > MAX_SIZE {
        let old = log.path.with_extension("log.1");
        let _ = std::fs::remove_file(&old);
        let _ = std::fs::rename(&log.path, &old);
        match open(log.path.clone()) {
            Some(new) => *log = new,
            None => {
                *guard = None;
                return;
            }
        }
    }

    let line = format!("{} {args}\n", timestamp());
    if log.file.write_all(line.as_bytes()).is_ok() {
        log.size += line.len() as u64;
    }
}

#[cfg(windows)]
fn timestamp() -> String {
    // SAFETY: no preconditions.
    let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
        t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond, t.wMilliseconds
    )
}

#[cfg(not(windows))]
fn timestamp() -> String {
    let d = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    format!("{}.{:03}", d.as_secs(), d.subsec_millis())
}

/// `log!("format {}", args)`: write a line to the debug log if it is on.
macro_rules! log {
    ($($arg:tt)*) => { $crate::log::write(format_args!($($arg)*)) };
}
pub(crate) use log;
