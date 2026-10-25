use serde::{Deserialize, Serialize};

pub const STEMS: usize = 4;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StemMetadata {
    pub version: u32,
    #[serde(default)]
    pub offset: i64,
    pub stems: Vec<StemInfo>,
    #[serde(default)]
    pub mastering_dsp: MasteringDsp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StemInfo {
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct MasteringDsp {
    #[serde(default)]
    pub compressor: Compressor,
    #[serde(default)]
    pub limiter: Limiter,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[allow(clippy::struct_field_names)]
pub struct Compressor {
    pub enabled: bool,
    pub ratio: f32,
    pub threshold: f32,
    pub attack: f32,
    pub release: f32,
    pub input_gain: f32,
    pub output_gain: f32,
    pub hp_cutoff: f32,
    pub dry_wet: f32,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Limiter {
    pub enabled: bool,
    pub threshold: f32,
    pub ceiling: f32,
    pub release: f32,
}

impl StemInfo {
    pub fn rgb(&self) -> Option<[u8; 3]> {
        let hex = self.color.strip_prefix('#')?;
        if hex.len() != 6 {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
        Some([byte(0)?, byte(2)?, byte(4)?])
    }
}

pub fn metadata(mp4: &[u8]) -> Option<StemMetadata> {
    let json = find_box(mp4, &[*b"moov", *b"udta", *b"stem"])?;
    serde_json::from_slice(json).ok()
}

struct BoxSpan {
    kind: [u8; 4],
    start: usize,
    header: usize,
    end: usize,
}

fn top_boxes(data: &[u8]) -> Option<Vec<BoxSpan>> {
    let mut boxes = Vec::new();
    let mut at = 0;
    while at + 8 <= data.len() {
        let size32 = u32::from_be_bytes(data[at..at + 4].try_into().ok()?);
        let kind: [u8; 4] = data[at + 4..at + 8].try_into().ok()?;
        let (header, size) = match size32 {
            1 => (
                16,
                usize::try_from(u64::from_be_bytes(
                    data.get(at + 8..at + 16)?.try_into().ok()?,
                ))
                .ok()?,
            ),
            0 => (8, data.len() - at),
            n => (8, usize::try_from(n).ok()?),
        };
        if size < header || at + size > data.len() {
            return None;
        }
        boxes.push(BoxSpan {
            kind,
            start: at,
            header,
            end: at + size,
        });
        at += size;
    }
    Some(boxes)
}

fn find_box<'a>(data: &'a [u8], path: &[[u8; 4]]) -> Option<&'a [u8]> {
    let (first, rest) = path.split_first()?;
    let found = top_boxes(data)?.into_iter().find(|b| b.kind == *first)?;
    let payload = &data[found.start + found.header..found.end];
    if rest.is_empty() {
        Some(payload)
    } else {
        find_box(payload, rest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boxed(kind: [u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut out = u32::try_from(payload.len() + 8)
            .unwrap()
            .to_be_bytes()
            .to_vec();
        out.extend_from_slice(&kind);
        out.extend_from_slice(payload);
        out
    }

    const JSON: &str = r##"{"mastering_dsp": {"compressor": {"ratio": 3, "output_gain": 0, "enabled": true, "threshold": 0, "attack": 0.003, "input_gain": 6, "release": 0.3, "hp_cutoff": 300, "dry_wet": 50}, "limiter": {"release": 0.001, "threshold": 0, "ceiling": 0, "enabled": true}}, "version": 1, "stems": [{"color": "#fd4a4a", "name": "Drums"}, {"color": "#ff40eb", "name": "Bass"}, {"color": "#00d690", "name": "Atmos & Chords"}, {"color": "#fa8d29", "name": "Vox & FX"}]}"##;

    #[test]
    fn reads_the_stem_box_inside_moov_udta() {
        let udta = boxed(
            *b"udta",
            &[boxed(*b"meta", &[0; 12]), boxed(*b"stem", JSON.as_bytes())].concat(),
        );
        let file = [
            boxed(*b"ftyp", b"M4A "),
            boxed(*b"moov", &[boxed(*b"mvhd", &[0; 20]), udta].concat()),
            boxed(*b"mdat", &[1; 32]),
        ]
        .concat();
        let meta = metadata(&file).unwrap();
        assert_eq!(meta.version, 1);
        assert_eq!(meta.stems.len(), STEMS);
        assert_eq!(meta.stems[2].name, "Atmos & Chords");
        assert_eq!(meta.stems[0].rgb(), Some([0xfd, 0x4a, 0x4a]));
        assert!(meta.mastering_dsp.compressor.enabled);
        assert!((meta.mastering_dsp.compressor.ratio - 3.0).abs() < f32::EPSILON);
        assert!((meta.mastering_dsp.limiter.release - 0.001).abs() < 1e-6);
    }

    #[test]
    fn files_without_a_stem_box_or_damaged_ones_have_none() {
        assert_eq!(metadata(&boxed(*b"moov", &boxed(*b"udta", &[]))), None);
        let mut cut = boxed(
            *b"moov",
            &boxed(*b"udta", &boxed(*b"stem", JSON.as_bytes())),
        );
        cut.truncate(cut.len() - 10);
        assert_eq!(metadata(&cut), None);
        assert_eq!(
            StemInfo {
                name: String::new(),
                color: "red".into()
            }
            .rgb(),
            None
        );
    }

    #[test]
    fn reads_factory_stems() {
        let Some(dir) = std::env::var_os("BACKSPN_STEM_FILES") else {
            return;
        };
        let dir = std::path::Path::new(&dir);
        let Some(file) = std::fs::read_dir(dir).ok().and_then(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .find(|p| p.to_string_lossy().ends_with(".stem.mp4"))
        }) else {
            return;
        };
        let meta = metadata(&std::fs::read(&file).unwrap()).unwrap();
        assert_eq!(meta.stems.len(), STEMS, "{}", file.display());
    }
}
