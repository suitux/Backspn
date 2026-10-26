use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};

use backspn_library::TrackRow;
use backspn_library::external::{ExternalLibrary, ExternalNode, ExternalTrack};
use eframe::egui::{self, RichText, Ui};

use super::dialogs::BrowserAsk;
use super::{Browser, BrowserAction, Source, SourceDrag};
use crate::i18n::tr;
use crate::modal::Ask;
use crate::widgets;

#[cfg(not(target_os = "macos"))]
pub(crate) const LIBRARY_XML_HINT: &str = "Shows the library XML another DJ program exports (DJ_PLAYLISTS) in the tree, to load and import from";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Outside {
    Xml,
    Music,
    Program(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Program {
    Serato,
    VirtualDj,
    EngineDj,
}

fn outside_key(kind: Outside) -> &'static str {
    match kind {
        Outside::Xml => "library-xml",
        Outside::Music => "apple-music",
        Outside::Program(_) => "dj-program",
    }
}

impl Program {
    const ALL: [Self; 3] = [Self::Serato, Self::VirtualDj, Self::EngineDj];

    fn name(self) -> &'static str {
        match self {
            Self::Serato => "Serato",
            Self::VirtualDj => "VirtualDJ",
            Self::EngineDj => "Engine DJ",
        }
    }

    fn find(self) -> Vec<PathBuf> {
        use backspn_library::outside::{engine, serato, virtualdj};
        match self {
            Self::Serato => serato::find(),
            Self::VirtualDj => virtualdj::find(),
            Self::EngineDj => engine::find(),
        }
    }

    fn read(self, root: &std::path::Path) -> Result<ExternalLibrary, String> {
        use backspn_library::outside::{engine, serato, virtualdj};
        match self {
            Self::Serato => serato::read(root),
            Self::VirtualDj => virtualdj::read(root),
            Self::EngineDj => engine::read(root),
        }
        .map_err(|e| e.to_string())
    }
}

fn drive_name(root: &std::path::Path) -> Option<String> {
    drive_name_in(
        root,
        &crate::dirs::home(),
        &backspn_library::outside::drives(),
    )
}

