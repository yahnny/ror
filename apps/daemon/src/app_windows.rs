//! Find an app's top-level windows. Which windows count, and which program
//! owns them, lives in the shared `ror-win` crate.

use std::collections::HashMap;

use ror_core::config::App;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{GA_ROOTOWNER, GetAncestor, GetForegroundWindow};

/// A window handle as a plain integer, so it can be stored, compared and
/// sent between threads.
pub type Hwnd = isize;

pub fn to_hwnd(h: Hwnd) -> HWND {
    HWND(h as *mut _)
}

fn from_hwnd(h: HWND) -> Hwnd {
    h.0 as isize
}

/// The app's windows that would show in the taskbar, topmost (most recently
/// used) first.
pub fn find(app: &App) -> Vec<Hwnd> {
    // pid -> does the process match the app; looked up once per process.
    let mut pids: HashMap<u32, bool> = HashMap::new();
    let mut found = Vec::new();
    ror_win::for_each_window(|hwnd| {
        if !ror_win::is_taskbar_window(hwnd) {
            return;
        }
        let pid = ror_win::window_pid(hwnd);
        let matches = *pids.entry(pid).or_insert_with(|| {
            ror_win::process_path(pid).is_some_and(|p| app.matches_process(ror_win::file_name(&p)))
        });
        if matches {
            found.push(from_hwnd(hwnd));
        }
    });
    found
}

/// The foreground window, mapped to one of `windows` if it belongs to them.
/// A focused dialog counts as its owner window.
pub fn foreground_among(windows: &[Hwnd]) -> Option<Hwnd> {
    // SAFETY: plain queries.
    unsafe {
        let fg = GetForegroundWindow();
        if fg.is_invalid() {
            return None;
        }
        let root = GetAncestor(fg, GA_ROOTOWNER);
        [from_hwnd(fg), from_hwnd(root)].into_iter().find(|w| windows.contains(w))
    }
}
