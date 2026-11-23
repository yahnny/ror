# CLAUDE.md

Guidance for working on this repo. For what ror does and how to use it, see [README.md](README.md). For design decisions and the progress log, see [docs/requirements.md](docs/requirements.md).

## Environment

- Developed in **WSL**; everything runs on **Windows**. Exes are cross-compiled with the `x86_64-pc-windows-gnu` target (MinGW).
- Windows programs can be run from WSL directly (`/mnt/c/.../ror.exe status`). Run them from `cd /mnt/c` to avoid the UNC working-directory warning, and strip `\r` from their output.
- The installed copy lives at `%LOCALAPPDATA%\Programs\ror` (`/mnt/c/Users/<user>/AppData/Local/Programs/ror`). The config is at `%APPDATA%\ror\config.toml`.
- WSL has Node 18, so the frontend uses Vite 5 (newer Vite needs Node 20+).

## Commands

| | |
|---|---|
| `make test` | `ror-core` unit tests, native in WSL. The only automated tests. |
| `make lint` | Clippy with `-D warnings`, on the Windows target and the core. Keep it clean. |
| `make release` | Release exes into `target/x86_64-pc-windows-gnu/release/` |
| `make install` | Build, stop rord, copy to Windows, register at login, start. Refuses while the settings window is open. |
| `make dev` | Vite in WSL plus a debug `ror-settings.exe` that loads it, for hot reload. |

## Layout

- `apps/daemon` (`rord.exe`), `apps/cli` (`ror.exe`), `apps/desktop` (`ror-settings.exe`, Tauri v2; React frontend in `web/`).
- `crates/ror-core`: pure logic, no Win32, so it's tested on Linux. Put decisions here (press decision, config parsing and editing, launch suggestions) so they can be tested.
- `crates/ror-win`: Win32 helpers shared by the apps (`#![cfg(windows)]`).
- The app crates are Windows-only; their `main` on other platforms just prints a message. Windows dependencies go under `[target.'cfg(windows)'.dependencies]`.

## Things that have bitten us

- **Test the installed copy, not just the build folder.** `ror-settings.exe` needs `WebView2Loader.dll` beside it. The build folder has it; the install must copy it (it does now). A run from `target/` passing proves nothing about the install.
- **The release CSP only applies to non-dev builds.** `make dev` turns the CSP off (`devCsp: null`), so content it blocks (e.g. `data:` icons, which needed `img-src data:`) only breaks in the installed app.
- **`custom-protocol` feature:** without it the desktop app loads the Vite dev server (`devUrl`). `make dev` builds without it; every other target, and `install.sh`, must pass `--features ror-settings/custom-protocol`.
- **Hot reload covers only the frontend.** After Rust changes (`apps/desktop/src`, `crates/*`), `make dev` must be restarted, or new commands won't exist.
- **Windows API features must be declared by each crate that uses them.** Workspace builds unify features, which once hid a missing `Win32_Security` in `ror-cli`. Check that a crate builds on its own (`cargo build -p <crate> --target x86_64-pc-windows-gnu`).
- **`windows` crate 0.62 signatures:** look them up in `~/.cargo/registry/src/*/windows-0.62.2/src/Windows/Win32/` rather than guessing; they change between versions.
- **Starting Windows programs from WSL:** `timeout` stops the WSL side but leaves the Windows window open. Close stray dev or test windows with `powershell.exe "Get-Process ror-settings | Where-Object { $_.Path -like '\\wsl*' } | Stop-Process"`. That filter never touches the user's installed copy.
- **`pkill -f` / `pgrep -f`** with a pattern that appears in your own command line kills your own shell. Match on something only the target has, or use PIDs.

## Working with the user

- Opening windows, popups or test runs on the user's Windows desktop is visible to them. Say so before doing it, and clean up afterwards.
- After launching something for the user (e.g. `make dev`), hand it over; don't keep polling.
- Record milestones in the Progress section of `docs/requirements.md`.
- Commit only when asked.
- No attribution or tool metadata (`Co-Authored-By:`, "Generated with Claude Code") in commits, PRs or docs.
