//! Win32 helpers shared by the ror apps: which top-level windows count as
//! "an app's windows", and which program owns them.
//!
//! `rord` uses these to find an app's windows on each press; the settings
//! app uses them to list running apps for "Add from running apps".
#![cfg(windows)]

mod info;
pub use info::{AppLook, app_look, expand_env, file_description, find_program, icon_png, running_process_paths};

use std::collections::HashSet;

use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, MAX_PATH};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GWL_EXSTYLE, GWL_STYLE, GetWindow, GetWindowLongPtrW, GetWindowTextW,
    GetWindowThreadProcessId, IsWindowVisible, WS_CAPTION, WS_EX_APPWINDOW, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_POPUP, WS_SYSMENU, WS_THICKFRAME,
};
use windows::core::{BOOL, PWSTR};

/// Call `f` for every top-level window, topmost (most recently used) first.
pub fn for_each_window(mut f: impl FnMut(HWND)) {
    let mut f: &mut dyn FnMut(HWND) = &mut f;
    // SAFETY: the callback only runs during this call, while `f` is alive.
    let _ = unsafe { EnumWindows(Some(enum_proc), LPARAM(&mut f as *mut &mut dyn FnMut(HWND) as isize)) };
}

unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: `lparam` is the `&mut &mut dyn FnMut` passed by `for_each_window`.
    let f = unsafe { &mut *(lparam.0 as *mut &mut dyn FnMut(HWND)) };
    f(hwnd);
    true.into()
}

/// Would this top-level window have a taskbar button? Also drops cloaked
/// windows (other virtual desktops, suspended UWP) and splash screens.
pub fn is_taskbar_window(hwnd: HWND) -> bool {
    // SAFETY: plain queries on a window handle; a stale handle just fails.
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            return false;
        }
        let mut cloaked: u32 = 0;
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut u32 as *mut _,
            size_of::<u32>() as u32,
        )
        .is_ok()
            && cloaked != 0
        {
            return false;
        }
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        if ex & WS_EX_APPWINDOW.0 != 0 {
            return true;
        }
        if ex & (WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0) != 0 {
            return false;
        }
        if GetWindow(hwnd, GW_OWNER).is_ok_and(|o| !o.is_invalid()) {
            return false;
        }
        // Splash screen: a bare popup with no title bar, border or system menu.
        let framed = WS_CAPTION.0 | WS_THICKFRAME.0 | WS_SYSMENU.0;
        !(style & WS_POPUP.0 != 0 && style & framed == 0)
    }
}

/// The id of the process that owns a window.
pub fn window_pid(hwnd: HWND) -> u32 {
    let mut pid = 0;
    // SAFETY: `pid` is a valid out pointer.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid
}

/// A window's title bar text.
pub fn window_title(hwnd: HWND) -> String {
    let mut buf = [0u16; 512];
    // SAFETY: the buffer outlives the call.
    let len = unsafe { GetWindowTextW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..len.max(0) as usize])
}

/// Full path of a process's exe, e.g. `C:\Program Files\Mozilla Firefox\firefox.exe`.
pub fn process_path(pid: u32) -> Option<String> {
    // SAFETY: the handle is closed before returning; the buffer outlives the call.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; MAX_PATH as usize * 2];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(process);
        ok.ok()?;
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

/// The file name part of a Windows path: `C:\x\firefox.exe` -> `firefox.exe`.
pub fn file_name(path: &str) -> &str {
    path.rsplit('\\').next().unwrap_or(path)
}

/// A program that currently has a window in the taskbar.
#[derive(Debug, Clone)]
pub struct RunningApp {
    /// Exe file name, e.g. `firefox.exe`.
    pub process: String,
    /// Full path to the exe.
    pub path: String,
    /// Title of its most recently used window.
    pub title: String,
}

/// Programs with taskbar windows, most recently used first, one entry per
/// exe name. Skips ror's own windows.
pub fn running_apps() -> Vec<RunningApp> {
    let me = std::process::id();
    let mut seen = HashSet::new();
    let mut apps = Vec::new();
    for_each_window(|hwnd| {
        if !is_taskbar_window(hwnd) {
            return;
        }
        let pid = window_pid(hwnd);
        if pid == me {
            return;
        }
        let Some(path) = process_path(pid) else { return };
        let process = file_name(&path).to_string();
        // ror's own windows (e.g. the settings app) aren't apps to bind.
        if process.eq_ignore_ascii_case("ror-settings.exe") {
            return;
        }
        if seen.insert(process.to_ascii_lowercase()) {
            apps.push(RunningApp { process, path, title: window_title(hwnd) });
        }
    });
    apps
}
