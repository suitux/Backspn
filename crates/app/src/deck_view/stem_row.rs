use backspn_control::feedback::StemFeedback;
use backspn_control::{StemPart, Trigger};
use backspn_engine::{Command, STEMS};
use eframe::egui::{self, Color32, RichText, Ui};

use super::DeckView;
use crate::{theme, widgets};

const FALLBACK: [[u8; 3]; STEMS] = [
    [230, 80, 80],
    [230, 90, 220],
    [70, 200, 140],
    [240, 150, 60],
];

fn stem_colour(info: &[backspn_formats::stem::StemInfo], stem: usize) -> [u8; 3] {
    info.get(stem)
        .and_then(backspn_formats::stem::StemInfo::rgb)
        .unwrap_or(FALLBACK[stem])
}

impl DeckView {
    pub fn stem_feedback(&self) -> Option<[StemFeedback; STEMS]> {
        let info = self.loaded.as_ref()?.stems.as_ref()?;
        Some(std::array::from_fn(|stem| {
            let c = self.stem_controls[stem];
            StemFeedback {
                colour: stem_colour(info, stem),
                muted: c.mute,
                filter_on: c.filter_on,
            }
        }))
    }

    pub fn stem_overlay(&self, stem: usize) -> Option<crate::screen::StemOverlay> {
        let info = self.loaded.as_ref()?.stems.as_ref()?;
        let c = self.stem_controls.get(stem)?;
        let [r, g, b] = stem_colour(info, stem);
        Some(crate::screen::StemOverlay {
            name: info
                .get(stem)
                .map_or_else(|| format!("Stem {}", stem + 1), |i| i.name.clone()),
            colour: Color32::from_rgb(r, g, b),
            volume: c.volume,
            muted: c.mute,
            filter: c.filter,
            filter_on: c.filter_on,
        })
    }

    /// All four stems for the screens, once the track has them.
    pub fn stem_overlays(&self) -> Option<[crate::screen::StemOverlay; 4]> {
        let all: Vec<_> = (0..4).filter_map(|stem| self.stem_overlay(stem)).collect();
        <[crate::screen::StemOverlay; 4]>::try_from(all).ok()
    }

    pub(super) fn stem_row(&mut self, ui: &mut Ui, out: &mut Vec<Command>) {
        let Some(info) = self.loaded.as_ref().and_then(|l| l.stems.clone()) else {
            return;
        };
        let t = theme::get(ui.ctx());
        ui.columns(STEMS, |columns| {
            for (stem, ui) in columns.iter_mut().enumerate() {
                let name = info
                    .get(stem)
                    .map_or_else(|| format!("Stem {}", stem + 1), |i| i.name.clone());
                let [r, g, b] = stem_colour(&info, stem);
                let colour = Color32::from_rgb(r, g, b);
                self.stem_strip(ui, stem, &name, colour, t.text_dim, out);
            }
        });
    }

    fn stem_strip(
        &mut self,
        ui: &mut Ui,
        stem: usize,
        name: &str,
        colour: Color32,
        dim: Color32,
        out: &mut Vec<Command>,
    ) {
        let c = self.stem_controls[stem];
        let shown = if c.mute { dim } else { colour };
        let label = egui::Button::new(RichText::new(name).color(Color32::BLACK).strong())
            .fill(shown)
            .min_size(egui::vec2(ui.available_width(), 18.0));
        if ui
            .add(label)
            .on_hover_text(if c.mute {
                "Muted: click to unmute"
            } else {
                "Click to mute"
            })
            .clicked()
        {
            self.apply_stem(stem, StemPart::Mute, Trigger::Press, out);
        }
        ui.horizontal_wrapped(|ui| {
            let mut volume = c.volume;
            if widgets::knob_compact(ui, &mut volume, 0.0..=1.0, 1.0, "VOL").changed() {
                self.apply_stem(stem, StemPart::Volume, Trigger::Set(volume), out);
            }
            if widgets::toggle(ui, "FLT", c.filter_on, colour).clicked() {
                self.apply_stem(stem, StemPart::FilterOn, Trigger::Press, out);
            }
            let mut filter = c.filter;
            if widgets::knob_compact(ui, &mut filter, 0.0..=1.0, 0.5, "FLTR").changed() {
                self.apply_stem(stem, StemPart::Filter, Trigger::Set(filter), out);
            }
            if widgets::toggle(ui, "FX", c.fx_on, colour).clicked() {
                self.apply_stem(stem, StemPart::FxOn, Trigger::Press, out);
            }
            let mut send = c.fx_send;
            if widgets::knob_compact(ui, &mut send, 0.0..=1.0, 1.0, "FX").changed() {
                self.apply_stem(stem, StemPart::FxAmount, Trigger::Set(send), out);
            }
        });
    }
}
