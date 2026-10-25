use std::path::Path;

use backspn_formats::nml::{KeyNotation, MusicalKey};
use lofty::config::{ParseOptions, WriteOptions};
use lofty::file::{AudioFile, FileType, TaggedFileExt};
use lofty::flac::FlacFile;
use lofty::id3::v2::{Frame, FrameId, Id3v2Tag, PopularimeterFrame};
use lofty::iff::aiff::AiffFile;
use lofty::iff::wav::WavFile;
use lofty::mp4::Mp4File;
use lofty::mpeg::MpegFile;
use lofty::ogg::{OpusFile, VorbisFile};
use lofty::tag::items::Timestamp;
use lofty::tag::{Accessor, ItemKey, MergeTag, SplitTag, Tag};

use crate::TrackRow;

pub const RATING_EMAIL: &str = "backspn";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FileTags {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub label: String,
    pub remixer: String,
    pub producer: String,
    pub comment: String,
    pub catalog_no: String,
    pub release_date: String,
    pub track_number: Option<u32>,
    pub bpm: Option<f64>,
    pub key_text: String,
    pub musical_key: Option<u8>,
    pub rating: u8,
    pub playcount: u32,
    pub bitrate: Option<u32>,
    pub duration_s: Option<f64>,
}

pub fn read(path: &Path) -> Option<FileTags> {
    let file = lofty::read_from_path(path).ok()?;
    let mut tags = file
        .primary_tag()
        .or_else(|| file.first_tag())
        .map(from_tag)
        .unwrap_or_default();
    if let Some(raw) = first_popm(path, file.file_type()) {
        tags.rating = raw;
    }
    let properties = file.properties();
    tags.bitrate = properties.audio_bitrate().map(|kbps| kbps * 1000);
    tags.duration_s = Some(properties.duration().as_secs_f64()).filter(|d| *d > 0.0);
    Some(tags)
}

fn first_popm(path: &Path, kind: FileType) -> Option<u8> {
    let options = ParseOptions::new().read_properties(false);
    let mut file = std::fs::File::open(path).ok()?;
    let tag = match kind {
        FileType::Mpeg => MpegFile::read_from(&mut file, options).ok()?.remove_id3v2(),
        FileType::Wav => WavFile::read_from(&mut file, options).ok()?.remove_id3v2(),
        FileType::Aiff => AiffFile::read_from(&mut file, options).ok()?.remove_id3v2(),
        _ => None,
    }?;
    tag.into_iter().find_map(|frame| match frame {
        Frame::Popularimeter(p) => Some(p.rating),
        _ => None,
    })
}

fn from_tag(tag: &Tag) -> FileTags {
    let text = |key: ItemKey| tag.get_string(key).unwrap_or_default().trim().to_owned();
    let trimmed = |value: Option<std::borrow::Cow<'_, str>>| {
        value.as_deref().unwrap_or_default().trim().to_owned()
    };
    let mut label = text(ItemKey::Label);
    if label.is_empty() {
        label = text(ItemKey::Publisher);
    }
    let key = text(ItemKey::InitialKey);
    let parsed = MusicalKey::parse(&key);
    let popularity = tag.ratings().next();
    FileTags {
        title: trimmed(tag.title()),
        artist: trimmed(tag.artist()),
        album: trimmed(tag.album()),
        genre: trimmed(tag.genre()),
        comment: trimmed(tag.comment()),
        label,
        remixer: text(ItemKey::Remixer),
        producer: text(ItemKey::Producer),
        catalog_no: text(ItemKey::CatalogNumber),
        release_date: tag
            .date()
            .map(|d| release_date(d.year, d.month, d.day))
            .unwrap_or_default(),
        track_number: tag.track(),
        bpm: [ItemKey::Bpm, ItemKey::IntegerBpm]
            .into_iter()
            .find_map(|k| {
                tag.get_string(k)?
                    .trim()
                    .replace(',', ".")
                    .parse::<f64>()
                    .ok()
            })
            .filter(|b| *b > 0.0),
        musical_key: parsed.map(MusicalKey::value),
        key_text: parsed.map_or(key, MusicalKey::open_key),
        rating: popularity
            .as_ref()
            .map_or(0, |p| (p.rating as u8).saturating_mul(51)),
        playcount: popularity.map_or(0, |p| u32::try_from(p.play_counter).unwrap_or(u32::MAX)),
        bitrate: None,
        duration_s: None,
    }
}

