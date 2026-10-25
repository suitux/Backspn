use std::cmp::Ordering;
use std::fmt::Write as _;
use std::path::Path;

use backspn_library::TrackRow;
use eframe::egui::{self, RichText, Ui};
use egui_extras::Column;
use serde::{Deserialize, Serialize};

use crate::key_view::KeyView;
use crate::theme::Theme;

pub type Sorting = Vec<(BrowserColumn, bool)>;

pub const MAX_SORT_CRITERIA: usize = 2;

pub fn sort_clicked(sorting: &[(BrowserColumn, bool)], column: BrowserColumn) -> Sorting {
    match sorting.first() {
        Some(&(c, true)) if c == column => {
            let mut next = sorting.to_vec();
            next[0].1 = false;
            next
        }
        Some(&(c, false)) if c == column => Vec::new(),
        _ => std::iter::once((column, true))
            .chain(sorting.iter().copied().filter(|&(c, _)| c != column))
            .take(MAX_SORT_CRITERIA)
            .collect(),
    }
}

pub fn compare_by(sorting: &[(BrowserColumn, bool)], a: &TrackRow, b: &TrackRow) -> Ordering {
    sorting
        .iter()
        .map(|&(c, ascending)| {
            let o = c.compare(a, b);
            if ascending { o } else { o.reverse() }
        })
        .find(|o| o.is_ne())
        .unwrap_or(Ordering::Equal)
}

