use std::path::Path;

use crate::{Error, ImportReport, Library, NodeKind, Result, nml_io};

pub fn m3u(paths: &[String]) -> String {
    let mut out = String::new();
    for p in paths {
        out.push_str(p.split('#').next().unwrap_or(p));
        out.push('\n');
    }
    out
}

pub fn m3u_paths(text: &str, file: &Path) -> Vec<std::path::PathBuf> {
    let dir = file.parent().unwrap_or(Path::new("/"));
    text.lines()
        .map(|l| l.trim_start_matches('\u{feff}').trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            let p = Path::new(l);
            if p.has_root() {
                p.to_path_buf()
            } else {
                dir.join(p)
            }
        })
        .collect()
}

impl Library {
    pub(crate) fn list(&self, node: Option<i64>) -> Result<(String, Vec<i64>)> {
        match node {
            None => Ok((
                "Collection".to_owned(),
                self.tracks()?.iter().map(|t| t.id).collect(),
            )),
            Some(id) => {
                let name = self
                    .nodes()?
                    .into_iter()
                    .find(|n| n.id == id)
                    .map(|n| n.name)
                    .ok_or_else(|| Error::Invalid("no such playlist".into()))?;
                Ok((name, self.node_tracks(id)?))
            }
        }
    }

    pub fn export_list_m3u(&self, node: Option<i64>) -> Result<String> {
        let (_, tracks) = self.list(node)?;
        let paths: Vec<String> = tracks
            .iter()
            .filter_map(|&id| self.track(id).ok().flatten().map(|t| t.path))
            .collect();
        Ok(m3u(&paths))
    }

    pub fn import_m3u(&mut self, file: &Path, parent: Option<i64>) -> Result<i64> {
        let text = std::fs::read(file).map_err(|source| Error::Io {
            path: file.to_owned(),
            source,
        })?;
        let text = String::from_utf8_lossy(&text);
        let name = file.file_stem().map_or_else(
            || "Playlist".to_owned(),
            |s| s.to_string_lossy().into_owned(),
        );
        let name = self.unique_node_name(parent, &name)?;
        let tracks: Vec<i64> = m3u_paths(&text, file)
            .iter()
            .filter_map(|p| self.add_file(p).ok())
            .collect();
        let node = self.create_node(parent, NodeKind::Playlist, &name, None)?;
        self.add_to_playlist(node, &tracks)?;
        Ok(node)
    }

    pub fn import_playlists_nml(
        &mut self,
        file: &Path,
        parent: Option<i64>,
    ) -> Result<ImportReport> {
        let nml = backspn_formats::nml::Nml::from_path(file)?;
        nml_io::merge(&mut self.conn, &nml, parent)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn m3u_lines_and_relative_paths() {
        assert_eq!(
            m3u(&["/m/a.mp3".into(), "/m/b.mp3#1".into()]),
            "/m/a.mp3\n/m/b.mp3\n"
        );
        let text = "\u{feff}#EXTM3U\n/abs/a.mp3\r\nsub/b.mp3\n\n#EXTINF:1,x\n";
        assert_eq!(
            m3u_paths(text, Path::new("/lists/set.m3u")),
            [
                PathBuf::from("/abs/a.mp3"),
                PathBuf::from("/lists/sub/b.mp3")
            ]
        );
    }

    #[test]
    fn playlists_round_trip_through_our_file_and_m3u() {
        let mut lib = Library::open_in_memory().unwrap();
        let a = lib.add_file(Path::new("/m/Ann - A.mp3")).unwrap();
        let b = lib.add_file(Path::new("/m/Bob - B.mp3")).unwrap();
        let list = lib
            .create_node(None, NodeKind::Playlist, "Set", None)
            .unwrap();
        lib.add_to_playlist(list, &[b, a, b]).unwrap();

        let dir = std::env::temp_dir().join(format!("backspn-exchange-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let own = dir.join("Set.backspn");
        std::fs::write(&own, lib.export_list(Some(list), "Set").unwrap()).unwrap();
        let m3u_file = dir.join("Set.m3u");
        std::fs::write(&m3u_file, lib.export_list_m3u(Some(list)).unwrap()).unwrap();

        let mut other = Library::open_in_memory().unwrap();
        let report = other.import_file(&own, None).unwrap();
        assert_eq!((report.tracks, report.playlists), (2, 1));
        let node = other
            .nodes()
            .unwrap()
            .into_iter()
            .find(|n| n.name == "Set")
            .unwrap();
        let titles: Vec<String> = other
            .node_tracks(node.id)
            .unwrap()
            .iter()
            .map(|&id| other.track(id).unwrap().unwrap().title)
            .collect();
        assert_eq!(titles, ["B", "A", "B"]);
        let report = other.import_file(&own, None).unwrap();
        assert_eq!(report.tracks, 0);
        assert!(other.nodes().unwrap().iter().any(|n| n.name == "Set (2)"));

        let id = other.import_m3u(&m3u_file, None).unwrap();
        assert_eq!(other.node_tracks(id).unwrap().len(), 3);
        assert_eq!(
            other.tracks().unwrap().len(),
            2,
            "same files, no new tracks"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn nml_playlists_are_added_to_the_library() {
        let nml =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/nml/small.nml");
        let mut lib = Library::open_in_memory().unwrap();
        let report = lib.import_playlists_nml(&nml, None).unwrap();
        assert_eq!(report.tracks, lib.tracks().unwrap().len());
        assert!(report.playlists > 0);
        let again = lib.import_playlists_nml(&nml, None).unwrap();
        assert_eq!(again.tracks, 0, "tracks reused");
    }
}
