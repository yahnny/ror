//! Code shared by `rord` (listener), `ror` (CLI) and, later, the settings UI.
//!
//! The pure parts (config parsing, press decisions, pipe messages) have no
//! Windows dependencies, so their tests run natively in WSL.

pub mod config;
pub mod decide;
pub mod edit;
pub mod ipc;
