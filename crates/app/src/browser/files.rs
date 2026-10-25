use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant};

use backspn_library::{FileFacts, Node, TrackRow};

use super::dialogs::BrowserAsk;
use super::{Browser, Source};
use crate::modal::Ask;

pub(super) fn audio_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|e| {
                    e.file_type()
                        .is_ok_and(|t| t.is_file() || (t.is_symlink() && e.path().is_file()))
                })
                .map(|e| e.path())
                .filter(|p| !is_hidden(p) && crate::is_audio(p))
                .collect()
        })
        .unwrap_or_default();
    files.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
    files
}

pub(super) fn subfolders(dir: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                .map(|e| e.path())
                .filter(|p| !is_hidden(p))
                .collect()
        })
        .unwrap_or_default();
    dirs.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
    dirs
}

pub(super) fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|n| n.to_string_lossy().starts_with('.'))
}

pub(crate) fn reveal(path: &str) {
    let file = path.split('#').next().unwrap_or(path);
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open")
        .args(["-R", file])
        .spawn();
    #[cfg(windows)]
    let result = std::process::Command::new("explorer")
        .args(["/select,", file])
        .spawn();
    #[cfg(not(any(target_os = "macos", windows)))]
    let result = std::process::Command::new("xdg-open")
        .arg(Path::new(file).parent().unwrap_or(Path::new("/")))
        .spawn();
    if let Err(e) = result {
        log::warn!("cannot show {file}: {e}");
    }
}

pub(super) fn open_file(path: &Path) {
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(path).spawn();
    #[cfg(windows)]
    let result = std::process::Command::new("explorer").arg(path).spawn();
    #[cfg(not(any(target_os = "macos", windows)))]
    let result = std::process::Command::new("xdg-open").arg(path).spawn();
    if let Err(e) = result {
        log::warn!("cannot open {}: {e}", path.display());
    }
}

pub(super) fn is_inside_other_app_data(path: &Path) -> bool {
    path.components()
        .any(|c| c.as_os_str() == backspn_formats::interop::DATA_FOLDER)
}

pub(super) fn remove_files(files: &[PathBuf]) -> Vec<String> {
    let mut problems = Vec::new();
    for file in files {
        let shown = file.display();
        if is_inside_other_app_data(file) {
            problems.push(format!(
                "The track {shown} was not deleted: it is inside the other DJ application's data folder."
            ));
        } else if !file.is_file() {
            problems.push(format!("The track {shown} could not be located!"));
        } else if let Err(e) = std::fs::remove_file(file) {
            log::warn!("cannot delete {shown}: {e}");
            problems.push(format!(
                "The track {shown} could not be deleted! Make sure that the track is not \
                 loaded in a deck or opened in another application."
            ));
        }
    }
    problems
}

const LIBRARY_FILE: &str = backspn_library::library_file::EXTENSION;
const LIBRARY_FILTER: &[(&str, &[&str])] = &[("Backspn library", &[LIBRARY_FILE])];

fn is_library_file(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(LIBRARY_FILE))
}

pub(super) const FOLDER_IMPORT_BATCH: usize = 200;

const IMPORT_REFRESH: Duration = Duration::from_secs(1);

const IMPORT_READERS: usize = 4;

pub(super) struct FolderImport {
    files: Receiver<(PathBuf, FileFacts)>,
    refreshed: Instant,
}

fn read_in_parallel(files: Vec<PathBuf>, tx: &Sender<(PathBuf, FileFacts)>) {
    let queue = std::sync::Arc::new(std::sync::Mutex::new(files));
    let readers: Vec<_> = (0..IMPORT_READERS)
        .map(|_| {
            let queue = std::sync::Arc::clone(&queue);
            let tx = tx.clone();
            std::thread::spawn(move || {
                loop {
                    let next = queue.lock().ok().and_then(|mut q| q.pop());
                    let Some(path) = next else { break };
                    let facts = FileFacts::read(&path);
                    if tx.send((path, facts)).is_err() {
                        break;
                    }
                }
            })
        })
        .collect();
    for r in readers {
        drop(r.join());
    }
}

pub fn library_path() -> PathBuf {
    if let Some(path) = std::env::var_os("BACKSPN_LIBRARY") {
        return PathBuf::from(path);
    }
    crate::dirs::data().join("library.sqlite")
}