#[derive(Debug, thiserror::Error)]
#[error(
    "Could not write the tags of {path}: the file is not taggable, missing or write protected ({reason})"
)]
pub struct WriteError {
    pub path: String,
    pub reason: String,
}

pub fn write(path: &Path, track: &TrackRow, notation: KeyNotation) -> Result<(), WriteError> {
    let fail = |reason: String| WriteError {
        path: path.display().to_string(),
        reason,
    };
    let kind = lofty::probe::Probe::open(path)
        .map_err(|e| fail(e.to_string()))?
        .guess_file_type()
        .map_err(|e| fail(e.to_string()))?
        .file_type()
        .ok_or_else(|| fail("unknown file type".into()))?;
    let id3 = |tag: Option<Id3v2Tag>| id3v2_with(tag.unwrap_or_default(), track, notation);
    let result = match kind {
        FileType::Mpeg => edit::<MpegFile>(path, |f| {
            let tag = id3(f.remove_id3v2());
            f.set_id3v2(tag);
        }),
        FileType::Wav => edit::<WavFile>(path, |f| {
            let tag = id3(f.remove_id3v2());
            f.set_id3v2(tag);
        }),
        FileType::Aiff => edit::<AiffFile>(path, |f| {
            let tag = id3(f.remove_id3v2());
            f.set_id3v2(tag);
        }),
        FileType::Flac => edit::<FlacFile>(path, |f| {
            let tag = merged(
                f.remove_vorbis_comments().unwrap_or_default(),
                track,
                notation,
            );
            f.set_vorbis_comments(tag);
        }),
        FileType::Vorbis => edit::<VorbisFile>(path, |f| {
            let tag = std::mem::take(f.vorbis_comments_mut());
            *f.vorbis_comments_mut() = merged(tag, track, notation);
        }),
        FileType::Opus => edit::<OpusFile>(path, |f| {
            let tag = std::mem::take(f.vorbis_comments_mut());
            *f.vorbis_comments_mut() = merged(tag, track, notation);
        }),
        FileType::Mp4 => edit::<Mp4File>(path, |f| {
            let tag = merged(f.remove_ilst().unwrap_or_default(), track, notation);
            f.set_ilst(tag);
        }),
        other => return Err(fail(format!("tags are not supported for {other:?} files"))),
    };
    result.map_err(fail)
}

fn edit<F: AudioFile>(path: &Path, change: impl FnOnce(&mut F)) -> Result<(), String> {
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut audio = F::read_from(&mut file, ParseOptions::new()).map_err(|e| e.to_string())?;
    drop(file);
    change(&mut audio);
    audio
        .save_to_path(path, WriteOptions::default())
        .map_err(|e| e.to_string())
}

fn merged<T, R>(tag: T, track: &TrackRow, notation: KeyNotation) -> T
where
    T: SplitTag<Remainder = R>,
    R: MergeTag<Merged = T>,
{
    let (rest, mut generic) = tag.split_tag();
    fill(&mut generic, track, notation);
    rest.merge_tag(generic)
}

fn id3v2_with(mut tag: Id3v2Tag, track: &TrackRow, notation: KeyNotation) -> Id3v2Tag {
    let popm = FrameId::Valid(std::borrow::Cow::Borrowed("POPM"));
    let mut counter = 0;
    let others: Vec<Frame<'static>> = tag
        .remove(&popm)
        .filter(|frame| match frame {
            Frame::Popularimeter(p) if p.email == RATING_EMAIL => {
                counter = p.counter;
                false
            }
            _ => true,
        })
        .collect();
    let mut tag = merged(tag, track, notation);
    tag.insert(Frame::Popularimeter(PopularimeterFrame::new(
        RATING_EMAIL,
        track.rating,
        counter,
    )));
    for frame in others {
        tag.insert(frame);
    }
    tag
}

