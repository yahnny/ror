//! Loading and validating `%APPDATA%\ror\config.toml`.

use std::fmt;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Modifier used when the config doesn't set one (QMK "Meh").
pub const DEFAULT_MODIFIER: &str = "ctrl+shift+alt";

/// Written to `config.toml` the first time ror runs (or `ror edit` is used)
/// and no config exists yet.
pub const DEFAULT_CONFIG: &str = r#"# ror config. Saved changes are picked up automatically.
#
# modifier: the keys you hold. Any of ctrl, shift, alt, win joined with "+".
# Each [[app]] has:
#   key     a-z, 0-9 or f1-f24
#   process the window's process name, or a list of names
#   launch  optional: path plus arguments (defaults to the process name).
#           %VARIABLES% are expanded. Quote the path if it has spaces and
#           doesn't end in .exe.

modifier = "ctrl+shift+alt"

# Write a debug log to %LOCALAPPDATA%\ror\rord.log.
log = false

[[app]]
key = "b"
process = "firefox.exe"

[[app]]
key = "t"
process = "WindowsTerminal.exe"
launch = "wt.exe"

[[app]]
key = "c"
process = "Code.exe"
launch = '%LOCALAPPDATA%\Programs\Microsoft VS Code\Code.exe'

[[app]]
key = "d"
process = "Discord.exe"
launch = '%LOCALAPPDATA%\Discord\Update.exe --processStart Discord.exe'

[[app]]
key = "s"
process = "Spotify.exe"
launch = '%APPDATA%\Spotify\Spotify.exe'

[[app]]
key = "o"
process = "Obsidian.exe"
launch = '%LOCALAPPDATA%\Programs\Obsidian\Obsidian.exe'

[[app]]
key = "g"
process = "steamwebhelper.exe"
launch = 'C:\Program Files (x86)\Steam\steam.exe'

[[app]]
key = "n"
process = "notepad++.exe"
launch = 'C:\Program Files\Notepad++\notepad++.exe'
"#;

/// `%APPDATA%\ror\config.toml`, or `None` if `APPDATA` isn't set.
pub fn default_path() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(appdata).join("ror").join("config.toml"))
}

/// `%LOCALAPPDATA%\ror\rord.log`, or `None` if `LOCALAPPDATA` isn't set.
pub fn log_path() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(local).join("ror").join("rord.log"))
}

/// Write [`DEFAULT_CONFIG`] to `path` if nothing is there yet.
/// Returns `true` if it created the file.
pub fn ensure_exists(path: &Path) -> std::io::Result<bool> {
    if path.exists() {
        return Ok(false);
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, DEFAULT_CONFIG)?;
    Ok(true)
}

/// The keys held down for every hotkey.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub win: bool,
}

impl Modifiers {
    /// Parse e.g. `"ctrl+shift+alt"`. Case and order don't matter.
    pub fn parse(s: &str) -> Result<Self, String> {
        let mut m = Modifiers::default();
        for part in s.split('+') {
            let part = part.trim().to_ascii_lowercase();
            let flag = match part.as_str() {
                "ctrl" | "control" => &mut m.ctrl,
                "shift" => &mut m.shift,
                "alt" => &mut m.alt,
                "win" | "windows" | "super" => &mut m.win,
                "" => return Err(format!("modifier {s:?} has an empty part")),
                other => {
                    return Err(format!(
                        "modifier {s:?}: unknown key {other:?} (use ctrl, shift, alt, win)"
                    ));
                }
            };
            if *flag {
                return Err(format!("modifier {s:?} lists {part:?} twice"));
            }
            *flag = true;
        }
        Ok(m)
    }

