//! Editing `config.toml` from the settings UI without losing comments or
//! layout from hand edits (via `toml_edit`).
//!
//! The UI works with [`Settings`]: the raw strings as typed. [`apply`] writes
//! them into the existing file text and validates the result with
//! [`Config::parse`], so a save can never produce a config `rord` rejects.

use serde::{Deserialize, Serialize};
use toml_edit::{Array, ArrayOfTables, DocumentMut, Item, Table, Value};

use crate::config::{Config, ConfigError, DEFAULT_MODIFIER};

/// The editable parts of the config, as plain strings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub modifier: String,
    pub log: bool,
    pub apps: Vec<AppSettings>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppSettings {
    pub key: String,
    pub processes: Vec<String>,
    /// Empty means "use the process name".
    pub launch: String,
}

/// Read the settings out of config text. Fails only if it isn't valid TOML.
pub fn read(text: &str) -> Result<Settings, String> {
    let doc: DocumentMut = text.parse().map_err(|e: toml_edit::TomlError| e.to_string())?;
    let apps = doc
        .get("app")
        .and_then(Item::as_array_of_tables)
        .map(|aot| {
            aot.iter()
                .map(|t| AppSettings {
                    key: t.get("key").and_then(Item::as_str).unwrap_or_default().to_string(),
                    processes: match t.get("process") {
                        Some(i) if i.is_str() => vec![i.as_str().unwrap_or_default().to_string()],
                        Some(i) => i
                            .as_array()
                            .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect())
                            .unwrap_or_default(),
                        None => Vec::new(),
                    },
                    launch: t.get("launch").and_then(Item::as_str).unwrap_or_default().to_string(),
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Settings {
        modifier: doc.get("modifier").and_then(Item::as_str).unwrap_or(DEFAULT_MODIFIER).to_string(),
        log: doc.get("log").and_then(Item::as_bool).unwrap_or(false),
        apps,
    })
}

/// Write `settings` into `text`, keeping its comments and layout.
/// Returns the new text, or every problem if the result wouldn't be valid.
pub fn apply(text: &str, settings: &Settings) -> Result<String, ConfigError> {
    let mut doc: DocumentMut =
        text.parse().map_err(|e: toml_edit::TomlError| ConfigError(vec![e.to_string()]))?;

    set(doc.as_table_mut(), "modifier", settings.modifier.trim().into());
    set(doc.as_table_mut(), "log", settings.log.into());

    if !doc.contains_key("app") {
        doc.insert("app", Item::ArrayOfTables(ArrayOfTables::new()));
    }
    let aot = doc["app"]
        .as_array_of_tables_mut()
        .ok_or_else(|| ConfigError(vec!["`app` should be a list of [[app]] tables".into()]))?;
    // Entries are matched by position: edit existing tables in place (keeping
    // their comments), add new ones on the end, drop any left over.
    while aot.len() > settings.apps.len() {
        aot.remove(aot.len() - 1);
    }
    for (i, app) in settings.apps.iter().enumerate() {
        if i == aot.len() {
            aot.push(Table::new());
        }
        let table = aot.get_mut(i).expect("index is in range");
        set(table, "key", app.key.trim().into());
        let processes: Vec<&str> =
            app.processes.iter().map(|p| p.trim()).filter(|p| !p.is_empty()).collect();
        let process = match processes.as_slice() {
            [one] => Value::from(*one),
            many => Value::Array(many.iter().copied().collect::<Array>()),
        };
        set(table, "process", process);
        if app.launch.trim().is_empty() {
            table.remove("launch");
        } else {
            set(table, "launch", app.launch.trim().into());
        }
    }

    let out = doc.to_string();
    Config::parse(&out)?;
    Ok(out)
}

/// Set `key` to `value`, keeping the old value's surrounding whitespace and
/// trailing comment.
fn set(table: &mut Table, key: &str, mut value: Value) {
    match table.get_mut(key).and_then(Item::as_value_mut) {
        Some(old) => {
            *value.decor_mut() = old.decor().clone();
            *old = value;
        }
        None => {
            table.insert(key, Item::Value(value));
        }
    }
}

/// Suggest a launch command for a program found running at `path`, so
/// adding an app needs no knowledge of where or how it is installed.
///
/// - Store apps run from a protected, versioned `WindowsApps` folder that
///   can't be launched directly; they get their launch alias instead (e.g.
///   Windows Terminal -> `wt.exe`).
/// - Apps that update themselves Squirrel-style (Discord, Slack, …) run from
///   a versioned folder (`…\Discord\app-1.0.9187\Discord.exe`); they get
///   their stable launcher: `…\Discord\Update.exe --processStart Discord.exe`.
/// - A helper nested inside an app's folder (Steam's window belongs to
///   `Steam\bin\cef\…\steamwebhelper.exe`) gets the app's own exe if the
///   folder has one named after it (`Steam\steam.exe`).
/// - Folders in `vars` (e.g. `("LOCALAPPDATA", "C:\Users\me\AppData\Local")`)
///   are written as `%LOCALAPPDATA%`.
///
/// `exes_in` lists the `.exe` file names in a folder ([`exes_in`] for real;
/// passed in to keep this testable).
pub fn suggest_launch(
    path: &str,
    vars: &[(&str, &str)],
    exes_in: impl Fn(&str) -> Vec<String>,
) -> String {
    let parts: Vec<&str> = path.split('\\').collect();
    let command = store_alias(&parts, vars, &exes_in)
        .or_else(|| squirrel_launcher(&parts, &exes_in))
        .or_else(|| folder_launcher(&parts, &exes_in))
        .unwrap_or_else(|| path.to_string());
    with_vars(command, vars)
}

/// `.exe` file names in a folder (empty if it can't be read).
pub fn exes_in(dir: &str) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    entries
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.to_ascii_lowercase().ends_with(".exe"))
        .collect()
}

