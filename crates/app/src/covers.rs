use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};

use backspn_library::TrackRow;
use eframe::egui::{self, Color32, ColorImage, Rect, TextureHandle};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Source {
    File(PathBuf),
    Stored(PathBuf),
    #[default]
    None,
}

impl Source {
    pub fn read(&self) -> Option<Vec<u8>> {
        match self {
            Self::File(path) => backspn_decode::cover_art(path),
            Self::Stored(path) => std::fs::read(path).ok(),
            Self::None => None,
        }
    }
}

pub fn store_dir() -> PathBuf {
    crate::browser::library_path().with_file_name("Covers")
}

pub fn source(track: &TrackRow, store: &Path) -> Source {
    match track.cover.as_deref() {
        None => {
            let file = track.path.split('#').next().unwrap_or(&track.path);
            Source::File(PathBuf::from(file))
        }
        Some("") => Source::None,
        Some(name) => Source::Stored(store.join(name)),
    }
}

pub fn store(store: &Path, picture: &Path) -> Result<String, String> {
    let bytes = std::fs::read(picture).map_err(|e| e.to_string())?;
    let extension = match image::guess_format(&bytes) {
        Ok(image::ImageFormat::Jpeg) => "jpg",
        Ok(image::ImageFormat::Png) => "png",
        _ => return Err("Unknown file format.".into()),
    };
    if image::load_from_memory(&bytes).is_err() {
        return Err("The file is corrupt.".into());
    }
    let name = format!("{:016x}.{extension}", fnv1a(&bytes));
    std::fs::create_dir_all(store).map_err(|e| e.to_string())?;
    let path = store.join(&name);
    if !path.exists() {
        std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    }
    Ok(name)
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

pub fn thumbnail(bytes: &[u8], side: f32) -> Option<ColorImage> {
    let image = image::load_from_memory(bytes).ok()?;
    let pixels = (side * 2.0) as u32;
    let small = image.thumbnail(pixels, pixels).to_rgba8();
    let size = [small.width() as usize, small.height() as usize];
    Some(ColorImage::from_rgba_unmultiplied(size, small.as_raw()))
}

pub fn paint(
    painter: &egui::Painter,
    rect: Rect,
    texture: Option<egui::TextureId>,
    colour: Color32,
) {
    if let Some(texture) = texture {
        painter.image(
            texture,
            rect,
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    } else {
        let radius = (rect.width() * 0.1).min(6.0);
        painter.rect_filled(rect, radius, colour.gamma_multiply(0.18));
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "♪",
            egui::FontId::proportional(rect.height() * 0.42),
            colour.gamma_multiply(0.7),
        );
    }
}

const CAPACITY: usize = 400;

pub struct CoverCache {
    requests: Sender<(i64, Source)>,
    results: Receiver<(i64, Option<ColorImage>)>,
    textures: HashMap<i64, Option<TextureHandle>>,
    pending: HashSet<i64>,
}

impl std::fmt::Debug for CoverCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CoverCache")
            .field("textures", &self.textures.len())
            .field("pending", &self.pending.len())
            .finish_non_exhaustive()
    }
}

impl CoverCache {
    pub fn new(ctx: &egui::Context, side: f32) -> Self {
        let (requests, jobs) = mpsc::channel::<(i64, Source)>();
        let (done, results) = mpsc::channel();
        let wake = ctx.clone();
        let spawned = std::thread::Builder::new()
            .name("covers".into())
            .spawn(move || {
                for (id, source) in jobs {
                    let image = source.read().and_then(|b| thumbnail(&b, side));
                    if done.send((id, image)).is_err() {
                        break;
                    }
                    wake.request_repaint();
                }
            });
        if let Err(e) = spawned {
            log::warn!("covers: cannot start the loader: {e}");
        }
        Self {
            requests,
            results,
            textures: HashMap::new(),
            pending: HashSet::new(),
        }
    }

    pub fn get(&mut self, ctx: &egui::Context, track: &TrackRow) -> Option<&TextureHandle> {
        let id = track.id;
        while let Ok((done, image)) = self.results.try_recv() {
            self.pending.remove(&done);
            let texture = image.map(|i| {
                ctx.load_texture(format!("cover-{done}"), i, egui::TextureOptions::LINEAR)
            });
            self.textures.insert(done, texture);
        }
        if !self.textures.contains_key(&id) && self.pending.insert(id) {
            if self.textures.len() >= CAPACITY {
                self.textures.clear();
            }
            drop(self.requests.send((id, source(track, &store_dir()))));
        }
        self.textures.get(&id).and_then(Option::as_ref)
    }

    pub fn forget(&mut self, tracks: &[i64]) {
        for id in tracks {
            self.textures.remove(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_track_cover_comes_from_its_file_the_store_or_nowhere() {
        let store = Path::new("/covers");
        let mut t = TrackRow {
            path: "/m/a.mp3#2".into(),
            ..TrackRow::default()
        };
        assert_eq!(source(&t, store), Source::File("/m/a.mp3".into()));
        t.cover = Some("x.png".into());
        assert_eq!(source(&t, store), Source::Stored("/covers/x.png".into()));
        t.cover = Some(String::new());
        assert_eq!(source(&t, store), Source::None);
    }

    #[test]
    fn only_pictures_that_decode_are_stored_once() {
        let dir = std::env::temp_dir().join(format!("backspn-covers-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("a.png");
        image::RgbImage::new(4, 4).save(&png).unwrap();
        let text = dir.join("b.png");
        std::fs::write(&text, b"not a picture").unwrap();
        let store_at = dir.join("store");
        let name = store(&store_at, &png).unwrap();
        assert_eq!(Path::new(&name).extension().unwrap(), "png");
        assert_eq!(
            store(&store_at, &png).unwrap(),
            name,
            "same content, same name"
        );
        assert!(store(&store_at, &text).is_err());
        assert_eq!(std::fs::read_dir(&store_at).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
