
use std::path::{Path, PathBuf};

use backspn_formats::nml::Entry;

pub fn audio_path(entry: &Entry) -> PathBuf {
    let rel = entry.location.relative_path();
    if entry.location.volume == "Macintosh HD" {
        PathBuf::from("/").join(rel)
    } else {
        PathBuf::from("/Volumes")
            .join(&entry.location.volume)
            .join(rel)
    }
}

pub fn cache_file(dir: &Path, path: &Path, extension: &str) -> PathBuf {
    let hash = path
        .to_string_lossy()
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
        });
    dir.join(format!("{hash:016x}.{extension}"))
}