fn find_exe(exes: &[String], name: &str) -> Option<String> {
    exes.iter().find(|e| e.eq_ignore_ascii_case(name)).cloned()
}

/// `…\WindowsApps\Microsoft.WindowsTerminal_1.23.0_x64__8wekyb3d8bbwe\WindowsTerminal.exe`
/// -> the alias in `%LOCALAPPDATA%\Microsoft\WindowsApps\Microsoft.WindowsTerminal_8wekyb3d8bbwe\`.
fn store_alias(parts: &[&str], vars: &[(&str, &str)], exes_in: impl Fn(&str) -> Vec<String>) -> Option<String> {
    let i = parts.iter().position(|p| p.eq_ignore_ascii_case("WindowsApps"))?;
    if i + 2 >= parts.len() {
        return None;
    }
    let (name, rest) = parts[i + 1].split_once('_')?;
    let (_, publisher) = rest.split_once("__")?;
    let exe = parts[parts.len() - 1];
    let local = vars.iter().find(|(n, _)| *n == "LOCALAPPDATA")?.1.trim_end_matches('\\');
    let aliases = exes_in(&format!(r"{local}\Microsoft\WindowsApps\{name}_{publisher}"));
    let alias = find_exe(&aliases, exe).or_else(|| (aliases.len() == 1).then(|| aliases[0].clone()));
    // Aliases are on the PATH, so the bare name runs it. With none found, the
    // process name is the best guess; the versioned path never works.
    Some(alias.unwrap_or_else(|| exe.to_string()))
}

/// `…\Discord\app-1.0.9187\Discord.exe` -> `…\Discord\Update.exe --processStart Discord.exe`.
fn squirrel_launcher(parts: &[&str], exes_in: impl Fn(&str) -> Vec<String>) -> Option<String> {
    let n = parts.len();
    if n < 3 {
        return None;
    }
    let version = parts[n - 2].to_ascii_lowercase();
    version.strip_prefix("app-")?.chars().next().filter(char::is_ascii_digit)?;
    let root = parts[..n - 2].join("\\");
    let update = find_exe(&exes_in(&root), "Update.exe")?;
    Some(format!(r"{root}\{update} --processStart {}", parts[n - 1]))
}

