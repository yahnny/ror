# ror — requirements

The design record: what was decided and why, open questions, and the progress log. For using ror, see the [README](../README.md).

Status: **agreed for v1** (2026-11-21). Nothing open blocks v1; see [Open questions](#open-questions) for what was deferred.

## Progress

### Done

- **2026-11-21:** requirements agreed for v1.
- **2026-11-21:** toolchain verified (Rust 1.100, `x86_64-pc-windows-gnu` via MinGW; exes built in WSL run on Windows).
- **2026-11-21:** project skeleton committed ([Project layout](#project-layout)).
  - Cargo workspace with `ror-core`, `rord` and `ror-cli`.
  - `make test` runs the `ror-core` tests natively in WSL (3 placeholder tests pass).
  - `make build` produces `rord.exe` and `ror.exe` with no warnings; `ror.exe` prints its version.
  - The modules are stubs. In `rord`, the window-list module is `app_windows.rs`, not `windows.rs`, so it doesn't clash with the `windows` crate.
  - Shared dependency versions (`serde`, `toml`, `windows`, `ror-core`) are commented out in the root `Cargo.toml`, because Cargo warns about unused ones. Uncomment each when a crate first uses it.

- **2026-11-22:** v1 implemented. Not yet tested by pressing hotkeys.
  - `ror-core`: config parsing and validation (reports every problem at once), the press decision, cycle order, launch guard and pipe protocol. 23 tests pass in WSL.
  - `rord`: hotkeys, window list, focus with an `AttachThreadInput` fallback, launch via `ShellExecuteExW` on its own thread, non-blocking popups, config watch (`ReadDirectoryChangesW`), named pipe server, rotating log and single instance.
  - `ror`: `status`, `reload`, `quit`, `start`, `edit`, `version`.
  - `make install` (`scripts/install.sh`) and `make lint` (clippy with `-D warnings`).
  - Smoke-tested on Windows from WSL: start, status, reload, quit, a second copy exiting, auto-reload on save, and a bad save keeping the last good config. Idle: 1.3 MB private memory, 3 threads, no CPU.
  - Choices made during the build:
    - On first run, `rord` (or `ror edit`) writes a default config with the starter key map. Launch paths point at each app's default install folder, and `%VARIABLES%` are expanded.
    - Splash screens are detected by style: a popup with no title bar, border or system menu. This is a guess; check it.
    - Cycle order and the launch guard are tracked per process list, so they survive a reload.
    - The pipe protocol is one JSON line each way (`serde_json`).
    - `ror start` uses `CreateProcessW` without handle inheritance. With `std::process::Command`, `ror.exe start` hung when run from WSL.
    - New `rord` modules: `daemon.rs` (state and message loop), `pipe.rs` and `log.rs`.

- **2026-11-22:** README and CLAUDE.md added; this file moved to `docs/requirements.md`.
- **2026-11-22:** settings UI made friendlier:
  - Each app shows its icon and display name, read from the program (`ror-win::app_look`).
  - Clicking an app opens the picker to swap it. The picker has icons and search, and marks apps already on a key.
  - The key is shown as a keycap. Process name and launch command are behind ⋯ → Edit details.
  - Known gaps: Windows Terminal gets a generic icon and is named "Windows Terminal Host"; Spotify's name can carry junk characters; an app that can't be found (e.g. Obsidian not installed) isn't flagged yet.
- **2026-11-22:** adding an app without editing anything by hand:
  - The settings window's **Add from running apps** lists programs with open windows. Picking one fills in the process name, a launch command and a free key based on the app's name. `ror apps` prints the same as config entries.
  - Launch suggestions (`ror-core::edit::suggest_launch`, tested): Store apps get their launch alias (Terminal -> `wt.exe`); self-updating apps get their stable launcher (Discord -> `Update.exe --processStart`); a helper nested in an app's folder gets the app's exe (Steam's window -> `steam.exe`); user folders become `%LOCALAPPDATA%` / `%APPDATA%`. Checked against the real running apps.
  - The window logic moved to a shared crate, `crates/ror-win`, used by the daemon, the desktop app and the CLI.
  - The ⋯ row menu (Move up, Move down, Remove) replaced the status and remove columns. A key rord couldn't register gets an amber outline.
- **2026-11-22:** `make dev` for the desktop app: Vite runs in WSL and a debug `ror-settings.exe` loads it from `devUrl`, so frontend edits show up immediately (checked: edits reach the window, and Ctrl+C stops both). Rust changes need a restart. Every other target builds with the `custom-protocol` feature, so installed builds use the embedded frontend.
- **2026-11-22:** desktop frontend rewritten in React + TypeScript (Vite 5, which supports WSL's Node 18), same look. `make build`, `release`, `lint` and `install` build it first. The installed `ror-settings.exe` exited at startup (code 53) because `WebView2Loader.dll` wasn't copied next to it; `make install` now copies it. (This was first misdiagnosed as `panic = "abort"`, which was removed; panics now unwind, which is fine for rord too.)
- **2026-11-22:** apps moved to `apps/` (`daemon`, `cli`, `desktop`); `crates/` now holds only shared libraries.
- **2026-11-22:** settings UI proof of concept (`ror-settings.exe`, opened with `ror settings`).
  - Tauri v2 in `apps/desktop/`; the page started as one plain HTML file and is now React (see above).
  - Edits the modifier, the log switch and the apps (key, processes, launch). Duplicate keys are flagged as you type, and each key shows whether rord registered it.
  - Saving goes through `ror-core::edit`, which uses `toml_edit` so comments and layout survive. The result is validated before it is written, then rord is asked to reload.
  - Apps are matched to `[[app]]` tables by position, so removing an app from the middle moves the comments of the apps after it.
  - Cross-compiling Tauri with `x86_64-pc-windows-gnu` from WSL works (open question 5).

### Next

1. **Test by hand on Windows** (`make install`, then use it):
   - Focus stealing: does Meh+key bring windows forward every time? (The log notes when the fallback was needed.)
   - Releasing Caps after a switch doesn't open Firefox's menu bar.
   - Each row of the "What a hotkey press does" table, including dialogs and minimized windows.
   - Tray apps (Discord, Spotify, Steam): running the launch command again shows the window and doesn't start a second copy.
   - Splash screens are skipped, and frameless apps (Discord, VS Code, Spotify) are still found.
   - Set `log = true` to see each press and how long it took.
2. Fix whatever that turns up, then commit v1.

## What it is

`ror` (run-or-raise) is a hotkey tool for Windows. Hold Caps Lock, press a letter, and you are in that app. It is like Win+1..9 for the taskbar, but you choose the keys and the apps.

### Background

- Replaces an earlier Linux/KDE script (`raise-or-launch`), which never worked properly. That OS is no longer in use. The old version is in the dotfiles git history (`scripts/raise-or-launch*`).
- Keyboard: QMK. Caps Lock **taps as Esc** and **holds as Meh** (Ctrl+Shift+Alt). ror's hotkeys are Meh + key.

## Decided

### Hotkeys

- A hotkey is a **combo**: hold the modifier, press the key. No key sequences or leader keys.
- The modifier defaults to `ctrl+shift+alt` and can be changed in the config.
- Keys are **mnemonic letters** (b = browser, t = terminal, …). Supported keys: `a–z`, `0–9`, `F1–F24`.
- Hotkeys are global and override the same shortcut in any app. Meh + an **unbound** key is not captured and still reaches the app.
- **Holding a key does not repeat:** one press is one action (`MOD_NOREPEAT`).
- **Key already taken** by another app: ror skips that key, registers the rest, shows **one popup** listing every key that failed, and reports them in `ror status`.

### What a hotkey press does

| State of the app | Action |
|---|---|
| Not running | Launch it |
| Running, but no visible window (e.g. closed to the tray) | Run its launch command again, which makes tray apps (Discord, Spotify, Steam) show their window |
| Has windows, none in front | Bring its most recently used window to the front, restoring it if minimized |
| In front, one window | Minimize it (same as Win+1) |
| In front, several windows | Cycle to its next window in a stable order, so repeated presses visit every window |

- **Matching:** windows are matched by **process name** (e.g. `firefox.exe`). There is no matching by window title or class, so a web app and its browser count as the same app.
- **Which windows count:** visible top-level windows that would show in the taskbar. Tool windows, cloaked (hidden) windows and splash screens are ignored.
- **Cycle order:** the order ror first saw each window, not front-to-back order (that changes with every switch). Minimized windows are included and restored when reached.
- **Dialogs:** if the app has a dialog open, the dialog is focused rather than the window behind it.
- **Virtual desktops:** not handled. Windows on other desktops are cloaked, so ror doesn't see them; if an app's only windows are on another desktop, ror treats it as having no visible window.
- **Double launch guard:** if ror launched the app less than ~3 seconds ago and no window has appeared yet, a second press does not launch it again.
- **Elevated (admin) windows:** not supported. ror runs as a normal user and needs no admin rights.

### Running

- Runs **all the time**, starting at login via the per-user Run registry key (`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`). No admin.
- **Two executables**, each its own crate in the Cargo workspace (see [Project layout](#project-layout)):
  - `rord.exe` (`apps/daemon`): the listener. Windows subsystem, so no window and no tray icon.
  - `ror.exe` (`apps/cli`): the command-line tool (console subsystem). A windowless exe can't print to a terminal, hence the split.
- `ror.exe` talks to `rord.exe` over a **named pipe**. The settings UI will use the same pipe later.
- Commands: `ror status`, `ror reload`, `ror quit`, `ror start`, `ror edit`, `ror settings`, `ror apps`, `ror version` (`ror uninstall` later).
- Errors (bad config, keys already taken, a launch failure) appear as a **plain Windows message box**, shown so it does not block hotkeys.
- Only one copy runs at a time. A second copy exits quietly; `ror start` prints "already running".
- No automatic restart after a crash in v1.

### Performance

The user cares a lot about performance.

- **No CPU use while idle.** It sleeps until a hotkey fires.
- Hotkeys are registered with `RegisterHotKey`. There is **no low-level keyboard hook**, so ror adds no delay to normal typing.
- Small memory footprint (Rust, no GC; target about 1–2 MB idle).
- Switching should feel instant; any delay should come from Windows redrawing, not from ror.

### Config

- Format: **TOML**, at `%APPDATA%\ror\config.toml`, on the Windows side only (not in dotfiles). `ror edit` opens it.
- Each entry has `key`, `process` (a name, or a list for apps that use several processes) and an optional `launch` (path plus arguments; defaults to `process`).
- **Reloads automatically** when saved. If the new file is invalid (e.g. half-saved), ror keeps running on the last good config and shows a popup.
- When the settings UI writes the file it uses `toml_edit`, so comments and layout from hand edits are kept.
- Example:

  ```toml
  modifier = "ctrl+shift+alt"

  [[app]]
  key = "b"
  process = "firefox.exe"

  [[app]]
  key = "g"
  process = "steamwebhelper.exe"
  launch = 'C:\Program Files (x86)\Steam\steam.exe'
  ```

- Starting key map:

  | Key | App | Process |
  |---|---|---|
  | b | Firefox | `firefox.exe` |
  | t | Windows Terminal | `WindowsTerminal.exe` |
  | c | VS Code | `Code.exe` |
  | d | Discord | `Discord.exe` |
  | s | Spotify | `Spotify.exe` |
  | o | Obsidian | `Obsidian.exe` |
  | g | Steam | `steamwebhelper.exe` (the window's process; launched via `steam.exe`) |
  | n | Notepad++ | `notepad++.exe` |

### Logging

- A small rotating log file, **off by default**. Turn it on with `log = true` in the config or `rord --debug`.

### Install and update

- Installed to `%LOCALAPPDATA%\Programs\ror\`, which is added to the user's PATH, so `ror` works in Windows terminals and as `ror.exe` from WSL.
- `make install` from WSL builds ror, stops the running copy, copies the new exes to the Windows side, registers it to run at login, and starts it.
- Re-running `make install` is how you update.
- Uninstall (later) removes the startup entry, the exes and the PATH entry. It keeps the config unless `--purge` is given.

### Settings UI (after v1)

- A settings app for changing hotkeys, apps and other options, so you can try things out.
- It is **separate from the always-running listener**. It only runs while open, edits the config, and the listener reloads. It adds nothing to the listener's footprint.
- Planned stack: Tauri v2 (Rust + WebView2, which ships with Windows 11).
- Ideas:
  - pick an app by clicking its window or choosing from running apps
  - show free and taken keys and conflicts
  - test a binding before saving
  - edit other settings
- Gets the listener's status (e.g. which keys failed to register and why) over the named pipe.

### Tech and repo

- **Rust**, using Microsoft's [`windows`](https://crates.io/crates/windows) crate for Win32, COM and WinRT.
  - Chosen over Go: no GC and a much smaller idle footprint (Go's runtime sits at several MB), the Tauri UI can share the Rust config code, and the `windows` crate is Microsoft's own, covering every Win32 API ror needs.
- Its own git repo, structured as a Cargo workspace laid out like a production Tauri v2 app (see [Project layout](#project-layout)).
- **Developed in WSL** and cross-compiled with the `x86_64-pc-windows-gnu` target (MinGW). Verified working on 2026-11-21.
- **Testing:** the "what does a press do" decision is a pure function with unit tests that run in WSL. Everything that touches Win32 is tested by hand on Windows.

## Project layout

A Cargo workspace. Each app (anything you run) has a folder under `apps/`, named for what it is. Shared libraries go under `crates/`. Folder names differ from package and exe names (e.g. `apps/cli` builds `ror.exe`).

The desktop app doesn't use Tauri's usual layout (`src-tauri/` plus a top-level `src/`), because it is built with plain `cargo`, not the Tauri CLI.

```
ror/
  Cargo.toml        workspace; shared versions in [workspace.dependencies]
  README.md, CLAUDE.md
  docs/             requirements.md (this file)
  Makefile          make test (WSL) / build / release / lint / install / dev
  scripts/          install.sh (make install), dev.sh (make dev)
  apps/
    daemon/         rord.exe: hotkeys, window list, focus, launch, popups, config watch
    cli/            ror.exe: the CLI
    desktop/        ror-settings.exe: Tauri v2 settings window
      web/          React + TypeScript frontend (Vite); built to web/dist and embedded
    # later: tui/
  crates/
    ror-core/       library: config, press decision, pipe protocol, config editing,
                    launch suggestions. Pure; tested in WSL
    ror-win/        Win32 helpers: which windows count, which program owns them,
                    running apps. Windows-only
```

- One crate per exe, so `rord` never links Tauri or WebView2 and stays small.
- `ror-core` is the shared boundary: config and the pipe protocol are written once and used by the listener, the CLI and the UI.
- Win32 code in `ror-core` sits behind `#[cfg(windows)]`, with `windows` as a Windows-only dependency, so `cargo test -p ror-core` runs on Linux.
- When the UI ships, its installer can bundle `rord.exe` as a Tauri sidecar (`bundle.externalBin`).

## Open questions

Deferred; none of these block v1.

1. **Pausing hotkeys** (e.g. for games): planned as a QMK game layer where Caps is plain Esc, so nothing changes in ror. Revisit if that isn't enough.
2. **Monitors:** should a launched app open on the monitor you're using? Not in v1.
3. **More keys:** punctuation or other named keys. Not in v1.
4. **Store/UWP apps** (e.g. Settings, Calculator) run inside `ApplicationFrameHost.exe` and can't be matched by process name. Left out for now.
5. ~~**Building the Tauri UI** for Windows from Linux is experimental.~~ Works with the gnu target (2026-11-22).

## Risks to check early

- **Focus-stealing rules.** Windows limits which programs may bring a window to the front. The hotkey should count as user input, with a fallback if it doesn't. **Prototype this first.**
- **Releasing Caps (Ctrl+Shift+Alt) after a switch** should not open menu bars (e.g. Firefox's Alt menu). Expected to be fine because a letter was pressed in between; confirm when testing.
- **Tray apps:** confirm that running the launch command again shows the existing window for each app rather than opening a second copy.

## Out of scope

- Linux and macOS
- Key sequences or leader keys
- Matching by window title or class
- Controlling elevated (admin) windows
- Virtual desktops (no switching, no per-desktop behaviour)