fn key_rank(track: &TrackRow) -> i16 {
    track
        .musical_key
        .and_then(backspn_formats::nml::MusicalKey::from_value)
        .map_or(-1, |k| i16::from(k.sort_rank()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BrowserColumn {
    Number,
    Title,
    Artist,
    Album,
    Genre,
    Label,
    Comment,
    Bpm,
    Key,
    Time,
    Rating,
    Plays,
    ImportDate,
    Modified,
    Loudness,
    Analyzed,
    Locked,
    FileName,
    Location,
    Remixer,
    Producer,
    Mix,
    CatalogNo,
    ReleaseDate,
    LastPlayed,
    Comment2,
    Bitrate,
    TrackNo,
    FileSize,
    Preview,
    Color,
    Cover,
    Stems,
}

impl BrowserColumn {
    pub const ALL: [Self; 33] = [
        Self::Number,
        Self::Cover,
        Self::Color,
        Self::Preview,
        Self::Title,
        Self::Artist,
        Self::Remixer,
        Self::Producer,
        Self::Mix,
        Self::Album,
        Self::TrackNo,
        Self::CatalogNo,
        Self::ReleaseDate,
        Self::Genre,
        Self::Label,
        Self::Comment,
        Self::Comment2,
        Self::Bpm,
        Self::Key,
        Self::Time,
        Self::Rating,
        Self::Plays,
        Self::LastPlayed,
        Self::ImportDate,
        Self::Modified,
        Self::Loudness,
        Self::Analyzed,
        Self::Locked,
        Self::Stems,
        Self::Bitrate,
        Self::FileSize,
        Self::FileName,
        Self::Location,
    ];

    pub fn defaults() -> Vec<Self> {
        vec![
            Self::Number,
            Self::Title,
            Self::Artist,
            Self::Bpm,
            Self::Key,
            Self::Time,
            Self::Rating,
            Self::Genre,
            Self::Comment,
        ]
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Number => "#",
            Self::Title => "Title",
            Self::Artist => "Artist",
            Self::Album => "Release",
            Self::Genre => "Genre",
            Self::Label => "Label",
            Self::Comment => "Comment",
            Self::Bpm => "BPM",
            Self::Key => "Key",
            Self::Time => "Time",
            Self::Rating => "Rating",
            Self::Plays => "Plays",
            Self::ImportDate => "Import Date",
            Self::Modified => "Modified",
            Self::Loudness => "Loudness",
            Self::Analyzed => "Analyzed",
            Self::Locked => "Lock",
            Self::FileName => "File Name",
            Self::Location => "Location",
            Self::Remixer => "Remixer",
            Self::Producer => "Producer",
            Self::Mix => "Mix",
            Self::CatalogNo => "Catalog No",
            Self::ReleaseDate => "Release Date",
            Self::LastPlayed => "Last Played",
            Self::Comment2 => "Comment 2",
            Self::Bitrate => "Bitrate",
            Self::TrackNo => "Track",
            Self::FileSize => "File Size",
            Self::Preview => "Preview",
            Self::Color => "Color",
            Self::Cover => "Cover Art",
            Self::Stems => "Stems",
        }
    }

    pub fn width(self) -> Column {
        match self {
            Self::Bpm => Column::exact(60.0),
            Self::Key => Column::exact(44.0),
            Self::Time | Self::Plays => Column::exact(52.0),
            Self::Rating => Column::exact(70.0),
            Self::Analyzed => Column::exact(64.0),
            Self::Number | Self::Locked => Column::exact(40.0),
            Self::Preview => Column::initial(130.0).at_least(60.0).clip(true),
            Self::Color => Column::exact(46.0),
            Self::Cover => Column::initial(40.0).at_least(24.0).clip(true),
            Self::ImportDate | Self::ReleaseDate | Self::LastPlayed => Column::exact(86.0),
            Self::Modified => Column::exact(124.0),
            Self::TrackNo => Column::exact(48.0),
            Self::Bitrate | Self::FileSize => Column::exact(78.0),
            Self::CatalogNo => Column::initial(100.0).clip(true),
            Self::Remixer | Self::Producer | Self::Mix => Column::initial(140.0).clip(true),
            Self::Loudness => Column::exact(74.0),
            Self::Title => Column::initial(260.0).clip(true),
            Self::Artist | Self::FileName => Column::initial(180.0).clip(true),
            Self::Album | Self::Label | Self::Genre => Column::initial(150.0).clip(true),
            Self::Comment | Self::Comment2 | Self::Location | Self::Stems => {
                Column::initial(200.0).clip(true)
            }
        }
    }

    pub fn sortable(self) -> bool {
        !matches!(
            self,
            Self::Number | Self::Preview | Self::Cover | Self::Stems
        )
    }

    pub fn attribute(self) -> Option<u32> {
        Some(match self {
            Self::Artist => 2,
            Self::Bitrate => 3,
            Self::Bpm => 4,
            Self::CatalogNo => 5,
            Self::Color => 6,
            Self::Comment => 7,
            Self::Comment2 => 8,
            Self::Location => 9,
            Self::Genre => 10,
            Self::Label => 13,
            Self::Mix => 14,
            Self::Producer => 15,
            Self::Album => 16,
            Self::Remixer => 17,
            Self::Title => 18,
            Self::Key => 20,
            Self::FileName => 23,
            Self::Time => 24,
            Self::Analyzed => 26,
            Self::Locked => 27,
            Self::ReleaseDate => 28,
            Self::Rating => 29,
            Self::Plays => 30,
            Self::LastPlayed => 32,
            Self::ImportDate => 33,
            _ => return None,
        })
    }

    pub fn from_attribute(id: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.attribute() == Some(id))
    }

    pub fn compare(self, a: &TrackRow, b: &TrackRow) -> Ordering {
        let text = |x: &str, y: &str| x.to_lowercase().cmp(&y.to_lowercase());
        let num = |x: Option<f64>, y: Option<f64>| x.unwrap_or(0.0).total_cmp(&y.unwrap_or(0.0));
        match self {
            Self::Number | Self::Preview | Self::Cover | Self::Stems => Ordering::Equal,
            Self::Color => a.color.cmp(&b.color),
            Self::Title => text(&a.title, &b.title),
            Self::Artist => text(&a.artist, &b.artist),
            Self::Album => text(&a.album, &b.album),
            Self::Genre => text(&a.genre, &b.genre),
            Self::Label => text(&a.label, &b.label),
            Self::Comment => text(&a.comment, &b.comment),
            Self::Bpm => num(a.bpm, b.bpm),
            Self::Key => key_rank(a)
                .cmp(&key_rank(b))
                .then_with(|| a.key_text.cmp(&b.key_text)),
            Self::Time => num(a.duration_s, b.duration_s),
            Self::Rating => a.rating.cmp(&b.rating),
            Self::Plays => a.playcount.cmp(&b.playcount),
            Self::ImportDate => date_key(&a.import_date).cmp(&date_key(&b.import_date)),
            Self::Modified => date_key(&a.modified_date)
                .cmp(&date_key(&b.modified_date))
                .then(a.modified_time.cmp(&b.modified_time)),
            Self::Loudness => num(a.lufs, b.lufs),
            Self::Analyzed => a.analyzed.cmp(&b.analyzed),
            Self::Locked => a.locked.cmp(&b.locked),
            Self::FileName => text(file_name(&a.path), file_name(&b.path)),
            Self::Location => text(folder(&a.path), folder(&b.path)),
            Self::Remixer => text(&a.remixer, &b.remixer),
            Self::Producer => text(&a.producer, &b.producer),
            Self::Mix => text(&a.mix, &b.mix),
            Self::CatalogNo => text(&a.catalog_no, &b.catalog_no),
            Self::ReleaseDate => date_key(&a.release_date).cmp(&date_key(&b.release_date)),
            Self::LastPlayed => date_key(&a.last_played).cmp(&date_key(&b.last_played)),
            Self::Comment2 => text(&a.comment2, &b.comment2),
            Self::Bitrate => a.bitrate.cmp(&b.bitrate),
            Self::TrackNo => a.track_number.cmp(&b.track_number),
            Self::FileSize => a.filesize_kib.cmp(&b.filesize_kib),
        }
    }

    pub fn text(self, n: usize, track: &TrackRow, keys: KeyView) -> String {
        let check = |on: bool| if on { "✓" } else { "" }.to_owned();
        match self {
            Self::Number => (n + 1).to_string(),
            Self::Title => track.title.clone(),
            Self::Artist => track.artist.clone(),
            Self::Album => track.album.clone(),
            Self::Genre => track.genre.clone(),
            Self::Label => track.label.clone(),
            Self::Comment => track.comment.clone(),
            Self::Bpm => track.bpm.map_or_else(String::new, |b| format!("{b:.2}")),
            Self::Key => keys.text(track),
            Self::Time => track.duration_s.map_or_else(String::new, |d| {
                let s = d.round() as u64;
                format!("{}:{:02}", s / 60, s % 60)
            }),
            Self::Rating => stars(track.rating),
            Self::Plays if track.playcount == 0 => String::new(),
            Self::Plays => track.playcount.to_string(),
            Self::ImportDate => track.import_date.clone(),
            Self::Modified => modified_text(track),
            Self::Loudness => track.lufs.map_or_else(String::new, |l| format!("{l:.1}")),
            Self::Analyzed => check(track.analyzed),
            Self::Locked => check(track.locked),
            Self::FileName => file_name(&track.path).to_owned(),
            Self::Location => folder(&track.path).to_owned(),
            Self::Remixer => track.remixer.clone(),
            Self::Producer => track.producer.clone(),
            Self::Mix => track.mix.clone(),
            Self::CatalogNo => track.catalog_no.clone(),
            Self::ReleaseDate => track.release_date.clone(),
            Self::LastPlayed => track.last_played.clone(),
            Self::Comment2 => track.comment2.clone(),
            Self::Bitrate => track
                .bitrate
                .map_or_else(String::new, |b| format!("{} kbps", b / 1000)),
            Self::TrackNo => track
                .track_number
                .map_or_else(String::new, |n| n.to_string()),
            Self::FileSize => track
                .filesize_kib
                .map_or_else(String::new, |k| format!("{:.1} MB", k as f64 / 1024.0)),
            Self::Preview | Self::Cover | Self::Stems => String::new(),
            Self::Color => backspn_library::smart::COLOR_NAMES[usize::from(track.color.min(7))]
                .replace("None", ""),
        }
    }

    pub fn cell(
        self,
        ui: &mut Ui,
        n: usize,
        track: &TrackRow,
        cx: &CellContext<'_>,
        t: &Theme,
    ) -> Option<egui::Rect> {
        if self == Self::Preview {
            return preview_cell(ui, track, cx, t);
        }
        if self == Self::Cover {
            let side = (ui.available_height() - 2.0)
                .min(ui.available_width())
                .max(0.0);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
            crate::covers::paint(ui.painter(), rect, cx.cover, t.text_dim);
            return None;
        }
        if self == Self::Stems {
            if let Some(path) = cx.stems {
                let path = path.display().to_string();
                ui.label(RichText::new(&path).color(t.text_dim))
                    .on_hover_text(path);
            }
            return None;
        }
        if self == Self::Color {
            if let Some(colour) = track_colour(track.color) {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(26.0, 12.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 0.0, colour);
            }
            return None;
        }
        let (played, keys) = (cx.played, cx.keys);
        let text = self.text(n, track, keys);
        let rich = match self {
            Self::Number if played => RichText::new(format!("✓ {text}"))
                .color(t.accent)
                .size(11.0),
            Self::Number => RichText::new(text).color(t.text_dim).size(11.0),
            Self::Title => RichText::new(text).color(t.text),
            Self::Artist | Self::Location => RichText::new(text).color(t.text_dim),
            Self::Key => RichText::new(text)
                .monospace()
                .size(12.0)
                .color(keys.colour(track).unwrap_or(t.text_dim)),
            Self::Rating => RichText::new(text).color(t.master).size(11.0),
            Self::Analyzed | Self::Locked => RichText::new(text).color(t.accent),
            Self::Bpm
            | Self::Plays
            | Self::Loudness
            | Self::Bitrate
            | Self::TrackNo
            | Self::FileSize => RichText::new(text).monospace().size(12.0),
            Self::ImportDate | Self::ReleaseDate | Self::LastPlayed | Self::Modified => {
                RichText::new(text).size(12.0)
            }
            _ => RichText::new(text),
        };
        ui.label(rich);
        None
    }
}

fn modified_text(track: &TrackRow) -> String {
    match track.modified_time {
        Some(t) if !track.modified_date.is_empty() => {
            format!("{} {:02}:{:02}", track.modified_date, t / 3600, t / 60 % 60)
        }
        _ => track.modified_date.clone(),
    }
}

pub fn track_colour(value: u8) -> Option<egui::Color32> {
    use egui::Color32 as C;
    Some(match value {
        1 => C::from_rgb(228, 72, 72),
        2 => C::from_rgb(240, 140, 48),
        3 => C::from_rgb(236, 206, 64),
        4 => C::from_rgb(88, 196, 96),
        5 => C::from_rgb(72, 132, 228),
        6 => C::from_rgb(148, 104, 220),
        7 => C::from_rgb(216, 84, 188),
        _ => return None,
    })
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CellContext<'a> {
    pub played: bool,
    pub keys: KeyView,
    pub preview_at: Option<f32>,
    pub cues: &'a [CueMark],
    pub cover: Option<egui::TextureId>,
    pub stems: Option<&'a std::path::Path>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CueMark {
    pub at: f32,
    pub hotcue: Option<usize>,
    pub kind: backspn_formats::nml::CueKind,
}

pub fn cue_marks(cues: &[backspn_library::CuePoint], duration_s: Option<f64>) -> Vec<CueMark> {
    let Some(length) = duration_s.filter(|d| *d > 0.0) else {
        return Vec::new();
    };
    cues.iter()
        .filter(|c| !c.is_beatgrid())
        .map(|c| CueMark {
            at: (c.start_s / length).clamp(0.0, 1.0) as f32,
            hotcue: c.hotcue.map(|h| usize::from(h) + 1),
            kind: backspn_formats::nml::CueKind::from(c.kind),
        })
        .collect()
}

const MARK_WIDTH: f32 = 11.0;

pub fn cue_under(rect: egui::Rect, pos: egui::Pos2, marks: &[CueMark]) -> Option<f32> {
    if !rect.contains(pos) || pos.y > rect.top() + MARK_WIDTH {
        return None;
    }
    marks
        .iter()
        .rev()
        .find(|m| {
            let x = rect.left() + m.at * rect.width();
            (x..=x + MARK_WIDTH).contains(&pos.x)
        })
        .map(|m| m.at)
}

fn preview_cell(
    ui: &mut Ui,
    track: &TrackRow,
    cx: &CellContext<'_>,
    t: &Theme,
) -> Option<egui::Rect> {
    let size = egui::vec2(ui.available_width(), ui.available_height() - 4.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    if track.overview.is_empty() {
        return None;
    }
    let painter = ui.painter_at(rect);
    let style = crate::waveform::WaveStyle::get(ui.ctx());
    crate::waveform::paint_overview(&painter, rect, &track.overview, cx.preview_at, &style);
    let whole = (0.0, 1.0);
    for mark in cx.cues {
        let colour = t.hotcue(mark.kind);
        let at = f64::from(mark.at);
        match mark.hotcue {
            Some(n) => crate::waveform::hotcue(&painter, rect, whole, at, n, colour),
            None => crate::waveform::cue_mark(&painter, rect, whole, at, colour),
        }
    }
    if let Some(at) = cx.preview_at {
        let x = rect.left() + at * rect.width();
        crate::waveform::marker(&painter, rect, x, t.danger, 1.5);
    }
    Some(rect)
}

pub fn preview_fraction(rect: egui::Rect, pos: egui::Pos2) -> Option<f32> {
    rect.contains(pos)
        .then(|| ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0))
}

pub fn html_list(
    title: &str,
    columns: &[BrowserColumn],
    rows: &[&TrackRow],
    keys: KeyView,
) -> String {
    fn escape(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    }
    let mut html = format!(
        "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\"><title>{t}</title>\n\
         <style>body{{font-family:sans-serif}}table{{border-collapse:collapse}}\
         td,th{{padding:2px 8px;border-bottom:1px solid #ccc;text-align:left}}</style>\n\
         </head><body><h1>{t}</h1>\n<table>\n<tr>",
        t = escape(title)
    );
    for c in columns {
        let _ = write!(html, "<th>{}</th>", escape(c.name()));
    }
    html.push_str("</tr>\n");
    for (n, track) in rows.iter().enumerate() {
        html.push_str("<tr>");
        for c in columns {
            let _ = write!(html, "<td>{}</td>", escape(&c.text(n, track, keys)));
        }
        html.push_str("</tr>\n");
    }
    html.push_str("</table>\n</body></html>\n");
    html
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "LayoutRepr")]
pub struct ColumnLayout {
    order: Vec<BrowserColumn>,
    hidden: Vec<BrowserColumn>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum LayoutRepr {
    Full {
        order: Vec<BrowserColumn>,
        hidden: Vec<BrowserColumn>,
    },
    Shown(Vec<BrowserColumn>),
}

impl From<LayoutRepr> for ColumnLayout {
    fn from(repr: LayoutRepr) -> Self {
        match repr {
            LayoutRepr::Full { order, hidden } => Self { order, hidden }.normalized(),
            LayoutRepr::Shown(shown) => Self::from_shown(&shown),
        }
    }
}

impl Default for ColumnLayout {
    fn default() -> Self {
        Self::from_shown(&BrowserColumn::defaults())
    }
}

impl ColumnLayout {
    pub fn from_shown(shown: &[BrowserColumn]) -> Self {
        Self {
            order: shown.to_vec(),
            hidden: Vec::new(),
        }
        .normalized()
    }