fn fill(tag: &mut Tag, t: &TrackRow, notation: KeyNotation) {
    let mut text = |key: ItemKey, value: &str| {
        tag.remove_key(key);
        if !value.trim().is_empty() {
            tag.insert_text(key, value.trim().to_owned());
        }
    };
    text(ItemKey::TrackTitle, &t.title);
    text(ItemKey::TrackArtist, &t.artist);
    text(ItemKey::AlbumTitle, &t.album);
    text(ItemKey::Genre, &t.genre);
    text(ItemKey::Label, &t.label);
    text(ItemKey::Remixer, &t.remixer);
    text(ItemKey::Producer, &t.producer);
    text(ItemKey::Comment, &t.comment);
    text(ItemKey::CatalogNumber, &t.catalog_no);
    let key = t
        .musical_key
        .and_then(MusicalKey::from_value)
        .map(|k| k.name(notation))
        .unwrap_or_default();
    text(ItemKey::InitialKey, &key);
    let bpm = t
        .bpm
        .filter(|b| *b > 0.0)
        .map(|b| format!("{}", b.round()))
        .unwrap_or_default();
    text(ItemKey::Bpm, "");
    text(ItemKey::IntegerBpm, &bpm);
    tag.remove_key(ItemKey::Year);
    tag.remove_key(ItemKey::RecordingDate);
    if let Some(date) = parse_release_date(&t.release_date) {
        tag.set_date(date);
    }
    match t.track_number.filter(|n| *n > 0) {
        Some(n) => tag.set_track(n),
        None => tag.remove_track(),
    }
}

fn parse_release_date(text: &str) -> Option<Timestamp> {
    let mut parts = text.trim().split(['/', '-']).map(str::parse::<u16>);
    let year = parts.next()?.ok().filter(|y| *y > 0)?;
    let mut part = |max: u16| {
        parts
            .next()
            .and_then(Result::ok)
            .filter(|v| (1..=max).contains(v))
            .and_then(|v| u8::try_from(v).ok())
    };
    let month = part(12);
    let day = month.and_then(|_| part(31));
    Some(Timestamp {
        year,
        month,
        day,
        ..Timestamp::default()
    })
}

fn release_date(year: u16, month: Option<u8>, day: Option<u8>) -> String {
    match (month, day) {
        (Some(m), Some(d)) => format!("{year}/{m}/{d}"),
        (Some(m), None) => format!("{year}/{m}"),
        _ => year.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_take_the_library_form() {
        assert_eq!(release_date(2024, Some(10), Some(4)), "2024/10/4");
        assert_eq!(release_date(2024, Some(3), None), "2024/3");
        assert_eq!(release_date(1999, None, None), "1999");
    }

    #[test]
    fn unreadable_files_have_no_tags() {
        assert_eq!(read(Path::new("/nonexistent/file.mp3")), None);
    }

    #[test]
    fn reads_the_fields_of_a_tag() {
        let mut tag = Tag::new(lofty::tag::TagType::Id3v2);
        tag.set_title("Flow".into());
        tag.set_artist("Ann".into());
        tag.set_genre("Techno".into());
        tag.insert_text(ItemKey::Publisher, "Label X".into());
        tag.insert_text(ItemKey::Remixer, "Bob".into());
        tag.insert_text(ItemKey::IntegerBpm, "128.5".into());
        tag.insert_text(ItemKey::InitialKey, "8A".into());
        tag.set_track(3);
        let out = from_tag(&tag);
        assert_eq!((out.title.as_str(), out.artist.as_str()), ("Flow", "Ann"));
        assert_eq!(out.label, "Label X", "publisher when there is no label");
        assert_eq!(out.remixer, "Bob");
        assert_eq!(out.bpm, Some(128.5));
        assert_eq!(out.key_text, "1m", "Camelot 8A kept as Open Key");
        assert_eq!(out.musical_key, Some(21), "A minor");
        assert_eq!(out.track_number, Some(3));
    }
}
