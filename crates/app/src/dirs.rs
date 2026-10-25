use std::ffi::OsString;
use std::path::{Path, PathBuf};

const APP: &str = "Backspn";

pub fn home() -> PathBuf {
    std::env::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

pub fn data() -> PathBuf {
    data_in(System::current(), &home(), &|name| std::env::var_os(name))
}

pub fn user_music() -> PathBuf {
    home().join("Music")
}

pub fn music() -> PathBuf {
    user_music().join(APP)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum System {
    Mac,
    Windows,
    Other,
}

impl System {
    fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Mac
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Other
        }
    }
}

fn data_in(system: System, home: &Path, var: &dyn Fn(&str) -> Option<OsString>) -> PathBuf {
    let absolute = |name: &str| var(name).map(PathBuf::from).filter(|p| p.is_absolute());
    match system {
        System::Mac => home.join("Library/Application Support"),
        System::Windows => absolute("APPDATA").unwrap_or_else(|| home.join("AppData/Roaming")),
        System::Other => absolute("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local/share")),
    }
    .join(APP)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> + use<> {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |name| {
            pairs
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| OsString::from(v))
        }
    }

    #[test]
    fn data_goes_where_each_system_keeps_it() {
        let home = std::env::temp_dir().join("u");
        let appdata = std::env::temp_dir().join("Roaming");
        let xdg = std::env::temp_dir().join("xdg");
        let set = vars(&[
            ("APPDATA", appdata.to_str().unwrap()),
            ("XDG_DATA_HOME", xdg.to_str().unwrap()),
        ]);
        assert_eq!(
            data_in(System::Mac, &home, &set),
            home.join("Library/Application Support/Backspn")
        );
        assert_eq!(
            data_in(System::Windows, &home, &set),
            appdata.join("Backspn")
        );
        assert_eq!(data_in(System::Other, &home, &set), xdg.join("Backspn"));
    }

    #[test]
    fn without_the_variables_data_is_under_home() {
        let home = std::env::temp_dir().join("u");
        let none = vars(&[("XDG_DATA_HOME", ""), ("APPDATA", "relative")]);
        assert_eq!(
            data_in(System::Windows, &home, &none),
            home.join("AppData/Roaming/Backspn")
        );
        assert_eq!(
            data_in(System::Other, &home, &none),
            home.join(".local/share/Backspn")
        );
    }
}