fn drive_name_in(
    root: &std::path::Path,
    home: &std::path::Path,
    drives: &[PathBuf],
) -> Option<String> {
    if root.starts_with(home) {
        return None;
    }
    drives.iter().find(|d| root.starts_with(d)).map(|d| {
        d.file_name().map_or_else(
            || d.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        )
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MusicSource {
    Api,
    Xml(PathBuf),
}

pub(super) struct OutsideLibrary {
    kind: Outside,
    pub path: Option<PathBuf>,
    music: Option<MusicSource>,
    program: Option<Program>,
    state: State,
}

enum State {
    Idle,
    Loading(Receiver<Result<ExternalLibrary, String>>),
    Loaded(ExternalLibrary),
    Failed(String),
}

impl OutsideLibrary {
    fn xml(path: PathBuf) -> Self {
        let mut lib = Self {
            kind: Outside::Xml,
            path: Some(path),
            music: None,
            program: None,
            state: State::Idle,
        };
        lib.load();
        lib
    }

    fn music(source: MusicSource) -> Self {
        Self {
            kind: Outside::Music,
            path: match &source {
                MusicSource::Xml(p) => Some(p.clone()),
                MusicSource::Api => None,
            },
            music: Some(source),
            program: None,
            state: State::Idle,
        }
    }

    fn program(index: usize, program: Program, root: PathBuf) -> Self {
        Self {
            kind: Outside::Program(index),
            path: Some(root),
            music: None,
            program: Some(program),
            state: State::Idle,
        }
    }

    fn load(&mut self) {
        let (tx, rx) = mpsc::channel();
        let path = self.path.clone();
        let music = self.music.clone();
        let program = self.program;
        std::thread::spawn(move || {
            let read = match (music, path) {
                (_, Some(root)) if program.is_some() => {
                    program.map_or_else(|| Err(String::new()), |p| p.read(&root))
                }
                (None, Some(file)) => backspn_formats::dj_playlists::read(&file)
                    .map(|lib| ExternalLibrary::from_dj_playlists(&lib))
                    .map_err(|e| e.to_string()),
                (Some(MusicSource::Xml(file)), _) => backspn_formats::music_library::read(&file)
                    .map(|lib| ExternalLibrary::from_music(&lib))
                    .map_err(|e| e.to_string()),
                (Some(MusicSource::Api), _) => read_music_api(),
                (None, None) => Err("No file".to_owned()),
            };
            tx.send(read).ok();
        });
        self.state = State::Loading(rx);
    }

    fn poll(&mut self) -> bool {
        if let State::Loading(rx) = &self.state
            && let Ok(read) = rx.try_recv()
        {
            self.state = match read {
                Ok(lib) if self.kind == Outside::Music && lib.tracks.is_empty() => {
                    State::Failed("The music library is empty!".to_owned())
                }
                Ok(lib) => State::Loaded(lib),
                Err(e) => State::Failed(e),
            };
            return true;
        }
        false
    }

    fn library(&self) -> Option<&ExternalLibrary> {
        match &self.state {
            State::Loaded(lib) => Some(lib),
            _ => None,
        }
    }

    fn label(&self) -> String {
        match (self.kind, &self.path) {
            (Outside::Music, _) => tr("Music").to_owned(),
            (Outside::Xml, Some(path)) => path.file_stem().map_or_else(
                || path.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            ),
            (Outside::Xml | Outside::Program(_), None) => String::new(),
            (Outside::Program(_), Some(root)) => {
                let name = self.program.map_or("", Program::name);
                drive_name(root).map_or_else(|| name.to_owned(), |d| format!("{name} — {d}"))
            }
        }
    }

    fn status(&self) -> Option<String> {
        match &self.state {
            State::Loading(_) => Some(tr("(loading)").to_owned()),
            State::Failed(e) => Some(e.clone()),
            State::Idle | State::Loaded(_) => None,
        }
    }
}

#[cfg(target_os = "macos")]
fn read_music_api() -> Result<ExternalLibrary, String> {
    crate::music_api::read().map(|lib| ExternalLibrary::from_music(&lib))
}

#[cfg(not(target_os = "macos"))]
fn read_music_api() -> Result<ExternalLibrary, String> {
    Err("The music library can only be read from its XML file here".to_owned())
}

fn with_marks(program: Option<Program>, track: &ExternalTrack) -> ExternalTrack {
    let mut track = track.clone();
    if program == Some(Program::Serato)
        && track.cues.is_empty()
        && track.grid.is_none()
        && let Some((cues, grid)) =
            backspn_library::outside::serato::file_marks(std::path::Path::new(&track.path))
    {
        track.cues = cues;
        track.grid = grid;
    }
    track
}

fn node_at<'a>(lib: &'a ExternalLibrary, at: &[usize]) -> Option<&'a ExternalNode> {
    let (first, rest) = at.split_first()?;
    let mut node = lib.playlists.get(*first)?;
    for &i in rest {
        let ExternalNode::Folder { children, .. } = node else {
            return None;
        };
        node = children.get(i)?;
    }
    Some(node)
}

fn tracks_at(lib: &ExternalLibrary, at: &[usize]) -> Vec<usize> {
    if at.is_empty() {
        (0..lib.tracks.len()).collect()
    } else {
        node_at(lib, at)
            .map(ExternalNode::all_tracks)
            .unwrap_or_default()
    }
}

fn row(id: i64, t: &ExternalTrack) -> TrackRow {
    TrackRow {
        id,
        path: t.path.clone(),
        title: t.title.clone(),
        artist: t.artist.clone(),
        album: t.album.clone(),
        genre: t.genre.clone(),
        label: t.label.clone(),
        comment: t.comment.clone(),
        key_text: t.key_text.clone(),
        musical_key: t.musical_key,
        bpm: t.bpm.or(t.grid.as_ref().map(|g| g.bpm)),
        duration_s: t.duration_s,
        rating: u8::try_from(t.rating).unwrap_or(0),
        playcount: u32::try_from(t.playcount).unwrap_or(0),
        import_date: t.import_date.clone(),
        last_played: t.last_played.clone(),
        remixer: t.remixer.clone(),
        producer: t.producer.clone(),
        mix: t.mix.clone(),
        bitrate: t.bitrate.and_then(|b| u32::try_from(b).ok()),
        track_number: t.track_number.and_then(|n| u32::try_from(n).ok()),
        filesize_kib: t.filesize_kib.and_then(|k| u64::try_from(k).ok()),
        color: t.color,
        ..TrackRow::default()
    }
}

const SYNC_TEXT: &str = "Syncing song info from the music library will overwrite the artist, \
     title, album and rating in your collection. Do you want to continue?";

fn pick<'a>(
    xml: Option<&'a OutsideLibrary>,
    music: Option<&'a OutsideLibrary>,
    programs: &'a [OutsideLibrary],
    kind: Outside,
) -> Option<&'a OutsideLibrary> {
    match kind {
        Outside::Xml => xml,
        Outside::Music => music,
        Outside::Program(i) => programs.get(i),
    }
}

