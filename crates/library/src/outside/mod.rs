pub mod engine;
pub mod serato;
pub mod virtualdj;

use std::path::PathBuf;

pub fn drives() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if cfg!(windows) {
        for letter in b'A'..=b'Z' {
            let root = PathBuf::from(format!("{}:\\", char::from(letter)));
            if root.is_dir() {
                out.push(root);
            }
        }
        return out;
    }
    let user = std::env::var("USER").unwrap_or_default();
    let parents = if cfg!(target_os = "macos") {
        vec![PathBuf::from("/Volumes")]
    } else {
        vec![
            PathBuf::from("/media").join(&user),
            PathBuf::from("/run/media").join(&user),
        ]
    };
    for parent in parents {
        if let Ok(entries) = std::fs::read_dir(&parent) {
            out.extend(
                entries
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.is_dir() && !p.is_symlink()),
            );
        }
    }
    out
}
