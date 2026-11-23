#!/usr/bin/env bash
# Run the desktop app with hot reload. Run from WSL via `make dev`.
#
# Starts Vite's dev server in WSL and a debug ror-settings.exe on Windows
# that loads the page from it (devUrl in tauri.conf.json). Edits under
# apps/desktop/web/src show up in the window immediately. Rust changes need
# a restart: close the window and run `make dev` again.
#
# Closing the window or pressing Ctrl+C stops both.
set -euo pipefail

TARGET=x86_64-pc-windows-gnu
WEB=apps/desktop/web
URL=http://127.0.0.1:5173
EXE="$PWD/target/$TARGET/debug/ror-settings.exe"

# Without the custom-protocol feature, the app loads devUrl.
cargo build -p ror-settings --target "$TARGET"

# Run Vite directly, not via npm, so stopping it actually stops it.
(cd "$WEB" && exec node_modules/.bin/vite --clearScreen false) &
VITE=$!

cleanup() {
    kill "$VITE" 2>/dev/null || true
    # Ctrl+C reaches the WSL side but not always the Windows window; close
    # only the dev build's window (the installed app runs from a C: path).
    powershell.exe -NoProfile -NonInteractive -Command \
        "Get-Process ror-settings -ErrorAction SilentlyContinue | Where-Object { \$_.Path -like '\\\\wsl*' } | Stop-Process" \
        >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

for _ in $(seq 100); do
    curl -fs "$URL" >/dev/null && break
    kill -0 "$VITE" 2>/dev/null || { echo "vite failed to start" >&2; exit 1; }
    sleep 0.1
done

echo "ror settings (dev): edit $WEB/src and the window updates. Close it or Ctrl+C to stop."
# From a C: folder so Windows doesn't warn about the WSL working directory.
(cd /mnt/c && "$EXE")