impl Browser {
    pub(crate) fn import_nml(&mut self) {
        let documents = crate::dirs::home().join(backspn_formats::interop::DATA_DIR);
        let Some(path) = rfd::FileDialog::new()
            .add_filter("NML collection", &["nml"])
            .set_directory(&documents)
            .pick_file()
        else {
            return;
        };
        if self.rows.is_empty() {
            self.ask_nml_tags(&path, true);
        } else {
            self.ask = Some(
                Ask::new(
                    "Import NML",
                    "Do you want to clear the current collection before importing the new one?\n\n\
                     Yes: the library becomes this collection. No: its tracks and playlists are \
                     added to the library. The NML file itself is only read.",
                )
                .button("Yes", BrowserAsk::NmlReplace(path.clone(), true))
                .button("No", BrowserAsk::NmlReplace(path, false))
                .cancel("Cancel"),
            );
        }
    }

    pub(super) fn ask_nml_tags(&mut self, path: &Path, replace: bool) {
        let path = path.to_path_buf();
        let import = |file_tags| BrowserAsk::ImportNml {
            path: path.clone(),
            replace,
            file_tags,
        };
        self.ask = Some(
            Ask::new(
                "Import NML",
                "The files' tags may not match the collection. Which ones should the import keep?",
            )
            .button("Collection", import(false))
            .button("File", import(true))
            .cancel("Cancel"),
        );
    }

    pub(super) fn import_nml_file(&mut self, path: &Path, replace: bool, file_tags: bool) {
        let before: HashSet<i64> = if replace {
            HashSet::new()
        } else {
            self.rows.iter().map(|r| r.id).collect()
        };
        let Ok(lib) = &mut self.lib else { return };
        let result = if replace {
            lib.import_nml(path)
        } else {
            lib.import_playlists_nml(path, None)
        };
        match result {
            Ok(r) => {
                self.message = Some(format!(
                    "Imported {} tracks, {} playlists, {} smartlists",
                    r.tracks, r.playlists, r.smartlists
                ));
            }
            Err(e) => crate::toast::error(format!("Import failed: {e}")),
        }
        if file_tags {
            self.prefer_file_tags(&before);
        }
        self.source = Source::Collection;
        self.refresh();
    }

    fn prefer_file_tags(&mut self, before: &HashSet<i64>) {
        match self.with_lib_value(|lib| lib.prefer_file_tags(before)) {
            Ok(0) => {}
            Ok(unread) => log::info!("import: {unread} files' tags could not be read"),
            Err(e) => crate::toast::error(e.to_string()),
        }
    }

    pub(crate) fn export_collection(&mut self) {
        let Some(path) = Self::save_path(&format!("collection.{LIBRARY_FILE}"), LIBRARY_FILTER)
        else {
            return;
        };
        let text = self.with_lib_value(|lib| lib.export_collection());
        self.exported(&path, text.map(|t| std::fs::write(&path, t)));
    }

    pub(crate) fn import_collection(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter(LIBRARY_FILTER[0].0, LIBRARY_FILTER[0].1)
            .pick_file()
        else {
            return;
        };
        self.import_library_file(&path, None);
    }

    fn import_library_file(&mut self, path: &Path, parent: Option<i64>) {
        match self.with_lib_value(|lib| lib.import_file(path, parent)) {
            Ok(r) => {
                self.message = Some(format!(
                    "Imported {} new tracks, {} playlists, {} smartlists",
                    r.tracks, r.playlists, r.smartlists
                ));
            }
            Err(e) => crate::toast::error(format!("Import failed: {e}")),
        }
        self.refresh();
    }

    pub(super) fn save_path(file_name: &str, filters: &[(&str, &[&str])]) -> Option<PathBuf> {
        let mut dialog = rfd::FileDialog::new().set_file_name(file_name);
        for (name, extensions) in filters {
            dialog = dialog.add_filter(*name, extensions);
        }
        let path = dialog.save_file()?;
        if is_inside_other_app_data(&path) {
            crate::toast::warning(
                "Refusing to write inside the other DJ application's data folder: export elsewhere and copy it in yourself.",
            );
            return None;
        }
        Some(path)
    }

