use crate::CuePoint;
use crate::external::Grid;

const FILE_MAGIC: &[u8; 4] = b"PMAI";
const BEATS: &[u8; 4] = b"PQTZ";
const CUES: &[u8; 4] = b"PCOB";
const CUE: &[u8; 4] = b"PCPT";
const CUES_EXTENDED: &[u8; 4] = b"PCO2";
const CUE_EXTENDED: &[u8; 4] = b"PCP2";
const LOOP: u8 = 2;
const NO_LOOP: u32 = u32::MAX;
const MAX_EXTRA_MARKERS: usize = 7;
const MAX_COMMENT: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Beat {
    pub time_ms: u32,
    pub bpm: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cue {
    pub hotcue: Option<u8>,
    pub start_ms: u32,
    pub end_ms: Option<u32>,
    pub comment: String,
    pub rgb: Option<[u8; 3]>,
}

fn u16_at(data: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(data.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

fn len_at(data: &[u8], at: usize) -> Option<usize> {
    usize::try_from(u32_at(data, at)?).ok()
}

/// Sections of an analysis file: (four-letter tag, whole section bytes).
pub fn sections(data: &[u8]) -> Vec<(&[u8], &[u8])> {
    let mut out = Vec::new();
    if !data.starts_with(FILE_MAGIC) {
        return out;
    }
    let Some(mut at) = len_at(data, 4) else {
        return out;
    };
    while let Some(len) = len_at(data, at + 8) {
        let Some(section) = data.get(at..at.saturating_add(len)).filter(|_| len >= 12) else {
            break;
        };
        out.push((&section[..4], section));
        at += len;
    }
    out
}

/// Entries of a list section, each starting with `tag`.
fn entries(section: &[u8], tag: [u8; 4]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let Some(mut at) = len_at(section, 4) else {
        return out;
    };
    while section.get(at..at + 4) == Some(tag.as_slice()) {
        let Some(entry) = len_at(section, at + 8)
            .filter(|len| *len >= 12)
            .and_then(|len| section.get(at..at.saturating_add(len)))
        else {
            break;
        };
        out.push(entry);
        at += entry.len();
    }
    out
}

pub fn beats(data: &[u8]) -> Vec<Beat> {
    let Some((_, section)) = sections(data).into_iter().find(|(t, _)| *t == BEATS) else {
        return Vec::new();
    };
    let (Some(start), Some(count)) = (len_at(section, 4), len_at(section, 20)) else {
        return Vec::new();
    };
    (0..count)
        .map_while(|i| {
            let at = start + i * 8;
            Some(Beat {
                bpm: f64::from(u16_at(section, at + 2)?) / 100.0,
                time_ms: u32_at(section, at + 4)?,
            })
        })
        .collect()
}

pub fn grid(beats: &[Beat]) -> Option<Grid> {
    let first = beats.iter().find(|b| b.bpm > 0.0)?;
    let mut bpm = first.bpm;
    let mut markers_s = Vec::new();
    for b in beats
        .iter()
        .filter(|b| b.bpm > 0.0 && b.time_ms > first.time_ms)
    {
        if (b.bpm - bpm).abs() >= 0.005 && markers_s.len() < MAX_EXTRA_MARKERS {
            markers_s.push(f64::from(b.time_ms) / 1000.0);
            bpm = b.bpm;
        }
    }
    Some(Grid {
        bpm: first.bpm,
        anchor_s: f64::from(first.time_ms) / 1000.0,
        markers_s,
    })
}

fn hotcue(number: u32) -> Option<u8> {
    number.checked_sub(1).and_then(|n| u8::try_from(n).ok())
}

fn end(kind: u8, start: u32, end: u32) -> Option<u32> {
    (kind == LOOP && end != NO_LOOP && end > start).then_some(end)
}

fn basic_cue(entry: &[u8]) -> Option<Cue> {
    let start_ms = u32_at(entry, 32)?;
    Some(Cue {
        hotcue: hotcue(u32_at(entry, 12)?),
        start_ms,
        end_ms: end(*entry.get(28)?, start_ms, u32_at(entry, 36)?),
        comment: String::new(),
        rgb: None,
    })
}

fn extended_cue(entry: &[u8]) -> Option<Cue> {
    let start_ms = u32_at(entry, 20)?;
    let comment_len = len_at(entry, 40).unwrap_or(0).min(MAX_COMMENT);
    let comment = entry
        .get(44..44 + comment_len)
        .map_or_else(String::new, |raw| {
            let units: Vec<u16> = raw
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_be_bytes(*c))
                .take_while(|u| *u != 0)
                .collect();
            String::from_utf16_lossy(&units)
        });
    let colour = entry.get(44 + comment_len..44 + comment_len + 4);
    let rgb = colour.filter(|c| c[0] != 0).map(|c| [c[1], c[2], c[3]]);
    Some(Cue {
        hotcue: hotcue(u32_at(entry, 12)?),
        start_ms,
        end_ms: end(*entry.get(16)?, start_ms, u32_at(entry, 24)?),
        comment,
        rgb,
    })
}

/// Cues from the extended analysis file, or else from the basic one.
pub fn cues(basic: &[u8], extended: &[u8]) -> Vec<Cue> {
    let read = |data: &[u8], list: &[u8; 4], tag: &[u8; 4], parse: fn(&[u8]) -> Option<Cue>| {
        sections(data)
            .into_iter()
            .filter(|(t, _)| *t == list)
            .flat_map(|(_, s)| entries(s, *tag))
            .filter_map(parse)
            .collect::<Vec<_>>()
    };
    let extended = read(extended, CUES_EXTENDED, CUE_EXTENDED, extended_cue);
    if extended.is_empty() {
        read(basic, CUES, CUE, basic_cue)
    } else {
        extended
    }
}

pub fn cue_point(c: &Cue) -> CuePoint {
    let start_s = f64::from(c.start_ms) / 1000.0;
    CuePoint {
        kind: if c.end_ms.is_some() {
            CuePoint::LOOP
        } else {
            CuePoint::CUE
        },
        name: c.comment.clone(),
        start_s,
        len_s: c.end_ms.map_or(0.0, |e| f64::from(e) / 1000.0 - start_s),
        hotcue: c.hotcue,
        display_order: 0,
        repeats: -1,
        color: c.rgb.map(|[r, g, b]| format!("#{r:02X}{g:02X}{b:02X}")),
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub fn file(sections: &[Vec<u8>]) -> Vec<u8> {
        let body: usize = sections.iter().map(Vec::len).sum();
        let mut out = FILE_MAGIC.to_vec();
        out.extend(28u32.to_be_bytes());
        out.extend(u32::try_from(28 + body).unwrap().to_be_bytes());
        out.resize(28, 0);
        for s in sections {
            out.extend(s);
        }
        out
    }

    fn section(tag: [u8; 4], header: usize, fields: &[u8], body: &[u8]) -> Vec<u8> {
        let mut out = tag.to_vec();
        out.extend(u32::try_from(header).unwrap().to_be_bytes());
        out.extend(u32::try_from(header + body.len()).unwrap().to_be_bytes());
        out.extend(fields);
        out.resize(header, 0);
        out.extend(body);
        out
    }

    pub fn beat_section(beats: &[(u16, u32)]) -> Vec<u8> {
        let mut fields = vec![0; 8];
        fields.extend(u32::try_from(beats.len()).unwrap().to_be_bytes());
        let mut body = Vec::new();
        for (i, (bpm, ms)) in beats.iter().enumerate() {
            body.extend(u16::try_from(i % 4 + 1).unwrap().to_be_bytes());
            body.extend(bpm.to_be_bytes());
            body.extend(ms.to_be_bytes());
        }
        section(*BEATS, 24, &fields, &body)
    }

    pub fn basic_cues(cues: &[(u32, u8, u32, u32)]) -> Vec<u8> {
        let mut body = Vec::new();
        for (hot, kind, start, end) in cues {
            let mut fields = hot.to_be_bytes().to_vec();
            fields.resize(16, 0);
            fields.push(*kind);
            fields.resize(20, 0);
            fields.extend(start.to_be_bytes());
            fields.extend(end.to_be_bytes());
            body.extend(section(*CUE, 0x38, &fields, &[]));
        }
        section(*CUES, 24, &[0; 12], &body)
    }

    type ExtendedCue<'a> = (u32, u8, u32, u32, &'a str, [u8; 4]);

    pub fn extended_cues(cues: &[ExtendedCue]) -> Vec<u8> {
        let mut body = Vec::new();
        for (hot, kind, start, end, comment, colour) in cues {
            let mut fields = hot.to_be_bytes().to_vec();
            fields.push(*kind);
            fields.resize(8, 0);
            fields.extend(start.to_be_bytes());
            fields.extend(end.to_be_bytes());
            fields.resize(28, 0);
            let mut text: Vec<u8> = comment
                .encode_utf16()
                .chain([0])
                .flat_map(u16::to_be_bytes)
                .collect();
            fields.extend(u32::try_from(text.len()).unwrap().to_be_bytes());
            fields.append(&mut text);
            fields.extend(colour);
            fields.resize(fields.len() + 4, 0);
            body.extend(CUE_EXTENDED);
            body.extend(16u32.to_be_bytes());
            body.extend(u32::try_from(12 + fields.len()).unwrap().to_be_bytes());
            body.extend(&fields);
        }
        section(*CUES_EXTENDED, 20, &[0; 8], &body)
    }

    #[test]
    fn beats_give_a_grid_with_tempo_changes() {
        let data = file(&[beat_section(&[
            (12800, 120),
            (12800, 589),
            (12800, 1058),
            (13000, 1526),
        ])]);
        let beats = beats(&data);
        assert_eq!(beats.len(), 4);
        assert_eq!(
            beats[1],
            Beat {
                time_ms: 589,
                bpm: 128.0
            }
        );
        let g = grid(&beats).unwrap();
        assert!((g.bpm - 128.0).abs() < 1e-9);
        assert!((g.anchor_s - 0.12).abs() < 1e-9);
        assert_eq!(g.markers_s, vec![1.526]);
        assert_eq!(grid(&[]), None);
    }

    #[test]
    fn extended_cues_win_over_basic_ones() {
        let basic = file(&[basic_cues(&[(1, 1, 1000, NO_LOOP), (0, 2, 2000, 4000)])]);
        let only_basic = cues(&basic, &[]);
        assert_eq!(only_basic.len(), 2);
        assert_eq!(only_basic[0].hotcue, Some(0));
        assert_eq!(only_basic[0].end_ms, None);
        assert_eq!(
            (only_basic[1].hotcue, only_basic[1].end_ms),
            (None, Some(4000))
        );

        let extended = file(&[extended_cues(&[
            (3, 1, 500, NO_LOOP, "Drop", [1, 0xFF, 0x10, 0x20]),
            (2, 2, 8000, 9000, "", [0, 0, 0, 0]),
        ])]);
        let both = cues(&basic, &extended);
        assert_eq!(both.len(), 2);
        assert_eq!(both[0].hotcue, Some(2));
        assert_eq!(both[0].comment, "Drop");
        assert_eq!(both[0].rgb, Some([0xFF, 0x10, 0x20]));
        let p = cue_point(&both[1]);
        assert_eq!((p.kind, p.hotcue, p.color), (CuePoint::LOOP, Some(1), None));
        assert!((p.start_s - 8.0).abs() < 1e-9 && (p.len_s - 1.0).abs() < 1e-9);
    }

    #[test]
    fn broken_files_give_nothing() {
        assert_eq!(sections(b"nope").len(), 0);
        let mut data = file(&[beat_section(&[(12000, 0)])]);
        data.truncate(data.len() - 3);
        assert_eq!(beats(&data), vec![]);
        assert_eq!(cues(&data, b"PMAI"), vec![]);
    }
}
