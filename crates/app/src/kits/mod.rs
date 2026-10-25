mod bundle;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, bail};
pub use backspn_engine::fx_pattern::Steps;
use backspn_engine::fx_pattern::{PATTERNS, Sound};
use serde::{Deserialize, Serialize};

pub const DEFAULT_KNOBS: [f32; 4] = [1.0, 0.5, 0.0, 0.5];

const KIT_FILE: &str = "kit.json";

pub const EXTENSION: &str = "bskit";

pub const OLD_EXTENSION: &str = "tkit";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KitSound {
    pub name: String,
    pub file: String,
    #[serde(default)]
    pub level_db: f32,
    pub patterns: [Steps; PATTERNS],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Kit {
    pub name: String,
    #[serde(default)]
    pub author: String,
    #[serde(default = "default_knobs")]
    pub knobs: [f32; 4],
    pub sounds: Vec<KitSound>,
}

fn default_knobs() -> [f32; 4] {
    DEFAULT_KNOBS
}

#[derive(Debug, Clone, PartialEq)]
pub struct StoredKit {
    pub id: String,
    pub kit: Kit,
}

type Decoded = (Arc<[f32]>, f32);

pub struct KitLibrary {
    dir: PathBuf,
    kits: Vec<StoredKit>,
    decoded: HashMap<PathBuf, Decoded>,
}

impl std::fmt::Debug for KitLibrary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KitLibrary")
            .field("dir", &self.dir)
            .field("kits", &self.kits.len())
            .finish_non_exhaustive()
    }
}

fn slug(name: &str) -> String {
    let s: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let s = s.trim_matches('-').to_owned();
    if s.is_empty() { "kit".to_owned() } else { s }
}

fn unique(base: &str, taken: impl Fn(&str) -> bool) -> String {
    first_free(base.to_owned(), |n| format!("{base}-{n}"), taken)
}

fn unique_file(name: &str, taken: impl Fn(&str) -> bool) -> String {
    let (stem, ext) = name
        .rsplit_once('.')
        .map_or((name, String::new()), |(s, e)| (s, format!(".{e}")));
    first_free(name.to_owned(), |n| format!("{stem} {n}{ext}"), taken)
}

fn first_free(
    first: String,
    numbered: impl Fn(u32) -> String,
    taken: impl Fn(&str) -> bool,
) -> String {
    let mut candidate = first;
    let mut n = 2;
    while taken(&candidate) {
        candidate = numbered(n);
        n += 1;
    }
    candidate
}

impl KitLibrary {
    pub fn open(dir: PathBuf) -> Self {
        let mut lib = Self {
            dir,
            kits: Vec::new(),
            decoded: HashMap::new(),
        };
        lib.reload();
        lib
    }

    #[cfg(test)]
    pub fn none() -> Self {
        Self {
            dir: PathBuf::new(),
            kits: Vec::new(),
            decoded: HashMap::new(),
        }
    }

    pub fn default_dir() -> PathBuf {
        crate::prefs::ui_state_path().with_file_name("Kits")
    }