    pub(super) fn export_list(&mut self, node: Option<&Node>) {
        let name = node.map_or("Collection", |n| n.name.as_str()).to_owned();
        let Some(path) = Self::save_path(
            &format!("{name}.{LIBRARY_FILE}"),
            &[
                ("Backspn playlist", &[LIBRARY_FILE]),
                ("M3U playlist", &["m3u", "m3u8"]),
            ],
        ) else {
            return;
        };
        let m3u = path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("m3u") || e.eq_ignore_ascii_case("m3u8"));
        let id = node.map(|n| n.id);
        let text = self.with_lib_value(|lib| {
            if m3u {
                lib.export_list_m3u(id)
            } else {
                lib.export_list(id, &name)
            }
        });
        self.exported(&path, text.map(|t| std::fs::write(&path, t)));
    }

    fn exported(&mut self, path: &Path, result: backspn_library::Result<std::io::Result<()>>) {
        match result {
            Ok(Ok(())) => self.message = Some(format!("Exported to {}", path.display())),
            Ok(Err(e)) => crate::toast::error(format!("Export failed: {e}")),
            Err(e) => crate::toast::error(format!("Export failed: {e}")),
        }
    }

    pub(super) fn print_list(&mut self) {
        let title = match &self.source {
            Source::Node(id) => self
                .nodes
                .iter()
                .find(|n| n.id == *id)
                .map_or_else(String::new, |n| n.name.clone()),
            _ => "Collection".to_owned(),
        };
        let Some(path) = Self::save_path(&format!("{title}.html"), &[("Web page", &["html"])])
        else {
            return;
        };
        let columns = self.columns.visible();
        let rows: Vec<&TrackRow> = self.view.iter().filter_map(|&id| self.row(id)).collect();
        let html = crate::columns::html_list(&title, &columns, &rows, self.keys);
        match std::fs::write(&path, html) {
            Ok(()) => {
                self.message = Some(format!("Printed to {}", path.display()));
                open_file(&path);
            }
            Err(e) => crate::toast::error(format!("Print failed: {e}")),
        }
    }

    pub(super) fn import_list(&mut self, parent: Option<i64>) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Playlist", &[LIBRARY_FILE, "nml", "m3u", "m3u8"])
            .pick_file()
        else {
            return;
        };
        if is_library_file(&path) {
            self.import_library_file(&path, parent);
            return;
        }
        let nml = path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("nml"));
        let result = self.with_lib_value(|lib| {
            if nml {
                lib.import_playlists_nml(&path, parent).map(|r| {
                    format!(
                        "Imported {} new tracks, {} playlists",
                        r.tracks, r.playlists
                    )
                })
            } else {
                lib.import_m3u(&path, parent)
                    .map(|_| "Imported the playlist".to_owned())
            }
        });
        match result {
            Ok(text) => self.message = Some(text),
            Err(e) => crate::toast::error(format!("Import failed: {e}")),
        }
        self.refresh();
    }

    pub(super) fn export_history(&mut self, session: i64, name: &str) {
        let Some(path) = Self::save_path(
            &format!("{name}.{LIBRARY_FILE}"),
            &[("History", &[LIBRARY_FILE])],
        ) else {
            return;
        };
        let text = self.with_lib_value(|lib| lib.export_history(session));
        self.exported(&path, text.map(|t| std::fs::write(&path, t)));
    }

    pub(super) fn import_histories(&mut self) {
        let Some(files) = rfd::FileDialog::new()
            .add_filter("History", &[LIBRARY_FILE, "nml"])
            .pick_files()
        else {
            return;
        };
        let (mut sessions, mut entries) = (0, 0);
        for file in files {
            let imported = if is_library_file(&file) {
                self.with_lib_value(|lib| lib.import_history_file(&file))
            } else {
                self.with_lib_value(|lib| lib.import_history(&file).map(|(_, n)| n))
            };
            match imported {
                Ok(n) => {
                    sessions += 1;
                    entries += n;
                }
                Err(e) => crate::toast::error(format!("{}: {e}", file.display())),
            }
        }
        if sessions > 0 {
            self.message = Some(format!(
                "Imported {sessions} histories ({entries} tracks played)"
            ));
        }
        self.refresh();
    }

    pub(super) fn delete_selection(&mut self) {
        let tracks: Vec<i64> = self
            .selected_rows()
            .into_iter()
            .map(|(_, id)| id)
            .filter(|&id| id >= 0)
            .collect();
        if tracks.iter().any(|t| self.loaded.contains(t)) {
            crate::toast::warning(
                "The track(s) you attempt to delete are currently loaded into the deck(s). \
                 Please unload them first.",
            );
            return;
        }
        if !tracks.is_empty() {
            self.deleting = Some((tracks, false));
        }
    }

    pub(super) fn delete_tracks(&mut self, tracks: &[i64], from_disk: bool) {
        let files: Vec<PathBuf> = if from_disk {
            tracks
                .iter()
                .filter_map(|&id| self.row(id))
                .map(|t| PathBuf::from(t.path.split('#').next().unwrap_or(&t.path)))
                .collect()
        } else {
            Vec::new()
        };
        self.selection.clear();
        let result = self.with_lib(|lib| lib.delete_tracks(tracks));
        Self::report(result);
        let problems = remove_files(&files);
        if !problems.is_empty() {
            crate::toast::error(problems.join("\n"));
        }
        self.refresh();
    }

    pub fn running_tasks(&self) -> Vec<&'static str> {
        [
            (self.folder_import.is_some(), "Importing music folders"),
            (!self.analysis.is_idle(), "Analyzing tracks"),
            (!self.rescan.is_idle(), "Rescanning tracks"),
            (!self.stem_jobs.is_idle(), "Generating stems"),
            (self.tag_writer.busy(), "Writing tags to files"),
        ]
        .into_iter()
        .filter_map(|(on, name)| on.then_some(name))
        .collect()
    }

    pub fn backup(&self) {
        let Ok(lib) = &self.lib else { return };
        let dir = library_path().with_file_name("Backup");
        match lib.backup_to(&dir, backspn_library::clock::LocalTime::now()) {
            Ok(Some(path)) => log::info!("library backed up to {}", path.display()),
            Ok(None) => {}
            Err(e) => log::warn!("cannot back the library up: {e}"),
        }
    }

    pub fn import_music_folders(&mut self) {
        let folders = self.folders.clone();
        let known: HashSet<String> = self.by_path.keys().cloned().collect();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut found = Vec::new();
            let mut stack = folders;
            while let Some(dir) = stack.pop() {
                stack.extend(subfolders(&dir));
                found.extend(
                    audio_files(&dir)
                        .into_iter()
                        .filter(|p| !known.contains(p.to_string_lossy().as_ref())),
                );
            }
            read_in_parallel(found, &tx);
        });
        self.folder_import = Some(FolderImport {
            files: rx,
            refreshed: Instant::now(),
        });
    }

    pub(super) fn continue_folder_import(&mut self) {
        let Some(import) = &mut self.folder_import else {
            return;
        };
        let mut batch = Vec::new();
        let finished = loop {
            match import.files.try_recv() {
                Ok(file) => {
                    batch.push(file);
                    if batch.len() >= FOLDER_IMPORT_BATCH {
                        break false;
                    }
                }
                Err(TryRecvError::Empty) => break false,
                Err(TryRecvError::Disconnected) => break true,
            }
        };
        let added: Vec<i64> = match &mut self.lib {
            Ok(lib) if !batch.is_empty() => lib
                .add_read_files(batch)
                .map(|ids| ids.into_iter().flatten().collect())
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        let refresh =
            finished || (!added.is_empty() && import.refreshed.elapsed() >= IMPORT_REFRESH);
        if refresh {
            import.refreshed = Instant::now();
        }
        if finished {
            self.folder_import = None;
            self.message = Some("Music folders imported".to_owned());
        }
        if refresh {
            self.refresh();
        }
        self.analyse_imported(&added);
    }

    pub(super) fn list_folder(&mut self, dir: &Path) -> Vec<i64> {
        self.external.clear();
        self.external_origin.clear();
        let mut ids = Vec::new();
        for path in audio_files(dir) {
            let text = path.to_string_lossy().into_owned();
            if let Some(&id) = self.by_path.get(&text) {
                ids.push(id);
                continue;
            }
            let (artist, title) = backspn_library::paths::names_from_file_name(&path);
            let id = -(self.external.len() as i64) - 1;
            self.external.push(TrackRow {
                id,
                path: text,
                title,
                artist,
                ..TrackRow::default()
            });
            ids.push(id);
        }
        ids
    }

    pub fn add_files(&mut self, files: &[PathBuf]) -> Vec<i64> {
        let Ok(lib) = &mut self.lib else {
            return Vec::new();
        };
        let ids: Vec<i64> = files
            .iter()
            .filter(|f| crate::is_audio(f))
            .filter_map(|f| lib.add_file(f).ok())
            .collect();
        self.message = Some(format!("Added {} file(s)", ids.len()));
        self.refresh();
        ids
    }

    pub(crate) fn add_files_keyed(&mut self, files: &[PathBuf]) -> Vec<Option<i64>> {
        let Ok(lib) = &mut self.lib else {
            return vec![None; files.len()];
        };
        let ids = files
            .iter()
            .map(|f| crate::is_audio(f).then(|| lib.add_file(f).ok()).flatten())
            .collect();
        self.refresh();
        ids
    }

    pub fn add_sample(
        &mut self,
        path: &PathBuf,
        bpm: f64,
        duration_s: f64,
        grid_anchor_s: Option<f64>,
    ) -> Option<i64> {
        let id = self
            .add_files(std::slice::from_ref(path))
            .first()
            .copied()?;
        let result = self.with_lib(|lib| {
            lib.set_sample_type(id, Some(0))?;
            lib.save_analysis(
                id,
                &backspn_library::AnalysisUpdate {
                    bpm: Some(bpm),
                    grid_anchor_s,
                    duration_s: Some(duration_s),
                    ..Default::default()
                },
            )?;
            if grid_anchor_s.is_some() {
                lib.set_locked(&[id], true)?;
            }
            let loops = lib.loops_list()?;
            lib.add_to_playlist(loops, &[id])
        });
        Self::report(result);
        self.refresh();
        if self.source == Source::Loops {
            self.recompute();
        }
        Some(id)
    }

    pub fn add_recordings(&mut self, files: &[PathBuf]) {
        let ids = self.add_files(files);
        if let Some(list) = self.recordings {
            let result = self.with_lib(|lib| lib.add_to_playlist(list, &ids));
            Self::report(result);
        }
        if self.source == Source::Recordings {
            self.recompute();
        }
    }

    pub(super) fn purge_recordings(&mut self) {
        if self.selection.len() > 0 {
            self.ask = Some(Ask::yes_no(
                "Delete",
                "Attention: The selected file will be PURGED! Do you really want to delete the \
                 selected tracks?",
                BrowserAsk::PurgeRecordings,
            ));
        }
    }

    pub(super) fn delete_recordings(&mut self) {
        let Some(list) = self.recordings else { return };
        let files: Vec<PathBuf> = self
            .selected_rows()
            .iter()
            .filter_map(|&(_, id)| self.row(id))
            .map(|t| PathBuf::from(&t.path))
            .collect();
        let entries = self.selected_entries();
        self.selection.clear();
        let result = self.with_lib(|lib| lib.remove_entries(list, &entries));
        Self::report(result);
        let problems = remove_files(&files);
        if !problems.is_empty() {
            crate::toast::error(problems.join("\n"));
        }
        self.recompute();
    }

    pub(super) fn change_cover(&mut self, import: bool) {
        let tracks = self.resolve_selection();
        if tracks.is_empty() {
            return;
        }
        let cover = if import {
            let Some(file) = rfd::FileDialog::new()
                .add_filter("Picture", &["jpg", "jpeg", "png"])
                .pick_file()
            else {
                return;
            };
            match crate::covers::store(&crate::covers::store_dir(), &file) {
                Ok(name) => name,
                Err(e) => {
                    crate::toast::error(e);
                    return;
                }
            }
        } else {
            String::new()
        };
        self.set_cover(&tracks, &cover);
    }

    pub(super) fn set_cover(&mut self, tracks: &[i64], cover: &str) {
        let result = self.with_lib(|lib| lib.set_cover(tracks, Some(cover)));
        Self::report(result);
        for &id in tracks {
            self.refresh_track(id);
        }
        self.suggestions.forget_covers(tracks);
        if let Some(covers) = self.covers.get_mut() {
            covers.forget(tracks);
        }
    }
}
