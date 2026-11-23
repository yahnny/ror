# Develop in WSL, run on Windows.
TARGET := x86_64-pc-windows-gnu
WEB := apps/desktop/web

.PHONY: test build release lint install dev web clean

# Release-style builds serve the embedded frontend; `make dev` loads Vite instead.
FEATURES := --features ror-settings/custom-protocol

# Unit tests for the pure logic, run natively in WSL.
test:
	cargo test -p ror-core

# Windows exes: target/$(TARGET)/debug/{rord,ror,ror-settings}.exe
build: web
	cargo build --workspace --target $(TARGET) $(FEATURES)

# Optimised exes: target/$(TARGET)/release/{rord,ror,ror-settings}.exe
release: web
	cargo build --workspace --release --target $(TARGET) $(FEATURES)

# Clippy on the Windows code and the pure core.
lint: web
	cargo clippy --workspace --all-targets --target $(TARGET) $(FEATURES) -- -D warnings
	cargo clippy -p ror-core --all-targets -- -D warnings

# Build, stop the running rord, copy to %LOCALAPPDATA%\Programs\ror,
# register it to start at login, add it to PATH, and start it.
install: web
	scripts/install.sh

# The desktop app with hot reload: Vite in WSL, debug ror-settings.exe on Windows.
dev: $(WEB)/node_modules
	scripts/dev.sh

# The desktop app's React frontend -> $(WEB)/dist, embedded into ror-settings.exe.
web: $(WEB)/node_modules
	npm --prefix $(WEB) run build

# Reinstall npm packages only when the lockfile changes.
$(WEB)/node_modules: $(WEB)/package-lock.json
	npm --prefix $(WEB) ci --no-fund --no-audit
	touch $@

clean:
	cargo clean
	rm -rf $(WEB)/dist
