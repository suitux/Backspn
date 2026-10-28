use std::path::PathBuf;

const APP: &str = "Backspn";

pub fn home() -> PathBuf {
    std::env::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

pub fn data() -> PathBuf {
    backspn_library::paths::data_dir()
}

pub fn user_music() -> PathBuf {
    home().join("Music")
}

pub fn music() -> PathBuf {
    user_music().join(APP)
}
