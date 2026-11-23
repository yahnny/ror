//! `ror.exe`: the command-line tool. Talks to `rord` over the named pipe.

#[cfg(windows)]
mod commands;

const USAGE: &str = "\
usage: ror <command>

commands:
  status    show whether rord is running, its config and keys
  reload    reload the config now
  quit      stop rord
  start     start rord (if it isn't running)
  edit      open the config file in your editor
  settings  open the settings window
  apps      list running apps, with the process and launch to use for each
  version   show the version";

#[cfg(windows)]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["status"] => commands::status(),
        ["reload"] => commands::reload(),
        ["quit" | "stop"] => commands::quit(),
        ["start"] => commands::start(),
        ["edit"] => commands::edit(),
        ["settings"] => commands::settings(),
        ["apps"] => commands::apps(),
        ["version" | "--version" | "-V"] => commands::version(),
        ["help" | "--help" | "-h"] => {
            println!("{USAGE}");
            0
        }
        _ => {
            eprintln!("{USAGE}");
            2
        }
    };
    std::process::exit(code);
}

#[cfg(not(windows))]
fn main() {
    eprintln!("ror runs on Windows only\n\n{USAGE}");
    std::process::exit(1);
}
