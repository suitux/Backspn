use std::path::{Path, PathBuf};

use backspn_formats::nml::Location;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum System {
    Mac,
    Windows,
    Linux,
}

const SYSTEM: System = if cfg!(target_os = "macos") {
    System::Mac
} else if cfg!(windows) {
    System::Windows
} else {
    System::Linux
};

const APP: &str = "Backspn";

/// Where Backspn keeps its settings, library and other data.
pub fn data_dir() -> PathBuf {
    let home = std::env::home_dir().unwrap_or_else(|| PathBuf::from("."));
    data_dir_in(SYSTEM, &home, &|name| std::env::var_os(name))
}

fn data_dir_in(
    system: System,
    home: &Path,
    var: &dyn Fn(&str) -> Option<std::ffi::OsString>,
) -> PathBuf {
    let absolute = |name: &str| var(name).map(PathBuf::from).filter(|p| p.is_absolute());
    match system {
        System::Mac => home.join("Library/Application Support"),
        System::Windows => absolute("APPDATA").unwrap_or_else(|| home.join("AppData/Roaming")),
        System::Linux => absolute("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local/share")),
    }
    .join(APP)
}

pub fn local_path(location: &Location) -> PathBuf {
    let user = std::env::var("USER").unwrap_or_default();
    PathBuf::from(local_path_on(
        SYSTEM,
        location,
        &startup_volume(),
        &user,
        &|p| Path::new(p).exists(),
    ))
}

fn local_path_on(
    system: System,
    location: &Location,
    startup: &str,
    user: &str,
    exists: &dyn Fn(&str) -> bool,
) -> String {
    let relative = location.relative_path();
    let volume = &location.volume;
    match system {
        System::Mac => canonical(&format!("/Volumes/{volume}/{relative}"), startup),
        System::Windows if is_drive(volume) => format!("{volume}\\{}", relative.replace('/', "\\")),
        System::Windows => format!("/Volumes/{volume}/{relative}"),
        System::Linux if volume == startup => format!("/{relative}"),
        System::Linux => {
            let mounts = [
                format!("/media/{user}/{volume}"),
                format!("/run/media/{user}/{volume}"),
                format!("/mnt/{volume}"),
            ];
            let root = mounts.iter().find(|m| exists(m)).unwrap_or(&mounts[0]);
            format!("{root}/{relative}")
        }
    }
}

pub fn canonical(path: &str, startup_volume: &str) -> String {
    let prefix = format!("/Volumes/{startup_volume}/");
    match path.strip_prefix(&prefix) {
        Some(rest) => format!("/{rest}"),
        None => path.to_owned(),
    }
}

pub fn local_file(path: &str) -> Option<PathBuf> {
    let file = Path::new(path.split('#').next().unwrap_or(path));
    file.has_root().then(|| file.to_path_buf())
}

pub fn names_from_file_name(path: &Path) -> (String, String) {
    let stem = path
        .file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    match stem.split_once(" - ") {
        Some((artist, title)) => (artist.to_owned(), title.to_owned()),
        None => (String::new(), stem),
    }
}

pub fn startup_volume() -> String {
    match SYSTEM {
        System::Mac => std::fs::read_dir("/Volumes")
            .ok()
            .and_then(|entries| {
                entries.flatten().find_map(|e| {
                    let target = std::fs::read_link(e.path()).ok()?;
                    (target == Path::new("/")).then(|| e.file_name().to_string_lossy().into_owned())
                })
            })
            .unwrap_or_else(|| "Macintosh HD".to_owned()),
        System::Windows => std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_owned()),
        System::Linux => "/".to_owned(),
    }
}

pub fn location_of(path: &Path, startup_volume: &str) -> Location {
    location_on(SYSTEM, &path.to_string_lossy(), startup_volume)
}

