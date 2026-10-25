pub mod crash;
pub mod github;
pub mod log;
pub mod reset;
pub mod update;

use std::path::PathBuf;

pub const REPO: &str = "suitux/Backspn";

pub fn version() -> &'static str {
    option_env!("BACKSPN_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))
}

pub fn system() -> String {
    format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)
}

fn folder(name: &str) -> PathBuf {
    crate::prefs::settings_path().with_file_name(name)
}