impl Browser {
    fn outside(&self, kind: Outside) -> Option<&OutsideLibrary> {
        pick(
            self.library_xml.as_ref(),
            self.music.as_ref(),
            &self.programs,
            kind,
        )
    }

    fn outside_mut(&mut self, kind: Outside) -> Option<&mut OutsideLibrary> {
        match kind {
            Outside::Xml => self.library_xml.as_mut(),
            Outside::Music => self.music.as_mut(),
            Outside::Program(i) => self.programs.get_mut(i),
        }
    }

    fn outside_library(&self, kind: Outside) -> Option<&ExternalLibrary> {
        self.outside(kind)?.library()
    }

    pub(super) fn outside_key(&self, kind: Outside) -> Option<String> {
        match kind {
            Outside::Xml => self.library_xml.as_ref().map(|_| "xml".to_owned()),
            Outside::Music => self.music.as_ref().map(|_| "music".to_owned()),
            Outside::Program(i) => Some(self.programs.get(i)?.program?.name().to_lowercase()),
        }
    }

    pub(super) fn outside_by_key(&self, key: &str) -> Option<Outside> {
        let kind = match key {
            "xml" => Outside::Xml,
            "music" => Outside::Music,
            _ => Outside::Program(
                self.programs
                    .iter()
                    .position(|p| p.program.is_some_and(|p| p.name().to_lowercase() == key))?,
            ),
        };
        self.outside(kind).map(|_| kind)
    }

    pub(super) fn outside_name(&self, kind: Outside, at: &[usize]) -> Option<String> {
        let lib = self.outside(kind)?;
        let Some((&first, rest)) = at.split_first() else {
            return Some(lib.label());
        };
        let mut node = lib.library()?.playlists.get(first)?;
        for &i in rest {
            let ExternalNode::Folder { children, .. } = node else {
                return None;
            };
            node = children.get(i)?;
        }
        Some(node.name().to_owned())
    }

    pub fn set_library_xml(&mut self, path: Option<PathBuf>) {
        if self.library_xml.as_ref().and_then(|x| x.path.as_ref()) != path.as_ref() {
            self.library_xml = path.map(OutsideLibrary::xml);
        }
    }

    pub fn set_music_source(&mut self, source: Option<MusicSource>) {
        if self.music.as_ref().and_then(|m| m.music.as_ref()) != source.as_ref() {
            self.music = source.map(OutsideLibrary::music);
            if matches!(self.source, Source::External(Outside::Music, _)) {
                self.show(Source::Collection);
            }
        }
    }

    pub(crate) fn import_library_xml(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Library XML", &["xml"])
            .pick_file()
        else {
            return;
        };
        self.library_xml = Some(OutsideLibrary::xml(path.clone()));
        self.tree_actions
            .push(BrowserAction::LibraryXmlChanged(Some(path)));
        self.show(Source::External(Outside::Xml, Vec::new()));
    }

