//! Messages between `ror` / the settings UI and `rord` over a named pipe.
//!
//! One request per connection: the client writes a JSON request and a newline,
//! `rord` answers with a JSON response and a newline.

use std::fmt;
use std::io::{BufRead, BufReader, Read, Write};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize, de::DeserializeOwned};

/// Name of the pipe `rord` listens on.
pub const PIPE_NAME: &str = r"\\.\pipe\ror";

/// Largest message either side accepts.
pub const MAX_MESSAGE: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "lowercase")]
pub enum Request {
    Status,
    Reload,
    Quit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "lowercase")]
pub enum Response {
    Status(Status),
    Ok { message: String },
    Error { message: String },
}

/// What `ror status` shows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    pub version: String,
    pub pid: u32,
    pub config_path: String,
    /// Set if the config file on disk is invalid; ror runs on the last good one.
    pub config_error: Option<String>,
    pub modifier: String,
    pub hotkeys: Vec<HotkeyStatus>,
    /// Where the log goes, if logging is on.
    pub log_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HotkeyStatus {
    pub key: String,
    pub processes: Vec<String>,
    /// Why `RegisterHotKey` failed, or `None` if the key is active.
    pub error: Option<String>,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "rord {} running (pid {})", self.version, self.pid)?;
        writeln!(f, "config:   {}", self.config_path)?;
        if let Some(e) = &self.config_error {
            writeln!(f, "          INVALID, using the last good config:")?;
            for line in e.lines() {
                writeln!(f, "            {line}")?;
            }
        }
        writeln!(f, "modifier: {}", self.modifier)?;
        writeln!(f, "log:      {}", self.log_path.as_deref().unwrap_or("off"))?;
        if self.hotkeys.is_empty() {
            writeln!(f, "keys:     none")?;
        } else {
            writeln!(f, "keys:")?;
        }
        for h in &self.hotkeys {
            let procs = h.processes.join(", ");
            match &h.error {
                None => writeln!(f, "  {:<4}{procs}", h.key)?,
                Some(e) => writeln!(f, "  {:<4}{procs}  [NOT REGISTERED: {e}]", h.key)?,
            }
        }
        Ok(())
    }
}

/// Serialize a message as one line of JSON.
pub fn encode<T: Serialize>(msg: &T) -> Vec<u8> {
    let mut v = serde_json::to_vec(msg).expect("ipc messages always serialize");
    v.push(b'\n');
    v
}

/// Parse one line of JSON.
pub fn decode<T: DeserializeOwned>(line: &[u8]) -> Result<T, String> {
    serde_json::from_slice(line.trim_ascii()).map_err(|e| format!("bad message: {e}"))
}

#[derive(Debug)]
pub enum ClientError {
    /// No `rord` is listening.
    NotRunning,
    Io(std::io::Error),
    Protocol(String),
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClientError::NotRunning => f.write_str("rord is not running"),
            ClientError::Io(e) => write!(f, "talking to rord: {e}"),
            ClientError::Protocol(e) => write!(f, "talking to rord: {e}"),
        }
    }
}

impl std::error::Error for ClientError {}

/// Send one request to `rord` and wait for its answer.
pub fn send(req: &Request) -> Result<Response, ClientError> {
    const ERROR_PIPE_BUSY: i32 = 231;
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut pipe = loop {
        match std::fs::OpenOptions::new().read(true).write(true).open(PIPE_NAME) {
            Ok(p) => break p,
            // The single pipe instance is busy with another client.
            Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY) && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(ClientError::NotRunning);
            }
            Err(e) => return Err(ClientError::Io(e)),
        }
    };
    pipe.write_all(&encode(req)).map_err(ClientError::Io)?;
    let mut line = Vec::new();
    BufReader::new(pipe.take(MAX_MESSAGE as u64))
        .read_until(b'\n', &mut line)
        .map_err(ClientError::Io)?;
    if line.is_empty() {
        return Err(ClientError::Protocol("no answer".into()));
    }
    decode(&line).map_err(ClientError::Protocol)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipe_name_is_in_pipe_namespace() {
        assert!(PIPE_NAME.starts_with(r"\\.\pipe\"));
    }

    #[test]
    fn messages_round_trip_as_single_lines() {
        let status = Response::Status(Status {
            version: "0.1.0".into(),
            pid: 42,
            config_path: r"C:\x\config.toml".into(),
            config_error: Some("line 1\nline 2".into()),
            modifier: "ctrl+shift+alt".into(),
            hotkeys: vec![HotkeyStatus {
                key: "b".into(),
                processes: vec!["firefox.exe".into()],
                error: None,
            }],
            log_path: None,
        });
        for msg in [Response::Ok { message: "ok".into() }, status] {
            let bytes = encode(&msg);
            assert_eq!(bytes.iter().filter(|&&b| b == b'\n').count(), 1);
            assert_eq!(decode::<Response>(&bytes).unwrap(), msg);
        }
        for req in [Request::Status, Request::Reload, Request::Quit] {
            assert_eq!(decode::<Request>(&encode(&req)).unwrap(), req);
        }
        assert_eq!(encode(&Request::Quit), b"{\"cmd\":\"quit\"}\n");
    }

    #[test]
    fn garbage_is_an_error() {
        assert!(decode::<Request>(b"{\"cmd\":\"explode\"}").is_err());
        assert!(decode::<Request>(b"not json").is_err());
    }

    #[test]
    fn status_shows_failed_keys() {
        let s = Status {
            hotkeys: vec![HotkeyStatus {
                key: "g".into(),
                processes: vec!["steamwebhelper.exe".into()],
                error: Some("already taken by another app".into()),
            }],
            ..Default::default()
        };
        assert!(s.to_string().contains("g   steamwebhelper.exe  [NOT REGISTERED: already taken"));
    }
}
