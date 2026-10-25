use backspn_engine::{Command, CueKind, DeckCommand, DeckState};
use eframe::egui::{self, Button, Rect, Sense, Ui, vec2};

use crate::cue_list::CueList;
use crate::theme;
use crate::waveform;
use crate::widgets;

use super::header::{key_label, resulting_key};
use super::{DeckView, OVERVIEW, PLAYHEAD_AT, ZOOM_STEP};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct OverviewKey {
    generation: u64,
    width: u32,
    colors: [egui::Color32; 3],
}

fn upload(
    ui: &Ui,
    slot: &mut Option<egui::TextureHandle>,
    name: &str,
    image: egui::ColorImage,
) -> egui::TextureHandle {
    match slot {
        Some(texture) => texture.set(image, egui::TextureOptions::LINEAR),
        None => {
            *slot = Some(
                ui.ctx()
                    .load_texture(name, image, egui::TextureOptions::LINEAR),
            );
        }
    }
    slot.clone().expect("just set")
}

impl DeckView {
    #[allow(clippy::too_many_lines)]
    pub(super) fn waveforms(&mut self, ui: &mut Ui, state: &DeckState, out: &mut Vec<Command>) {
        self.zoom_view(ui, state, self.zoom_height, out);
        ui.add_space(2.0);
        self.overview_row(ui, state, false, out);
    }