    pub(super) fn list_external(&mut self, kind: Outside, at: &[usize]) -> Vec<i64> {
        self.external.clear();
        self.external_origin.clear();
        let Some(lib) = pick(
            self.library_xml.as_ref(),
            self.music.as_ref(),
            &self.programs,
            kind,
        )
        .and_then(OutsideLibrary::library) else {
            return Vec::new();
        };
        let mut ids = Vec::new();
        for t in tracks_at(lib, at) {
            let track = &lib.tracks[t];
            if let Some(&id) = self.by_path.get(&track.path) {
                ids.push(id);
                continue;
            }
            let id = -(self.external.len() as i64) - 1;
            self.external.push(row(id, track));
            self.external_origin.push(Some(t));
            ids.push(id);
        }
        ids
    }

    pub(super) fn external_track_to_import(&self, id: i64) -> Option<ExternalTrack> {
        let Source::External(kind, _) = &self.source else {
            return None;
        };
        let program = self.outside(*kind)?.program;
        Some(with_marks(program, self.external_track(id)?))
    }

    pub(super) fn external_track(&self, id: i64) -> Option<&ExternalTrack> {
        let Source::External(kind, _) = &self.source else {
            return None;
        };
        let index = usize::try_from(-id - 1).ok()?;
        let t = (*self.external_origin.get(index)?)?;
        self.outside_library(*kind)?.tracks.get(t)
    }

    fn import_outside_tracks(&mut self, kind: Outside, at: &[usize]) {
        let Some(lib) = pick(
            self.library_xml.as_ref(),
            self.music.as_ref(),
            &self.programs,
            kind,
        )
        .and_then(OutsideLibrary::library) else {
            return;
        };
        let program = pick(
            self.library_xml.as_ref(),
            self.music.as_ref(),
            &self.programs,
            kind,
        )
        .and_then(|l| l.program);
        let owned: Vec<ExternalTrack> = tracks_at(lib, at)
            .into_iter()
            .map(|t| &lib.tracks[t])
            .filter(|t| !t.path.is_empty())
            .map(|t| with_marks(program, t))
            .collect();
        let tracks: Vec<&ExternalTrack> = owned.iter().collect();
        let Ok(db) = &mut self.lib else { return };
        match db.import_external_tracks(&tracks) {
            Ok(ids) => {
                self.usage.imports.add(outside_key(kind));
                self.message = Some(format!("Imported {} tracks", ids.len()));
            }
            Err(e) => crate::toast::error(format!("Import failed: {e}")),
        }
        self.refresh();
    }

    fn import_outside_playlists(&mut self, kind: Outside, at: &[usize]) {
        let Some(lib) = pick(
            self.library_xml.as_ref(),
            self.music.as_ref(),
            &self.programs,
            kind,
        )
        .and_then(OutsideLibrary::library) else {
            return;
        };
        let program = pick(
            self.library_xml.as_ref(),
            self.music.as_ref(),
            &self.programs,
            kind,
        )
        .and_then(|l| l.program);
        let nodes: Vec<ExternalNode> = if at.is_empty() {
            lib.playlists.clone()
        } else {
            node_at(lib, at).into_iter().cloned().collect()
        };
        let mut lib = lib.clone();
        if program.is_some() {
            for t in nodes.iter().flat_map(ExternalNode::all_tracks) {
                if let Some(track) = lib.tracks.get_mut(t) {
                    *track = with_marks(program, track);
                }
            }
        }
        let lib = &lib;
        let Ok(db) = &mut self.lib else { return };
        let (mut tracks, mut lists) = (0, 0);
        for node in &nodes {
            match db.import_external_node(lib, node, None) {
                Ok(r) => {
                    self.usage.imports.add(outside_key(kind));
                    tracks += r.tracks;
                    lists += r.playlists;
                }
                Err(e) => {
                    crate::toast::error(format!("Import failed: {e}"));
                    break;
                }
            }
        }
        self.message = Some(format!("Imported {tracks} tracks, {lists} playlists"));
        self.refresh();
    }

