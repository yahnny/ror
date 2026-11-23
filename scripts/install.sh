#!/usr/bin/env bash
# Build ror and install it on the Windows side. Run from WSL via `make install`.
# Re-running it updates an existing install.
#
#   1. build the release exes
#   2. stop the running rord
#   3. copy rord.exe, ror.exe, ror-settings.exe and WebView2Loader.dll to %LOCALAPPDATA%\Programs\ror
#   4. start rord at login (HKCU Run key) and add the folder to the user PATH
#   5. start rord
set -euo pipefail

TARGET=x86_64-pc-windows-gnu
BIN="target/$TARGET/release"

# Windows env var, without cmd.exe's trailing CR (run from C: to avoid the UNC warning).
winenv() { (cd /mnt/c && cmd.exe /c "echo %$1%" 2>/dev/null) | tr -d '\r'; }

cargo build --release --target "$TARGET" -p rord -p ror-cli -p ror-settings \
    --features ror-settings/custom-protocol

LOCALAPPDATA_WIN=$(winenv LOCALAPPDATA)
[[ -n "$LOCALAPPDATA_WIN" && "$LOCALAPPDATA_WIN" != *%* ]] || { echo "can't read %LOCALAPPDATA%" >&2; exit 1; }
DEST_WIN="$LOCALAPPDATA_WIN\\Programs\\ror"
DEST=$(wslpath -u "$DEST_WIN")
mkdir -p "$DEST"

# The settings window may have unsaved edits, so don't close it for the user.
if tasklist.exe /FI "IMAGENAME eq ror-settings.exe" /NH 2>/dev/null | grep -qi ror-settings.exe; then
    echo "close the ror settings window first" >&2
    exit 1
fi

# Stop the running copy so its exe can be replaced: ask nicely, then force.
if [[ -x "$DEST/ror.exe" ]]; then
    "$DEST/ror.exe" quit >/dev/null 2>&1 || true
fi
taskkill.exe /IM rord.exe /F >/dev/null 2>&1 || true
for _ in $(seq 50); do
    tasklist.exe /FI "IMAGENAME eq rord.exe" /NH 2>/dev/null | grep -qi rord.exe || break
    sleep 0.1
done

# ror-settings.exe loads WebView2Loader.dll from its own folder (the gnu
# toolchain can't link it statically); without it the window never opens.
cp "$BIN/rord.exe" "$BIN/ror.exe" "$BIN/ror-settings.exe" "$BIN/WebView2Loader.dll" "$DEST/"
echo "installed to $DEST_WIN"

reg.exe add 'HKCU\Software\Microsoft\Windows\CurrentVersion\Run' \
    /v ror /t REG_SZ /d "\"$DEST_WIN\\rord.exe\"" /f >/dev/null
echo "starts at login"

# Add to the user PATH (registry value, not this shell's) if it isn't there yet.
powershell.exe -NoProfile -NonInteractive -Command "
    \$dir = '$DEST_WIN'
    \$path = [Environment]::GetEnvironmentVariable('Path', 'User')
    if (-not \$path) { \$path = '' }
    if ((\$path -split ';') -notcontains \$dir) {
        [Environment]::SetEnvironmentVariable('Path', ((\$path.TrimEnd(';') + ';' + \$dir).TrimStart(';')), 'User')
        'added to PATH (open a new terminal to use it)'
    }
" | tr -d '\r'

"$DEST/ror.exe" start