    #[allow(clippy::too_many_lines)]
    pub fn zoom_view(
        &mut self,
        ui: &mut Ui,
        state: &DeckState,
        height: f32,
        out: &mut Vec<Command>,
    ) {
        let mut zoom_texture = self.zoom_texture.take();
        let t = theme::get(ui.ctx());
        let width = ui.available_width();
        let mut seek = None;
        let mut save = false;

        let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
        let painter = ui.painter_at(rect);
        let radius = if self.size.parallel_wave().is_some() {
            0.0
        } else {
            6.0
        };
        painter.rect_filled(rect, radius, t.well);
        let style = waveform::WaveStyle::get(ui.ctx());
        self.zoom_buttons(ui, rect, &t);
        let cue_marks: Vec<(f64, CueKind)> = self
            .cues()
            .into_iter()
            .flat_map(CueList::cues)
            .filter(|c| c.hotcue.is_none())
            .map(|c| (c.start_s, CueKind::from(c.kind)))
            .collect();
        let peaks = self
            .loaded
            .as_ref()
            .filter(|_| state.loaded)
            .map(|l| &l.peaks);
        let span = 2.0 * self.zoom_s * state.tempo.max(0.05);
        let window = (
            state.position_s - span * PLAYHEAD_AT,
            state.position_s + span * (1.0 - PLAYHEAD_AT),
        );
        if let Some(peaks) = peaks {
            if let Some(grid) = state.grid {
                for (at, bar) in grid.beats_in(window.0, window.1) {
                    let (color, width) = if bar {
                        (t.text.gamma_multiply(0.75), 1.5)
                    } else {
                        (t.beat.gamma_multiply(1.6), 1.0)
                    };
                    waveform::marker(
                        &painter,
                        rect,
                        waveform::x_of(rect, window, at),
                        color,
                        width,
                    );
                }
            }
            if let (Some(start), Some(end)) = (state.loop_start_s, state.loop_end_s) {
                waveform::loop_region(&painter, rect, window, (start, end), state.loop_active);
            }
            if let Some(freeze) = state.freeze {
                for at in freeze.bounds_s {
                    let x = waveform::x_of(rect, window, at);
                    waveform::marker(&painter, rect, x, t.accent.gamma_multiply(0.8), 1.5);
                }
            }
            if style.mode == waveform::WaveMode::Spectrum {
                let image = waveform::density_image(ui, rect, peaks, window, &style);
                let texture = upload(ui, &mut zoom_texture, "zoom", image);
                waveform::paint_texture(&painter, rect, &texture);
                waveform::dim_played(&painter, rect, window, state.position_s);
            } else {
                waveform::paint(&painter, rect, peaks, window, state.position_s, &style);
            }
            Self::markers(&painter, rect, window, state, &cue_marks, &t);
            slip_marker(&painter, rect, window, state, 3.0, &t);
            if response.clicked()
                && let Some(pos) = response.interact_pointer_pos()
            {
                seek = Some(waveform::time_at(rect, window, pos));
            }
            if let Some(handle) = Self::loop_handle(
                ui,
                (rect, window),
                state,
                ("zoom loop", self.index),
                &mut save,
            ) && handle.clicked()
                && let Some(pos) = handle.interact_pointer_pos()
            {
                seek = Some(waveform::time_at(rect, window, pos));
            }
        }
        let x = waveform::x_of(rect, window, state.position_s);
        waveform::marker(&painter, rect, x, t.danger, 2.0);
        painter.add(egui::Shape::convex_polygon(
            vec![
                egui::pos2(x - 5.0, rect.top()),
                egui::pos2(x + 5.0, rect.top()),
                egui::pos2(x, rect.top() + 7.0),
            ],
            t.danger,
            egui::Stroke::NONE,
        ));

        self.zoom_texture = zoom_texture;
        self.pending_save_loop |= save;
        if let Some(seconds) = seek {
            out.push(self.deck(DeckCommand::Seek { seconds }));
        }
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn overview_row(
        &mut self,
        ui: &mut Ui,
        state: &DeckState,
        micro: bool,
        out: &mut Vec<Command>,
    ) {
        let mut overview_texture = self.overview_texture.take();
        let t = theme::get(ui.ctx());
        let style = waveform::WaveStyle::get(ui.ctx());
        let mut seek = None;
        let mut save = false;
        let cue_marks: Vec<(f64, CueKind)> = self
            .cues()
            .into_iter()
            .flat_map(CueList::cues)
            .filter(|c| c.hotcue.is_none())
            .map(|c| (c.start_s, CueKind::from(c.kind)))
            .collect();
        let peaks = self
            .loaded
            .as_ref()
            .filter(|_| state.loaded)
            .map(|l| &l.peaks);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.spacing_mut().button_padding.x = 3.0;
            if micro {
                let icon = if state.playing {
                    widgets::Icon::Pause
                } else {
                    widgets::Icon::Play
                };
                if widgets::transport_icon(ui, icon, state.playing, t.play, vec2(44.0, OVERVIEW))
                    .clicked()
                {
                    out.push(self.deck(DeckCommand::TogglePlay));
                }
            } else if ui
                .add_sized(vec2(26.0, OVERVIEW), Button::new("⏮"))
                .on_hover_text(crate::i18n::tr("Back to the start"))
                .clicked()
            {
                out.push(self.deck(DeckCommand::Seek { seconds: 0.0 }));
            }
            let key_width = if micro { 0.0 } else { KEY_CONTROL };
            let strip = (ui.available_width() - key_width - 4.0).max(40.0);
            let (rect, response) =
                ui.allocate_exact_size(vec2(strip, OVERVIEW), Sense::click_and_drag());
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 0.0, t.well);
            if let Some(peaks) = peaks.filter(|_| state.duration_s > 0.0) {
                let whole = (0.0, state.duration_s);
                if let (Some(start), Some(end)) = (state.loop_start_s, state.loop_end_s) {
                    waveform::loop_region(&painter, rect, whole, (start, end), state.loop_active);
                }
                if style.mode == waveform::WaveMode::Spectrum {
                    let key = OverviewKey {
                        generation: state.generation,
                        width: rect.width().round() as u32,
                        colors: style.colors,
                    };
                    let texture = match &overview_texture {
                        Some((texture, k)) if *k == key => texture.clone(),
                        _ => {
                            let image = waveform::density_image(ui, rect, peaks, whole, &style);
                            let texture = ui.ctx().load_texture(
                                "overview",
                                image,
                                egui::TextureOptions::LINEAR,
                            );
                            overview_texture = Some((texture.clone(), key));
                            texture
                        }
                    };
                    waveform::paint_texture(&painter, rect, &texture);
                    waveform::dim_played(&painter, rect, whole, state.position_s);
                } else {
                    waveform::paint(&painter, rect, peaks, whole, state.position_s, &style);
                }
                if let Some(since) = self.end_warning_since
                    && crate::track_end::overlay_on(since.elapsed().as_secs_f64())
                {
                    let x = waveform::x_of(rect, whole, state.position_s)
                        .clamp(rect.left(), rect.right());
                    painter.rect_filled(
                        Rect::from_min_max(rect.min, egui::pos2(x, rect.bottom())),
                        0.0,
                        t.danger.gamma_multiply(0.35),
                    );
                }
                Self::markers(&painter, rect, whole, state, &cue_marks, &t);
                slip_marker(&painter, rect, whole, state, 1.0, &t);
                waveform::marker(
                    &painter,
                    rect,
                    waveform::x_of(rect, whole, state.position_s),
                    t.danger,
                    1.5,
                );
                if let Some(pos) = response
                    .interact_pointer_pos()
                    .filter(|_| response.clicked() || response.dragged())
                {
                    seek = Some(waveform::time_at(rect, whole, pos));
                }
                if let Some(handle) = Self::loop_handle(
                    ui,
                    (rect, whole),
                    state,
                    ("overview loop", self.index),
                    &mut save,
                ) && handle.clicked()
                    && let Some(pos) = handle.interact_pointer_pos()
                {
                    seek = Some(waveform::time_at(rect, whole, pos));
                }
            }
            if !micro {
                self.key_control(ui, state, key_width, out);
            }
        });
        self.overview_texture = overview_texture;
        self.pending_save_loop |= save;
        if let Some(seconds) = seek {
            out.push(self.deck(DeckCommand::Seek { seconds }));
        }
    }

    fn loop_handle(
        ui: &Ui,
        (rect, window): (Rect, (f64, f64)),
        state: &DeckState,
        (name, deck): (&str, usize),
        save: &mut bool,
    ) -> Option<egui::Response> {
        let (Some(start), Some(end)) = (state.loop_start_s, state.loop_end_s) else {
            return None;
        };
        let span = waveform::loop_span(rect, window, (start, end)).filter(|_| state.loop_active)?;
        let handle = ui
            .interact(
                span,
                ui.scope_id().with((name, deck)),
                Sense::click_and_drag(),
            )
            .on_hover_cursor(egui::CursorIcon::Grab)
            .on_hover_text(crate::i18n::tr(
                "Drag the loop onto a sample deck cell or onto Loops; right-click to save it",
            ));
        Self::loop_drag_source(&handle, deck, save);
        Some(handle)
    }

    pub(super) fn loop_drag_source(response: &egui::Response, deck: usize, save: &mut bool) {
        response.dnd_set_drag_payload(crate::sample_view::LoopDrag::Deck(deck));
        response.context_menu(|ui| {
            if ui.button(crate::i18n::tr("Save as Loop")).clicked() {
                *save = true;
                ui.close();
            }
        });
    }

    fn key_control(&self, ui: &mut Ui, state: &DeckState, width: f32, out: &mut Vec<Command>) {
        let t = theme::get(ui.ctx());
        let text = self
            .loaded
            .as_ref()
            .filter(|_| state.loaded)
            .and_then(|l| l.key_text.as_deref());
        let label = key_label(text, self.key_notation, state.keylock, state.key_adjust);
        let colour =
            resulting_key(text, state.keylock, state.key_adjust).map(crate::key_view::key_colour);
        ui.spacing_mut().item_spacing.x = 0.0;
        let arrow = |ui: &mut Ui, label: &str, hint: &str| {
            ui.add_sized(
                vec2(KEY_ARROW, OVERVIEW),
                Button::new(label).corner_radius(0.0),
            )
            .on_hover_text(crate::i18n::tr(hint))
            .clicked()
        };
        if arrow(ui, "‹", "Key a semitone down (heard with keylock)") {
            let to = crate::key_shift::semitone_step(state.key_adjust, false);
            out.push(self.deck(DeckCommand::SetKeyAdjust(to)));
        }
        let (rect, response) =
            ui.allocate_exact_size(vec2(width - 2.0 * KEY_ARROW, OVERVIEW), Sense::click());
        let painter = ui.painter_at(rect);
        let (fill, ink) = match (state.keylock, colour) {
            (true, Some(c)) => (c.gamma_multiply(0.3), c),
            (true, None) => (t.accent.gamma_multiply(0.3), t.accent),
            (false, _) => (t.well, t.text_dim),
        };
        painter.rect_filled(rect, 0.0, fill);
        if response.hovered() {
            painter.rect_stroke(
                rect.shrink(0.5),
                0.0,
                egui::Stroke::new(1.0, ink),
                egui::StrokeKind::Inside,
            );
        }
        match label {
            Some(label) => {
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    label,
                    egui::FontId::monospace(13.0),
                    ink,
                );
            }
            None => padlock(&painter, rect.center(), state.keylock, ink),
        }
        if response
            .on_hover_text(crate::i18n::tr(if state.keylock {
                "Keylock on: the key stays as the tempo changes. Click to turn it off"
            } else {
                "Keylock off: the key follows the tempo. Click to turn it on"
            }))
            .clicked()
        {
            out.push(self.deck(DeckCommand::SetKeylock(!state.keylock)));
        }
        if arrow(ui, "›", "Key a semitone up (heard with keylock)") {
            let to = crate::key_shift::semitone_step(state.key_adjust, true);
            out.push(self.deck(DeckCommand::SetKeyAdjust(to)));
        }
    }

    fn zoom_buttons(&mut self, ui: &mut Ui, rect: Rect, t: &theme::Theme) {
        let size = vec2(20.0, 18.0);
        let buttons = [
            ("−", ZOOM_STEP, "Zoom out"),
            ("+", 1.0 / ZOOM_STEP, "Zoom in"),
        ];
        for (i, (label, factor, hint)) in buttons.into_iter().enumerate() {
            let min = rect.right_top() + vec2(-(2.0 - i as f32) * (size.x + 2.0) - 2.0, 4.0);
            let area = Rect::from_min_size(min, size);
            let id = ui.scope_id().with(("zoom", self.index, i));
            let response = ui.interact(area, id, Sense::click()).on_hover_text(hint);
            let fill = if response.hovered() {
                t.well.gamma_multiply(1.6)
            } else {
                t.well
            };
            ui.painter()
                .rect_filled(area, 0.0, fill.gamma_multiply(0.9));
            ui.painter().text(
                area.center(),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(13.0),
                t.text_dim,
            );
            if response.clicked() {
                self.set_zoom(self.zoom_s * factor);
            }
        }
    }

    fn markers(
        painter: &egui::Painter,
        rect: Rect,
        window: (f64, f64),
        state: &DeckState,
        cue_marks: &[(f64, CueKind)],
        t: &theme::Theme,
    ) {
        let cue_x = waveform::x_of(rect, window, state.cue_s);
        waveform::marker(painter, rect, cue_x, t.cue, 2.0);
        for &(at, kind) in cue_marks {
            waveform::cue_mark(painter, rect, window, at, t.hotcue(kind));
        }
        for (n, h) in state.hotcues.iter().enumerate() {
            if let Some(h) = h {
                waveform::hotcue(painter, rect, window, h.start_s, n + 1, t.hotcue(h.kind));
            }
        }
    }
}

