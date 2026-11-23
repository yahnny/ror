fn main() {
    // Tauri's codegen (icons, config, capabilities) is only needed for the real Windows build.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        tauri_build::build();
    }
}
