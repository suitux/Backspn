use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

static REQUESTED: AtomicBool = AtomicBool::new(false);

pub const DELETED: &[&str] = &[
    "Settings and preferences",
    "Window and panel layout",
    "Controller and MIDI mappings",
    "Downloaded stem separation model",
    "Logs and crash reports",
    "Anonymous usage counts and the install's random ID",
];

pub const KEPT: &[&str] = &[
    "Music library: collection, playlists, analysis and cues",
    "Library backups and covers",
    "Own kits",
    "Recordings, samples, stems and music files",
];

pub fn request() {
    REQUESTED.store(true, Ordering::SeqCst);
}

struct Targets {
    files: Vec<PathBuf>,
    dirs: Vec<PathBuf>,
}

fn with_backup(path: PathBuf) -> [PathBuf; 2] {
    let mut backup = path.as_os_str().to_owned();
    backup.push(".bak");
    [path, PathBuf::from(backup)]
}

fn targets(settings: &Path, ui_state: &Path, mappings: &Path, model: &Path) -> Targets {
    let mut files = vec![settings.with_file_name("insights.json")];
    for path in [settings, ui_state, mappings] {
        files.extend(with_backup(path.to_owned()));
    }
    let mut dirs: Vec<PathBuf> = ["Logs", "Crashlogs"]
        .iter()
        .map(|name| settings.with_file_name(name))
        .collect();
    dirs.extend(model.parent().map(Path::to_owned));
    Targets { files, dirs }
}

fn remove(path: &Path, result: std::io::Result<()>) {
    match result {
        Err(e) if e.kind() != ErrorKind::NotFound => {
            eprintln!("cannot delete {}: {e}", path.display());
        }
        _ => {}
    }
}

fn relaunch() {
    let exe = std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .map_or_else(std::env::current_exe, Ok);
    match exe {
        Ok(exe) => {
            if let Err(e) = std::process::Command::new(&exe).spawn() {
                eprintln!("cannot start {}: {e}", exe.display());
            }
        }
        Err(e) => eprintln!("cannot find the app to start it again: {e}"),
    }
}

pub fn finish() {
    if !REQUESTED.load(Ordering::SeqCst) {
        return;
    }
    let targets = targets(
        &crate::prefs::settings_path(),
        &crate::prefs::ui_state_path(),
        &crate::controls::mappings_path(),
        &crate::prefs::stem_model_path(),
    );
    for file in &targets.files {
        remove(file, fs::remove_file(file));
    }
    for dir in &targets.dirs {
        remove(dir, fs::remove_dir_all(dir));
    }
    relaunch();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_configuration_goes() {
        let data = std::env::temp_dir().join("Backspn");
        let t = targets(
            &data.join("settings.json"),
            &data.join("ui-state.ron"),
            &data.join("mappings.json"),
            &data.join("models").join("model.onnx"),
        );
        assert!(t.files.contains(&data.join("settings.json.bak")));
        assert!(t.files.contains(&data.join("ui-state.ron")));
        assert!(t.files.contains(&data.join("mappings.json.bak")));
        assert!(t.files.contains(&data.join("insights.json")));
        assert!(t.dirs.contains(&data.join("models")));
        assert!(t.dirs.contains(&data.join("Crashlogs")));
        for kept in ["library.sqlite", "Backup", "Covers", "Kits"] {
            let kept = data.join(kept);
            assert!(!t.files.contains(&kept) && !t.dirs.contains(&kept));
        }
    }
}
