# ror

**Run or raise for Windows.** Hold a modifier, press a letter, and you're in that app: it opens if it isn't running, comes to the front if it is, and cycles through its windows if it has several. Like Win+1…9 for the taskbar, but you choose the keys and the apps.

```
Caps+B  →  Firefox        Caps+T  →  Terminal        Caps+C  →  VS Code
```

## Features

- **One key per app.** The same key launches the app, brings it forward, minimizes it, or cycles its windows, depending on the app's state.
- **Instant and idle.** Uses `RegisterHotKey`, with no keyboard hook, so it adds no lag to normal typing. It uses no CPU while idle and about 1–2 MB of memory.
- **Settings window.** Add an app by picking it from the ones you have open, with icons and real names. Change keys and save; changes apply immediately.
- **Plain config file.** TOML that you can also edit by hand. Comments survive edits made in the settings window, and saved changes reload automatically.
- **Tray apps work.** Discord, Spotify and Steam get their window shown again instead of a second copy started.
- **No admin rights.** It runs as your user and starts at login.

## What a key press does

| The app is… | Pressing its key… |
|---|---|
| not running | launches it |
| running, but has no visible window (e.g. closed to the tray) | runs its launch command again, which shows the window |
| open, but not in front | brings its most recently used window to the front (restoring it if minimized) |
| in front, with one window | minimizes it |
| in front, with several windows | moves to its next window; repeated presses visit them all |

Windows are matched by **process name** (e.g. `firefox.exe`). If the app has a dialog open, the dialog gets focus.

## Install

ror is built from source in WSL and cross-compiled for Windows. You need:

- Windows 11 (or Windows 10 with the WebView2 runtime) and WSL
- In WSL: Rust with the `x86_64-pc-windows-gnu` target, MinGW (`gcc-mingw-w64-x86-64`), and Node.js 18+

```sh
rustup target add x86_64-pc-windows-gnu
sudo apt install gcc-mingw-w64-x86-64
make install
```

`make install` builds everything and copies it to `%LOCALAPPDATA%\Programs\ror`. It also sets ror to start at login, adds it to your PATH, and starts it. Run it again to update. Open a new terminal so `ror` is on the PATH.

## Use

The default modifier is **Ctrl+Shift+Alt** ("Meh"). ror was built for a QMK keyboard where holding Caps Lock sends exactly that, but any modifier combination works.

The first run creates a starter config with: `b` Firefox, `t` Windows Terminal, `c` VS Code, `d` Discord, `s` Spotify, `o` Obsidian, `g` Steam, `n` Notepad++.

### Settings window

```
ror settings
```

- **Add app:** open the app first, then pick it from the list. The process name, launch command and a free key are filled in for you.
- Click an app to swap it for another. The **⋯** menu has Edit details, Move up/down and Remove.
- **Save** (or Ctrl+S) checks the config, writes it, and the hotkeys update immediately.

### Commands

| Command | What it does |
|---|---|
| `ror status` | Whether ror is running, the config path, and each key's state |
| `ror settings` | Open the settings window |
| `ror edit` | Open `config.toml` in your editor |
| `ror apps` | List open apps as ready-to-paste config entries |
| `ror reload` | Reload the config now (normally automatic) |
| `ror start` / `ror quit` | Start or stop the background listener |
| `ror version` | Show the version |

### Config

`%APPDATA%\ror\config.toml`:

```toml
modifier = "ctrl+shift+alt"   # any of ctrl, shift, alt, win
log = false                   # debug log at %LOCALAPPDATA%\ror\rord.log

[[app]]
key = "b"                     # a-z, 0-9 or f1-f24
process = "firefox.exe"       # the program that owns the app's windows

[[app]]
key = "g"
process = "steamwebhelper.exe"                       # Steam's window belongs to its helper…
launch = 'C:\Program Files (x86)\Steam\steam.exe'    # …but Steam starts from steam.exe

[[app]]
key = "d"
process = "Discord.exe"
launch = '%LOCALAPPDATA%\Discord\Update.exe --processStart Discord.exe'
```

- `process` can be a list (`["a.exe", "b.exe"]`) for apps that use several programs.
- `launch` is optional and defaults to the process name. `%VARIABLES%` are expanded.
- To find an app's process name, open Task Manager → Details while the app is running, or run `ror apps`.

If you save a broken config, ror keeps using the last good one and shows a popup explaining the problem.

## How it works

Three programs, installed together:

- **`rord.exe`**: the background listener. It registers the hotkeys, decides what each press does, and switches windows. It has no window or tray icon, and sleeps until a hotkey fires.
- **`ror.exe`**: the command-line tool. It talks to `rord` over a named pipe.
- **`ror-settings.exe`**: the settings window (Tauri + React). It runs only while open, edits the config, and `rord` reloads it.

## Limitations

- Windows only. Elevated (admin) windows can't be controlled.
- Store apps that run inside `ApplicationFrameHost.exe` (Settings, Calculator) can't be told apart by process name.
- Windows on other virtual desktops aren't seen.

## Development

```
apps/
  daemon/     rord.exe: hotkeys, window switching, launch, config watch, pipe server
  cli/        ror.exe
  desktop/    ror-settings.exe (Tauri v2); React + TypeScript frontend in web/
crates/
  ror-core/   shared, pure logic: config, press decision, pipe protocol, config editing
  ror-win/    shared Win32 helpers: window filtering, running apps, app names and icons
docs/
  requirements.md   design decisions, open questions and progress log
```

| Command | What it does |
|---|---|
| `make test` | Unit tests for `ror-core`, run natively in WSL |
| `make lint` | Clippy with warnings as errors |
| `make build` / `make release` | Build the Windows exes into `target/x86_64-pc-windows-gnu/` |
| `make dev` | Settings window with hot reload: Vite in WSL, the app on Windows |
| `make install` | Build, install and restart ror on Windows |

The decision logic in `ror-core` is unit-tested. Anything that touches Win32 is tested by hand on Windows. See [docs/requirements.md](docs/requirements.md) for the full design.
