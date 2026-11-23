//! `rord.exe`: the always-running listener.
//!
//! Usage: `rord [--debug]`. `--debug` turns on the log file
//! (`%LOCALAPPDATA%\ror\rord.log`) regardless of the config.

// Build as a GUI-subsystem exe, so no console window appears at login.
#![windows_subsystem = "windows"]

#[cfg(windows)]
mod app_windows;
#[cfg(windows)]
mod daemon;
#[cfg(windows)]
mod focus;
#[cfg(windows)]
mod hotkey;
#[cfg(windows)]
mod launch;
#[cfg(windows)]
mod log;
#[cfg(windows)]
mod pipe;
#[cfg(windows)]
mod popup;
#[cfg(windows)]
mod watch;

#[cfg(windows)]
fn main() {
    let debug = std::env::args().skip(1).any(|a| a == "--debug");
    daemon::run(debug);
}

#[cfg(not(windows))]
fn main() {
    eprintln!("rord runs on Windows only");
    std::process::exit(1);
}
