use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::target::Target;

const STAGING: &str = ".Backspn-update";
const OLD_BUNDLE: &str = ".Backspn.app.old";
const WAIT: &str = r#"while kill -0 "$1" 2>/dev/null; do sleep 0.2; done"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Staged {
    target: Target,
    file: PathBuf,
}

fn run(program: &str, args: &[&std::ffi::OsStr]) -> Result<(), String> {
    let status = Command::new(program)
        .args(args)
        .status()
        .map_err(|e| format!("cannot run {program}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} failed ({status})"))
    }
}

fn side(path: &Path, name: &str) -> PathBuf {
    path.with_file_name(name)
}

pub fn stage(target: &Target, download: &Path) -> Result<Staged, String> {
    let file = match target {
        Target::Bundle(app) => {
            let staging = side(app, STAGING);
            let _ = std::fs::remove_dir_all(&staging);
            std::fs::create_dir_all(&staging)
                .map_err(|e| format!("cannot write next to {}: {e}", app.display()))?;
            run(
                "/usr/bin/tar",
                &[
                    "-xzf".as_ref(),
                    download.as_os_str(),
                    "-C".as_ref(),
                    staging.as_os_str(),
                ],
            )?;
            let new = staging.join("Backspn.app");
            run(
                "/usr/bin/codesign",
                &[
                    "--verify".as_ref(),
                    "--deep".as_ref(),
                    "--strict".as_ref(),
                    new.as_os_str(),
                ],
            )?;
            new
        }
        Target::AppImage(path) => {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            let new = side(path, &format!(".{name}.new"));
            std::fs::copy(download, &new)
                .map_err(|e| format!("cannot write next to {}: {e}", path.display()))?;
            executable(&new)?;
            new
        }
        Target::Nsis(_) | Target::Deb(_) => download.to_owned(),
        Target::Unsupported(why) => return Err((*why).to_owned()),
    };
    Ok(Staged {
        target: target.clone(),
        file,
    })
}

#[cfg(unix)]
fn executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())
}

