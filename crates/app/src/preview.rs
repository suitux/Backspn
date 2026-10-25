use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};

use backspn_decode::Track;
use backspn_engine::{DeckCommand, DeckState};
use eframe::egui::{self, RichText, Sense, Ui, vec2};

use crate::browser::TrackDrag;
use crate::i18n::tr;
use crate::theme;
use crate::waveform::{self, Peaks, WaveStyle};

const HEIGHT: f32 = 34.0;

type Decoded = Option<(Arc<Track>, Peaks)>;

type TextureKey = (u32, [egui::Color32; 3], bool);

struct Loaded {
    track: i64,
    title: String,
    peaks: Option<Peaks>,
    loading: Option<Receiver<Decoded>>,
    from: Option<f32>,
    duration_s: f64,
    texture: Option<(egui::TextureHandle, TextureKey)>,
}

#[derive(Default)]
pub struct Preview {
    loaded: Option<Loaded>,
    failed: Option<String>,
}

impl std::fmt::Debug for Preview {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Preview")
            .field("track", &self.loaded.as_ref().map(|l| l.track))
            .finish_non_exhaustive()
    }
}

impl Preview {
    pub fn track(&self) -> Option<i64> {
        self.loaded.as_ref().map(|l| l.track)
    }

    pub fn load_or_play(
        &mut self,
        track: i64,
        path: PathBuf,
        title: String,
        from: Option<f32>,
    ) -> Vec<DeckCommand> {
        if let Some(loaded) = self.loaded.as_mut().filter(|l| l.track == track) {
            return match from {
                Some(at) if loaded.loading.is_none() => vec![
                    DeckCommand::Seek {
                        seconds: f64::from(at) * loaded.duration_s,
                    },
                    DeckCommand::Play,
                ],
                Some(at) => {
                    loaded.from = Some(at);
                    Vec::new()
                }
                None => vec![DeckCommand::TogglePlay],
            };
        }
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let decoded = backspn_decode::decode_file(&path).ok().map(|t| {
                let peaks = Peaks::compute(&t);
                (Arc::new(t), peaks)
            });
            drop(tx.send(decoded));
        });
        self.loaded = Some(Loaded {
            track,
            title,
            peaks: None,
            loading: Some(rx),
            from,
            duration_s: 0.0,
            texture: None,
        });
        vec![DeckCommand::Unload]
    }

    pub fn unload(&mut self) -> Vec<DeckCommand> {
        self.loaded = None;
        vec![DeckCommand::Unload]
    }

    pub fn take_failure(&mut self) -> Option<String> {
        self.failed.take()
    }

    pub fn poll(&mut self) -> Vec<DeckCommand> {
        let Some(loaded) = &mut self.loaded else {
            return Vec::new();
        };
        let Some(Ok(result)) = loaded.loading.as_ref().map(Receiver::try_recv) else {
            return Vec::new();
        };
        loaded.loading = None;
        let Some((track, peaks)) = result else {
            self.failed = self.loaded.take().map(|l| l.title);
            return Vec::new();
        };
        loaded.peaks = Some(peaks);
        loaded.duration_s = track.duration_s();
        let start = loaded
            .from
            .take()
            .map(|at| f64::from(at) * loaded.duration_s);
        let mut out = vec![DeckCommand::Load(track)];
        if let Some(seconds) = start {
            out.push(DeckCommand::Seek { seconds });
        }
        out.push(DeckCommand::Play);
        out
    }

    pub fn ui(&mut self, ui: &mut Ui, state: &DeckState) -> Vec<DeckCommand> {
        let mut out = Vec::new();
        let t = theme::get(ui.ctx());
        let Some(loaded) = &mut self.loaded else {
            return out;
        };
        ui.horizontal(|ui| {
            ui.set_height(HEIGHT);
            let label = if state.playing { "⏸" } else { "▶" };
            if ui
                .add(egui::Button::new(RichText::new(label).size(14.0)))
                .on_hover_text(tr("Play/pause the preview (headphones)"))
                .clicked()
            {
                out.push(DeckCommand::TogglePlay);
            }
            let header = ui
                .add(
                    egui::Label::new(RichText::new(format!("🎧 {}", loaded.title)).color(t.text))
                        .truncate()
                        .sense(Sense::click_and_drag()),
                )
                .on_hover_text(tr("Drag onto a deck to load it there"));
            header.dnd_set_drag_payload(TrackDrag::single(loaded.track, loaded.title.clone()));
            let width = (ui.available_width() - 30.0).max(60.0);
            let (rect, response) =
                ui.allocate_exact_size(vec2(width, HEIGHT - 6.0), Sense::click());
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 0.0, t.well);
            if let Some(peaks) = &loaded.peaks
                && state.duration_s > 0.0
            {
                let whole = (0.0, state.duration_s);
                let style = WaveStyle::get(ui.ctx());
                if style.mode == waveform::WaveMode::Spectrum {
                    let key = (rect.width().round() as u32, style.colors, style.direct);
                    let texture = match &loaded.texture {
                        Some((texture, k)) if *k == key => texture.clone(),
                        _ => {
                            let image = waveform::density_image(ui, rect, peaks, whole, &style);
                            let texture = ui.ctx().load_texture(
                                "preview",
                                image,
                                egui::TextureOptions::LINEAR,
                            );
                            loaded.texture = Some((texture.clone(), key));
                            texture
                        }
                    };
                    waveform::paint_texture(&painter, rect, &texture);
                    waveform::dim_played(&painter, rect, whole, state.position_s);
                } else {
                    waveform::paint(&painter, rect, peaks, whole, state.position_s, &style);
                }
                let x = waveform::x_of(rect, whole, state.position_s);
                waveform::marker(&painter, rect, x, t.danger, 1.5);
                if response.clicked()
                    && let Some(pos) = response.interact_pointer_pos()
                {
                    out.push(DeckCommand::Seek {
                        seconds: waveform::time_at(rect, whole, pos),
                    });
                }
            } else {
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "Loading…",
                    egui::FontId::proportional(11.0),
                    t.text_dim,
                );
            }
            if ui
                .small_button("×")
                .on_hover_text(tr("Close the preview"))
                .clicked()
            {
                out.push(DeckCommand::Unload);
            }
        });
        if out.iter().any(|c| matches!(c, DeckCommand::Unload)) {
            self.loaded = None;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_track_toggles_and_another_loads() {
        let mut p = Preview::default();
        let first = p.load_or_play(1, PathBuf::from("/nonexistent.wav"), "One".into(), None);
        assert!(matches!(first.as_slice(), [DeckCommand::Unload]));
        assert_eq!(p.track(), Some(1));
        assert!(matches!(
            p.load_or_play(1, PathBuf::new(), "One".into(), None)
                .as_slice(),
            [DeckCommand::TogglePlay]
        ));
        for _ in 0..200 {
            p.poll();
            if p.track().is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(p.track(), None);
        assert_eq!(p.take_failure().as_deref(), Some("One"));
    }
}