fn is_drive(volume: &str) -> bool {
    let b = volume.as_bytes();
    b.len() == 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

fn location_on(system: System, path: &str, startup: &str) -> Location {
    let windows = system == System::Windows;
    let separators: &[char] = if windows { &['/', '\\'] } else { &['/'] };
    let drive = path.get(..2).filter(|d| windows && is_drive(d));
    let rest = drive.map_or(path, |d| &path[d.len()..]);
    let components: Vec<String> = rest
        .split(separators)
        .filter(|c| !c.is_empty() && *c != ".")
        .map(str::to_owned)
        .collect();
    let (volume, rest) = match (system, components.as_slice()) {
        (System::Windows, all) if drive.is_some() => (
            drive.map_or_else(|| startup.to_owned(), str::to_ascii_uppercase),
            all.to_vec(),
        ),
        (System::Mac | System::Windows, [first, volume, rest @ ..])
            if first == "Volumes" && !rest.is_empty() =>
        {
            (volume.clone(), rest.to_vec())
        }
        (System::Linux, [m, _user, volume, rest @ ..]) if m == "media" && !rest.is_empty() => {
            (volume.clone(), rest.to_vec())
        }
        (System::Linux, [r, m, _user, volume, rest @ ..])
            if r == "run" && m == "media" && !rest.is_empty() =>
        {
            (volume.clone(), rest.to_vec())
        }
        (System::Linux, [m, volume, rest @ ..]) if m == "mnt" && !rest.is_empty() => {
            (volume.clone(), rest.to_vec())
        }
        _ => (startup.to_owned(), components.clone()),
    };
    let (file, dirs) = rest
        .split_last()
        .map_or((String::new(), &[][..]), |(f, d)| (f.clone(), d));
    let mut dir = String::from("/:");
    for d in dirs {
        dir.push_str(d);
        dir.push_str("/:");
    }
    Location {
        dir,
        file,
        volume_id: Some(volume.clone()),
        volume,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<std::ffi::OsString> + use<> {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |name| {
            pairs
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| std::ffi::OsString::from(v))
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
            data_dir_in(System::Mac, &home, &set),
            home.join("Library/Application Support/Backspn")
        );
        assert_eq!(
            data_dir_in(System::Windows, &home, &set),
            appdata.join("Backspn")
        );
        assert_eq!(data_dir_in(System::Linux, &home, &set), xdg.join("Backspn"));
    }

    #[test]
    fn without_the_variables_data_is_under_home() {
        let home = std::env::temp_dir().join("u");
        let none = vars(&[("XDG_DATA_HOME", ""), ("APPDATA", "relative")]);
        assert_eq!(
            data_dir_in(System::Windows, &home, &none),
            home.join("AppData/Roaming/Backspn")
        );
        assert_eq!(
            data_dir_in(System::Linux, &home, &none),
            home.join(".local/share/Backspn")
        );
    }

    fn none(_: &str) -> bool {
        false
    }

    #[test]
    fn startup_disk_paths_have_one_form() {
        assert_eq!(
            canonical("/Volumes/Mac HD/Users/x/a.mp3", "Mac HD"),
            "/Users/x/a.mp3"
        );
        assert_eq!(
            canonical("/Volumes/USB/a.mp3", "Mac HD"),
            "/Volumes/USB/a.mp3"
        );
        assert_eq!(canonical("/Users/x/a.mp3#1", "Mac HD"), "/Users/x/a.mp3#1");
        let a = location_on(System::Mac, "/Users/x/a.mp3", "Mac HD");
        let b = location_on(System::Mac, "/Volumes/Mac HD/Users/x/a.mp3", "Mac HD");
        assert_eq!(a, b);
    }

    #[test]
    fn local_files_start_from_a_root() {
        assert_eq!(local_file("/m/a.mp3#2"), Some(PathBuf::from("/m/a.mp3")));
        assert_eq!(local_file("streaming:123"), None);
        assert_eq!(local_file("a.mp3"), None);
        #[cfg(windows)]
        assert_eq!(
            local_file(r"C:\m\a.mp3"),
            Some(PathBuf::from(r"C:\m\a.mp3"))
        );
    }

    #[test]
    fn names_come_from_artist_dash_title_file_names() {
        assert_eq!(
            names_from_file_name(Path::new("/m/Adam Beyer - Overdose.mp3")),
            ("Adam Beyer".to_owned(), "Overdose".to_owned())
        );
        assert_eq!(
            names_from_file_name(Path::new("/m/A - B - C.flac")),
            ("A".to_owned(), "B - C".to_owned())
        );
        assert_eq!(
            names_from_file_name(Path::new("/m/untitled.wav")),
            (String::new(), "untitled".to_owned())
        );
    }

    #[test]
    fn maps_both_ways() {
        let loc = location_on(
            System::Mac,
            "/Volumes/DJ/Music/House/a b.flac",
            "Macintosh HD",
        );
        assert_eq!(loc.volume, "DJ");
        assert_eq!(loc.dir, "/:Music/:House/:");
        assert_eq!(loc.file, "a b.flac");
        assert_eq!(
            local_path_on(System::Mac, &loc, "Macintosh HD", "x", &none),
            "/Volumes/DJ/Music/House/a b.flac"
        );

        let home = location_on(System::Mac, "/Users/x/Music/t.mp3", "Macintosh HD");
        assert_eq!(home.volume, "Macintosh HD");
        assert_eq!(home.dir, "/:Users/:x/:Music/:");
        assert_eq!(
            local_path_on(System::Mac, &home, "Macintosh HD", "x", &none),
            "/Users/x/Music/t.mp3"
        );
    }

    #[test]
    fn windows_volumes_are_drives() {
        let loc = location_on(System::Windows, r"d:\Music\House\a.flac", "C:");
        assert_eq!(loc.volume, "D:");
        assert_eq!(loc.dir, "/:Music/:House/:");
        assert_eq!(loc.file, "a.flac");
        assert_eq!(
            local_path_on(System::Windows, &loc, "C:", "x", &none),
            r"D:\Music\House\a.flac"
        );
        let slashes = location_on(System::Windows, "C:/Users/x/t.mp3", "C:");
        assert_eq!(
            (slashes.volume.as_str(), slashes.dir.as_str()),
            ("C:", "/:Users/:x/:")
        );
        let mac = location_on(System::Mac, "/Volumes/DJ/Music/a.flac", "Macintosh HD");
        let local = local_path_on(System::Windows, &mac, "C:", "x", &none);
        assert_eq!(local, "/Volumes/DJ/Music/a.flac");
        assert_eq!(location_on(System::Windows, &local, "C:"), mac);
    }

    #[test]
    fn linux_volumes_are_mounted_drives() {
        let loc = location_on(System::Linux, "/home/x/Music/t.mp3", "/");
        assert_eq!(loc.volume, "/");
        assert_eq!(loc.dir, "/:home/:x/:Music/:");
        assert_eq!(
            local_path_on(System::Linux, &loc, "/", "x", &none),
            "/home/x/Music/t.mp3"
        );
        for path in [
            "/media/x/USB/a/t.mp3",
            "/run/media/x/USB/a/t.mp3",
            "/mnt/USB/a/t.mp3",
        ] {
            let usb = location_on(System::Linux, path, "/");
            assert_eq!(
                (usb.volume.as_str(), usb.dir.as_str()),
                ("USB", "/:a/:"),
                "{path}"
            );
        }
        let usb = location_on(System::Linux, "/run/media/x/USB/a/t.mp3", "/");
        let run = |p: &str| p.starts_with("/run/media");
        assert_eq!(
            local_path_on(System::Linux, &usb, "/", "x", &run),
            "/run/media/x/USB/a/t.mp3"
        );
        assert_eq!(
            local_path_on(System::Linux, &usb, "/", "x", &none),
            "/media/x/USB/a/t.mp3",
            "not mounted: the usual place"
        );
    }
}
