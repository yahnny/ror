//! Reload the config when the file is saved.
//!
//! A thread blocks in `ReadDirectoryChangesW` on the config folder (no CPU
//! while idle). When `config.toml` changes it waits briefly, so an editor's
//! several writes count as one save, then asks the main thread to reload.

use std::path::Path;
use std::time::Duration;

use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_LIST_DIRECTORY, FILE_NOTIFY_CHANGE_FILE_NAME,
    FILE_NOTIFY_CHANGE_LAST_WRITE, FILE_NOTIFY_CHANGE_SIZE, FILE_NOTIFY_INFORMATION,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, ReadDirectoryChangesW,
};
use windows::core::HSTRING;

use crate::log::log;

/// Let an editor finish saving before reading the file.
const SETTLE: Duration = Duration::from_millis(150);

/// Watch `config`'s folder; call `on_change` after each save.
pub fn spawn(config: &Path, on_change: impl Fn() + Send + 'static) {
    let Some(dir) = config.parent().map(Path::to_path_buf) else { return };
    let name = config.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    std::thread::spawn(move || {
        if let Err(e) = watch(&dir, &name, on_change) {
            log!("watch: stopped watching {}: {e}", dir.display());
        }
    });
}

fn watch(dir: &Path, name: &str, on_change: impl Fn()) -> windows::core::Result<()> {
    // SAFETY: the handle is closed on return; the buffer outlives each call.
    unsafe {
        let handle = CreateFileW(
            &HSTRING::from(dir.as_os_str()),
            FILE_LIST_DIRECTORY.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            None,
        )?;
        // u32 elements keep the buffer DWORD-aligned, as the API requires.
        let mut buf = vec![0u32; 4096];
        let result = loop {
            let mut bytes = 0u32;
            if let Err(e) = ReadDirectoryChangesW(
                handle,
                buf.as_mut_ptr().cast(),
                (buf.len() * 4) as u32,
                false,
                FILE_NOTIFY_CHANGE_LAST_WRITE | FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_SIZE,
                Some(&mut bytes),
                None,
                None,
            ) {
                break Err(e);
            }
            // bytes == 0: the buffer overflowed and the details were lost; reload to be safe.
            if bytes == 0 || mentions(&buf, name) {
                std::thread::sleep(SETTLE);
                on_change();
            }
        };
        let _ = CloseHandle(handle);
        result
    }
}

/// Does a `FILE_NOTIFY_INFORMATION` list name the file?
fn mentions(buf: &[u32], name: &str) -> bool {
    let base = buf.as_ptr().cast::<u8>();
    let mut offset = 0usize;
    loop {
        // SAFETY: the OS wrote a chain of records inside `buf`, each DWORD-aligned.
        let info = unsafe { &*base.add(offset).cast::<FILE_NOTIFY_INFORMATION>() };
        let len = info.FileNameLength as usize / 2;
        // SAFETY: the name runs `FileNameLength` bytes past `FileName`.
        let file = unsafe { std::slice::from_raw_parts(info.FileName.as_ptr(), len) };
        if String::from_utf16_lossy(file).eq_ignore_ascii_case(name) {
            return true;
        }
        if info.NextEntryOffset == 0 {
            return false;
        }
        offset += info.NextEntryOffset as usize;
        if offset >= buf.len() * 4 {
            return false;
        }
    }
}
