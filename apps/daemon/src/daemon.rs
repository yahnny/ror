//! The listener: owns the config and hotkeys and runs the message loop.
//!
//! Everything that changes state happens on the main thread. Other threads
//! (config watcher, pipe server) wake it with thread messages, so the
//! process sleeps in `GetMessageW` while idle.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ror_core::config::{self, App, Config};
use ror_core::decide::{Action, CycleOrder, Press, decide, launched_recently};
use ror_core::ipc::{Request, Response, Status};
use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, LPARAM, WPARAM};
use windows::Win32::System::Threading::{CreateMutexW, GetCurrentThreadId};
use windows::Win32::UI::WindowsAndMessaging::{
    ASFW_ANY, AllowSetForegroundWindow, DispatchMessageW, GetMessageW, MSG, PM_NOREMOVE,
    PeekMessageW, PostThreadMessageW, TranslateMessage, WM_APP, WM_HOTKEY, WM_USER,
};
use windows::core::w;

use crate::app_windows::{self, Hwnd};
use crate::hotkey::{self, Hotkeys};
use crate::log::log;
use crate::{focus, launch, pipe, popup, watch};

/// The config file changed on disk.
const WM_CONFIG_CHANGED: u32 = WM_APP + 1;
/// A pipe command is waiting in the channel.
const WM_PIPE_COMMAND: u32 = WM_APP + 2;
/// `ror quit`.
const WM_STOP: u32 = WM_APP + 3;

/// Commands from the pipe thread that need the main thread.
enum Command {
    Reload(mpsc::Sender<Response>),
}

pub fn run(debug: bool) {
    // Only one copy runs at a time; a second one exits quietly.
    // SAFETY: the mutex handle is deliberately kept until the process exits.
    unsafe {
        let mutex = CreateMutexW(None, false, w!("Local\\ror-rord"));
        if mutex.is_err() || GetLastError() == ERROR_ALREADY_EXISTS {
            return;
        }
    }

    // SAFETY: creates this thread's message queue so other threads can post
    // to it before the loop starts.
    let main_thread = unsafe {
        let mut msg = MSG::default();
        let _ = PeekMessageW(&mut msg, None, WM_USER, WM_USER, PM_NOREMOVE);
        GetCurrentThreadId()
    };
    let post = move |msg: u32| {
        // SAFETY: posting to our own main thread.
        let _ = unsafe { PostThreadMessageW(main_thread, msg, WPARAM(0), LPARAM(0)) };
    };

    let Some(config_path) = config::default_path() else {
        popup::show("ror can't start: %APPDATA% is not set.");
        std::thread::sleep(Duration::from_secs(60));
        return;
    };
    if let Err(e) = config::ensure_exists(&config_path) {
        popup::show(format!("ror couldn't create {}:\n{e}", config_path.display()));
    }

    let mut daemon = Daemon::new(config_path.clone(), debug);
    log!("rord {} starting (pid {})", env!("CARGO_PKG_VERSION"), std::process::id());
    daemon.reload(Reason::Startup);

    watch::spawn(&config_path, move || post(WM_CONFIG_CHANGED));

    let (tx, rx) = mpsc::channel::<Command>();
    let status = daemon.status.clone();
    pipe::spawn(
        move |req| match req {
            Request::Status => Response::Status(status.lock().unwrap_or_else(|e| e.into_inner()).clone()),
            Request::Reload => {
                let (reply_tx, reply_rx) = mpsc::channel();
                if tx.send(Command::Reload(reply_tx)).is_err() {
                    return Response::Error { message: "rord is shutting down".into() };
                }
                post(WM_PIPE_COMMAND);
                reply_rx.recv_timeout(Duration::from_secs(5)).unwrap_or(Response::Error {
                    message: "rord didn't answer in time".into(),
                })
            }
            Request::Quit => Response::Ok { message: "rord stopped".into() },
        },
        move |req| {
            if *req == Request::Quit {
                post(WM_STOP);
            }
        },
    );

    daemon.message_loop(&rx);
    daemon.hotkeys.unregister_all();
    log!("rord stopped");
}

enum Reason {
    Startup,
    FileChanged,
    Requested,
}

struct Daemon {
    config_path: PathBuf,
    debug: bool,
    config: Config,
    /// Text of the last file read, so repeated change events for one save
    /// don't reload (and re-show popups) more than once.
    last_text: Option<String>,
    hotkeys: Hotkeys,
    /// Per app (keyed by its process list), so reloads keep cycle order.
    cycles: HashMap<String, CycleOrder<Hwnd>>,
    launches: HashMap<String, Instant>,
    /// Snapshot served to `ror status`.
    status: Arc<Mutex<Status>>,
}

fn app_id(app: &App) -> String {
    app.processes.join("|").to_ascii_lowercase()
}