fn slip_marker(
    painter: &egui::Painter,
    rect: Rect,
    window: (f64, f64),
    state: &DeckState,
    width: f32,
    t: &theme::Theme,
) {
    let Some(at) = state.slip_position_s else {
        return;
    };
    let x = waveform::x_of(rect, window, at);
    waveform::marker(
        painter,
        rect,
        x,
        egui::Color32::from_black_alpha(160),
        width + 2.0,
    );
    waveform::marker(painter, rect, x, t.accent, width);
}

const KEY_CONTROL: f32 = 104.0;
const KEY_ARROW: f32 = 18.0;

fn padlock(painter: &egui::Painter, center: egui::Pos2, locked: bool, colour: egui::Color32) {
    let body = Rect::from_center_size(center + vec2(0.0, 3.0), vec2(12.0, 9.0));
    painter.rect_filled(body, 1.5, colour);
    let stroke = egui::Stroke::new(1.6, colour);
    let (left, right) = (body.left() + 3.0, body.right() - 3.0);
    let top = body.top() - 6.0;
    let right_end = if locked { body.top() } else { body.top() - 3.0 };
    let arc: Vec<egui::Pos2> = (0..=12)
        .map(|i| {
            let a = std::f32::consts::PI * (1.0 + i as f32 / 12.0);
            let r = (right - left) / 2.0;
            egui::pos2(center.x + r * a.cos(), top + r + r * a.sin())
        })
        .collect();
    painter.add(egui::Shape::line(arc, stroke));
    let r = (right - left) / 2.0;
    painter.line_segment(
        [egui::pos2(left, top + r), egui::pos2(left, body.top())],
        stroke,
    );
    painter.line_segment(
        [egui::pos2(right, top + r), egui::pos2(right, right_end)],
        stroke,
    );
}
