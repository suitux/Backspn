use std::path::PathBuf;

use eframe::egui::{self, Color32, RichText, Sense, TextureHandle, Ui, Vec2};

use super::{Settings, search};
use crate::i18n::tr;
use crate::screen::{self, Layout, ScreenDeck};
use crate::theme::ThemeId;
use crate::waveform::{Peaks, WaveStyle};

const PREVIEW: Vec2 = Vec2::new(160.0, 120.0);
const PREVIEW_WIDE: Vec2 = Vec2::new(180.0, 102.0);

#[derive(Default)]
pub(super) struct Previews {
    rendered_for: Option<(ThemeId, WaveStyle, (usize, usize))>,
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

    fn ensure(&mut self, ctx: &egui::Context, s: &Settings, size: (usize, usize)) {
        let theme = crate::theme::get(ctx);
        let style = s.wave_style(&theme);
        if self.rendered_for == Some((s.theme, style, size)) {
            return;
        }
        let peaks = self.peaks.get_or_insert_with(sample_peaks);
        let state = sample_state();
        let other = backspn_engine::DeckState {
            position_s: 74.9,
            tempo: 1.0,
            loop_active: false,
            ..state
        };
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
            stems: Some(sample_stems()),
            partner: Some(screen::Partner {
                index: 1,
                state: &other,
                peaks: Some(peaks),
                key: backspn_formats::nml::MusicalKey::parse("9A"),
            }),
        };
        self.textures = Layout::for_panel(size)
            .into_iter()
            .map(|layout| {
                let frame = screen::render_panel(&deck, &style, &theme, layout, size);
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
        self.rendered_for = Some((s.theme, style, size));
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
        grid: Some(backspn_engine::Grid {
            bpm: 124.0,
            anchor_s: 0.2,
            ..backspn_engine::Grid::FIXED
        }),
        ..backspn_engine::DeckState::default()
    };
    let cue = |start_s, loop_s| {
        Some(backspn_engine::HotcuePoint {
            start_s,
            kind: backspn_engine::CueKind::Cue,
            loop_s,
        })
    };
    state.hotcues[0] = cue(15.0, None);
    state.hotcues[1] = cue(46.7, None);
    state.hotcues[2] = cue(77.6, Some(7.7));
    state.hotcues[4] = cue(139.6, None);
    state
}

pub(super) fn sample_stems() -> [screen::StemOverlay; 4] {
    let stem = |name: &str, (r, g, b), volume, muted, filter_on| screen::StemOverlay {
        name: name.to_owned(),
        colour: Color32::from_rgb(r, g, b),
        volume,
        muted,
        filter: 0.3,
        filter_on,
    };
    [
        stem("Drums", (0xE8, 0x5D, 0x3A), 1.0, false, false),
        stem("Bass", (0xE8, 0xB8, 0x3A), 0.8, false, true),
        stem("Other", (0x3A, 0xB8, 0xE8), 0.6, false, false),
        stem("Vocals", (0xB0, 0x5D, 0xE8), 0.0, true, false),
    ]
}

fn sample_peaks() -> Peaks {
    let rate = 8000;
    let samples: Vec<f32> = (0..rate * 180 * 2)
        .map(|i| (i as f32 * 0.05).sin() * ((i / 4000) % 7) as f32 / 7.0)
        .collect();
    let track = backspn_decode::Track::from_stereo(rate as u32, samples);
    Peaks::compute(&track)
}

/// Previews in each row of the layout picker.
const PER_ROW: usize = 3;

/// The screens' settings for a controller whose panels are `size`: the
/// layouts that fit it, each with a preview.
pub(super) fn screens(
    ui: &mut Ui,
    s: &mut Settings,
    previews: &mut Previews,
    size: (usize, usize),
) {
    search::title(ui, "Screens:");
    previews.ensure(ui.ctx(), s, size);
    let t = crate::theme::get(ui.ctx());
    let wide = size == screen::WIDE;
    egui::Grid::new("screen-layouts")
        .num_columns(PER_ROW)
        .show(ui, |ui| {
            for (i, (layout, texture)) in previews.textures.iter().enumerate() {
                let chosen_layout = if wide {
                    &mut s.screen_layout_wide
                } else {
                    &mut s.screen_layout
                };
                let chosen = *chosen_layout == *layout;
                let shown = if layout.is_wide() {
                    PREVIEW_WIDE
                } else {
                    PREVIEW
                };
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
                                    .fit_to_exact_size(shown)
                                    .sense(Sense::click()),
                            );
                            if image.clicked() {
                                *chosen_layout = *layout;
                            }
                            search::radio(ui, chosen_layout, *layout, layout.label())
                                .on_hover_text(tr(layout.description()));
                        });
                    });
                if i % PER_ROW == PER_ROW - 1 {
                    ui.end_row();
                }
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
    if wide {
        ui.label(
            RichText::new(tr(
                "The 320 × 240 layouts sit in the middle of these larger screens.",
            ))
            .weak(),
        );
    }
    ui.add(egui::Image::new(previews.logo(ui.ctx(), s)).fit_to_exact_size(PREVIEW));
}