    pub(super) fn sync_song_info(&mut self) {
        let Some(lib) = self.outside_library(Outside::Music) else {
            return;
        };
        let tracks = lib.tracks.clone();
        match self.with_lib_value(|db| db.sync_song_info(&tracks)) {
            Ok(n) => self.message = Some(format!("Synced {n} tracks")),
            Err(e) => crate::toast::error(e.to_string()),
        }
        self.refresh();
    }

    pub(super) fn open_outside(&mut self, kind: Outside, at: Vec<usize>) {
        if let Some(lib) = self.outside_mut(kind)
            && matches!(lib.state, State::Idle)
        {
            lib.load();
        }
        self.show(Source::External(kind, at));
    }

    pub(super) fn music_tree(&mut self, ui: &mut Ui) {
        if self.music.is_some() {
            self.outside_tree(ui, Outside::Music);
        }
    }

    pub fn find_programs(&mut self) {
        let found: Vec<(Program, PathBuf)> = Program::ALL
            .into_iter()
            .flat_map(|p| p.find().into_iter().map(move |root| (p, root)))
            .collect();
        let same = found.len() == self.programs.len()
            && found
                .iter()
                .zip(&self.programs)
                .all(|((p, root), lib)| lib.program == Some(*p) && lib.path.as_ref() == Some(root));
        if same {
            return;
        }
        self.programs = found
            .into_iter()
            .enumerate()
            .map(|(i, (p, root))| OutsideLibrary::program(i, p, root))
            .collect();
        if matches!(self.source, Source::External(Outside::Program(_), _)) {
            self.show(Source::Collection);
        }
    }

    pub(super) fn programs_tree(&mut self, ui: &mut Ui) {
        ui.add_space(8.0);
        let faint = crate::theme::get(ui.ctx()).text_faint;
        let caption = widgets::hud_label(ui, "DJ SOFTWARE", 10.0, faint);
        caption.context_menu(|ui| {
            if ui.button(tr("Look for Libraries Again")).clicked() {
                self.find_programs();
                ui.close();
            }
        });
        if self.programs.is_empty() {
            ui.label(
                RichText::new(tr("No Serato, VirtualDJ or Engine DJ library found."))
                    .weak()
                    .size(11.0),
            );
        }
        for i in 0..self.programs.len() {
            self.outside_tree(ui, Outside::Program(i));
        }
    }

    pub(super) fn library_xml_tree(&mut self, ui: &mut Ui) {
        if self.library_xml.is_some() {
            ui.add_space(8.0);
            widgets::caption(ui, "LIBRARY XML");
            self.outside_tree(ui, Outside::Xml);
        }
    }

    fn outside_tree(&mut self, ui: &mut Ui, kind: Outside) {
        let Some(lib) = self.outside_mut(kind) else {
            return;
        };
        if lib.poll() && matches!(self.source, Source::External(k, _) if k == kind) {
            self.recompute();
        }
        let Some(lib) = self.outside(kind) else {
            return;
        };
        let (label, status, path) = (lib.label(), lib.status(), lib.path.clone());
        let top: Vec<ExternalNode> = lib
            .library()
            .map(|l| l.playlists.clone())
            .unwrap_or_default();
        let icon = if kind == Outside::Music {
            "♫"
        } else {
            "🗂"
        };
        let selected = self.source == Source::External(kind, Vec::new());
        let mut response = ui
            .selectable_label(selected, format!("{icon}  {label}"))
            .interact(egui::Sense::drag());
        response.dnd_set_drag_payload(SourceDrag(Source::External(kind, Vec::new())));
        if response.clicked() {
            self.open_outside(kind, Vec::new());
        }
        if let Some(path) = &path {
            response = response.on_hover_text(path.display().to_string());
        }
        let mut remove = false;
        response.context_menu(|ui| {
            self.outside_menu(ui, kind, &[]);
            let refresh = if kind == Outside::Music {
                "Refresh Playlists"
            } else {
                "Refresh"
            };
            if ui.button(tr(refresh)).clicked() {
                if let Some(lib) = self.outside_mut(kind) {
                    lib.load();
                }
                ui.close();
            }
            if kind == Outside::Music {
                let ready = self.outside_library(kind).is_some();
                if ui
                    .add_enabled(ready, egui::Button::new(tr("Sync Song Info")))
                    .on_hover_text(tr(
                        "Copies the artist, title, album and rating of the library's tracks into the collection",
                    ))
                    .clicked()
                {
                    self.ask = Some(
                        Ask::new("Sync Song Info", SYNC_TEXT)
                            .cancel("No")
                            .button("Yes", BrowserAsk::SyncSongInfo),
                    );
                    ui.close();
                }
            } else if kind == Outside::Xml && ui.button(tr("Remove from list")).clicked() {
                remove = true;
                ui.close();
            }
        });
        if let Some(status) = status {
            ui.label(RichText::new(status).weak().size(11.0));
        }
        ui.push_id(kind, |ui| {
            ui.indent("outside", |ui| {
                for (i, node) in top.iter().enumerate() {
                    self.outside_node(ui, kind, node, &[i]);
                }
            });
        });
        if remove {
            self.library_xml = None;
            if matches!(self.source, Source::External(Outside::Xml, _)) {
                self.show(Source::Collection);
            }
            self.tree_actions
                .push(BrowserAction::LibraryXmlChanged(None));
        }
    }

