use backspn_engine::{Command, DeckCommand, DeckState, SyncCommand, SyncState};
use backspn_formats::nml::{KeyNotation, MusicalKey};
use eframe::egui::{self, Align, Button, Layout, Rect, RichText, Sense, Slider, Ui, vec2};

use crate::covers;
use crate::deck_informer::{self, Level, Show};
use crate::i18n::tr;
use crate::theme;
use crate::widgets;

use super::{COVER, DeckEvent, DeckSize, DeckView, JUMP_BUTTON, TEMPO_RANGES};

const SMALL_ROW: f32 = 24.0;

fn mono(text: &str, size: f32, color: egui::Color32) -> RichText {
    RichText::new(text).monospace().size(size).color(color)
}

const SYNC_PHASE_TOLERANCE: f32 = 0.008;

fn phase_meter(ui: &Ui, rect: Rect, phase: f32, t: &theme::Theme) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, t.well);
    let phase = phase.clamp(-0.5, 0.5);
    let centre = rect.center().x;
    let head = centre + rect.width() * phase;
    let inner = rect.shrink2(vec2(0.0, 3.0));
    if (head - centre).abs() >= 1.0 {
        let bar = Rect::from_x_y_ranges(head.min(centre)..=head.max(centre), inner.y_range());
        painter.rect_filled(bar, 0.0, t.sync.gamma_multiply(0.45));
        painter.line_segment(
            [
                egui::pos2(head, inner.top()),
                egui::pos2(head, inner.bottom()),
            ],
            egui::Stroke::new(2.0, t.sync),
        );
    }
    for (mark, reach) in [(0.25, 6.0), (0.5, 3.0), (0.75, 6.0)] {
        let x = rect.left() + rect.width() * mark;
        painter.line_segment(
            [
                egui::pos2(x, rect.top() + reach),
                egui::pos2(x, rect.bottom() - reach),
            ],
            egui::Stroke::new(1.0, t.text_dim.gamma_multiply(0.6)),
        );
    }
}

fn readout(ui: &mut Ui, text: &str, size: f32, color: egui::Color32) {
    ui.add(egui::Label::new(mono(text, size, color)).extend());
}

pub(crate) fn clock(seconds: f64) -> String {
    let s = seconds.max(0.0).floor() as u64;
    format!("{:02}:{:02}", s / 60, s % 60)
}

pub(super) fn show_key(text: &str, notation: KeyNotation) -> String {
    MusicalKey::parse(text).map_or_else(|| text.to_owned(), |k| k.name(notation))
}

pub(crate) fn key_label(
    text: Option<&str>,
    notation: KeyNotation,
    keylock: bool,
    semitones: f32,
) -> Option<String> {
    if !keylock {
        return text.map(|t| show_key(t, notation));
    }
    let steps = semitones.round();
    match text
        .and_then(MusicalKey::parse)
        .and_then(|k| k.shifted(semitones))
    {
        Some(k) if steps != 0.0 => Some(format!("{} {steps:+}", k.name(notation))),
        Some(k) => Some(k.name(notation)),
        None => (semitones.abs() > 0.01).then(|| format!("{semitones:+.2}")),
    }
}

pub(crate) fn resulting_key(
    text: Option<&str>,
    keylock: bool,
    semitones: f32,
) -> Option<MusicalKey> {
    let key = text.and_then(MusicalKey::parse)?;
    if keylock {
        key.shifted(semitones)
    } else {
        Some(key)
    }
}