impl Daemon {
    fn new(config_path: PathBuf, debug: bool) -> Self {
        let log_path = crate::log::set_enabled(debug);
        let status = Status {
            version: env!("CARGO_PKG_VERSION").into(),
            pid: std::process::id(),
            config_path: config_path.display().to_string(),
            log_path: log_path.map(|p| p.display().to_string()),
            ..Default::default()
        };
        Daemon {
            config_path,
            debug,
            config: Config::default(),
            last_text: None,
            hotkeys: Hotkeys::default(),
            cycles: HashMap::new(),
            launches: HashMap::new(),
            status: Arc::new(Mutex::new(status)),
        }
    }

    fn message_loop(&mut self, commands: &Receiver<Command>) {
        let mut msg = MSG::default();
        // SAFETY: standard message loop on the thread that owns the hotkeys.
        while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {
            match msg.message {
                WM_HOTKEY => self.press(msg.wParam.0),
                WM_CONFIG_CHANGED => {
                    self.reload(Reason::FileChanged);
                }
                WM_PIPE_COMMAND => {
                    while let Ok(Command::Reload(reply)) = commands.try_recv() {
                        let _ = reply.send(self.reload(Reason::Requested));
                    }
                }
                WM_STOP => break,
                // SAFETY: `msg` came from GetMessageW.
                _ => unsafe {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                },
            }
        }
    }

    /// One hotkey press. Must stay fast: this is the user waiting.
    fn press(&mut self, hotkey_id: usize) {
        let Some(app) = hotkey::app_index(hotkey_id).and_then(|i| self.config.apps.get(i)) else {
            return;
        };
        let start = Instant::now();
        let id = app_id(app);
        let found = app_windows::find(app);
        let order = self.cycles.entry(id.clone()).or_default().update(&found);
        let action = decide(&Press {
            windows: order,
            most_recent: found.first().copied(),
            foreground: app_windows::foreground_among(&found),
            launched_recently: launched_recently(self.launches.get(&id).copied(), start),
        });
        match action {
            Action::Launch => {
                // Let the launched app (or a tray app's running copy) take the foreground.
                // SAFETY: no preconditions.
                let _ = unsafe { AllowSetForegroundWindow(ASFW_ANY) };
                self.launches.insert(id, start);
                launch::spawn(&app.launch);
            }
            Action::AlreadyLaunching => {}
            Action::Raise(w) | Action::Cycle(w) => focus::raise(w),
            Action::Minimize(w) => focus::minimize(w),
        }
        log!(
            "press {}: {} window(s), {action:x?} in {:?}",
            app.key,
            found.len(),
            start.elapsed()
        );
    }

    /// Re-read the config file. On any problem, keep the last good config
    /// and show a popup.
    fn reload(&mut self, reason: Reason) -> Response {
        let text = match std::fs::read_to_string(&self.config_path) {
            Ok(t) => t,
            // Mid-save (some editors delete then rename): the next event will catch it.
            Err(_) if matches!(reason, Reason::FileChanged) => {
                return Response::Ok { message: "unchanged".into() };
            }
            Err(e) => {
                let message = format!("can't read {}: {e}", self.config_path.display());
                self.set_config_error(Some(message.clone()));
                popup::show(format!("ror: {message}"));
                return Response::Error { message };
            }
        };
        if matches!(reason, Reason::FileChanged) && self.last_text.as_ref() == Some(&text) {
            return Response::Ok { message: "unchanged".into() };
        }
        self.last_text = Some(text.clone());

        match Config::parse(&text) {
            Err(e) => {
                let message = e.to_string();
                log!("config invalid: {}", message.replace('\n', " | "));
                self.set_config_error(Some(message.clone()));
                let fallback = if matches!(reason, Reason::Startup) {
                    "No hotkeys are active until it is fixed."
                } else {
                    "ror keeps using the last good config."
                };
                popup::show(format!(
                    "{} has problems:\n\n{message}\n\n{fallback}",
                    self.config_path.display()
                ));
                Response::Error { message }
            }
            Ok(config) => self.apply(config),
        }
    }

    fn apply(&mut self, config: Config) -> Response {
        let log_path = crate::log::set_enabled(self.debug || config.log);
        self.hotkeys.unregister_all();
        let hotkeys = self.hotkeys.register(&config);

        let failed: Vec<String> = hotkeys
            .iter()
            .filter_map(|h| {
                h.error.as_ref().map(|e| format!("  {} ({}): {e}", h.key, h.processes.join(", ")))
            })
            .collect();
        if !failed.is_empty() {
            popup::show(format!(
                "ror couldn't register these keys ({}+key):\n\n{}",
                config.modifier,
                failed.join("\n")
            ));
        }
        let message = format!(
            "{} key(s) registered{}",
            hotkeys.len() - failed.len(),
            if failed.is_empty() { String::new() } else { format!(", {} failed", failed.len()) }
        );
        log!("config loaded: {message}");

        {
            let mut s = self.status.lock().unwrap_or_else(|e| e.into_inner());
            s.config_error = None;
            s.modifier = config.modifier.to_string();
            s.hotkeys = hotkeys;
            s.log_path = log_path.map(|p| p.display().to_string());
        }
        self.config = config;
        Response::Ok { message }
    }

    fn set_config_error(&self, error: Option<String>) {
        self.status.lock().unwrap_or_else(|e| e.into_inner()).config_error = error;
    }
}