    fn reload(&mut self) {
        self.kits.clear();
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return;
        };
        for entry in entries.flatten() {
            let id = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path().join(KIT_FILE);
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            match serde_json::from_str(&text) {
                Ok(kit) => self.kits.push(StoredKit { id, kit }),
                Err(e) => log::warn!("cannot read kit {}: {e}", path.display()),
            }
        }
        self.sort();
    }

    fn sort(&mut self) {
        self.kits.sort_by_key(|k| k.kit.name.to_lowercase());
    }

    pub fn kits(&self) -> &[StoredKit] {
        &self.kits
    }

    pub fn find(&self, id: &str) -> Option<&StoredKit> {
        self.kits.iter().find(|k| k.id == id)
    }

    fn folder(&self, id: &str) -> PathBuf {
        self.dir.join(id)
    }

    pub fn save(&mut self, id: &str, kit: &Kit) -> anyhow::Result<()> {
        let folder = self.folder(id);
        std::fs::create_dir_all(&folder)?;
        std::fs::write(folder.join(KIT_FILE), serde_json::to_string_pretty(kit)?)?;
        match self.kits.iter_mut().find(|k| k.id == id) {
            Some(k) => k.kit = kit.clone(),
            None => self.kits.push(StoredKit {
                id: id.to_owned(),
                kit: kit.clone(),
            }),
        }
        self.sort();
        Ok(())
    }

    fn free_id(&self, name: &str) -> String {
        unique(&slug(name), |c| {
            self.find(c).is_some() || self.folder(c).exists()
        })
    }

    pub fn create(&mut self, name: &str) -> anyhow::Result<String> {
        let id = self.free_id(name);
        let kit = Kit {
            name: name.to_owned(),
            author: String::new(),
            knobs: DEFAULT_KNOBS,
            sounds: Vec::new(),
        };
        self.save(&id, &kit)?;
        Ok(id)
    }

    pub fn duplicate(&mut self, id: &str) -> anyhow::Result<String> {
        let mut kit = self.find(id).context("no such kit")?.kit.clone();
        kit.name = format!("{} copy", kit.name);
        let new = self.free_id(&kit.name);
        std::fs::create_dir_all(self.folder(&new))?;
        for sound in &kit.sounds {
            std::fs::copy(
                self.folder(id).join(&sound.file),
                self.folder(&new).join(&sound.file),
            )?;
        }
        self.save(&new, &kit)?;
        Ok(new)
    }

    pub fn delete(&mut self, id: &str) -> anyhow::Result<()> {
        if self.find(id).is_none() {
            bail!("no such kit");
        }
        std::fs::remove_dir_all(self.folder(id))?;
        self.kits.retain(|k| k.id != id);
        Ok(())
    }

    pub fn add_sound(&mut self, id: &str, path: &Path) -> anyhow::Result<()> {
        decode_mono(path)?;
        let mut kit = self.find(id).context("no such kit")?.kit.clone();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|n| bundle::is_plain_name(n))
            .context("unusable file name")?;
        let folder = self.folder(id);
        let file = unique_file(name, |c| folder.join(c).exists());
        std::fs::copy(path, folder.join(&file))?;
        kit.sounds.push(KitSound {
            name: path
                .file_stem()
                .map_or_else(|| file.clone(), |s| s.to_string_lossy().into_owned()),
            file,
            level_db: 0.0,
            patterns: [Steps::default(); PATTERNS],
        });
        self.save(id, &kit)
    }

    pub fn remove_sound(&mut self, id: &str, index: usize) -> anyhow::Result<()> {
        let mut kit = self.find(id).context("no such kit")?.kit.clone();
        if index >= kit.sounds.len() {
            bail!("no such sound");
        }
        let gone = kit.sounds.remove(index);
        if kit.sounds.iter().all(|s| s.file != gone.file) {
            let path = self.folder(id).join(&gone.file);
            self.decoded.remove(&path);
            std::fs::remove_file(path)?;
        }
        self.save(id, &kit)
    }

    fn decoded(&mut self, id: &str, file: &str) -> anyhow::Result<Decoded> {
        let path = self.folder(id).join(file);
        if let Some(d) = self.decoded.get(&path) {
            return Ok(d.clone());
        }
        let d = decode_mono(&path)?;
        self.decoded.insert(path, d.clone());
        Ok(d)
    }

    pub fn sounds(&mut self, id: &str) -> Vec<Sound> {
        let Some(kit) = self.find(id).map(|k| k.kit.clone()) else {
            return Vec::new();
        };
        kit.sounds
            .iter()
            .filter_map(|s| match self.decoded(id, &s.file) {
                Ok((samples, rate)) => Some(Sound {
                    name: s.name.clone(),
                    samples,
                    rate,
                    level: 10f32.powf(s.level_db / 20.0),
                    patterns: s.patterns,
                }),
                Err(e) => {
                    log::warn!("kit {id}: cannot read {}: {e}", s.file);
                    None
                }
            })
            .collect()
    }

    pub fn preview(&mut self, id: &str, file: &str) -> anyhow::Result<backspn_engine::Track> {
        let (samples, rate) = self.decoded(id, file)?;
        let stereo = samples.iter().flat_map(|&x| [x, x]).collect();
        Ok(backspn_engine::Track::from_stereo(rate as u32, stereo))
    }

    pub fn export(&self, id: &str, to: &Path) -> anyhow::Result<()> {
        let kit = &self.find(id).context("no such kit")?.kit;
        let mut files = bundle::Files::new();
        for sound in &kit.sounds {
            if files.iter().all(|(n, _)| *n != sound.file) {
                files.push((
                    sound.file.clone(),
                    std::fs::read(self.folder(id).join(&sound.file))?,
                ));
            }
        }
        std::fs::write(to, bundle::pack(&serde_json::to_string(kit)?, &files))?;
        Ok(())
    }

    pub fn import(&mut self, from: &Path) -> anyhow::Result<String> {
        let (json, files) = bundle::unpack(&std::fs::read(from)?)?;
        let kit: Kit = serde_json::from_str(&json)?;
        for sound in &kit.sounds {
            if !files.iter().any(|(n, _)| *n == sound.file) {
                bail!("kit file lacks {}", sound.file);
            }
        }
        let id = self.free_id(&kit.name);
        let folder = self.folder(&id);
        std::fs::create_dir_all(&folder)?;
        for (name, data) in files {
            std::fs::write(folder.join(name), data)?;
        }
        self.save(&id, &kit)?;
        Ok(id)
    }
}

