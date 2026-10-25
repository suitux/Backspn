use std::path::Path;
use std::time::UNIX_EPOCH;

use crate::tags::{self, FileTags};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FileScan {
    pub tags: FileTags,
    pub size_kib: Option<u64>,
    pub mtime: Option<i64>,
}

pub fn file_mtime(path: &Path) -> Option<i64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    i64::try_from(modified.duration_since(UNIX_EPOCH).ok()?.as_secs()).ok()
}

pub fn read(path: &Path) -> Option<FileScan> {
    let tags = tags::read(path)?;
    let meta = std::fs::metadata(path).ok();
    Some(FileScan {
        tags,
        size_kib: meta.map(|m| m.len() / 1024),
        mtime: file_mtime(path),
    })
}

pub fn changed(stored: Option<i64>, current: Option<i64>) -> bool {
    stored.is_none() || stored != current
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_files_changed_since_their_last_read_count() {
        assert!(changed(None, Some(10)));
        assert!(changed(Some(10), Some(11)));
        assert!(changed(Some(10), None));
        assert!(!changed(Some(10), Some(10)));
    }
}
