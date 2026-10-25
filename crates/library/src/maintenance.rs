use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::params;

use crate::{Error, Library, Result};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConsistencyReport {
    pub total: usize,
    pub missing: Vec<i64>,
    pub not_analyzed: Vec<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RelocateReport {
    pub found: usize,
    pub ambiguous: Vec<i64>,
    pub duplicates: Vec<i64>,
    pub missing: Vec<i64>,
}

fn file_of(path: &str) -> &Path {
    Path::new(path.split('#').next().unwrap_or(path))
}

fn is_local(path: &str) -> bool {
    crate::paths::local_file(path).is_some()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Found {
    At(PathBuf),
    Ambiguous,
    Nowhere,
}

#[derive(Debug, Clone, Default)]
pub struct FolderIndex(HashMap<String, Vec<PathBuf>>);

impl FolderIndex {
    pub fn scan(root: &Path) -> Self {
        let mut index: HashMap<String, Vec<PathBuf>> = HashMap::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.filter_map(std::result::Result::ok) {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with('.') {
                    continue;
                }
                match entry.file_type() {
                    Ok(t) if t.is_dir() => stack.push(entry.path()),
                    Ok(t) if t.is_file() => index.entry(name).or_default().push(entry.path()),
                    _ => {}
                }
            }
        }
        Self(index)
    }

    pub fn find(&self, old: &Path) -> Found {
        let Some(candidates) = old
            .file_name()
            .and_then(|n| self.0.get(n.to_string_lossy().as_ref()))
        else {
            return Found::Nowhere;
        };
        let tail = |p: &Path| -> usize {
            old.components()
                .rev()
                .zip(p.components().rev())
                .take_while(|(a, b)| a == b)
                .count()
        };
        let mut scored: Vec<(usize, &PathBuf)> = candidates.iter().map(|p| (tail(p), p)).collect();
        scored.sort_by_key(|s| std::cmp::Reverse(s.0));
        match scored.as_slice() {
            [] => Found::Nowhere,
            [(_, only)] => Found::At((*only).clone()),
            [(a, best), (b, _), ..] if a > b => Found::At((*best).clone()),
            _ => Found::Ambiguous,
        }
    }
}

pub const BACKUPS_KEPT: usize = 50;

impl Library {
    pub fn backup_to(&self, dir: &Path, now: crate::clock::LocalTime) -> Result<Option<PathBuf>> {
        std::fs::create_dir_all(dir).map_err(|source| Error::Io {
            path: dir.to_owned(),
            source,
        })?;
        let name = now.history_name().replacen("history", "library", 1);
        let path = dir.join(format!("{name}.sqlite"));
        if path.exists() {
            return Ok(None);
        }
        self.conn
            .execute("VACUUM INTO ?1", [path.to_string_lossy()])?;
        let mut backups: Vec<PathBuf> = std::fs::read_dir(dir)
            .map(|e| {
                e.filter_map(std::result::Result::ok)
                    .map(|e| e.path())
                    .filter(|p| {
                        p.file_name()
                            .is_some_and(|n| n.to_string_lossy().starts_with("library_"))
                            && p.extension().is_some_and(|e| e == "sqlite")
                    })
                    .collect()
            })
            .unwrap_or_default();
        backups.sort();
        backups.reverse();
        if let Some(previous) = backups.iter().find(|p| **p != path)
            && std::fs::read(previous).ok() == std::fs::read(&path).ok()
        {
            drop(std::fs::remove_file(&path));
            return Ok(None);
        }
        for old in backups.iter().skip(BACKUPS_KEPT) {
            drop(std::fs::remove_file(old));
        }
        Ok(Some(path))
    }

    pub fn clear_collection(&mut self) -> Result<()> {
        let all: Vec<i64> = self.tracks()?.iter().map(|t| t.id).collect();
        self.delete_tracks(&all)
    }

    pub fn consistency_report(&self) -> Result<ConsistencyReport> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, path, analyzed FROM tracks ORDER BY id")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, bool>(2)?,
            ))
        })?;
        let mut report = ConsistencyReport::default();
        for row in rows {
            let (id, path, analyzed) = row?;
            report.total += 1;
            if is_local(&path) && !file_of(&path).is_file() {
                report.missing.push(id);
            }
            if !analyzed {
                report.not_analyzed.push(id);
            }
        }
        Ok(report)
    }

    pub fn relocate(&mut self, tracks: &[i64], root: &Path) -> Result<RelocateReport> {
        let index = FolderIndex::scan(root);
        let mut report = RelocateReport::default();
        for &id in tracks {
            let Some(track) = self.track(id)? else {
                continue;
            };
            match index.find(file_of(&track.path)) {
                Found::At(new) => {
                    let text = new.to_string_lossy().into_owned();
                    if self.track_by_path(&text)?.is_some() {
                        report.duplicates.push(id);
                    } else {
                        self.set_track_path(id, &text)?;
                        report.found += 1;
                    }
                }
                Found::Nowhere => report.missing.push(id),
                Found::Ambiguous => report.ambiguous.push(id),
            }
        }
        Ok(report)
    }

    pub fn set_track_path(&mut self, id: i64, path: &str) -> Result<()> {
        let size = std::fs::metadata(path)
            .ok()
            .and_then(|m| i64::try_from(m.len() / 1024).ok());
        self.conn.execute(
            "UPDATE tracks SET path = ?2, filesize_kib = COALESCE(?3, filesize_kib), dirty = 1 WHERE id = ?1",
            params![id, path, size],
        )?;
        Ok(())
    }

    pub fn delete_tracks(&mut self, tracks: &[i64]) -> Result<()> {
        let tx = self.conn.transaction()?;
        for id in tracks {
            tx.execute("DELETE FROM playlist_entries WHERE track_id = ?1", [id])?;
            tx.execute("DELETE FROM history_entries WHERE track_id = ?1", [id])?;
            tx.execute("DELETE FROM tracks WHERE id = ?1", [id])?;
        }
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NodeKind;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("backspn-maint-{name}-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&dir));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"x").unwrap();
    }

    #[test]
    fn relocation_prefers_the_most_folders_in_common() {
        let root = scratch("pick");
        touch(&root.join("House/a.mp3"));
        touch(&root.join("Techno/a.mp3"));
        touch(&root.join("Other/b.mp3"));
        let index = FolderIndex::scan(&root);
        let found = |p: &str| index.find(Path::new(p));
        assert_eq!(
            found("/Volumes/Old/Music/Techno/a.mp3"),
            Found::At(root.join("Techno/a.mp3"))
        );
        assert_eq!(
            found("/Volumes/Old/Music/Trance/a.mp3"),
            Found::Ambiguous,
            "two equally likely"
        );
        assert_eq!(found("/x/b.mp3"), Found::At(root.join("Other/b.mp3")));
        assert_eq!(found("/x/none.mp3"), Found::Nowhere);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn backups_skip_repeats_and_keep_the_newest() {
        let dir = scratch("backup");
        let mut lib = Library::open_in_memory().unwrap();
        lib.add_file(Path::new("/m/A.mp3")).unwrap();
        let at = |s: u32| crate::clock::LocalTime {
            year: 2026,
            month: 10,
            day: 17,
            seconds: s,
        };
        let first = lib.backup_to(&dir, at(10)).unwrap();
        assert!(
            first
                .as_ref()
                .is_some_and(|p| p.ends_with("library_2026y10m17d_00h00m10s.sqlite"))
        );
        assert_eq!(lib.backup_to(&dir, at(20)).unwrap(), None, "same content");
        lib.add_file(Path::new("/m/B.mp3")).unwrap();
        assert!(lib.backup_to(&dir, at(30)).unwrap().is_some());
        let restored = Library::open(first.unwrap()).unwrap();
        assert_eq!(restored.tracks().unwrap().len(), 1, "a usable library");
        lib.clear_collection().unwrap();
        assert_eq!(lib.tracks().unwrap(), []);
        drop(restored);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn report_relocate_and_delete() {
        let root = scratch("lib");
        let here = root.join("Here/Kept.mp3");
        touch(&here);
        let mut lib = Library::open_in_memory().unwrap();
        let kept = lib.add_file(&here).unwrap();
        let gone = lib
            .add_file(Path::new("/Volumes/Gone/Set/Moved.mp3"))
            .unwrap();
        let lost = lib.add_file(Path::new("/Volumes/Gone/Lost.mp3")).unwrap();
        let report = lib.consistency_report().unwrap();
        assert_eq!(report.total, 3);
        assert_eq!(report.missing, [gone, lost]);
        assert_eq!(report.not_analyzed.len(), 3);

        touch(&root.join("New/Set/Moved.mp3"));
        let r = lib.relocate(&report.missing, &root).unwrap();
        assert_eq!((r.found, r.missing.as_slice()), (1, [lost].as_slice()));
        assert!(Path::new(&lib.track(gone).unwrap().unwrap().path).ends_with("New/Set/Moved.mp3"));

        let list = lib
            .create_node(None, NodeKind::Playlist, "Set", None)
            .unwrap();
        lib.add_to_playlist(list, &[kept, lost]).unwrap();
        lib.delete_tracks(&[lost]).unwrap();
        assert_eq!(lib.node_tracks(list).unwrap(), [kept]);
        assert!(lib.track(lost).unwrap().is_none());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
