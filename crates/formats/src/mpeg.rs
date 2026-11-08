//! The start of an MPEG audio (Layer III) stream: the header frame some
//! encoders write before the music and the encoder delay it announces.
//!
//! Our decoder skips that header frame and trims the encoder delay, so sample
//! 0 is the first sample of the music. A decoder that plays the header frame
//! as silence and keeps the delay puts [`StreamStart::untrimmed_lead`] samples
//! before it, and every position it stores is later than ours by that much.

const HEADER_LEN: usize = 4;
pub const ID3_HEADER_LEN: usize = 10;
const VBRI_OFFSET: usize = HEADER_LEN + 32;
const LAME_DECODER_DELAY: u32 = 529;

/// Bytes past the `ID3v2` tag that are enough to find the first frame and read
/// its header tag.
pub const SCAN_LEN: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamStart {
    pub sample_rate: u32,
    /// Samples in the leading Xing/Info/VBRI frame, 0 without one.
    pub header_frame: u32,
    /// Samples trimmed from the start as the LAME tag says, 0 without one.
    pub encoder_delay: u32,
}

impl StreamStart {
    /// Samples that come before our first one in an untrimmed decode.
    pub fn untrimmed_lead(&self) -> u32 {
        self.header_frame + self.encoder_delay
    }

    pub fn untrimmed_lead_s(&self) -> f64 {
        f64::from(self.untrimmed_lead()) / f64::from(self.sample_rate)
    }
}

