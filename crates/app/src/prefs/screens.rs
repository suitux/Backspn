use std::path::PathBuf;

use eframe::egui::{self, Color32, RichText, Sense, TextureHandle, Ui, Vec2};

use super::{Settings, search};
use crate::i18n::tr;
use crate::screen::{self, Layout, ScreenDeck};
use crate::theme::ThemeId;
use crate::waveform::{Peaks, WaveStyle};

const PREVIEW: Vec2 = Vec2::new(160.0, 120.0);

#[derive(Default)]
pub(super) struct Previews {
    rendered_for: Option<(ThemeId, WaveStyle)>,
    textures: Vec<(Layout, TextureHandle)>,
    peaks: Option<Peaks>,
    logo: Option<(ThemeId, Option<PathBuf>, TextureHandle)>,
}

impl Previews {
    fn logo(&mut self, ctx: &egui::Context, s: &Settings) -> &TextureHandle {
        let fresh = self
            .logo
            .as_ref()
            .is_some_and(|(theme, path, _)| *theme == s.theme && *path == s.screen_logo);
        if !fresh {
            self.logo = None;
        }
        &self
            .logo
            .get_or_insert_with(|| {
                let frame = screen::render_logo(&crate::theme::get(ctx), s.screen_logo.as_deref());
                let image = egui::ColorImage::from_rgb(
                    [frame.width, frame.height],
                    &screen::rgb_bytes(&frame),
                );
                let texture =
                    ctx.load_texture("screen-preview-logo", image, egui::TextureOptions::LINEAR);
                (s.theme, s.screen_logo.clone(), texture)
            })
            .2
    }

    fn ensure(&mut self, ctx: &egui::Context, s: &Settings) {
        let theme = crate::theme::get(ctx);
        let style = s.wave_style(&theme);
        if self.rendered_for == Some((s.theme, style)) {
            return;
        }
        let peaks = self.peaks.get_or_insert_with(sample_peaks);
        let state = sample_state();
        let deck = ScreenDeck {
            index: 0,
            state: &state,
            name: "Artist - Title of the Track",
            peaks: Some(peaks),
            loop_beats: 8.0,
            end_warning: false,
            key: Some("8A"),
            key_color: Some(Color32::from_rgb(0x40, 0xC0, 0xFF)),
            synced: true,
            master: false,
            zoom_s: 4.0,
            clock_ms: 0,
            stem: None,
            sample_deck: None,
        };
        self.textures = Layout::ALL
            .into_iter()
            .map(|layout| {
                let frame = screen::render(&deck, &style, &theme, layout);
                let image = egui::ColorImage::from_rgb(
                    [frame.width, frame.height],
                    &screen::rgb_bytes(&frame),
                );
                let name = format!("screen-preview-{layout:?}");
                (
                    layout,
                    ctx.load_texture(name, image, egui::TextureOptions::LINEAR),
                )
            })
            .collect();
        self.rendered_for = Some((s.theme, style));
    }
}

fn sample_state() -> backspn_engine::DeckState {
    let mut state = backspn_engine::DeckState {
        loaded: true,
        playing: true,
        duration_s: 180.0,
        position_s: 61.3,
        tempo: 1.02,
        keylock: true,
        loop_active: true,
        ..backspn_engine::DeckState::default()
    };
    state.hotcues[0] = Some(backspn_engine::HotcuePoint {
        start_s: 15.0,
        kind: backspn_engine::CueKind::Cue,
        loop_s: None,
    });
    state
}

fn sample_peaks() -> Peaks {
    let rate = 8000;
    let samples: Vec<f32> = (0..rate * 180 * 2)
        .map(|i| (i as f32 * 0.05).sin() * ((i / 4000) % 7) as f32 / 7.0)
        .collect();
    let track = backspn_decode::Track::from_stereo(rate as u32, samples);
    Peaks::compute(&track)
}

pub(super) fn screens(ui: &mut Ui, s: &mut Settings, previews: &mut Previews) {
    search::title(ui, "Screens:");
    previews.ensure(ui.ctx(), s);
    let t = crate::theme::get(ui.ctx());
    ui.horizontal_wrapped(|ui| {
        for (layout, texture) in &previews.textures {
            let chosen = s.screen_layout == *layout;
            let stroke = if chosen {
                egui::Stroke::new(2.0, t.accent)
            } else {
                egui::Stroke::new(1.0, t.border)
            };
            egui::Frame::new()
                .stroke(stroke)
                .inner_margin(6.0)
                .corner_radius(4.0)
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        let image = ui.add(
                            egui::Image::new(texture)
                                .fit_to_exact_size(PREVIEW)
                                .sense(Sense::click()),
                        );
                        if image.clicked() {
                            s.screen_layout = *layout;
                        }
                        search::radio(ui, &mut s.screen_layout, *layout, layout.label())
                            .on_hover_text(layout.description());
                    });
                });
        }
    });
    search::check(ui, &mut s.screen_idle_logo, "Show the logo on empty decks");
    search::check(
        ui,
        &mut s.screen_quit_logo,
        "Leave the logo on the screens after quitting",
    );
    ui.horizontal(|ui| {
        search::title(ui, "Logo:");
        if ui.button(tr("Choose…")).clicked()
            && let Some(file) = rfd::FileDialog::new()
                .add_filter(tr("Images"), &screen::LOGO_EXTENSIONS)
                .pick_file()
        {
            s.screen_logo = Some(file);
        }
        if let Some(path) = &s.screen_logo {
            ui.label(RichText::new(path.display().to_string()).monospace());
            if ui.button(tr("Use Default")).clicked() {
                s.screen_logo = None;
            }
        } else {
            ui.label(RichText::new(tr("Backspn logo")).weak());
        }
    })
    .response
    .on_hover_text(tr("A PNG or JPEG picture, fitted to the 320 × 240 screens"));
    ui.add(egui::Image::new(previews.logo(ui.ctx(), s)).fit_to_exact_size(PREVIEW));
}
