use std::path::{Path, PathBuf};

use base64::Engine as _;

pub fn hash(data: &[u8]) -> [u32; 4] {
    let mut state = [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476];
    let (blocks, rest) = data.as_chunks::<64>();
    for block in blocks {
        compress(&mut state, block);
    }
    let mut last = [0u8; 64];
    last[..rest.len()].copy_from_slice(rest);
    compress(&mut state, &last);
    state
}

#[allow(clippy::many_single_char_names)]
fn compress(state: &mut [u32; 4], block: &[u8; 64]) {
    const SHIFTS: [[u32; 4]; 4] = [
        [7, 12, 17, 22],
        [5, 9, 14, 20],
        [4, 11, 16, 23],
        [6, 10, 15, 21],
    ];
    let words: [u32; 16] = std::array::from_fn(|i| {
        u32::from_le_bytes(block[4 * i..4 * i + 4].try_into().expect("4 bytes"))
    });
    let [mut a, mut b, mut c, mut d] = *state;
    for i in 0..64 {
        let (f, g) = match i / 16 {
            0 => ((b & c) | (!b & d), i),
            1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
            2 => (b ^ c ^ d, (3 * i + 5) % 16),
            _ => (c ^ (b | !d), (7 * i) % 16),
        };
        let k = ((i as f64 + 1.0).sin().abs() * 4_294_967_296.0) as u32;
        let rotated = a
            .wrapping_add(f)
            .wrapping_add(k)
            .wrapping_add(words[g])
            .rotate_left(SHIFTS[i / 16][i % 4]);
        (a, d, c) = (d, c, b);
        b = b.wrapping_add(rotated);
    }
    for (s, v) in state.iter_mut().zip([a, b, c, d]) {
        *s = s.wrapping_add(v);
    }
}

pub fn file_name(hash: [u32; 4]) -> (String, String) {
    const LETTERS: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ012345";
    let name = hash
        .iter()
        .flat_map(|w| (0..7).map(move |k| LETTERS[((w >> (5 * k)) & 31) as usize] as char))
        .collect();
    (format!("{:03}", hash[0] & 0x7f), name)
}

pub fn track_hash(audio_id: Option<&str>, file: &Path) -> std::io::Result<[u32; 4]> {
    if let Some(bytes) = audio_id.and_then(|id| {
        base64::engine::general_purpose::STANDARD
            .decode(id.trim())
            .ok()
    }) {
        let mut id = [0u8; 256];
        let n = bytes.len().min(id.len());
        id[..n].copy_from_slice(&bytes[..n]);
        return Ok(hash(&id));
    }
    Ok(hash(&std::fs::read(file)?))
}

pub fn hash_text(hash: [u32; 4]) -> String {
    format!(
        "{:08x}{:08x}{:08x}{:08x}",
        hash[0], hash[1], hash[2], hash[3]
    )
}

pub fn parse_hash(text: &str) -> Option<[u32; 4]> {
    if text.len() != 32 {
        return None;
    }
    let word = |i: usize| u32::from_str_radix(text.get(8 * i..8 * i + 8)?, 16).ok();
    Some([word(0)?, word(1)?, word(2)?, word(3)?])
}

pub fn stem_file(base: &Path, hash: [u32; 4]) -> PathBuf {
    let (folder, name) = file_name(hash);
    base.join(folder).join(format!("{name}.stem.mp4"))
}

pub fn own_stem_file(base: &Path, hash: [u32; 4]) -> PathBuf {
    let (folder, name) = file_name(hash);
    base.join(folder)
        .join(format!("{name}.{}", backspn_formats::bstem::EXTENSION))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_compression_is_md5s() {
        let mut state = [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476];
        let mut block = [0u8; 64];
        block[0] = 0x80;
        compress(&mut state, &block);
        let digest: Vec<u8> = state.iter().flat_map(|w| w.to_le_bytes()).collect();
        let expected: Vec<u8> = (0..16)
            .map(|k| {
                u8::from_str_radix(&"d41d8cd98f00b204e9800998ecf8427e"[2 * k..2 * k + 2], 16)
                    .unwrap()
            })
            .collect();
        assert_eq!(digest, expected, "MD5 of nothing");
    }

    #[test]
    fn hashes_round_trip_as_text() {
        let h = [1, 0xdead_beef, 0, u32::MAX];
        assert_eq!(parse_hash(&hash_text(h)), Some(h));
        assert_eq!(parse_hash("xyz"), None);
    }

    #[test]
    fn names_follow_the_layout() {
        let (folder, name) = file_name([0x0000_0081, 0, 0, u32::MAX]);
        assert_eq!(folder, "001");
        assert_eq!(name.len(), 28);
        assert!(name.starts_with("BEAAAAA"), "{name}");
        assert!(name.ends_with("555555D"), "{name}");
        let base = Path::new("/stems");
        assert_eq!(
            stem_file(base, [5, 0, 0, 0]),
            Path::new("/stems/005/FAAAAAAAAAAAAAAAAAAAAAAAAAAA.stem.mp4")
        );
        assert_eq!(
            own_stem_file(base, [5, 0, 0, 0]),
            Path::new("/stems/005/FAAAAAAAAAAAAAAAAAAAAAAAAAAA.bstem")
        );
    }

    #[test]
    fn the_audio_id_decides_and_files_without_one_use_their_contents() {
        let dir = std::env::temp_dir().join(format!("backspn-storage-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.mp3");
        std::fs::write(&file, b"some audio").unwrap();
        let id = base64::engine::general_purpose::STANDARD.encode([7u8; 256]);
        let by_id = track_hash(Some(&id), &file).unwrap();
        assert_eq!(by_id, hash(&[7u8; 256]));
        assert_eq!(track_hash(None, &file).unwrap(), hash(b"some audio"));
        assert_eq!(
            track_hash(Some("not base64!"), &file).unwrap(),
            hash(b"some audio")
        );
        assert!(track_hash(None, &dir.join("missing")).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
