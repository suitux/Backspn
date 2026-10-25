use crate::Error;
use crate::xml;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Tsi {
    pub devices: Vec<Device>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Device {
    pub entry: String,
    pub name: String,
    pub comment: String,
    pub in_port: String,
    pub out_port: String,
    pub target: u32,
    pub mappings: Vec<Mapping>,
    pub controls: Vec<ControlDef>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ControlDef {
    pub name: String,
    pub min: f32,
    pub max: f32,
    pub encoder: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Typed {
    #[default]
    None,
    Int(i32),
    Float(f32),
}

impl Typed {
    pub fn as_f64(self) -> Option<f64> {
        match self {
            Self::None => None,
            Self::Int(i) => Some(f64::from(i)),
            Self::Float(f) => Some(f64::from(f)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Condition {
    pub control: u32,
    pub channel: i32,
    pub value: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct MappingOptions {
    pub controller: u32,
    pub auto_repeat: bool,
    pub invert: bool,
    pub soft_takeover: bool,
    pub sensitivity: f32,
    pub acceleration: f32,
    pub set_to_value: bool,
    pub conditions: [Option<Condition>; 2],
    pub range: (Typed, Typed),
    pub midi_range: (i32, i32),
    pub output_invert: bool,
    pub blend: bool,
    pub step: Typed,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Mapping {
    pub control: String,
    pub output: bool,
    pub command: u32,
    pub interaction: u32,
    pub assignment: u32,
    pub options: MappingOptions,
    pub value: i32,
    pub comment: String,
    pub raw: Vec<u32>,
}

impl Tsi {
    pub fn parse(text: &str) -> Result<Self, Error> {
        let doc = xml::parse(text)?;
        let mut devices = Vec::new();
        let settings = doc
            .root
            .child(crate::interop::TSI_SETTINGS)
            .unwrap_or(&doc.root);
        for entry in settings.children_named("Entry") {
            let name = entry.attr("Name").unwrap_or_default();
            if !name.starts_with("DeviceIO.Config.") {
                continue;
            }
            let blob = decode_base64(entry.attr("Value").unwrap_or_default())
                .ok_or_else(|| bad(format!("{name}: invalid base64")))?;
            for mut device in parse_blob(&blob)? {
                name.clone_into(&mut device.entry);
                devices.push(device);
            }
        }
        Ok(Self { devices })
    }

    pub fn from_path(path: impl AsRef<std::path::Path>) -> Result<Self, Error> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
            path: path.to_owned(),
            source,
        })?;
        Self::parse(&text)
    }
}

pub fn parse_blob(blob: &[u8]) -> Result<Vec<Device>, Error> {
    let mut devices = Vec::new();
    let Some(root) = chunks(blob).find(|c| c.tag == *b"DIOM") else {
        return Err(bad("missing DIOM chunk".into()));
    };
    let Some(devs) = chunks(root.body).find(|c| c.tag == *b"DEVS") else {
        return Ok(devices);
    };
    for devi in chunks(devs.body.get(4..).unwrap_or_default()).filter(|c| c.tag == *b"DEVI") {
        devices.push(parse_device(devi.body)?);
    }
    Ok(devices)
}

fn parse_device(body: &[u8]) -> Result<Device, Error> {
    let mut r = Reader::new(body);
    let name = r.string()?;
    let mut device = Device {
        name,
        ..Device::default()
    };
    let Some(ddat) = chunks(r.rest()).find(|c| c.tag == *b"DDAT") else {
        return Ok(device);
    };
    let mut bindings = std::collections::HashMap::new();
    let mut raw_mappings = Vec::new();
    for chunk in chunks(ddat.body) {
        match &chunk.tag {
            b"DDIC" => device.comment = Reader::new(chunk.body).string().unwrap_or_default(),
            b"DDIF" => device.target = Reader::new(chunk.body).u32().unwrap_or_default(),
            b"DDDC" => device.controls = control_defs(chunk.body),
            b"DDPT" => {
                let mut p = Reader::new(chunk.body);
                device.in_port = p.string().unwrap_or_default();
                device.out_port = p.string().unwrap_or_default();
            }
            b"DDCB" => {
                for sub in chunks(chunk.body) {
                    match &sub.tag {
                        b"CMAS" => {
                            let list = sub.body.get(4..).unwrap_or_default();
                            raw_mappings
                                .extend(chunks(list).filter(|c| c.tag == *b"CMAI").map(|c| c.body));
                        }
                        b"DCBM" => {
                            let list = sub.body.get(4..).unwrap_or_default();
                            for b in chunks(list).filter(|c| c.tag == *b"DCBM") {
                                let mut br = Reader::new(b.body);
                                let id = br.u32()?;
                                bindings.insert(id, br.string()?);
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    for body in raw_mappings {
        let mut m = Reader::new(body);
        let binding = m.u32()?;
        let output = m.u32()? != 0;
        let command = m.u32()?;
        let cmad = chunks(m.rest())
            .find(|c| c.tag == *b"CMAD")
            .map(|c| c.body)
            .unwrap_or_default();
        let mut c = Reader::new(cmad);
        let mut words = Vec::new();
        while words.len() < 12 {
            match c.u32() {
                Ok(w) => words.push(w),
                Err(_) => break,
            }
        }
        let comment = c.string().unwrap_or_default();
        while let Ok(w) = c.u32() {
            words.push(w);
        }
        let word = |i: usize| words.get(i).copied().unwrap_or(0);
        device.mappings.push(Mapping {
            control: bindings.get(&binding).cloned().unwrap_or_default(),
            output,
            command,
            interaction: word(2),
            assignment: word(3),
            options: options(&words),
            value: word(11) as i32,
            comment,
            raw: words,
        });
    }
    Ok(device)
}

fn control_defs(body: &[u8]) -> Vec<ControlDef> {
    let Some(ddci) = chunks(body).find(|c| c.tag == *b"DDCI") else {
        return Vec::new();
    };
    chunks(ddci.body.get(4..).unwrap_or_default())
        .filter(|c| c.tag == *b"DCDT")
        .filter_map(|c| {
            let mut r = Reader::new(c.body);
            let name = r.string().ok()?;
            r.u32().ok()?;
            Some(ControlDef {
                name,
                min: f32::from_bits(r.u32().ok()?),
                max: f32::from_bits(r.u32().ok()?),
                encoder: r.u32().ok()?,
            })
        })
        .collect()
}

fn typed(words: &[u32], i: usize) -> Typed {
    let (Some(&tag), Some(&payload)) = (words.get(i), words.get(i + 1)) else {
        return Typed::None;
    };
    match tag {
        0 | 1 => Typed::Int(payload as i32),
        2 => Typed::Float(f32::from_bits(payload)),
        _ => Typed::None,
    }
}

fn options(words: &[u32]) -> MappingOptions {
    let word = |i: usize| words.get(i).copied().unwrap_or(0);
    let defaults = MappingOptions {
        sensitivity: 100.0,
        midi_range: (0, 127),
        step: Typed::Int(1),
        ..MappingOptions::default()
    };
    if word(0) != 4 {
        return defaults;
    }
    let condition = |i: usize| {
        let control = word(i);
        (control as i32 >= 1).then(|| Condition {
            control,
            channel: word(i + 1) as i32,
            value: word(i + 2) as i32,
        })
    };
    MappingOptions {
        controller: word(1),
        auto_repeat: word(4) != 0,
        invert: word(5) != 0,
        soft_takeover: word(6) != 0,
        sensitivity: f32::from_bits(word(7)) * 20.0,
        acceleration: f32::from_bits(word(8)),
        set_to_value: word(9) != 0,
        conditions: [condition(12), condition(15)],
        range: (typed(words, 18), typed(words, 20)),
        midi_range: (word(22) as i32, word(23) as i32),
        output_invert: word(24) != 0,
        blend: word(25) != 0,
        step: match typed(words, 26) {
            Typed::None => defaults.step,
            step => step,
        },
    }
}

struct Chunk<'a> {
    tag: [u8; 4],
    body: &'a [u8],
}

fn chunks(data: &[u8]) -> impl Iterator<Item = Chunk<'_>> {
    let mut i = 0;
    std::iter::from_fn(move || {
        let header = data.get(i..i + 8)?;
        let tag: [u8; 4] = header[..4].try_into().ok()?;
        if !tag.iter().all(u8::is_ascii_uppercase) {
            return None;
        }
        let size = u32::from_be_bytes(header[4..8].try_into().ok()?) as usize;
        let body = data.get(i + 8..i + 8 + size)?;
        i += 8 + size;
        Some(Chunk { tag, body })
    })
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn u32(&mut self) -> Result<u32, Error> {
        let bytes = self
            .data
            .get(self.pos..self.pos + 4)
            .ok_or_else(|| bad("truncated chunk".into()))?;
        self.pos += 4;
        Ok(u32::from_be_bytes(bytes.try_into().expect("4 bytes")))
    }

    fn string(&mut self) -> Result<String, Error> {
        let n = self.u32()? as usize;
        let bytes = self
            .data
            .get(self.pos..self.pos + 2 * n)
            .ok_or_else(|| bad("truncated string".into()))?;
        self.pos += 2 * n;
        let (pairs, _) = bytes.as_chunks::<2>();
        let units: Vec<u16> = pairs.iter().map(|&c| u16::from_be_bytes(c)).collect();
        Ok(String::from_utf16_lossy(&units))
    }

    fn rest(&self) -> &'a [u8] {
        &self.data[self.pos.min(self.data.len())..]
    }
}

fn decode_base64(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let mut acc = 0u32;
    let mut bits = 0;
    for c in text.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' | b'\n' | b'\r' | b' ' => continue,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

fn bad(message: String) -> Error {
    Error::Invalid {
        element: "tsi".into(),
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_4_options() {
        let f = f32::to_bits;
        let words = [
            4,
            0,
            2,
            u32::MAX,
            1,
            0,
            1,
            f(2.5),
            f(0.5),
            1,
            1,
            3,
            2549,
            u32::MAX,
            3,
            0,
            0,
            0,
            2,
            f(0.0),
            2,
            f(1.0),
            10,
            100,
            1,
            1,
            2,
            f(0.0625),
            0,
        ];
        let o = options(&words);
        assert!(o.auto_repeat && !o.invert && o.soft_takeover && o.set_to_value);
        assert!((o.sensitivity - 50.0).abs() < 1e-6);
        assert_eq!(
            o.conditions,
            [
                Some(Condition {
                    control: 2549,
                    channel: -1,
                    value: 3
                }),
                None
            ]
        );
        assert_eq!(o.range, (Typed::Float(0.0), Typed::Float(1.0)));
        assert_eq!(o.midi_range, (10, 100));
        assert!(o.output_invert && o.blend);
        assert_eq!(o.step, Typed::Float(0.0625));
        let old = options(&[3, 0, 2, 0]);
        assert_eq!((old.sensitivity, old.midi_range), (100.0, (0, 127)));
        assert_eq!(old.step, Typed::Int(1));
    }

    fn chunk(tag: [u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = tag.to_vec();
        out.extend(u32::try_from(body.len()).unwrap().to_be_bytes());
        out.extend(body);
        out
    }

    fn string(s: &str) -> Vec<u8> {
        let units: Vec<u16> = s.encode_utf16().collect();
        let mut out = u32::try_from(units.len()).unwrap().to_be_bytes().to_vec();
        for u in units {
            out.extend(u.to_be_bytes());
        }
        out
    }

    fn words(w: &[u32]) -> Vec<u8> {
        w.iter().flat_map(|v| v.to_be_bytes()).collect()
    }

    fn sample_blob() -> Vec<u8> {
        let mut cmad = words(&[4, 0, 3, 1, 0, 0, 0, 0x40A0_0000, 0, 1, 1, (-9i32) as u32]);
        cmad.extend(string("note"));
        cmad.extend(words(&[0, 127]));
        let mut cmai = words(&[5, 0, 103]);
        cmai.extend(chunk(*b"CMAD", &cmad));
        let mut cmas = words(&[1]);
        cmas.extend(chunk(*b"CMAI", &cmai));
        let mut dcbm_entry = words(&[5]);
        dcbm_entry.extend(string("Shift+T"));
        let mut dcbm = words(&[1]);
        dcbm.extend(chunk(*b"DCBM", &dcbm_entry));
        let mut ddcb = chunk(*b"CMAS", &cmas);
        ddcb.extend(chunk(*b"DCBM", &dcbm));
        let mut ddpt = string("In Port");
        ddpt.extend(string("Out Port"));
        let mut dcdt = string("Ch01.CC.010");
        dcdt.extend(words(&[7, 0, 127f32.to_bits(), 0, u32::MAX]));
        let mut ddci = words(&[1]);
        ddci.extend(chunk(*b"DCDT", &dcdt));
        let mut ddat = chunk(*b"DDIC", &string("My mapping"));
        ddat.extend(chunk(*b"DDDC", &chunk(*b"DDCI", &ddci)));
        ddat.extend(chunk(*b"DDPT", &ddpt));
        ddat.extend(chunk(*b"DDCB", &ddcb));
        let mut devi = string("Generic Keyboard");
        devi.extend(chunk(*b"DDAT", &ddat));
        let mut devs = words(&[1]);
        devs.extend(chunk(*b"DEVI", &devi));
        let mut diom = chunk(*b"DIOI", &words(&[1]));
        diom.extend(chunk(*b"DEVS", &devs));
        chunk(*b"DIOM", &diom)
    }

    #[test]
    fn parses_devices_and_mappings() {
        let devices = parse_blob(&sample_blob()).unwrap();
        assert_eq!(devices.len(), 1);
        let d = &devices[0];
        assert_eq!(
            (d.name.as_str(), d.comment.as_str()),
            ("Generic Keyboard", "My mapping")
        );
        assert_eq!(
            (d.in_port.as_str(), d.out_port.as_str()),
            ("In Port", "Out Port")
        );
        let m = &d.mappings[0];
        assert_eq!(m.control, "Shift+T");
        assert_eq!(
            (m.command, m.interaction, m.assignment, m.value),
            (103, 3, 1, -9)
        );
        assert_eq!(m.comment, "note");
        assert!(!m.output);
        assert_eq!(
            d.controls,
            [ControlDef {
                name: "Ch01.CC.010".into(),
                min: 0.0,
                max: 127.0,
                encoder: 0,
            }]
        );
    }

    #[test]
    fn parses_settings_xml() {
        let blob = sample_blob();
        let b64 = encode_base64(&blob);
        let (root, settings) = (crate::interop::TSI_ROOT, crate::interop::TSI_SETTINGS);
        let xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\" ?>\n<{root}><{settings}>\
             <Entry Name=\"Other\" Type=\"1\" Value=\"0\"></Entry>\n\
             <Entry Name=\"DeviceIO.Config.Keyboard\" Type=\"3\" Value=\"{b64}\"></Entry>\n\
             </{settings}>\n</{root}>\n"
        );
        let tsi = Tsi::parse(&xml).unwrap();
        assert_eq!(tsi.devices.len(), 1);
        assert_eq!(tsi.devices[0].entry, "DeviceIO.Config.Keyboard");
    }

    #[test]
    fn base64_round_trip() {
        let data: Vec<u8> = (0..=255u8).collect();
        assert_eq!(decode_base64(&encode_base64(&data)).unwrap(), data);
        assert!(decode_base64("!!").is_none());
    }

    fn encode_base64(data: &[u8]) -> String {
        const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for c in data.chunks(3) {
            let n = (u32::from(c[0]) << 16)
                | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
                | u32::from(*c.get(2).unwrap_or(&0));
            for k in 0..4 {
                if k <= c.len() {
                    out.push(T[(n >> (18 - 6 * k)) as usize & 63] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }
}
