//! The named pipe server that `ror` (and later the settings UI) talks to.
//!
//! One pipe instance on its own thread, one request per connection; the
//! thread sleeps in `ConnectNamedPipe` while idle. Remote clients are refused.

use ror_core::ipc::{self, MAX_MESSAGE, PIPE_NAME, Request, Response};
use windows::Win32::Foundation::{ERROR_PIPE_CONNECTED, HANDLE, INVALID_HANDLE_VALUE};
use windows::Win32::Storage::FileSystem::{
    FILE_FLAG_FIRST_PIPE_INSTANCE, FlushFileBuffers, PIPE_ACCESS_DUPLEX, ReadFile, WriteFile,
};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE,
    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT,
};
use windows::core::HSTRING;

use crate::log::log;

/// Serve requests forever. `handle` answers each one; `after` runs once the
/// answer has been sent (so `quit` can reply before the process exits).
pub fn spawn(
    handle: impl Fn(&Request) -> Response + Send + 'static,
    after: impl Fn(&Request) + Send + 'static,
) {
    std::thread::spawn(move || {
        // SAFETY: plain pipe creation; the handle lives as long as the process.
        let pipe = unsafe {
            CreateNamedPipeW(
                &HSTRING::from(PIPE_NAME),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                MAX_MESSAGE as u32,
                MAX_MESSAGE as u32,
                0,
                None,
            )
        };
        if pipe == INVALID_HANDLE_VALUE {
            log!("pipe: can't create {PIPE_NAME}: {}", windows::core::Error::from_thread());
            return;
        }
        loop {
            // SAFETY: `pipe` is a valid server handle.
            let connected = unsafe { ConnectNamedPipe(pipe, None) };
            if let Err(e) = connected
                && e.code() != ERROR_PIPE_CONNECTED.to_hresult()
            {
                log!("pipe: connect failed: {e}");
                continue;
            }
            let request = read_line(pipe).and_then(|line| ipc::decode::<Request>(&line));
            let response = match &request {
                Ok(req) => handle(req),
                Err(e) => Response::Error { message: e.clone() },
            };
            write_all(pipe, &ipc::encode(&response));
            // SAFETY: `pipe` is valid; flush waits until the client has read the answer.
            unsafe {
                let _ = FlushFileBuffers(pipe);
                let _ = DisconnectNamedPipe(pipe);
            }
            if let Ok(req) = &request {
                after(req);
            }
        }
    });
}

fn read_line(pipe: HANDLE) -> Result<Vec<u8>, String> {
    let mut line = Vec::new();
    let mut chunk = [0u8; 1024];
    while !line.contains(&b'\n') {
        let mut n = 0u32;
        // SAFETY: `chunk` and `n` outlive the synchronous call.
        unsafe { ReadFile(pipe, Some(&mut chunk), Some(&mut n), None) }
            .map_err(|e| format!("read failed: {e}"))?;
        if n == 0 {
            break;
        }
        line.extend_from_slice(&chunk[..n as usize]);
        if line.len() > MAX_MESSAGE {
            return Err("request too large".into());
        }
    }
    Ok(line)
}

fn write_all(pipe: HANDLE, mut bytes: &[u8]) {
    while !bytes.is_empty() {
        let mut n = 0u32;
        // SAFETY: `bytes` and `n` outlive the synchronous call.
        if unsafe { WriteFile(pipe, Some(bytes), Some(&mut n), None) }.is_err() || n == 0 {
            return;
        }
        bytes = &bytes[n as usize..];
    }
}