fn decode_mono(path: &Path) -> anyhow::Result<Decoded> {
    let track = backspn_decode::decode_file(path)
        .with_context(|| format!("cannot decode {}", path.display()))?;
    let mono: Arc<[f32]> = track
        .samples()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&[l, r]| f32::midpoint(l, r))
        .collect();
    if mono.is_empty() {
        bail!("{} holds no audio", path.display());
    }
    Ok((mono, track.sample_rate() as f32))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("backspn-kits-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn wav(path: &Path, frames: u32) {
        let data = frames * 2;
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + data).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&44_100u32.to_le_bytes());
        b.extend_from_slice(&88_200u32.to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&data.to_le_bytes());
        for i in 0..frames {
            b.extend_from_slice(&(((i % 50) as i16 - 25) * 1000).to_le_bytes());
        }
        std::fs::write(path, b).unwrap();
    }

    #[test]
    fn names_stay_unique() {
        assert_eq!(slug(" My Kit! "), "my-kit");
        assert_eq!(slug("!!"), "kit");
        assert_eq!(unique("a", |c| c == "a" || c == "a-2"), "a-3");
        assert_eq!(unique_file("k.wav", |c| c == "k.wav"), "k 2.wav");
        assert_eq!(unique_file("k", |c| c == "k"), "k 2");
    }

    #[test]
    fn kits_are_created_filled_exported_and_imported() {
        let dir = temp_dir("lib");
        let audio = dir.join("in");
        std::fs::create_dir_all(&audio).unwrap();
        wav(&audio.join("kick.wav"), 4410);
        let mut lib = KitLibrary::open(dir.join("Kits"));
        let id = lib.create("Mine").unwrap();
        lib.add_sound(&id, &audio.join("kick.wav")).unwrap();
        lib.add_sound(&id, &audio.join("kick.wav")).unwrap();
        let kit = &lib.find(&id).unwrap().kit;
        assert_eq!(kit.sounds[1].file, "kick 2.wav", "second copy renamed");
        let sounds = lib.sounds(&id);
        assert_eq!(sounds.len(), 2);
        assert_eq!(sounds[0].samples.len(), 4410);
        assert!((sounds[0].rate - 44_100.0).abs() < 1e-3);

        let bundle = dir.join(format!("mine.{EXTENSION}"));
        lib.export(&id, &bundle).unwrap();
        let copy = lib.import(&bundle).unwrap();
        assert_ne!(copy, id);
        assert_eq!(lib.find(&copy).unwrap().kit, lib.find(&id).unwrap().kit);
        assert_eq!(lib.sounds(&copy).len(), 2);

        lib.remove_sound(&id, 0).unwrap();
        assert!(!dir.join("Kits").join(&id).join("kick.wav").exists());
        let dup = lib.duplicate(&id).unwrap();
        assert_eq!(lib.find(&dup).unwrap().kit.name, "Mine copy");
        lib.delete(&id).unwrap();
        assert!(lib.find(&id).is_none());

        let again = KitLibrary::open(dir.join("Kits"));
        assert_eq!(again.kits().len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