    fn normalized(mut self) -> Self {
        let mut seen = std::collections::HashSet::new();
        self.order.retain(|c| seen.insert(*c));
        for c in BrowserColumn::ALL {
            if !self.order.contains(&c) {
                self.order.push(c);
                if !self.hidden.contains(&c) {
                    self.hidden.push(c);
                }
            }
        }
        if self.visible().is_empty() {
            return Self::default();
        }
        self
    }

    pub fn visible(&self) -> Vec<BrowserColumn> {
        self.order
            .iter()
            .copied()
            .filter(|c| !self.hidden.contains(c))
            .collect()
    }

    pub fn all(&self) -> &[BrowserColumn] {
        &self.order
    }

    pub fn is_shown(&self, column: BrowserColumn) -> bool {
        !self.hidden.contains(&column)
    }

    pub fn set_shown(&mut self, column: BrowserColumn, shown: bool) {
        if shown {
            self.hidden.retain(|&c| c != column);
        } else if self.is_shown(column) && self.visible().len() > 1 {
            self.hidden.push(column);
        }
    }

    pub fn move_column(&mut self, column: BrowserColumn, target: BrowserColumn, after: bool) {
        if column == target {
            return;
        }
        self.order.retain(|&c| c != column);
        let at = self
            .order
            .iter()
            .position(|&c| c == target)
            .map_or(self.order.len(), |i| i + usize::from(after));
        self.order.insert(at, column);
    }
}

pub fn stars(rating: u8) -> String {
    let n = ((f32::from(rating) / 51.0).round() as usize).min(5);
    format!("{}{}", "★".repeat(n), "☆".repeat(5 - n))
}

fn file_name(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
}

fn folder(path: &str) -> &str {
    Path::new(path)
        .parent()
        .and_then(Path::to_str)
        .unwrap_or("")
}

fn date_key(date: &str) -> (u32, u32, u32) {
    let mut parts = date.split('/').map(|p| p.trim().parse().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn printed_lists_escape_and_follow_the_columns() {
        let t = TrackRow {
            title: "A & <B>".into(),
            bpm: Some(128.0),
            ..TrackRow::default()
        };
        let html = html_list(
            "Set",
            &[BrowserColumn::Title, BrowserColumn::Bpm],
            &[&t],
            KeyView::default(),
        );
        assert!(html.contains("<th>Title</th><th>BPM</th>"));
        assert!(html.contains("<td>A &amp; &lt;B&gt;</td><td>128.00</td>"));
    }

    use super::*;

    fn track(path: &str) -> TrackRow {
        TrackRow {
            path: path.into(),
            ..TrackRow::default()
        }
    }

    #[test]
    fn dates_sort_by_value_not_text() {
        let mut a = track("/a.mp3");
        let mut b = track("/b.mp3");
        a.import_date = "2024/9/30".into();
        b.import_date = "2024/10/2".into();
        assert_eq!(BrowserColumn::ImportDate.compare(&a, &b), Ordering::Less);
    }

    #[test]
    fn file_name_and_location_split_the_path() {
        let a = track("/Music/Techno/Track.mp3");
        assert_eq!(file_name(&a.path), "Track.mp3");
        assert_eq!(folder(&a.path), "/Music/Techno");
    }

    #[test]
    fn header_clicks_stack_up_to_two_criteria() {
        use BrowserColumn::{Artist, Bpm, Title};
        let s = sort_clicked(&[], Artist);
        assert_eq!(s, [(Artist, true)]);
        let s = sort_clicked(&s, Bpm);
        assert_eq!(s, [(Bpm, true), (Artist, true)]);
        let s = sort_clicked(&s, Bpm);
        assert_eq!(s, [(Bpm, false), (Artist, true)]);
        let s = sort_clicked(&s, Title);
        assert_eq!(s, [(Title, true), (Bpm, false)]);
        let s = sort_clicked(&s, Bpm);
        assert_eq!(s, [(Bpm, true), (Title, true)]);
        let s = sort_clicked(&sort_clicked(&s, Bpm), Bpm);
        assert_eq!(s, []);
    }

    #[test]
    fn ties_go_to_the_next_criterion() {
        let row = |artist: &str, title: &str| TrackRow {
            artist: artist.into(),
            title: title.into(),
            ..TrackRow::default()
        };
        let (a, b) = (row("Ann", "B"), row("Ann", "A"));
        let s = [(BrowserColumn::Artist, true), (BrowserColumn::Title, false)];
        assert_eq!(compare_by(&s, &a, &b), Ordering::Less);
        assert_eq!(compare_by(&s[..1], &a, &b), Ordering::Equal);
    }

    #[test]
    fn every_column_has_a_distinct_name() {
        let names: std::collections::HashSet<_> =
            BrowserColumn::ALL.iter().map(|c| c.name()).collect();
        assert_eq!(names.len(), BrowserColumn::ALL.len());
        assert!(
            BrowserColumn::defaults()
                .iter()
                .all(|c| BrowserColumn::ALL.contains(c))
        );
    }

    #[test]
    fn star_strings() {
        assert_eq!(stars(0), "☆☆☆☆☆");
        assert_eq!(stars(255), "★★★★★");
        assert_eq!(stars(153), "★★★☆☆");
    }

    use BrowserColumn::{Artist, Bpm, Genre, Key, Time, Title};

    #[test]
    fn a_hidden_column_comes_back_in_its_place() {
        let mut layout = ColumnLayout::from_shown(&[Title, Bpm, Key, Time]);
        layout.set_shown(Key, false);
        assert_eq!(layout.visible(), [Title, Bpm, Time]);
        layout.set_shown(Key, true);
        assert_eq!(layout.visible(), [Title, Bpm, Key, Time]);
    }

    #[test]
    fn the_last_shown_column_stays() {
        let mut layout = ColumnLayout::from_shown(&[Title]);
        layout.set_shown(Title, false);
        assert_eq!(layout.visible(), [Title]);
    }

    #[test]
    fn older_settings_and_new_columns_load() {
        let old: ColumnLayout = serde_json::from_str(r#"["Title","Genre"]"#).unwrap();
        assert_eq!(old.visible(), [Title, Genre]);
        let full: ColumnLayout =
            serde_json::from_str(r#"{"order":["Title","Title","Bpm"],"hidden":[]}"#).unwrap();
        assert_eq!(full.visible(), [Title, Bpm]);
        assert!(!full.is_shown(Artist));
        let text = serde_json::to_string(&full).unwrap();
        assert_eq!(serde_json::from_str::<ColumnLayout>(&text).unwrap(), full);
    }

    #[test]
    fn a_layout_with_nothing_shown_falls_back_to_the_defaults() {
        let empty: ColumnLayout = serde_json::from_str("[]").unwrap();
        assert_eq!(empty.visible(), BrowserColumn::defaults());
    }

    #[test]
    fn cue_marks_skip_grid_markers_and_their_flags_are_clickable() {
        use backspn_library::CuePoint;
        let cue = |kind, start_s, hotcue| CuePoint {
            kind,
            name: String::new(),
            start_s,
            len_s: 0.0,
            hotcue,
            display_order: 0,
            repeats: -1,
            color: None,
        };
        let cues = [
            cue(CuePoint::GRID, 0.0, None),
            cue(CuePoint::CUE, 25.0, Some(0)),
            cue(CuePoint::LOAD, 50.0, None),
        ];
        let marks = cue_marks(&cues, Some(100.0));
        assert_eq!(marks.len(), 2, "no grid marker");
        assert_eq!((marks[0].at, marks[0].hotcue), (0.25, Some(1)));
        assert!(cue_marks(&cues, None).is_empty(), "no length, no places");
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(100.0, 20.0));
        assert_eq!(cue_under(rect, egui::pos2(30.0, 5.0), &marks), Some(0.25));
        assert_eq!(
            cue_under(rect, egui::pos2(30.0, 15.0), &marks),
            None,
            "below the flag"
        );
        assert_eq!(cue_under(rect, egui::pos2(40.0, 5.0), &marks), None);
    }

    #[test]
    fn the_cover_column_is_a_picture_and_joins_old_layouts_hidden() {
        use BrowserColumn::{Cover, Title};
        assert_eq!(Cover.name(), "Cover Art");
        assert!(!Cover.sortable());
        assert_eq!(Cover.attribute(), None);
        assert_eq!(Cover.text(0, &TrackRow::default(), KeyView::default()), "");
        let old: ColumnLayout = serde_json::from_str(r#"["Title"]"#).unwrap();
        assert_eq!(old.visible(), [Title]);
        assert!(!old.is_shown(Cover));
    }

    #[test]
    fn a_dragged_column_lands_beside_the_one_it_is_dropped_on() {
        use BrowserColumn::{Artist, Bpm, Key, Title};
        let mut layout = ColumnLayout::from_shown(&[Title, Artist, Bpm, Key]);
        layout.move_column(Key, Artist, false);
        assert_eq!(layout.visible(), [Title, Key, Artist, Bpm]);
        layout.move_column(Title, Bpm, true);
        assert_eq!(layout.visible(), [Key, Artist, Bpm, Title]);
        layout.move_column(Bpm, Bpm, true);
        assert_eq!(layout.visible(), [Key, Artist, Bpm, Title], "onto itself");
    }
}