impl DeckView {
    #[allow(clippy::too_many_lines)]
    pub(super) fn header(
        &mut self,
        ui: &mut Ui,
        state: &DeckState,
        sync: &SyncState,
        out: &mut Vec<Command>,
    ) {
        let t = theme::get(ui.ctx());
        let color = t.deck(self.index);
        let loaded = self.loaded.as_mut().filter(|_| state.loaded);
        let (title, artist, comment) = match (&self.loading, &loaded) {
            _ if self.live => ("Live Input".to_owned(), String::new(), String::new()),
            _ if self.sample_deck.is_some() => (
                self.sample_deck
                    .as_ref()
                    .map_or_else(String::new, |r| r.set.title.clone()),
                "Sample Set".to_owned(),
                String::new(),
            ),
            (Some(loading), _) if !loading.decoded => (
                format!("Loading {}…", loading.request.title),
                String::new(),
                String::new(),
            ),
            (_, Some(l)) => (l.title.clone(), l.artist.clone(), l.comment.clone()),
            _ => ("Empty".to_owned(), String::new(), String::new()),
        };
        let notation = self.key_notation;
        let key = key_label(
            loaded.as_ref().and_then(|l| l.key_text.as_deref()),
            notation,
            state.keylock,
            state.key_adjust,
        );
        let cover = loaded.and_then(|l| {
            if l.cover_texture.is_none()
                && let Some(image) = l.cover.take()
            {
                l.cover_texture = Some(ui.ctx().load_texture(
                    "cover",
                    image,
                    egui::TextureOptions::LINEAR,
                ));
            }
            l.cover_texture.clone()
        });
        let (third, third_color) = match &self.loading {
            Some(l) if l.decoded => ("Analysing…".to_owned(), t.text_dim),
            _ => (comment, t.text_dim),
        };
        let now = deck_informer::now();
        self.informer.update(now);
        if let Some(wait) = self.informer.next_change(now) {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(wait));
        }
        let (title, title_color, artist, artist_color) = match self.informer.show(now) {
            Show::Notice { notice, bright } => {
                let color = notice_color(&t, notice.level, bright);
                (
                    tr(&notice.short).to_owned(),
                    color,
                    tr(&notice.long).to_owned(),
                    color,
                )
            }
            Show::Track => (title, t.text, artist, t.text_dim),
        };
        let small = self.sample_deck.is_some()
            || matches!(self.size, DeckSize::Micro | DeckSize::Essential);
        let row = if small { SMALL_ROW } else { COVER };
        ui.horizontal_top(|ui| {
            if !small {
                let (rect, response) = ui.allocate_exact_size(vec2(COVER, COVER), Sense::click());
                covers::paint(
                    ui.painter(),
                    rect,
                    cover.as_ref().map(egui::TextureHandle::id),
                    color,
                );
                widgets::hover_outline(ui, &response);
                if response.on_hover_text(tr("Load a file…")).clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("Audio", crate::AUDIO_EXTENSIONS)
                        .pick_file()
                {
                    self.pending_open = Some(path);
                }
            }
            let right = 250.0;
            let text_width = (ui.available_width() - right).max(60.0);
            let names = ui.allocate_ui_with_layout(
                vec2(text_width, row),
                Layout::top_down(Align::Min),
                |ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    let line =
                        |ui: &mut Ui, text: &str, size: f32, color: egui::Color32, strong: bool| {
                            let mut rich = RichText::new(text).size(size).color(color);
                            if strong {
                                rich = rich.strong();
                            }
                            ui.add(egui::Label::new(rich).truncate());
                        };
                    line(ui, &title, 17.0, title_color, true);
                    if !small {
                        line(ui, &artist, 13.5, artist_color, false);
                        line(ui, &third, 12.5, third_color, false);
                    }
                },
            );
            if let Some(track) = self.track_id() {
                let names = names.response.interact(Sense::click_and_drag());
                names.dnd_set_drag_payload(crate::sample_view::DeckDrag(self.index));
                names.context_menu(|ui| {
                    if ui.button(tr("Show in browser")).clicked() {
                        self.events.push(DeckEvent::ShowInBrowser(track));
                        ui.close();
                    }
                });
            }
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                let letter = ui
                    .add(
                        egui::Label::new(
                            RichText::new(self.letter().to_string())
                                .size(if small { 24.0 } else { 40.0 })
                                .strong()
                                .color(t.deck_ink(self.index)),
                        )
                        .sense(Sense::click()),
                    )
                    .on_hover_text(tr("Deck size: click for the next one"));
                if letter.clicked() {
                    self.set_size(self.size().next());
                }
                let (rect, mark) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::click());
                let fill = if self.focused { t.accent } else { t.border };
                ui.painter().rect_filled(rect, 2.0, fill);
                if mark
                    .on_hover_text(tr("Deck focus: click to focus this deck"))
                    .clicked()
                {
                    self.events
                        .push(DeckEvent::Action(backspn_control::Action::DeckFocus {
                            deck: self.index,
                        }));
                }
                ui.add_space(4.0);
                let bpm = state.effective_bpm();
                let original = state.grid.map(|g| g.bpm);
                ui.allocate_ui_with_layout(vec2(88.0, row), Layout::top_down(Align::Max), |ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    readout(
                        ui,
                        &bpm.map_or("—".into(), |b| format!("{b:.2}")),
                        19.0,
                        t.text,
                    );
                    if small {
                        return;
                    }
                    readout(
                        ui,
                        &format!("{:+.1}%", (state.tempo - 1.0) * 100.0),
                        13.5,
                        t.text_dim,
                    );
                    readout(
                        ui,
                        &original.map_or(String::new(), |b| format!("{b:.2}")),
                        13.5,
                        t.text_dim,
                    );
                });
                ui.add_space(8.0);
                ui.allocate_ui_with_layout(vec2(84.0, row), Layout::top_down(Align::Max), |ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    let remaining = (state.duration_s - state.position_s).max(0.0);
                    let warn = crate::track_end::warning(state, self.end_warning_s);
                    readout(
                        ui,
                        &format!("-{}", clock(remaining)),
                        19.0,
                        if warn { t.danger } else { t.text },
                    );
                    if small {
                        return;
                    }
                    readout(ui, &clock(state.duration_s), 13.5, t.text_dim);
                    let key_color = if state.keylock { t.accent } else { t.text_dim };
                    let response = ui
                        .add(
                            egui::Label::new(mono(key.as_deref().unwrap_or("—"), 13.5, key_color))
                                .extend()
                                .sense(Sense::click()),
                        )
                        .on_hover_text(if state.keylock {
                            "Keylock on (click to turn off)"
                        } else {
                            "Click: keylock"
                        });
                    if response.clicked() && state.loaded {
                        out.push(self.deck(DeckCommand::SetKeylock(!state.keylock)));
                    }
                });
                let _ = sync;
            });
        });
    }

    pub(super) fn control_row(
        &mut self,
        ui: &mut Ui,
        state: &DeckState,
        sync: &SyncState,
        out: &mut Vec<Command>,
    ) {
        let t = theme::get(ui.ctx());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let synced = sync.synced[self.index];
            let phase = sync.phase[self.index];
            let lit = match (synced, sync.phase_locked[self.index]) {
                (false, _) => widgets::Lit::Off,
                (true, true) if phase.abs() < SYNC_PHASE_TOLERANCE => widgets::Lit::On,
                (true, _) => widgets::Lit::Half,
            };
            let sync_button = widgets::toggle_lit(ui, "SYNC", lit, t.sync);
            if sync_button
                .on_hover_text(tr("Synchronise the tempo and phase to the tempo master"))
                .clicked()
            {
                let on = !synced || lit == widgets::Lit::Half;
                out.push(Command::Sync(SyncCommand::SetSync(self.index, on)));
            }
            let is_master = sync.master == Some(self.index);
            if widgets::toggle(ui, "MASTER", is_master, t.master)
                .on_hover_text(tr("Make this deck the tempo master"))
                .clicked()
            {
                let master = (!is_master).then_some(self.index);
                out.push(Command::Sync(SyncCommand::SetMaster(master)));
            }
            let bends = 2.0 * (JUMP_BUTTON + 4.0);
            let width = (ui.available_width() - bends).max(40.0);
            let (rect, response) = ui.allocate_exact_size(vec2(width, 22.0), Sense::drag());
            phase_meter(ui, rect, phase, &t);
            let dragged = response.drag_delta().x;
            if dragged != 0.0
                && let Some(grid) = state.grid.filter(|_| state.loaded)
            {
                let beats = f64::from(dragged / rect.width());
                out.push(self.deck(DeckCommand::Seek {
                    seconds: state.position_s + beats * grid.period_s(),
                }));
                if synced {
                    out.push(Command::Sync(SyncCommand::ReleasePhase(self.index)));
                }
            }
            response
                .on_hover_cursor(egui::CursorIcon::ResizeHorizontal)
                .on_hover_text(tr("Phase offset to the tempo master; drag to adjust"));
            for (n, (label, direction, hint)) in [
                ("◀", -1.0, "Bend back: slows the deck down while held"),
                ("▶", 1.0, "Bend forward: speeds the deck up while held"),
            ]
            .into_iter()
            .enumerate()
            {
                let button = ui
                    .add_sized(vec2(JUMP_BUTTON, 22.0), Button::new(label))
                    .on_hover_text(hint);
                if let Some(down) = self.bend[n].button(&button) {
                    self.events.push(DeckEvent::Bend {
                        deck: self.index,
                        direction,
                        down,
                    });
                }
            }
        });
    }

    pub(super) fn tempo_fader(
        &mut self,
        ui: &mut Ui,
        state: &DeckState,
        sync: &SyncState,
        height: f32,
        out: &mut Vec<Command>,
    ) {
        let follower = sync.synced[self.index] && sync.master != Some(self.index);
        if follower || (state.tempo - 1.0).abs() > f64::from(self.tempo_range) {
            self.tempo_fader = (state.tempo as f32 - 1.0) / self.tempo_range;
        }
        let range_row = 20.0;
        ui.spacing_mut().slider_width = (height - range_row - 10.0).max(40.0);
        let mut shown = (-self.tempo_fader).clamp(-1.0, 1.0);
        let slider = ui
            .add_enabled(
                !follower,
                Slider::new(&mut shown, -1.0..=1.0)
                    .vertical()
                    .show_value(false)
                    .trailing_fill(false),
            )
            .on_hover_text(tr("Tempo (double-click resets)"));
        if slider.changed() {
            self.tempo_fader = -shown;
            out.push(self.deck(DeckCommand::SetTempo(
                1.0 + f64::from(self.tempo_fader * self.tempo_range),
            )));
        }
        if slider.double_clicked() {
            self.tempo_fader = 0.0;
            out.push(self.deck(DeckCommand::SetTempo(1.0)));
        }
        if ui
            .add(
                egui::Label::new(
                    RichText::new(format!("±{:.0}", self.tempo_range * 100.0))
                        .size(10.5)
                        .color(theme::get(ui.ctx()).text_dim),
                )
                .sense(Sense::click()),
            )
            .on_hover_text(tr("Tempo range (click to change)"))
            .clicked()
        {
            let at = usize::from(self.tempo_range_index());
            self.tempo_range = TEMPO_RANGES[(at + 1) % TEMPO_RANGES.len()];
        }
    }
}

fn notice_color(t: &theme::Theme, level: Level, bright: bool) -> egui::Color32 {
    let color = match level {
        Level::Error => t.danger,
        Level::Warning => t.cue,
        Level::Status => t.text,
    };
    if bright {
        color
    } else {
        color.gamma_multiply(0.6)
    }
}
