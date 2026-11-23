//! Register the hotkeys with `RegisterHotKey`.
//!
//! Hotkeys are registered without a window, so `WM_HOTKEY` arrives in the
//! main thread's message queue (see `main.rs`). The hotkey id is the app's
//! index in the config plus one.

use ror_core::config::Config;
use ror_core::ipc::HotkeyStatus;
use windows::Win32::Foundation::ERROR_HOTKEY_ALREADY_REGISTERED;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey,
};

/// Registered hotkey ids, so they can be unregistered on reload.
#[derive(Default)]
pub struct Hotkeys {
    ids: Vec<i32>,
}

impl Hotkeys {
    /// Register every app's key; returns the status of each.
    /// Keys taken by another app are skipped, not fatal.
    pub fn register(&mut self, config: &Config) -> Vec<HotkeyStatus> {
        let mods = HOT_KEY_MODIFIERS(config.modifier.win32_flags()) | MOD_NOREPEAT;
        config
            .apps
            .iter()
            .enumerate()
            .map(|(i, app)| {
                let id = i as i32 + 1;
                // SAFETY: thread-level hotkey, unregistered in `unregister_all`.
                let result = unsafe { RegisterHotKey(None, id, mods, app.key.vk()) };
                let error = match result {
                    Ok(()) => {
                        self.ids.push(id);
                        None
                    }
                    Err(e) if e.code() == ERROR_HOTKEY_ALREADY_REGISTERED.to_hresult() => {
                        Some("already taken by another app".to_string())
                    }
                    Err(e) => Some(e.message()),
                };
                HotkeyStatus { key: app.key.to_string(), processes: app.processes.clone(), error }
            })
            .collect()
    }

    pub fn unregister_all(&mut self) {
        for id in self.ids.drain(..) {
            // SAFETY: `id` was registered by this thread.
            let _ = unsafe { UnregisterHotKey(None, id) };
        }
    }
}

/// App index for a `WM_HOTKEY` id.
pub fn app_index(id: usize) -> Option<usize> {
    id.checked_sub(1)
}
