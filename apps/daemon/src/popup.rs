//! Non-blocking error message boxes.
//!
//! Each box runs on its own thread, so the main thread keeps handling
//! hotkeys while it is open (a message box runs a modal loop, which would
//! also swallow the main thread's thread messages).

use windows::Win32::UI::WindowsAndMessaging::{
    MB_ICONWARNING, MB_OK, MB_SETFOREGROUND, MB_TOPMOST, MessageBoxW,
};
use windows::core::{HSTRING, w};

use crate::log::log;

pub fn show(text: impl Into<String>) {
    let text = text.into();
    log!("popup: {}", text.replace('\n', " | "));
    std::thread::spawn(move || {
        let text = HSTRING::from(text);
        // SAFETY: both strings outlive the call.
        unsafe {
            MessageBoxW(None, &text, w!("ror"), MB_OK | MB_ICONWARNING | MB_SETFOREGROUND | MB_TOPMOST);
        }
    });
}
