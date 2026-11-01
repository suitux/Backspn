//! Where the separation model's runtime comes from. Windows and Linux link it
//! into the binary; macOS loads a universal library shipped in the app bundle
//! (`Contents/Frameworks`), which needs a newer system than the app itself.

use ort::environment::EnvironmentBuilder;

use crate::Error;

/// Oldest macOS the bundled runtime library runs on.
pub const MIN_MACOS: (isize, isize) = (13, 4);

/// File name of the runtime library on macOS.
#[cfg(target_os = "macos")]
const MACOS_LIBRARY: &str = "libonnxruntime.dylib";

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

#[cfg(target_os = "macos")]
pub(crate) fn builder() -> Result<EnvironmentBuilder, Error> {
    if !supported() {
        return Err(Error::Unsupported);
    }
    ort::init_from(library()).map_err(|e| Error::Runtime(e.to_string()))
}

#[cfg(not(target_os = "macos"))]
#[allow(clippy::unnecessary_wraps, reason = "same signature as on macOS")]
pub(crate) fn builder() -> Result<EnvironmentBuilder, Error> {
    Ok(ort::init())
}

/// `ORT_DYLIB_PATH` when set, else the bundle's `Frameworks` folder, else
/// next to the executable (development builds).
#[cfg(target_os = "macos")]
fn library() -> std::path::PathBuf {
    if let Some(path) = std::env::var_os("ORT_DYLIB_PATH").filter(|p| !p.is_empty()) {
        return path.into();
    }
    let exe = std::env::current_exe().unwrap_or_default();
    let dir = exe.parent().unwrap_or(std::path::Path::new(""));
    let bundled = dir.join("../Frameworks").join(MACOS_LIBRARY);
    if bundled.exists() {
        bundled
    } else {
        dir.join(MACOS_LIBRARY)
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn needs_the_runtime_minimum() {
        assert!(!runs_on(12, 7));
        assert!(!runs_on(13, 3));
        assert!(runs_on(13, 4));
        assert!(runs_on(14, 0));
        assert!(runs_on(26, 0));
    }
}
