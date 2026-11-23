//! Bring a window to the front, minimize or restore it (incl. the focus-stealing workaround).
//!
//! Windows only lets a process take the foreground in some cases; receiving
//! the hotkey (`WM_HOTKEY`) counts as the last input event, so the plain
//! `SetForegroundWindow` normally works. If it doesn't, we attach to the
//! foreground thread's input queue and try again.

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, GetForegroundWindow, GetLastActivePopup, GetWindowThreadProcessId, IsIconic,
    IsWindowVisible, SW_MINIMIZE, SW_RESTORE, SetForegroundWindow, ShowWindow,
};
use windows::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled;

use crate::app_windows::{Hwnd, to_hwnd};
use crate::log::log;

/// Restore `window` if minimized and bring it (or its open dialog) to the front.
pub fn raise(window: Hwnd) {
    let hwnd = to_hwnd(window);
    // SAFETY: plain window calls; a stale handle just fails.
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        // If the app has a dialog open, focus the dialog, not the window behind it.
        let popup = GetLastActivePopup(hwnd);
        let target = if popup != hwnd
            && IsWindowVisible(popup).as_bool()
            && IsWindowEnabled(popup).as_bool()
        {
            popup
        } else {
            hwnd
        };
        if !set_foreground(target) {
            log!("focus: could not bring {:#x} to the front", target.0 as isize);
        }
    }
}

/// Minimize `window`; Windows activates the next window, like Win+1 does.
pub fn minimize(window: Hwnd) {
    // SAFETY: plain window call.
    unsafe {
        let _ = ShowWindow(to_hwnd(window), SW_MINIMIZE);
    }
}

unsafe fn set_foreground(target: HWND) -> bool {
    // SAFETY: plain window/thread calls, input is always detached again.
    unsafe {
        if SetForegroundWindow(target).as_bool() && GetForegroundWindow() == target {
            return true;
        }
        log!("focus: SetForegroundWindow refused, attaching to foreground thread");
        let fg_thread = GetWindowThreadProcessId(GetForegroundWindow(), None);
        let me = GetCurrentThreadId();
        let attached = fg_thread != 0 && fg_thread != me && AttachThreadInput(me, fg_thread, true).as_bool();
        let _ = BringWindowToTop(target);
        let _ = SetForegroundWindow(target);
        if attached {
            let _ = AttachThreadInput(me, fg_thread, false);
        }
        GetForegroundWindow() == target
    }
}