    /// `MOD_*` flags for `RegisterHotKey` (without `MOD_NOREPEAT`).
    pub fn win32_flags(self) -> u32 {
        const MOD_ALT: u32 = 0x1;
        const MOD_CONTROL: u32 = 0x2;
        const MOD_SHIFT: u32 = 0x4;
        const MOD_WIN: u32 = 0x8;
        let mut f = 0;
        if self.alt {
            f |= MOD_ALT;
        }
        if self.ctrl {
            f |= MOD_CONTROL;
        }
        if self.shift {
            f |= MOD_SHIFT;
        }
        if self.win {
            f |= MOD_WIN;
        }
        f
    }
}

impl fmt::Display for Modifiers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parts: Vec<&str> = [
            (self.ctrl, "ctrl"),
            (self.shift, "shift"),
            (self.alt, "alt"),
            (self.win, "win"),
        ]
        .into_iter()
        .filter_map(|(on, name)| on.then_some(name))
        .collect();
        f.write_str(&parts.join("+"))
    }
}

/// A key that can be bound: `a–z`, `0–9` or `F1–F24`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    /// Lowercase ASCII letter or digit.
    Char(char),
    /// Function key 1..=24.
    F(u8),
}

impl Key {
    pub fn parse(s: &str) -> Result<Self, String> {
        let lower = s.trim().to_ascii_lowercase();
        let mut chars = lower.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) if c.is_ascii_lowercase() || c.is_ascii_digit() => {
                return Ok(Key::Char(c));
            }
            _ => {}
        }
        if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<u8>().ok())
            && (1..=24).contains(&n)
            && !lower[1..].starts_with('0')
        {
            return Ok(Key::F(n));
        }
        Err(format!("key {s:?} isn't supported (use a-z, 0-9 or f1-f24)"))
    }

    /// Win32 virtual-key code.
    pub fn vk(self) -> u32 {
        match self {
            Key::Char(c) if c.is_ascii_lowercase() => c.to_ascii_uppercase() as u32,
            Key::Char(c) => c as u32, // '0'..='9' are VK 0x30..0x39
            Key::F(n) => 0x70 + u32::from(n) - 1, // VK_F1 = 0x70
        }
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Key::Char(c) => write!(f, "{c}"),
            Key::F(n) => write!(f, "f{n}"),
        }
    }
}

/// A launch command split into program and arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub program: String,
    pub args: String,
}

impl Launch {
    /// Split `"path args"`. The path may be quoted. An unquoted path may
    /// contain spaces if it ends in `.exe` (e.g. `C:\Program Files\x\x.exe -a`).
    pub fn parse(s: &str) -> Self {
        let s = s.trim();
        if let Some(rest) = s.strip_prefix('"') {
            if let Some(end) = rest.find('"') {
                return Launch {
                    program: rest[..end].to_string(),
                    args: rest[end + 1..].trim().to_string(),
                };
            }
            return Launch { program: rest.to_string(), args: String::new() };
        }
        let lower = s.to_ascii_lowercase();
        let mut search = 0;
        while let Some(i) = lower[search..].find(".exe") {
            let end = search + i + 4;
            if end == s.len() || s[end..].starts_with(char::is_whitespace) {
                return Launch {
                    program: s[..end].to_string(),
                    args: s[end..].trim().to_string(),
                };
            }
            search = end;
        }
        match s.split_once(char::is_whitespace) {
            Some((p, a)) => Launch { program: p.to_string(), args: a.trim().to_string() },
            None => Launch { program: s.to_string(), args: String::new() },
        }
    }
}

/// One `[[app]]` entry, validated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct App {
    pub key: Key,
    /// Process names, matched case-insensitively against window owners.
    pub processes: Vec<String>,
    pub launch: Launch,
}

impl App {
    pub fn matches_process(&self, name: &str) -> bool {
        self.processes.iter().any(|p| p.eq_ignore_ascii_case(name))
    }
}

/// A validated config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub modifier: Modifiers,
    pub log: bool,
    pub apps: Vec<App>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            modifier: Modifiers::parse(DEFAULT_MODIFIER).expect("default modifier is valid"),
            log: false,
            apps: Vec::new(),
        }
    }
}

