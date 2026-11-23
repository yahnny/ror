// Typed wrappers for the Rust commands in apps/desktop/src/main.rs.
// The shapes mirror ror_core::edit::Settings and ror_core::ipc::Status.
import { invoke } from "@tauri-apps/api/core";

export interface AppSettings {
  key: string;
  processes: string[];
  /** Empty means "use the process name". */
  launch: string;
}

export interface Settings {
  modifier: string;
  log: boolean;
  apps: AppSettings[];
}

export interface Loaded {
  path: string;
  /** null if config.toml isn't valid TOML; `error` says why. */
  settings: Settings | null;
  error: string | null;
}

export interface HotkeyStatus {
  key: string;
  processes: string[];
  /** Why the key couldn't be registered, or null if it is active. */
  error: string | null;
}

export interface Status {
  version: string;
  pid: number;
  config_path: string;
  config_error: string | null;
  modifier: string;
  hotkeys: HotkeyStatus[];
  log_path: string | null;
}

export const load = () => invoke<Loaded>("load");
/** Resolves to a message for the user; rejects with the validation errors. */
export const save = (settings: Settings) => invoke<string>("save", { settings });
/** null if rord isn't running. */
export const status = () => invoke<Status | null>("status");

/** A program with an open window, for the app picker. */
export interface RunningApp {
  /** Exe file name, e.g. "firefox.exe". */
  process: string;
  /** Display name, e.g. "Visual Studio Code". */
  name: string;
  /** Title of its most recently used window. */
  title: string;
  /** Suggested launch command. */
  launch: string;
  /** PNG data URL, if the program has an icon. */
  icon: string | null;
}

/** How a configured app looks: display name and icon. */
export interface AppLook {
  name: string;
  icon: string | null;
}

/** Programs with taskbar windows, most recently used first. */
export const runningApps = () => invoke<RunningApp[]>("running_apps");

/** Names and icons for configured apps, in the same order. */
export const describeApps = (apps: AppSettings[]) => invoke<AppLook[]>("describe_apps", { apps });
