use std::path::{Path, PathBuf};

use super::manifest::Kind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Bundle(PathBuf),
    Nsis(PathBuf),
    AppImage(PathBuf),
    Deb(PathBuf),
    Unsupported(&'static str),
}

pub const MOVE_TO_APPLICATIONS: &str =
    "Move Backspn to the Applications folder to update it from here.";
const NOT_INSTALLED: &str = "This copy of Backspn was not installed from a release.";

impl Target {
    pub fn current() -> Self {
        let Ok(exe) = std::env::current_exe() else {
            return Self::Unsupported(NOT_INSTALLED);
        };
        let appimage = std::env::var_os("APPIMAGE").map(PathBuf::from);
        detect(std::env::consts::OS, &exe, appimage.as_deref(), &|p| {
            p.exists()
        })
    }

    pub fn kind(&self) -> Option<Kind> {
        match self {
            Self::Bundle(_) => Some(Kind::App),
            Self::Nsis(_) => Some(Kind::Nsis),
            Self::AppImage(_) => Some(Kind::AppImage),
            Self::Deb(_) => Some(Kind::Deb),
            Self::Unsupported(_) => None,
        }
    }
}

fn detect(os: &str, exe: &Path, appimage: Option<&Path>, exists: &dyn Fn(&Path) -> bool) -> Target {
    match os {
        "macos" => bundle(exe),
        "windows" => match exe.parent() {
            Some(dir) if exists(&dir.join("uninstall.exe")) => Target::Nsis(exe.to_owned()),
            _ => Target::Unsupported(NOT_INSTALLED),
        },
        "linux" => match appimage {
            Some(file) if file.is_absolute() => Target::AppImage(file.to_owned()),
            _ if exe.starts_with("/usr/bin") && exists(Path::new("/usr/bin/pkexec")) => {
                Target::Deb(exe.to_owned())
            }
            _ => Target::Unsupported(NOT_INSTALLED),
        },
        _ => Target::Unsupported(NOT_INSTALLED),
    }
}

fn bundle(exe: &Path) -> Target {
    let mut up = exe.ancestors().skip(1);
    let (Some(macos), Some(contents), Some(app)) = (up.next(), up.next(), up.next()) else {
        return Target::Unsupported(NOT_INSTALLED);
    };
    if macos.file_name() != Some("MacOS".as_ref())
        || contents.file_name() != Some("Contents".as_ref())
        || app.extension() != Some("app".as_ref())
    {
        return Target::Unsupported(NOT_INSTALLED);
    }
    let translocated = app
        .components()
        .any(|c| c.as_os_str() == "AppTranslocation");
    if translocated || app.starts_with("/Volumes") {
        return Target::Unsupported(MOVE_TO_APPLICATIONS);
    }
    Target::Bundle(app.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none(_: &Path) -> bool {
        false
    }

    #[test]
    fn a_mac_bundle_updates_unless_it_runs_from_a_disk_image() {
        let mac = |exe: &str| detect("macos", Path::new(exe), None, &none);
        assert_eq!(
            mac("/Applications/Backspn.app/Contents/MacOS/Backspn"),
            Target::Bundle("/Applications/Backspn.app".into())
        );
        assert_eq!(
            mac("/private/var/folders/x/AppTranslocation/ABC/d/Backspn.app/Contents/MacOS/Backspn"),
            Target::Unsupported(MOVE_TO_APPLICATIONS)
        );
        assert_eq!(
            mac("/Volumes/Backspn/Backspn.app/Contents/MacOS/Backspn"),
            Target::Unsupported(MOVE_TO_APPLICATIONS)
        );
        assert_eq!(
            mac("/Users/me/git/Backspn/target/release/Backspn"),
            Target::Unsupported(NOT_INSTALLED)
        );
    }

    #[test]
    fn windows_needs_the_installer_next_to_the_app() {
        let exe = Path::new(r"C:\Users\me\AppData\Local\Backspn\Backspn.exe");
        let installed = |p: &Path| p.ends_with("uninstall.exe");
        assert_eq!(
            detect("windows", exe, None, &installed),
            Target::Nsis(exe.to_owned())
        );
        assert_eq!(
            detect("windows", exe, None, &none),
            Target::Unsupported(NOT_INSTALLED)
        );
    }

    #[test]
    fn linux_tells_an_appimage_from_a_package() {
        let file = Path::new("/home/me/Apps/Backspn.AppImage");
        let mounted = Path::new("/tmp/.mount_BackspXYZ/usr/bin/Backspn");
        assert_eq!(
            detect("linux", mounted, Some(file), &none),
            Target::AppImage(file.to_owned())
        );
        let usr = Path::new("/usr/bin/backspn");
        let pkexec = |p: &Path| p.ends_with("pkexec");
        assert_eq!(
            detect("linux", usr, None, &pkexec),
            Target::Deb(usr.to_owned())
        );
        assert_eq!(
            detect("linux", usr, None, &none),
            Target::Unsupported(NOT_INSTALLED)
        );
        assert_eq!(
            detect("linux", Path::new("/home/me/backspn"), None, &pkexec),
            Target::Unsupported(NOT_INSTALLED)
        );
    }
}