    fn outside_node(&mut self, ui: &mut Ui, kind: Outside, node: &ExternalNode, at: &[usize]) {
        let selected = self.source == Source::External(kind, at.to_vec());
        let response = match node {
            ExternalNode::Playlist { name, .. } => {
                ui.selectable_label(selected, format!("≡  {name}"))
            }
            ExternalNode::Folder { name, children } => {
                let id = ui.make_persistent_id(("outside", kind, at));
                let state = egui::collapsing_header::CollapsingState::load_with_default_open(
                    ui.ctx(),
                    id,
                    false,
                );
                let mut response = None;
                state
                    .show_header(ui, |ui| {
                        response = Some(ui.selectable_label(selected, format!("📁  {name}")));
                    })
                    .body(|ui| {
                        for (i, child) in children.iter().enumerate() {
                            let mut path = at.to_vec();
                            path.push(i);
                            self.outside_node(ui, kind, child, &path);
                        }
                    });
                response.expect("header shown")
            }
        };
        let response = response.interact(egui::Sense::drag());
        response.dnd_set_drag_payload(SourceDrag(Source::External(kind, at.to_vec())));
        if response.clicked() {
            self.open_outside(kind, at.to_vec());
        }
        response.context_menu(|ui| self.outside_menu(ui, kind, at));
    }

    fn outside_menu(&mut self, ui: &mut Ui, kind: Outside, at: &[usize]) {
        let ready = self.outside_library(kind).is_some();
        if ui
            .add_enabled(ready, egui::Button::new(tr("Import to Collection")))
            .on_hover_text(tr(
                "Adds its tracks to the collection with their cues and grid",
            ))
            .clicked()
        {
            self.import_outside_tracks(kind, at);
            ui.close();
        }
        if ui
            .add_enabled(ready, egui::Button::new(tr("Import to Playlists")))
            .on_hover_text(tr(
                "Copies its playlists and folders into Playlists, with their tracks",
            ))
            .clicked()
        {
            self.import_outside_playlists(kind, at);
            ui.close();
        }
    }
}

pub fn music_source(use_xml: bool, xml: Option<PathBuf>) -> Option<MusicSource> {
    let xml = xml.unwrap_or_else(default_music_xml);
    if cfg!(target_os = "macos") && !use_xml {
        Some(MusicSource::Api)
    } else if cfg!(target_os = "macos") || xml.is_file() {
        Some(MusicSource::Xml(xml))
    } else {
        None
    }
}

