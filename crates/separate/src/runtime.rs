//! Where the separation model's runtime comes from: a library shipped with
//! the app and loaded the first time stems are generated, never at start-up,
//! so the app runs on processors and systems the runtime can't. It lives in
//! `Contents/Frameworks` on macOS (where it needs a newer system than the app
//! itself), next to `Backspn.exe` on Windows and in `/usr/lib/Backspn` on Linux.

use std::path::{Path, PathBuf};

use ort::environment::EnvironmentBuilder;

use crate::Error;

/// Oldest macOS the bundled runtime library runs on.
pub const MIN_MACOS: (isize, isize) = (13, 4);

/// File name of the runtime library.
#[cfg(target_os = "macos")]
const LIBRARY: &str = "libonnxruntime.dylib";
#[cfg(windows)]
const LIBRARY: &str = "onnxruntime.dll";
#[cfg(all(unix, not(target_os = "macos")))]
const LIBRARY: &str = "libonnxruntime.so";

/// Where the installed app keeps the library, from the executable's folder.
#[cfg(target_os = "macos")]
const INSTALLED: &str = "../Frameworks";
#[cfg(windows)]
const INSTALLED: &str = ".";
#[cfg(all(unix, not(target_os = "macos")))]
const INSTALLED: &str = "../lib/Backspn";

/// Whether this system can generate stems.
#[cfg(target_os = "macos")]
pub fn supported() -> bool {
    let v = objc2_foundation::NSProcessInfo::processInfo().operatingSystemVersion();
    runs_on(v.majorVersion, v.minorVersion)
}

/// Whether this system can generate stems.
#[cfg(not(target_os = "macos"))]
pub fn supported() -> bool {
    true
}

#[cfg(target_os = "macos")]
fn runs_on(major: isize, minor: isize) -> bool {
    (major, minor) >= MIN_MACOS
}

pub(crate) fn builder() -> Result<EnvironmentBuilder, Error> {
    if !supported() {
        return Err(Error::Unsupported);
    }
    ort::init_from(library()).map_err(|e| Error::Runtime(e.to_string()))
}

/// `ORT_DYLIB_PATH` when set, else where the installed app keeps it.
fn library() -> PathBuf {
    if let Some(path) = std::env::var_os("ORT_DYLIB_PATH").filter(|p| !p.is_empty()) {
        return path.into();
    }
    let exe = std::env::current_exe().unwrap_or_default();
    library_in(exe.parent().unwrap_or(Path::new("")), Path::exists)
}

/// The installed app's library, else the one next to the executable
/// (development builds).
fn library_in(exe_dir: &Path, exists: impl Fn(&Path) -> bool) -> PathBuf {
    let installed = exe_dir.join(INSTALLED).join(LIBRARY);
    if exists(&installed) {
        installed
    } else {
        exe_dir.join(LIBRARY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_installed_library_first() {
        let dir = Path::new("/app/bin");
        let installed = dir.join(INSTALLED).join(LIBRARY);
        assert_eq!(library_in(dir, |p| p == installed), installed);
        assert_eq!(library_in(dir, |_| false), dir.join(LIBRARY));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn needs_the_runtime_minimum() {
        assert!(!runs_on(12, 7));
        assert!(!runs_on(13, 3));
        assert!(runs_on(13, 4));
        assert!(runs_on(14, 0));
        assert!(runs_on(26, 0));
    }
}
