//! `ror-settings.exe`: a small settings window (proof of concept).
//!
//! It only edits `config.toml`; `rord` picks up the change. It runs only
//! while open, so it adds nothing to the always-running listener.

// No console window behind the app.
#![windows_subsystem = "windows"]

#[cfg(windows)]
mod commands {
    use ror_core::config;
    use std::collections::HashMap;

    use ror_core::config::Launch;
    use ror_core::edit::{self, AppSettings, Settings};
    use ror_core::ipc::{self, ClientError, Request, Response, Status};
    use serde::Serialize;

    #[derive(Serialize)]
    pub struct Loaded {
        path: String,
        /// `None` if the file isn't valid TOML; `error` says why.
        settings: Option<Settings>,
        error: Option<String>,
    }

    #[tauri::command]
    pub fn load() -> Result<Loaded, String> {
        let path = config::default_path().ok_or("%APPDATA% is not set")?;
        config::ensure_exists(&path).map_err(|e| e.to_string())?;
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let (settings, error) = match edit::read(&text) {
            Ok(s) => (Some(s), config::Config::parse(&text).err().map(|e| e.to_string())),
            Err(e) => (None, Some(e)),
        };
        Ok(Loaded { path: path.display().to_string(), settings, error })
    }

    /// Validate and write the settings, then ask rord to reload.
    /// Returns what rord said (or that it isn't running).
    #[tauri::command]
    pub fn save(settings: Settings) -> Result<String, String> {
        let path = config::default_path().ok_or("%APPDATA% is not set")?;
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let new = edit::apply(&text, &settings).map_err(|e| e.to_string())?;
        std::fs::write(&path, new).map_err(|e| e.to_string())?;
        // Keys rord couldn't register get their own popup and an outline in
        // the table, so a plain "Saved" is enough here.
        Ok(match ipc::send(&Request::Reload) {
            Err(ClientError::NotRunning) => "Saved, but rord isn't running (run `ror start`)".into(),
            Ok(Response::Error { message }) => format!("Saved, but rord says: {message}"),
            _ => "Saved".into(),
        })
    }

    /// A program with an open window, for the app picker.
    #[derive(Serialize)]
    pub struct RunningApp {
        process: String,
        /// The app's display name, e.g. "Visual Studio Code".
        name: String,
        /// Title of its most recently used window.
        title: String,
        /// Suggested launch command (see `ror_core::edit::suggest_launch`).
        launch: String,
        /// `data:image/png;base64,…`, if the program has an icon.
        icon: Option<String>,
    }

    /// Programs with taskbar windows, most recently used first.
    #[tauri::command]
    pub async fn running_apps() -> Vec<RunningApp> {
        blocking(|| {
            let vars: Vec<(&str, String)> = ["LOCALAPPDATA", "APPDATA", "USERPROFILE"]
                .into_iter()
                .filter_map(|v| Some((v, std::env::var(v).ok()?)))
                .collect();
            let vars: Vec<(&str, &str)> = vars.iter().map(|(n, v)| (*n, v.as_str())).collect();
            ror_win::running_apps()
                .into_iter()
                // Store apps all run inside this host, so they can't be told apart by process.
                .filter(|a| !a.process.eq_ignore_ascii_case("ApplicationFrameHost.exe"))
                .map(|a| {
                    let launch = edit::suggest_launch(&a.path, &vars, edit::exes_in);
                    let running = HashMap::from([(a.process.to_ascii_lowercase(), a.path.clone())]);
                    let look = ror_win::app_look(
                        std::slice::from_ref(&a.process),
                        &Launch::parse(&launch).program,
                        &running,
                    );
                    RunningApp { name: look.name, icon: look.icon.map(data_url), process: a.process, title: a.title, launch }
                })
                .collect()
        })
        .await
    }

    /// How an app in the config looks: its display name and icon.
    #[derive(Serialize)]
    pub struct AppLook {
        name: String,
        icon: Option<String>,
    }

    /// Names and icons for the apps in the config, in the same order.
    #[tauri::command]
    pub async fn describe_apps(apps: Vec<AppSettings>) -> Vec<AppLook> {
        blocking(move || {
            let all: Vec<String> = apps.iter().flat_map(|a| a.processes.clone()).collect();
            let running = ror_win::running_process_paths(&all);
            apps.iter()
                .map(|a| {
                    let launch = if a.launch.trim().is_empty() {
                        a.processes.first().cloned().unwrap_or_default()
                    } else {
                        a.launch.clone()
                    };
                    let look = ror_win::app_look(&a.processes, &Launch::parse(&launch).program, &running);
                    AppLook { name: look.name, icon: look.icon.map(data_url) }
                })
                .collect()
        })
        .await
    }

    /// Run slow Win32 work (process lists, icon files) off the UI thread.
    async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
        tauri::async_runtime::spawn_blocking(f).await.expect("worker thread panicked")
    }

    fn data_url(png: Vec<u8>) -> String {
        const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::from("data:image/png;base64,");
        for chunk in png.chunks(3) {
            let n = chunk.iter().enumerate().fold(0u32, |n, (i, &b)| n | (b as u32) << (16 - 8 * i));
            for i in 0..4 {
                out.push(if i <= chunk.len() { ABC[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
            }
        }
        out
    }

    /// rord's status, or `None` if it isn't running.
    #[tauri::command]
    pub fn status() -> Option<Status> {
        match ipc::send(&Request::Status) {
            Ok(Response::Status(s)) => Some(s),
            _ => None,
        }
    }
}

#[cfg(windows)]
fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::load,
            commands::save,
            commands::status,
            commands::running_apps,
            commands::describe_apps
        ])
        .run(tauri::generate_context!())
        .expect("error while running ror settings");
}

#[cfg(not(windows))]
fn main() {
    eprintln!("ror-settings runs on Windows only");
    std::process::exit(1);
}