/// `…\Steam\bin\cef\cef.win64\steamwebhelper.exe` -> `…\Steam\steam.exe`.
/// Only folders above the exe's own folder, nearest first.
fn folder_launcher(parts: &[&str], exes_in: impl Fn(&str) -> Vec<String>) -> Option<String> {
    let n = parts.len();
    (1..n.saturating_sub(2)).rev().find_map(|k| {
        let dir = parts[..=k].join("\\");
        let exe = find_exe(&exes_in(&dir), &format!("{}.exe", parts[k]))?;
        Some(format!(r"{dir}\{exe}"))
    })
}

/// Write folders from `vars` as `%NAME%`; the longest match wins, so
/// `%LOCALAPPDATA%` beats `%USERPROFILE%`.
fn with_vars(command: String, vars: &[(&str, &str)]) -> String {
    let mut vars: Vec<&(&str, &str)> = vars.iter().filter(|(_, v)| !v.is_empty()).collect();
    vars.sort_by_key(|(_, v)| std::cmp::Reverse(v.len()));
    for (name, value) in vars {
        let value = value.trim_end_matches('\\');
        let prefix = format!("{}\\", value.to_ascii_lowercase());
        if command.to_ascii_lowercase().starts_with(&prefix) {
            return format!("%{name}%{}", &command[value.len()..]);
        }
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DEFAULT_CONFIG;

    const TEXT: &str = r#"# my config
modifier = "ctrl+shift+alt" # meh

[[app]]
# the browser
key = "b"
process = "firefox.exe"

[[app]]
key = "g"
process = ["steam.exe", "steamwebhelper.exe"]
launch = 'C:\Steam\steam.exe'
"#;

    #[test]
    fn read_gets_raw_values() {
        let s = read(TEXT).unwrap();
        assert_eq!(s.modifier, "ctrl+shift+alt");
        assert!(!s.log);
        assert_eq!(s.apps[0], AppSettings { key: "b".into(), processes: vec!["firefox.exe".into()], launch: "".into() });
        assert_eq!(s.apps[1].processes, ["steam.exe", "steamwebhelper.exe"]);
        assert_eq!(s.apps[1].launch, r"C:\Steam\steam.exe");
    }

    #[test]
    fn unchanged_settings_keep_the_file_as_is_apart_from_log() {
        let out = apply(TEXT, &read(TEXT).unwrap()).unwrap();
        assert_eq!(out.replace("log = false\n", ""), TEXT);
    }

    #[test]
    fn edits_keep_comments() {
        let mut s = read(TEXT).unwrap();
        s.modifier = "alt+win".into();
        s.apps[0].key = "f".into();
        let out = apply(TEXT, &s).unwrap();
        assert!(out.contains("# my config"));
        assert!(out.contains(r#"modifier = "alt+win" # meh"#), "{out}");
        assert!(out.contains("# the browser\nkey = \"f\""), "{out}");
    }

    #[test]
    fn add_and_remove_apps() {
        let mut s = read(TEXT).unwrap();
        s.apps.remove(1);
        s.apps.push(AppSettings { key: "n".into(), processes: vec![" notepad.exe ".into()], launch: "".into() });
        let out = apply(TEXT, &s).unwrap();
        let c = Config::parse(&out).unwrap();
        assert_eq!(c.apps.len(), 2);
        assert_eq!(c.apps[1].processes, ["notepad.exe"]);
        assert!(!out.contains("steam"));
        assert!(!out.contains("launch"));
    }

    #[test]
    fn works_on_an_empty_file_and_the_default_config() {
        let s = Settings {
            modifier: "ctrl+alt".into(),
            log: true,
            apps: vec![AppSettings { key: "x".into(), processes: vec!["x.exe".into()], launch: "".into() }],
        };
        let c = Config::parse(&apply("", &s).unwrap()).unwrap();
        assert!(c.log);
        assert_eq!(c.apps[0].key.to_string(), "x");
        assert!(apply(DEFAULT_CONFIG, &read(DEFAULT_CONFIG).unwrap()).is_ok());
    }

    #[test]
    fn invalid_settings_are_refused() {
        let mut s = read(TEXT).unwrap();
        s.apps[1].key = "b".into();
        s.apps[0].processes.clear();
        let err = apply(TEXT, &s).unwrap_err();
        assert_eq!(err.0.len(), 2, "{err}");
    }

    #[test]
    fn launch_suggestions() {
        let vars = [
            ("APPDATA", r"C:\Users\me\AppData\Roaming"),
            ("LOCALAPPDATA", r"C:\Users\me\AppData\Local"),
            ("USERPROFILE", r"C:\Users\me"),
        ];
        // A fake disk: folder -> exes in it.
        let disk = |entries: &'static [(&'static str, &'static [&'static str])]| {
            move |dir: &str| -> Vec<String> {
                entries
                    .iter()
                    .find(|(d, _)| d.eq_ignore_ascii_case(dir))
                    .map(|(_, exes)| exes.iter().map(|e| e.to_string()).collect())
                    .unwrap_or_default()
            }
        };
        let empty = disk(&[]);

        // Plain installs keep their path; user folders become variables.
        assert_eq!(
            suggest_launch(r"C:\Program Files\Mozilla Firefox\firefox.exe", &vars, empty),
            r"C:\Program Files\Mozilla Firefox\firefox.exe"
        );
        assert_eq!(
            suggest_launch(r"C:\Users\me\AppData\Local\Programs\Obsidian\Obsidian.exe", &vars, empty),
            r"%LOCALAPPDATA%\Programs\Obsidian\Obsidian.exe"
        );
        assert_eq!(
            suggest_launch(r"c:\users\ME\appdata\roaming\Spotify\Spotify.exe", &vars, empty),
            r"%APPDATA%\Spotify\Spotify.exe"
        );
        assert_eq!(
            suggest_launch(r"C:\Users\me\AppData\LocalX\a.exe", &vars, empty),
            r"%USERPROFILE%\AppData\LocalX\a.exe"
        );

        // Squirrel-style apps use their stable launcher, if it exists.
        let discord = r"C:\Users\me\AppData\Local\Discord\app-1.0.9187\Discord.exe";
        let squirrel = disk(&[(r"C:\Users\me\AppData\Local\Discord", &["Update.exe"])]);
        let launch = suggest_launch(discord, &vars, squirrel);
        assert_eq!(launch, r"%LOCALAPPDATA%\Discord\Update.exe --processStart Discord.exe");
        assert!(Config::parse(&format!("[[app]]\nkey='d'\nprocess='Discord.exe'\nlaunch='{launch}'")).is_ok());
        assert_eq!(suggest_launch(discord, &vars, empty), r"%LOCALAPPDATA%\Discord\app-1.0.9187\Discord.exe");

        // Store apps use their launch alias.
        let terminal = r"C:\Program Files\WindowsApps\Microsoft.WindowsTerminal_1.23.20211.0_x64__8wekyb3d8bbwe\WindowsTerminal.exe";
        let aliases = disk(&[(
            r"C:\Users\me\AppData\Local\Microsoft\WindowsApps\Microsoft.WindowsTerminal_8wekyb3d8bbwe",
            &["wt.exe"],
        )]);
        assert_eq!(suggest_launch(terminal, &vars, aliases), "wt.exe");
        assert_eq!(suggest_launch(terminal, &vars, empty), "WindowsTerminal.exe");

        // A nested helper gets the app's own exe.
        let steam = disk(&[(r"C:\Program Files (x86)\Steam", &["steam.exe", "uninstall.exe"])]);
        assert_eq!(
            suggest_launch(r"C:\Program Files (x86)\Steam\bin\cef\cef.win64\steamwebhelper.exe", &vars, steam),
            r"C:\Program Files (x86)\Steam\steam.exe"
        );
        // …but not an exe in its own folder that happens to match.
        let own = disk(&[(r"C:\Tools\Tools", &["Tools.exe", "x.exe"])]);
        assert_eq!(suggest_launch(r"C:\Tools\Tools\x.exe", &vars, own), r"C:\Tools\Tools\x.exe");
    }
}
