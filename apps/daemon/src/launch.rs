//! Start apps. The ~3 second double-launch guard is decided in
//! `ror_core::decide`; `rord` records launch times.
//!
//! Launching goes through `ShellExecuteExW`, which (unlike `CreateProcess`)
//! finds apps registered under "App Paths" (e.g. `firefox.exe`) and runs
//! shortcuts and `.cmd` files. It can be slow, so it runs on its own thread.

use std::path::Path;

use ror_core::config::Launch;
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx};
use windows::Win32::UI::Shell::{SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, ShellExecuteExW};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{HSTRING, PCWSTR, w};

use crate::log::log;
use crate::popup;

/// Run the launch command in the background. Failures show a popup.
pub fn spawn(launch: &Launch) {
    let program = ror_win::expand_env(&launch.program);
    let args = ror_win::expand_env(&launch.args);
    std::thread::spawn(move || {
        log!("launch: {program} {args}");
        if let Err(e) = run(&program, &args) {
            popup::show(format!("Couldn't launch {program}:\n{}", e.message()));
        }
    });
}

fn run(program: &str, args: &str) -> windows::core::Result<()> {
    // Start in the program's folder if it's a full path (some apps, e.g. Steam, care).
    let dir = Path::new(program)
        .parent()
        .filter(|d| d.is_absolute())
        .map(|d| HSTRING::from(d.as_os_str()));
    let file = HSTRING::from(program);
    let params = HSTRING::from(args);

    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
        lpVerb: w!("open"),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: if args.is_empty() { PCWSTR::null() } else { PCWSTR(params.as_ptr()) },
        lpDirectory: dir.as_ref().map_or(PCWSTR::null(), |d| PCWSTR(d.as_ptr())),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    // SAFETY: COM is initialised for this thread as ShellExecuteEx recommends;
    // every string in `info` outlives the call.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
        ShellExecuteExW(&mut info)
    }
}