/// Reads the start of a Layer III stream from the first bytes of the file,
/// or from the bytes right after its `ID3v2` tag. `None` when no Layer III
/// frame is found in `bytes`.
pub fn stream_start(bytes: &[u8]) -> Option<StreamStart> {
    let (at, header) = first_frame(bytes, id3v2_len(bytes))?;
    let frame = &bytes[at..at + header.frame_len];
    let delay = info_tag(frame, &header).or_else(|| vbri_tag(frame, &header).then_some(0));
    Some(StreamStart {
        sample_rate: header.sample_rate,
        header_frame: if delay.is_some() { header.samples } else { 0 },
        encoder_delay: delay.unwrap_or(0),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Header {
    mpeg1: bool,
    sample_rate: u32,
    mono: bool,
    crc: bool,
    samples: u32,
    frame_len: usize,
}

impl Header {
    fn parse(word: u32) -> Option<Self> {
        const MPEG1: [u32; 15] = [
            0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
        ];
        const MPEG2: [u32; 15] = [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160];
        if word & 0xffe0_0000 != 0xffe0_0000 || (word >> 17) & 0x3 != 0b01 {
            return None;
        }
        let (mpeg1, rates) = match (word >> 19) & 0x3 {
            0b11 => (true, [44_100, 48_000, 32_000]),
            0b10 => (false, [22_050, 24_000, 16_000]),
            0b00 => (false, [11_025, 12_000, 8_000]),
            _ => return None,
        };
        let sample_rate = *rates.get(((word >> 10) & 0x3) as usize)?;
        let kbps = match ((word >> 12) & 0xf) as usize {
            0 | 15 => return None,
            i if mpeg1 => MPEG1[i],
            i => MPEG2[i],
        };
        let factor = if mpeg1 { 144 } else { 72 };
        Some(Self {
            mpeg1,
            sample_rate,
            mono: (word >> 6) & 0x3 == 0b11,
            crc: word & 0x1_0000 == 0,
            samples: if mpeg1 { 1152 } else { 576 },
            frame_len: (factor * kbps * 1000 / sample_rate + ((word >> 9) & 1)) as usize,
        })
    }

    fn similar(&self, other: &Self) -> bool {
        (self.mpeg1, self.sample_rate, self.mono) == (other.mpeg1, other.sample_rate, other.mono)
    }

    fn side_info_len(&self) -> usize {
        match (self.mpeg1, self.mono) {
            (true, false) => 32,
            (true, true) | (false, false) => 17,
            (false, true) => 9,
        }
    }

    fn header_len(&self) -> usize {
        HEADER_LEN + if self.crc { 2 } else { 0 }
    }
}

fn word_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(*bytes.get(at..)?.first_chunk()?))
}

/// The first frame whose next frame header follows right after it and
/// matches it, so that sync words inside other data are skipped.
fn first_frame(bytes: &[u8], from: usize) -> Option<(usize, Header)> {
    let mut at = from;
    while at + HEADER_LEN <= bytes.len() {
        if let Some(header) = Header::parse(word_at(bytes, at)?) {
            let next = at + header.frame_len;
            let follows = word_at(bytes, next).and_then(Header::parse);
            match follows {
                Some(n) if header.similar(&n) => return Some((at, header)),
                None if next + HEADER_LEN > bytes.len() => return None,
                _ => {}
            }
        }
        at += 1;
    }
    None
}

/// Length of the `ID3v2` tag the file starts with, from its first
/// [`ID3_HEADER_LEN`] bytes; 0 without one. Skipping it saves reading cover
/// art before the audio.
pub fn id3v2_len(bytes: &[u8]) -> usize {
    let Some(head) = bytes.first_chunk::<ID3_HEADER_LEN>() else {
        return 0;
    };
    if &head[..3] != b"ID3" || head[6..].iter().any(|b| b & 0x80 != 0) {
        return 0;
    }
    let size = head[6..]
        .iter()
        .fold(0usize, |n, &b| (n << 7) | usize::from(b));
    let footer = if head[5] & 0x10 != 0 {
        ID3_HEADER_LEN
    } else {
        0
    };
    ID3_HEADER_LEN + size + footer
}

/// `Some(delay)` when the frame holds a Xing/Info tag; the delay is 0
/// without a usable LAME extension.
fn info_tag(frame: &[u8], header: &Header) -> Option<u32> {
    let offset = HEADER_LEN + header.side_info_len();
    let id = frame.get(offset..offset + 4)?;
    if (id != b"Xing" && id != b"Info")
        || frame
            .get(header.header_len()..offset)?
            .iter()
            .any(|&b| b != 0)
    {
        return None;
    }
    let flags = word_at(frame, offset + 4)?;
    let fields = [(0x1, 4), (0x2, 4), (0x4, 100), (0x8, 4)];
    let lame = offset
        + 8
        + fields
            .iter()
            .filter(|(f, _)| flags & f != 0)
            .map(|(_, n)| n)
            .sum::<usize>();
    if lame > frame.len() {
        return None;
    }
    Some(lame_delay(frame, header, lame).unwrap_or(0))
}

fn lame_delay(frame: &[u8], header: &Header, at: usize) -> Option<u32> {
    const MIN_LEN: usize = 24;
    const FULL_LEN: usize = 36;
    let ext = frame.get(at..at + MIN_LEN)?;
    let encoder = &ext[..4];
    let trim = u32::from_be_bytes([0, ext[21], ext[22], ext[23]]);
    let has_crc = header.crc || encoder == b"LAME";
    let crc_at = at + FULL_LEN - 2;
    if has_crc && let Some(stored) = frame.get(crc_at..crc_at + 2) {
        let stored = u16::from_be_bytes([stored[0], stored[1]]);
        if stored != 0 && stored != crc16(&frame[..crc_at]) {
            return None;
        }
    }
    Some(
        if [b"LAME", b"Lavf", b"Lavc"].contains(&encoder.try_into().ok()?) {
            LAME_DECODER_DELAY + (trim >> 12)
        } else {
            0
        },
    )
}

fn vbri_tag(frame: &[u8], header: &Header) -> bool {
    const MIN_LEN: usize = 26;
    frame.len() >= VBRI_OFFSET + MIN_LEN
        && &frame[VBRI_OFFSET..VBRI_OFFSET + 4] == b"VBRI"
        && frame[header.header_len()..VBRI_OFFSET]
            .iter()
            .all(|&b| b == 0)
}

/// CRC-16 with the reflected 0x8005 polynomial and a zero start, as the LAME
/// tag uses.
fn crc16(bytes: &[u8]) -> u16 {
    bytes.iter().fold(0u16, |crc, &b| {
        (0..8).fold(crc ^ u16::from(b), |c, _| {
            if c & 1 == 0 {
                c >> 1
            } else {
                (c >> 1) ^ 0xa001
            }
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// MPEG-1 Layer III, 128 kbit/s, 44.1 kHz, stereo: 417-byte frames.
    const HEADER: [u8; 4] = [0xff, 0xfb, 0x90, 0x00];

    fn frame() -> Vec<u8> {
        let mut f = vec![0; 417];
        f[..4].copy_from_slice(&HEADER);
        f
    }

    /// A header frame with a LAME tag announcing `delay` and `padding`.
    fn info_frame(encoder: [u8; 4], delay: u32, padding: u32, crc_ok: bool) -> Vec<u8> {
        let mut f = frame();
        f[36..40].copy_from_slice(b"Info");
        f[40..44].copy_from_slice(&0xfu32.to_be_bytes());
        let lame = 36 + 8 + 4 + 4 + 100 + 4;
        f[lame..lame + 4].copy_from_slice(&encoder);
        let trim = (delay - LAME_DECODER_DELAY) << 12 | padding;
        f[lame + 21..lame + 24].copy_from_slice(&trim.to_be_bytes()[1..]);
        let crc = crc16(&f[..lame + 34]) ^ u16::from(!crc_ok);
        f[lame + 34..lame + 36].copy_from_slice(&crc.to_be_bytes());
        f
    }

    /// `frames` silent audio frames after `lead`.
    fn stream(lead: &[u8], frames: usize) -> Vec<u8> {
        let mut out = lead.to_vec();
        for _ in 0..frames {
            out.extend(frame());
        }
        out
    }

    fn start(bytes: &[u8]) -> (u32, u32) {
        let s = stream_start(bytes).unwrap();
        assert_eq!(s.sample_rate, 44_100);
        (s.header_frame, s.encoder_delay)
    }

    #[test]
    fn crc_matches_the_reference_check_value() {
        assert_eq!(crc16(b"123456789"), 0xbb3d);
    }

    #[test]
    fn plain_stream_has_no_lead() {
        assert_eq!(start(&stream(&[], 4)), (0, 0));
    }

    #[test]
    fn lame_tag_adds_header_frame_and_delay() {
        let bytes = stream(&info_frame(*b"LAME", 1105, 600, true), 4);
        assert_eq!(start(&bytes), (1152, 1105));
        let s = stream_start(&bytes).unwrap();
        assert_eq!(s.untrimmed_lead(), 2257);
        assert!((s.untrimmed_lead_s() - 2257.0 / 44_100.0).abs() < 1e-12);
    }

    #[test]
    fn other_encoders_tags_count_only_the_frame() {
        assert_eq!(
            start(&stream(&info_frame(*b"Lavc", 1105, 0, true), 4)),
            (1152, 1105)
        );
        assert_eq!(
            start(&stream(&info_frame(*b"Fhg ", 1105, 0, true), 4)),
            (1152, 0)
        );
    }

    #[test]
    fn bad_crc_drops_the_delay_but_not_the_frame() {
        assert_eq!(
            start(&stream(&info_frame(*b"LAME", 1105, 0, false), 4)),
            (1152, 0)
        );
    }

    #[test]
    fn vbri_frame_counts_as_header_frame() {
        let mut vbri = frame();
        vbri[36..40].copy_from_slice(b"VBRI");
        assert_eq!(start(&stream(&vbri, 4)), (1152, 0));
    }

    #[test]
    fn id3_tag_and_junk_are_skipped() {
        let mut bytes = b"ID3\x04\x00\x00\x00\x00\x01\x00".to_vec();
        bytes.extend([0xff; 128]);
        bytes.extend([0xff, 0xfb, 0x90, 0x00, 1, 2, 3]);
        bytes.extend(stream(&info_frame(*b"LAME", 1105, 0, true), 4));
        assert_eq!(id3v2_len(&bytes), 138);
        assert_eq!(start(&bytes), (1152, 1105));
        assert_eq!(start(&bytes[138..]), (1152, 1105));
    }

    #[test]
    fn mono_mpeg2_uses_its_own_offsets() {
        // MPEG-2 Layer III, 64 kbit/s, 22.05 kHz, mono: 208-byte frames.
        let header = [0xff, 0xf3, 0x80, 0xc0];
        let mut f = vec![0; 208];
        f[..4].copy_from_slice(&header);
        let mut tag = f.clone();
        tag[13..17].copy_from_slice(b"Xing");
        let mut bytes = tag;
        for _ in 0..3 {
            bytes.extend(&f);
        }
        let s = stream_start(&bytes).unwrap();
        assert_eq!(
            (s.sample_rate, s.header_frame, s.encoder_delay),
            (22_050, 576, 0)
        );
    }

    #[test]
    fn non_mpeg_data_has_no_start() {
        assert_eq!(stream_start(b"fLaC\0\0\0\x22 not an mpeg stream"), None);
        assert_eq!(stream_start(&[]), None);
    }
}