/// Everything wrong with a config file, one problem per line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError(pub Vec<String>);

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.join("\n"))
    }
}

impl std::error::Error for ConfigError {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    modifier: Option<String>,
    log: Option<bool>,
    #[serde(default)]
    app: Vec<RawApp>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawApp {
    key: String,
    process: OneOrMany,
    launch: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

impl Config {
    /// Read and validate a config file.
    pub fn load(path: &Path) -> Result<Config, ConfigError> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| ConfigError(vec![format!("can't read {}: {e}", path.display())]))?;
        Config::parse(&text)
    }

    /// Parse and validate config text. Reports every problem, not just the first.
    pub fn parse(text: &str) -> Result<Config, ConfigError> {
        let raw: RawConfig =
            toml::from_str(text).map_err(|e| ConfigError(vec![e.to_string().trim().to_string()]))?;
        let mut errors = Vec::new();

        let modifier = Modifiers::parse(raw.modifier.as_deref().unwrap_or(DEFAULT_MODIFIER))
            .unwrap_or_else(|e| {
                errors.push(e);
                Modifiers::default()
            });

        let mut apps: Vec<App> = Vec::new();
        for (i, raw_app) in raw.app.into_iter().enumerate() {
            let label = format!("app #{} (key {:?})", i + 1, raw_app.key);
            let key = match Key::parse(&raw_app.key) {
                Ok(k) => Some(k),
                Err(e) => {
                    errors.push(format!("app #{}: {e}", i + 1));
                    None
                }
            };

            let processes: Vec<String> = match raw_app.process {
                OneOrMany::One(p) => vec![p],
                OneOrMany::Many(ps) => ps,
            }
            .into_iter()
            .map(|p| p.trim().to_string())
            .collect();
            if processes.is_empty() {
                errors.push(format!("{label}: process list is empty"));
            }
            for p in &processes {
                if p.is_empty() {
                    errors.push(format!("{label}: process name is empty"));
                } else if p.contains(['\\', '/']) {
                    errors.push(format!(
                        "{label}: process {p:?} should be a name like \"firefox.exe\", not a path \
                         (put the path in `launch`)"
                    ));
                }
            }

            if raw_app.launch.as_deref().is_some_and(|l| l.trim().is_empty()) {
                errors.push(format!("{label}: launch is empty"));
            }
            let launch_text = raw_app
                .launch
                .or_else(|| processes.first().cloned())
                .unwrap_or_default();

            if let Some(key) = key {
                if let Some(j) = apps.iter().position(|a| a.key == key) {
                    errors.push(format!(
                        "key \"{key}\" is used by more than one app (#{} and #{})",
                        j + 1,
                        i + 1
                    ));
                }
                apps.push(App { key, processes, launch: Launch::parse(&launch_text) });
            }
        }

        if errors.is_empty() {
            Ok(Config { modifier, log: raw.log.unwrap_or(false), apps })
        } else {
            Err(ConfigError(errors))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid_and_has_starter_keys() {
        let c = Config::parse(DEFAULT_CONFIG).unwrap();
        assert_eq!(c.modifier.to_string(), DEFAULT_MODIFIER);
        let keys: String = c.apps.iter().map(|a| a.key.to_string()).collect();
        assert_eq!(keys, "btcdsogn");
        assert!(!c.log);
    }

    #[test]
    fn requirements_example_parses() {
        let c = Config::parse(
            r#"
            modifier = "ctrl+shift+alt"

            [[app]]
            key = "b"
            process = "firefox.exe"

            [[app]]
            key = "g"
            process = "steamwebhelper.exe"
            launch = 'C:\Program Files (x86)\Steam\steam.exe'
            "#,
        )
        .unwrap();
        assert_eq!(c.apps[0].launch.program, "firefox.exe");
        assert_eq!(c.apps[1].launch.program, r"C:\Program Files (x86)\Steam\steam.exe");
        assert_eq!(c.apps[1].launch.args, "");
    }

    #[test]
    fn modifier_defaults_and_flags() {
        let c = Config::parse("").unwrap();
        assert_eq!(c.modifier.win32_flags(), 0x1 | 0x2 | 0x4);
        let m = Modifiers::parse("Win + Alt").unwrap();
        assert_eq!(m.win32_flags(), 0x8 | 0x1);
        assert_eq!(m.to_string(), "alt+win");
    }

    #[test]
    fn bad_modifiers_are_rejected() {
        assert!(Modifiers::parse("ctrl+meta").is_err());
        assert!(Modifiers::parse("ctrl+ctrl").is_err());
        assert!(Modifiers::parse("").is_err());
        assert!(Modifiers::parse("ctrl+").is_err());
    }

    #[test]
    fn keys_and_virtual_key_codes() {
        assert_eq!(Key::parse("B").unwrap().vk(), 0x42);
        assert_eq!(Key::parse("0").unwrap().vk(), 0x30);
        assert_eq!(Key::parse("9").unwrap().vk(), 0x39);
        assert_eq!(Key::parse("F1").unwrap().vk(), 0x70);
        assert_eq!(Key::parse("f24").unwrap().vk(), 0x87);
        for bad in ["", "ab", "f0", "f25", "f01", ";", "é", "space"] {
            assert!(Key::parse(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn process_can_be_a_list_and_matching_ignores_case() {
        let c = Config::parse(
            r#"
            [[app]]
            key = "x"
            process = ["a.exe", "B.exe"]
            "#,
        )
        .unwrap();
        let app = &c.apps[0];
        assert!(app.matches_process("b.EXE"));
        assert!(!app.matches_process("c.exe"));
        assert_eq!(app.launch.program, "a.exe");
    }

    #[test]
    fn every_problem_is_reported() {
        let err = Config::parse(
            r#"
            modifier = "hyper"
            [[app]]
            key = "b"
            process = "a.exe"
            [[app]]
            key = "B"
            process = 'C:\x\b.exe'
            [[app]]
            key = "!!"
            process = []
            "#,
        )
        .unwrap_err();
        let text = err.to_string();
        assert_eq!(err.0.len(), 5, "{text}");
        assert!(text.contains("hyper"));
        assert!(text.contains("more than one app (#1 and #2)"));
        assert!(text.contains("not a path"));
        assert!(text.contains("\"!!\""));
        assert!(text.contains("empty"));
    }

    #[test]
    fn unknown_fields_and_bad_toml_are_errors() {
        assert!(Config::parse("[[app]]\nkey = \"b\"\nprocess = \"a\"\ntitle = \"x\"").is_err());
        assert!(Config::parse("modifer = \"ctrl\"").is_err());
        // A half-saved file.
        assert!(Config::parse("[[app]]\nkey = \"b\"\nprocess = \"fire").is_err());
    }

    #[test]
    fn launch_splitting() {
        let cases = [
            ("firefox.exe", "firefox.exe", ""),
            ("wt.exe -p Ubuntu", "wt.exe", "-p Ubuntu"),
            (r#""C:\Program Files\A\a.exe" --x "y z""#, r"C:\Program Files\A\a.exe", r#"--x "y z""#),
            (r"C:\Program Files (x86)\Steam\steam.exe", r"C:\Program Files (x86)\Steam\steam.exe", ""),
            (r"C:\Program Files\B\b.EXE -silent", r"C:\Program Files\B\b.EXE", "-silent"),
            (r"C:\a.exe.dir\b.exe x", r"C:\a.exe.dir\b.exe", "x"),
            ("notepad file.txt", "notepad", "file.txt"),
            ("  spotify  ", "spotify", ""),
        ];
        for (input, program, args) in cases {
            let l = Launch::parse(input);
            assert_eq!((l.program.as_str(), l.args.as_str()), (program, args), "{input}");
        }
    }
}