pub fn default_music_xml() -> PathBuf {
    crate::dirs::user_music()
        .join("iTunes")
        .join("iTunes Music Library.xml")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib() -> ExternalLibrary {
        let text = r#"<DJ_PLAYLISTS><COLLECTION>
            <TRACK TrackID="1" Name="One" TotalTime="10" Location="file://localhost/a/one.mp3"/>
            <TRACK TrackID="2" Name="Two" TotalTime="10" Location="file://localhost/a/two.mp3"/>
          </COLLECTION><PLAYLISTS><NODE Type="0" Name="ROOT">
            <NODE Type="0" Name="F"><NODE Type="1" Name="L" KeyType="0"><TRACK Key="2"/></NODE></NODE>
          </NODE></PLAYLISTS></DJ_PLAYLISTS>"#;
        ExternalLibrary::from_dj_playlists(&backspn_formats::dj_playlists::parse(text).unwrap())
    }

    #[test]
    fn nodes_are_found_by_their_path() {
        let lib = lib();
        assert_eq!(tracks_at(&lib, &[]), [0, 1]);
        assert_eq!(tracks_at(&lib, &[0]), [1]);
        assert_eq!(node_at(&lib, &[0, 0]).map(ExternalNode::name), Some("L"));
        assert_eq!(node_at(&lib, &[0, 1]), None);
        assert_eq!(node_at(&lib, &[3]), None);
    }

    fn browser_with(lib: ExternalLibrary) -> Browser {
        let mut b = Browser::in_memory();
        b.library_xml = Some(OutsideLibrary {
            kind: Outside::Xml,
            path: Some(PathBuf::from("/x/library.xml")),
            music: None,
            program: None,
            state: State::Loaded(lib),
        });
        b
    }

    #[test]
    fn loading_a_row_brings_the_track_in_and_import_copies_the_tree() {
        let mut b = browser_with(lib());
        b.show(Source::External(Outside::Xml, vec![0]));
        assert_eq!(b.view, [-1]);
        let id = b.resolve(-1).unwrap();
        assert!(id >= 0);
        let row = b.row(id).unwrap();
        assert_eq!(
            (row.title.as_str(), row.path.as_str()),
            ("Two", "/a/two.mp3")
        );
        b.show(Source::External(Outside::Xml, Vec::new()));
        assert_eq!(b.view.len(), 2);
        assert!(b.view.contains(&id));

        b.import_outside_playlists(Outside::Xml, &[]);
        let names: Vec<String> = b.nodes.iter().map(|n| n.name.clone()).collect();
        assert!(names.contains(&"F".to_owned()) && names.contains(&"L".to_owned()));
        b.import_outside_tracks(Outside::Xml, &[]);
        assert_eq!(b.rows.len(), 2);
    }

    #[test]
    fn a_program_library_lists_like_the_others() {
        let mut b = Browser::in_memory();
        let mut serato = OutsideLibrary::program(0, Program::Serato, PathBuf::from("/x/_Serato_"));
        serato.state = State::Loaded(lib());
        b.programs.push(serato);
        b.show(Source::External(Outside::Program(0), vec![0, 0]));
        assert_eq!(b.view, [-1]);
        assert_eq!(b.external_track(-1).map(|t| t.title.as_str()), Some("Two"));
        assert_eq!(b.programs[0].label(), "Serato");
        let drives = [PathBuf::from("/Volumes/DJ")];
        let home = PathBuf::from("/Users/me");
        assert_eq!(
            drive_name_in(&PathBuf::from("/Volumes/DJ/_Serato_"), &home, &drives).as_deref(),
            Some("DJ")
        );
        assert_eq!(
            drive_name_in(&home.join("Music/_Serato_"), &home, &drives),
            None
        );
    }

    #[test]
    fn a_track_shows_with_its_data() {
        let lib = lib();
        let r = row(-1, &lib.tracks[1]);
        assert_eq!(
            (r.id, r.title.as_str(), r.path.as_str()),
            (-1, "Two", "/a/two.mp3")
        );
    }

    #[test]
    fn music_reads_the_api_on_macos_unless_the_xml_is_chosen() {
        let file = PathBuf::from("/nowhere/library.xml");
        if cfg!(target_os = "macos") {
            assert_eq!(music_source(false, None), Some(MusicSource::Api));
            assert_eq!(
                music_source(true, Some(file.clone())),
                Some(MusicSource::Xml(file))
            );
        } else {
            assert_eq!(music_source(false, Some(file)), None, "no file, no node");
        }
    }
}
