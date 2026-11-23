//! The `ror` commands. Each returns the process exit code.

use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use ror_core::config;
use ror_core::ipc::{self, ClientError, Request, Response};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Threading::{
    CREATE_BREAKAWAY_FROM_JOB, CREATE_NEW_PROCESS_GROUP, CreateProcessW, DETACHED_PROCESS,
    PROCESS_CREATION_FLAGS, PROCESS_INFORMATION, STARTUPINFOW,
};
use windows::Win32::UI::Shell::{SEE_MASK_FLAG_NO_UI, SHELLEXECUTEINFOW, ShellExecuteExW};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{HSTRING, PCWSTR, PWSTR, w};

pub fn status() -> i32 {
    match ipc::send(&Request::Status) {
        Ok(Response::Status(s)) => {
            print!("{s}");
            if s.config_error.is_some() || s.hotkeys.iter().any(|h| h.error.is_some()) { 1 } else { 0 }
        }
        other => report(other),
    }
}

pub fn reload() -> i32 {
    report(ipc::send(&Request::Reload))
}

pub fn quit() -> i32 {
    let code = report(ipc::send(&Request::Quit));
    if code == 0 {
        // Wait until it has really gone, so `make install` can replace the exe.
        if !wait_until(Duration::from_secs(5), || !running()) {
            eprintln!("rord is still running");
            return 1;
        }
    }
    code
}

pub fn start() -> i32 {
    if running() {
        println!("already running");
        return 0;
    }
    let rord = match std::env::current_exe() {
        Ok(exe) => exe.with_file_name("rord.exe"),
        Err(e) => {
            eprintln!("can't find rord.exe: {e}");
            return 1;
        }
    };
    if let Err(e) = spawn_detached(&rord) {
        eprintln!("can't start {}: {e}", rord.display());
        return 1;
    }
    if wait_until(Duration::from_secs(3), running) {
        println!("started");
        0
    } else {
        eprintln!("started {}, but it isn't answering", rord.display());
        1
    }
}

pub fn edit() -> i32 {
    let Some(path) = config::default_path() else {
        eprintln!("%APPDATA% is not set");
        return 1;
    };
    match config::ensure_exists(&path) {
        Ok(true) => println!("created {}", path.display()),
        Ok(false) => {}
        Err(e) => {
            eprintln!("can't create {}: {e}", path.display());
            return 1;
        }
    }
    // The app associated with .toml files, or Notepad if there isn't one.
    if shell_open(&path).is_err()
        && let Err(e) = Command::new("notepad.exe").arg(&path).spawn()
    {
        eprintln!("can't open {}: {e}", path.display());
        return 1;
    }
    println!("{}", path.display());
    0
}

pub fn settings() -> i32 {
    let exe = match std::env::current_exe() {
        Ok(exe) => exe.with_file_name("ror-settings.exe"),
        Err(e) => {
            eprintln!("can't find ror-settings.exe: {e}");
            return 1;
        }
    };
    match spawn_detached(&exe) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("can't start {}: {e}", exe.display());
            1
        }
    }
}

/// Running apps, as config entries ready to paste (the settings window's
/// "Add from running apps" does the same).
pub fn apps() -> i32 {
    let vars: Vec<(&str, String)> = ["LOCALAPPDATA", "APPDATA", "USERPROFILE"]
        .into_iter()
        .filter_map(|v| Some((v, std::env::var(v).ok()?)))
        .collect();
    let vars: Vec<(&str, &str)> = vars.iter().map(|(n, v)| (*n, v.as_str())).collect();
    for app in ror_win::running_apps() {
        if app.process.eq_ignore_ascii_case("ApplicationFrameHost.exe") {
            continue; // Store apps; can't be told apart by process.
        }
        let launch = ror_core::edit::suggest_launch(&app.path, &vars, ror_core::edit::exes_in);
        println!("# {}", app.title);
        println!("[[app]]\nkey = \"?\"\nprocess = \"{}\"\nlaunch = '{launch}'\n", app.process);
    }
    0
}

pub fn version() -> i32 {
    println!("ror {}", env!("CARGO_PKG_VERSION"));
    if let Ok(Response::Status(s)) = ipc::send(&Request::Status) {
        println!("rord {} (running, pid {})", s.version, s.pid);
    }
    0
}

/// Print a response and turn it into an exit code.
fn report(result: Result<Response, ClientError>) -> i32 {
    match result {
        Ok(Response::Ok { message }) => {
            println!("{message}");
            0
        }
        Ok(Response::Error { message }) => {
            eprintln!("{message}");
            1
        }
        Ok(Response::Status(_)) => {
            eprintln!("unexpected answer from rord");
            1
        }
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

fn running() -> bool {
    !matches!(ipc::send(&Request::Status), Err(ClientError::NotRunning))
}

fn wait_until(timeout: Duration, mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if done() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Start `exe` with no console, outliving this process.
///
/// Uses `CreateProcessW` without handle inheritance: `std::process::Command`
/// always inherits, so rord would hold this process's stdout open and a
/// caller waiting for it to close (e.g. WSL running `ror.exe start`) would hang.
fn spawn_detached(exe: &Path) -> windows::core::Result<()> {
    let app = HSTRING::from(exe.as_os_str());
    let dir = exe.parent().map(|d| HSTRING::from(d.as_os_str())).unwrap_or_default();
    let startup = STARTUPINFOW { cb: size_of::<STARTUPINFOW>() as u32, ..Default::default() };
    let spawn = |flags: PROCESS_CREATION_FLAGS| {
        // CreateProcessW may write to the command line buffer, so it must be mutable.
        let mut cmdline: Vec<u16> = format!("\"{}\"", exe.display()).encode_utf16().chain([0]).collect();
        let mut info = PROCESS_INFORMATION::default();
        // SAFETY: all buffers outlive the call; the returned handles are closed.
        unsafe {
            CreateProcessW(
                &app,
                Some(PWSTR(cmdline.as_mut_ptr())),
                None,
                None,
                false,
                flags,
                None,
                &dir,
                &startup,
                &mut info,
            )?;
            let _ = CloseHandle(info.hThread);
            let _ = CloseHandle(info.hProcess);
        }
        Ok(())
    };
    let flags = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
    // Leave any job object (e.g. a terminal's) so rord isn't killed with it;
    // not every job allows that, so fall back to staying in it.
    spawn(flags | CREATE_BREAKAWAY_FROM_JOB).or_else(|_| spawn(flags))
}

fn shell_open(path: &Path) -> windows::core::Result<()> {
    let file = HSTRING::from(path.as_os_str());
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_FLAG_NO_UI,
        lpVerb: w!("open"),
        lpFile: PCWSTR(file.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    // SAFETY: `file` outlives the call.
    unsafe { ShellExecuteExW(&mut info) }
}