#[cfg(not(unix))]
fn executable(_: &Path) -> Result<(), String> {
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Launch {
    program: PathBuf,
    args: Vec<OsString>,
}

fn shell(script: &str, pid: u32, paths: &[&Path]) -> Launch {
    let mut args: Vec<OsString> = vec![
        "-c".into(),
        script.into(),
        "sh".into(),
        pid.to_string().into(),
    ];
    args.extend(paths.iter().map(|p| p.as_os_str().to_owned()));
    Launch {
        program: "/bin/sh".into(),
        args,
    }
}

fn after_exit(staged: &Staged, relaunch: bool, pid: u32) -> Option<Launch> {
    let script = |first: String, start: &str| {
        if relaunch {
            format!("{first}; {start}")
        } else {
            first
        }
    };
    Some(match &staged.target {
        Target::Bundle(app) => shell(
            &script(format!(r#"{WAIT}; rm -rf "$2" "$3""#), r#"open "$4""#),
            pid,
            &[&side(app, OLD_BUNDLE), &side(app, STAGING), app],
        ),
        Target::AppImage(path) => shell(&script(WAIT.to_owned(), r#"exec "$2""#), pid, &[path]),
        Target::Deb(exe) => shell(
            &script(
                format!(r#"{WAIT}; pkexec apt-get install -y "$2""#),
                r#"exec "$3""#,
            ),
            pid,
            &[&staged.file, exe],
        ),
        Target::Nsis(_) => {
            let mut args: Vec<OsString> = vec!["/P".into(), "/NS".into()];
            if relaunch {
                args.push("/R".into());
            }
            Launch {
                program: staged.file.clone(),
                args,
            }
        }
        Target::Unsupported(_) => return None,
    })
}

fn swap(staged: &Staged) -> Result<(), String> {
    match &staged.target {
        Target::Bundle(app) => {
            let old = side(app, OLD_BUNDLE);
            let _ = std::fs::remove_dir_all(&old);
            std::fs::rename(app, &old).map_err(|e| e.to_string())?;
            if let Err(e) = std::fs::rename(&staged.file, app) {
                let _ = std::fs::rename(&old, app);
                return Err(e.to_string());
            }
            Ok(())
        }
        Target::AppImage(path) => std::fs::rename(&staged.file, path).map_err(|e| e.to_string()),
        Target::Nsis(_) | Target::Deb(_) | Target::Unsupported(_) => Ok(()),
    }
}

pub fn install(staged: &Staged, relaunch: bool) -> Result<(), String> {
    swap(staged)?;
    let Some(launch) = after_exit(staged, relaunch, std::process::id()) else {
        return Ok(());
    };
    Command::new(&launch.program)
        .args(&launch.args)
        .spawn()
        .map(drop)
        .map_err(|e| format!("cannot start {}: {e}", launch.program.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn staged(target: Target, file: &str) -> Staged {
        Staged {
            target,
            file: file.into(),
        }
    }

    fn args(launch: &Launch) -> Vec<String> {
        launch
            .args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn the_bundle_cleans_up_after_quitting_and_opens_if_asked() {
        let s = staged(
            Target::Bundle("/Applications/Backspn.app".into()),
            "/Applications/.Backspn-update/Backspn.app",
        );
        let l = after_exit(&s, true, 42).unwrap();
        assert_eq!(l.program, Path::new("/bin/sh"));
        assert_eq!(
            args(&l)[1..],
            [
                format!(r#"{WAIT}; rm -rf "$2" "$3"; open "$4""#),
                "sh".into(),
                "42".into(),
                "/Applications/.Backspn.app.old".into(),
                "/Applications/.Backspn-update".into(),
                "/Applications/Backspn.app".into(),
            ]
        );
        let quiet = after_exit(&s, false, 42).unwrap();
        assert_eq!(args(&quiet)[1], format!(r#"{WAIT}; rm -rf "$2" "$3""#));
    }

    #[test]
    fn linux_waits_for_the_app_to_quit() {
        let image = staged(
            Target::AppImage("/opt/Backspn.AppImage".into()),
            "/opt/.Backspn.AppImage.new",
        );
        assert_eq!(
            args(&after_exit(&image, true, 7).unwrap())[1..],
            [
                format!(r#"{WAIT}; exec "$2""#),
                "sh".into(),
                "7".into(),
                "/opt/Backspn.AppImage".into()
            ]
        );
        assert_eq!(args(&after_exit(&image, false, 7).unwrap())[1], WAIT);
        let deb = staged(
            Target::Deb("/usr/bin/Backspn".into()),
            "/data/Updates/b.deb",
        );
        assert_eq!(
            args(&after_exit(&deb, true, 7).unwrap())[1..],
            [
                format!(r#"{WAIT}; pkexec apt-get install -y "$2"; exec "$3""#),
                "sh".into(),
                "7".into(),
                "/data/Updates/b.deb".into(),
                "/usr/bin/Backspn".into(),
            ]
        );
    }

    #[test]
    fn the_windows_installer_runs_with_a_progress_bar_only() {
        let s = staged(
            Target::Nsis(r"C:\Backspn\Backspn.exe".into()),
            r"C:\Updates\setup.exe",
        );
        let l = after_exit(&s, true, 1).unwrap();
        assert_eq!(l.program, Path::new(r"C:\Updates\setup.exe"));
        assert_eq!(args(&l), ["/P", "/NS", "/R"]);
        assert_eq!(args(&after_exit(&s, false, 1).unwrap()), ["/P", "/NS"]);
        assert_eq!(
            after_exit(&staged(Target::Unsupported("x"), ""), true, 1),
            None
        );
    }

    #[test]
    fn an_appimage_swaps_in_place() {
        let dir = std::env::temp_dir().join(format!("backspn-update-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let current = dir.join("Backspn.AppImage");
        let download = dir.join("download");
        std::fs::write(&current, b"old").unwrap();
        std::fs::write(&download, b"new").unwrap();
        let s = stage(&Target::AppImage(current.clone()), &download).unwrap();
        assert_eq!(s.file, dir.join(".Backspn.AppImage.new"));
        swap(&s).unwrap();
        assert_eq!(std::fs::read(&current).unwrap(), b"new");
        assert!(!s.file.exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
