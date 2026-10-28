use backspn_engine::{Command, EngineState, MAX_CLOCK_BPM, MIN_CLOCK_BPM, SyncCommand};
use eframe::egui::{self, Align, DragValue, FontId, Layout, RichText, TextStyle, Ui, vec2};

use crate::i18n::tr;
use crate::{App, key_view, midi_clock, suggest_panel, theme, widgets};

const GLOBAL_BLOCK: f32 = 380.0;
const GLOBAL_TEMPO_WIDTH: f32 = 210.0;
const GLOBAL_TEMPO_SIZE: f32 = 24.0;
const GLOBAL_ROW: f32 = 24.0;
const GLOBAL_TOP: f32 = 2.0 * GLOBAL_ROW + GLOBAL_GAP;
const GLOBAL_GAP: f32 = 4.0;

pub(crate) const BROWSER_MIN: f32 = 120.0;
pub(crate) const GAP: f32 = -theme::BORDER;

fn side_width(ui: &Ui, middle: f32, min: f32) -> f32 {
    ((visible_width(ui) - middle) / 2.0 + theme::BORDER).max(min)
}

pub(crate) fn seam(ui: &mut Ui) {
    let spacing = ui.spacing().item_spacing;
    let gap = if ui.layout().main_dir().is_horizontal() {
        spacing.x
    } else {
        spacing.y
    };
    ui.add_space(-gap - theme::BORDER);
}

fn visible_width(ui: &Ui) -> f32 {
    ui.clip_rect().width().min(ui.available_width())
}

const LOGO_SIZE: f32 = 28.0;

impl App {
    pub(crate) fn top_bar(&mut self, ui: &mut Ui) {
        let t = theme::get(ui.ctx());
        ui.horizontal(|ui| {
            if let Some(logo) = crate::icon::logo(ui.ctx()) {
                ui.image((logo.id(), eframe::egui::Vec2::splat(LOGO_SIZE)));
            }
            ui.label(
                RichText::new("BACKSPN")
                    .strong()
                    .size(16.0)
                    .color(t.accent)
                    .extra_letter_spacing(2.0),
            );
            self.update_notice(ui);
            if cfg!(not(target_os = "macos"))
                && widgets::toggle(ui, "⚙", self.prefs.open, t.accent)
                    .on_hover_text(tr("Preferences (Ctrl+,)"))
                    .clicked()
            {
                self.prefs.toggle();
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                self.layout_selector(ui);
                if self.settings.show_browser
                    && widgets::toggle(ui, "⤢ Browser", self.browser_maximized, t.accent)
                        .on_hover_text(tr("Maximize browser: the browser takes the whole window"))
                        .clicked()
                {
                    self.browser_maximized = !self.browser_maximized;
                }
                if widgets::toggle(ui, "?", self.settings.show_tooltips, t.accent)
                    .on_hover_text(tr("Show tooltips"))
                    .clicked()
                {
                    self.settings.show_tooltips = !self.settings.show_tooltips;
                    crate::theme::set_tooltips(ui.ctx(), self.settings.show_tooltips);
                }
                ui.separator();
                self.recorder_bar(ui);
            });
        });
    }

    fn layout_selector(&mut self, ui: &mut Ui) {
        let active = self.active_layout();
        let shown = active.map_or_else(
            || tr("Unsaved layout").to_owned(),
            |i| self.settings.layouts[i].name.clone(),
        );
        let mut picked = None;
        let mut manage = false;
        egui::ComboBox::from_id_salt("layout-selector")
            .selected_text(shown)
            .width(150.0)
            .show_ui(ui, |ui| {
                for (i, layout) in self.settings.layouts.iter().enumerate() {
                    if ui
                        .selectable_label(active == Some(i), &layout.name)
                        .clicked()
                    {
                        picked = Some(i);
                    }
                }
                if !self.settings.layouts.is_empty() {
                    ui.separator();
                }
                manage = ui.button(tr("Manage layouts…")).clicked();
            })
            .response
            .on_hover_text(tr("Layout: decks, FX units and what the screen shows"));
        if let Some(i) = picked {
            self.apply_layout(i);
        }
        if manage {
            self.prefs.open_layouts();
        }
    }

    pub(crate) fn global_row(&mut self, ui: &mut Ui, state: &EngineState) {
        let gap = ui.spacing().item_spacing.x;
        let units = self.settings.fx_units;
        let middle = self.middle_width();
        let side = side_width(ui, middle, 200.0);
        let mut fx_height = 0.0f32;
        self.fx.set_pattern_steps(&state.fx);
        ui.horizontal_top(|ui| {
            if self.settings.global_left_loop_recorder {
                ui.allocate_ui_with_layout(vec2(side, 0.0), Layout::top_down(Align::Min), |ui| {
                    fx_height = self.loop_recorder_panel(ui, state);
                });
            } else if self.settings.show_fx {
                ui.allocate_ui_with_layout(vec2(side, 0.0), Layout::top_down(Align::Min), |ui| {
                    fx_height = self.fx.side(ui, 0, units, &mut self.outbox);
                });
            } else {
                ui.add_space(side + gap);
            }
            seam(ui);
            ui.allocate_ui_with_layout(vec2(middle, 0.0), Layout::top_down(Align::Min), |ui| {
                self.global_block(ui, state, fx_height);
            });
            if self.settings.show_fx {
                seam(ui);
                let right = visible_width(ui).max(side);
                ui.allocate_ui_with_layout(vec2(right, 0.0), Layout::top_down(Align::Min), |ui| {
                    self.fx.side(ui, 1, units, &mut self.outbox);
                });
            }
        });
    }

    fn global_block(&mut self, ui: &mut Ui, state: &EngineState, height: f32) {
        let t = theme::get(ui.ctx());
        theme::card(&t).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let inner = (height - theme::CARD_CHROME).max(0.0);
            ui.set_min_height(inner);
            let midi = self.settings.midi_clock_send;
            let rows = if midi { 2.0 } else { 1.0 };
            let content = GLOBAL_TOP + rows * (GLOBAL_GAP + GLOBAL_ROW);
            ui.add_space(((inner - content) / 2.0).max(0.0));
            let side = ((ui.available_width() - GLOBAL_TEMPO_WIDTH) / 2.0).max(0.0);
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, GLOBAL_GAP);
                ui.allocate_ui_with_layout(
                    vec2(side, content),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.set_min_width(side);
                        self.snap_quantize(ui, &t);
                        if self.settings.clock_source_link {
                            self.link_button(ui, t.accent);
                        } else {
                            self.ext_button(ui, &state.sync, t.accent);
                        }
                    },
                );
                ui.allocate_ui_with_layout(
                    vec2(GLOBAL_TEMPO_WIDTH, content),
                    Layout::top_down(Align::Center),
                    |ui| {
                        ui.set_min_width(GLOBAL_TEMPO_WIDTH);
                        self.tempo_section(ui, state, &t);
                        if midi {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 3.0;
                                self.midi_clock_controls(ui, state.midi_clock_running, t.accent);
                            });
                        }
                    },
                );
                ui.allocate_ui_with_layout(
                    vec2(side, content),
                    Layout::top_down(Align::Max),
                    |ui| {
                        ui.set_min_width(side);
                        ui.allocate_ui_with_layout(
                            vec2(side, GLOBAL_TOP),
                            Layout::top_down(Align::Max),
                            |ui| {
                                ui.set_min_height(GLOBAL_TOP);
                                self.mixer.main_volume(ui, &state.mixer, &mut self.outbox);
                            },
                        );
                        self.mixer.limiter(ui, &state.mixer, &mut self.outbox);
                    },
                );
            });
        });
    }

    fn ext_button(&mut self, ui: &mut Ui, sync: &backspn_engine::SyncState, accent: egui::Color32) {
        let lit = match (sync.external, sync.external_playing) {
            (true, true) => widgets::Lit::On,
            (true, false) => widgets::Lit::Half,
            _ => widgets::Lit::Off,
        };
        if widgets::toggle_lit(ui, "EXT", lit, accent)
            .on_hover_text(tr("Sync the master clock to an external MIDI clock"))
            .clicked()
        {
            self.outbox
                .push(Command::Sync(SyncCommand::SetExternal(!sync.external)));
        }
    }

    fn snap_quantize(&mut self, ui: &mut Ui, t: &theme::Theme) {
        if widgets::toggle(ui, "SNAP", self.settings.snap, t.accent)
            .on_hover_text(tr("Cue, hotcue and loop points snap to beats"))
            .clicked()
        {
            self.settings.snap = !self.settings.snap;
            self.send_deck_settings();
        }
        if widgets::toggle(ui, "QUANT", self.settings.quantize, t.accent)
            .on_hover_text(tr("Jumps keep the beat phase"))
            .clicked()
        {
            self.settings.quantize = !self.settings.quantize;
            self.send_deck_settings();
        }
    }

    fn tempo_section(&mut self, ui: &mut Ui, state: &EngineState, t: &theme::Theme) {
        let sync = state.sync;
        let clock = sync.master.is_none();
        let (rect, _) =
            ui.allocate_exact_size(vec2(GLOBAL_TEMPO_WIDTH, GLOBAL_TOP), egui::Sense::hover());
        if clock && !sync.external {
            let drag = ui.scope(|ui| {
                let style = ui.style_mut();
                style
                    .text_styles
                    .insert(TextStyle::Monospace, FontId::monospace(GLOBAL_TEMPO_SIZE));
                style.drag_value_text_style = TextStyle::Monospace;
                ui.put(
                    rect,
                    DragValue::new(&mut self.clock_bpm)
                        .range(MIN_CLOCK_BPM..=MAX_CLOCK_BPM)
                        .speed(0.05)
                        .fixed_decimals(2),
                )
            });
            if drag.inner.changed() {
                self.outbox
                    .push(Command::Sync(SyncCommand::SetClockBpm(self.clock_bpm)));
            }
        } else {
            let leader = match sync.master {
                Some(deck) => format!("Tempo master: deck {}", backspn_control::deck_letter(deck)),
                None if sync.external => "Tempo master: external MIDI clock".to_owned(),
                None => "Tempo master: internal clock".to_owned(),
            };
            ui.put(
                rect,
                egui::Label::new(
                    RichText::new(format!("{:.2}", sync.clock_bpm))
                        .monospace()
                        .size(GLOBAL_TEMPO_SIZE)
                        .color(t.text),
                ),
            )
            .on_hover_text(leader);
        }
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            if widgets::toggle(ui, "AUTO", self.settings.auto_master, t.accent)
                .on_hover_text(tr(
                    "Auto Master Mode: when the tempo master stops, a playing deck takes over",
                ))
                .clicked()
                && !self.link.enabled()
            {
                self.settings.auto_master = !self.settings.auto_master;
                self.send_deck_settings();
            }
            let master = ui
                .add_enabled_ui(!self.settings.auto_master, |ui| {
                    widgets::toggle(ui, "MASTER", clock, t.accent)
                        .on_hover_text(tr("Make the internal clock the tempo master"))
                })
                .inner;
            if master.clicked() && !clock {
                self.outbox
                    .push(Command::Sync(SyncCommand::SetMaster(None)));
            }
            if widgets::toggle(ui, "TICK", state.master_tick, t.accent)
                .on_hover_text(tr("A click on each clock beat, in the headphones"))
                .clicked()
            {
                self.outbox.push(Command::MasterTick(!state.master_tick));
            }
            let tap = ui
                .add_enabled_ui(!sync.external, |ui| {
                    widgets::toggle(ui, "TAP", self.tap.engaged(), t.accent)
                        .on_hover_text(tr("Tap the clock tempo"))
                })
                .inner;
            if tap.clicked() {
                self.master_tap();
            }
        });
    }

    fn midi_clock_controls(&mut self, ui: &mut Ui, running: bool, accent: egui::Color32) {
        let lit = match (self.midi_clock.send, running) {
            (true, true) => widgets::Lit::On,
            (true, false) => widgets::Lit::Half,
            _ => widgets::Lit::Off,
        };
        let hover = self.midi_clock.error.as_deref().map_or_else(
            || "MIDI clock: Start on the next beat, or Stop".to_owned(),
            |e| format!("MIDI clock output: {e}"),
        );
        if widgets::toggle_lit(ui, "START", lit, accent)
            .on_hover_text(hover)
            .clicked()
        {
            let on = !self.midi_clock.send;
            self.midi_clock.set_send(on, &mut self.outbox);
        }
        if widgets::toggle(ui, "SYNC", false, accent)
            .on_hover_text(tr(
                "Stop the MIDI clock devices and start them again on the next beat",
            ))
            .clicked()
        {
            midi_clock::MidiClock::resync(&mut self.outbox);
        }
        let ms = &mut self.settings.midi_clock_offset_ms;
        if ui
            .add(egui::DragValue::new(ms).range(-1000..=1000).suffix(" ms"))
            .on_hover_text(tr("MIDI clock offset: positive sends earlier"))
            .changed()
        {
            midi_clock::MidiClock::set_offset_ms(*ms, &mut self.outbox);
        }
    }

    pub(crate) fn master_tap(&mut self) {
        if let Some(bpm) = self.tap.tap(self.epoch.elapsed().as_secs_f64()) {
            self.clock_bpm = bpm;
            self.outbox
                .push(Command::Sync(SyncCommand::SetClockBpm(bpm)));
        }
    }

    pub(crate) fn status_bar(&mut self, ui: &mut Ui) {
        let t = theme::get(ui.ctx());
        ui.horizontal(|ui| match &mut self.engine {
            Ok(engine) => {
                let info = engine.info().clone();
                let stats = engine.controller().stats();
                let dim = |text: String| RichText::new(text).size(11.0).color(t.text_dim);
                let (dot, health) = if engine.lost() {
                    (t.danger, "device unplugged, waiting for it")
                } else if stats.xruns == 0 && stats.overruns == 0 {
                    (t.play, "audio ok")
                } else {
                    (t.danger, "dropouts")
                };
                ui.label(RichText::new("●").color(dot).size(10.0));
                ui.label(dim(format!("{health} · {}", info.device)));
                ui.label(dim(format!(
                    "{} Hz · {} · {:.1} ms · load {:.1} %",
                    info.sample_rate,
                    info.buffer_frames
                        .map_or_else(|| "?".into(), |f| format!("{f} samples")),
                    stats.output_latency.as_secs_f64() * 1e3,
                    stats.load_permille as f64 / 10.0,
                )));
                ui.label(dim(format!(
                    "xruns {} · overruns {}",
                    stats.xruns, stats.overruns
                )));
                ui.label(dim(info.monitor.map_or_else(
                    || "phones: none (2-channel device)".to_owned(),
                    |(l, r)| format!("phones: outputs {}/{}", l + 1, r + 1),
                )));
            }
            Err(e) => {
                let wait = if self.audio_missing {
                    " (it starts when the device is plugged in)"
                } else {
                    ""
                };
                ui.colored_label(t.danger, format!("Audio unavailable: {e}{wait}"));
            }
        });
    }

    pub(crate) fn status_row(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            self.status_bar(ui);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.browser.progress(ui);
                self.ai_set_status(ui);
            });
        });
    }

    fn ai_set_status(&mut self, ui: &mut Ui) {
        let Some((at, of, paused)) = self.remote.progress() else {
            return;
        };
        if ui.small_button(tr("Stop")).clicked() {
            self.remote.stop();
        }
        let text = if paused {
            format!("{} {at}/{of} · {}", tr("AI set"), tr("paused"))
        } else {
            format!("{} {at}/{of}", tr("AI set"))
        };
        ui.label(RichText::new(text).size(11.0));
    }

    fn middle_width(&self) -> f32 {
        GLOBAL_BLOCK.max(self.mixer.natural_width())
    }

    pub(crate) fn key_view(&self, state: &EngineState) -> key_view::KeyView {
        let reference = key_view::reference_deck(
            state.sync.master,
            self.focus.deck,
            self.settings.key_reference_focus,
        )
        .filter(|&d| state.decks.get(d).is_some_and(|s| s.keylock))
        .and_then(|d| self.decks[d].key_text())
        .and_then(backspn_formats::nml::MusicalKey::parse);
        key_view::KeyView {
            notation: backspn_formats::nml::KeyNotation::from_value(self.settings.key_notation),
            highlight: self.settings.key_highlight,
            reference,
        }
    }

    pub(crate) fn suggestion_master(
        &mut self,
        state: &EngineState,
    ) -> Option<suggest_panel::Master> {
        let deck = state.sync.master?;
        Some(suggest_panel::Master {
            deck,
            track_id: self.decks[deck].track_id()?,
            bpm: state.decks[deck].effective_bpm(),
        })
    }

    pub(crate) fn decks_and_mixer(&mut self, ui: &mut Ui, state: &EngineState) {
        let mixer_width = self.middle_width();
        let deck_width = side_width(ui, mixer_width, 300.0);
        let rows = self.settings.deck_count / 2;
        let row_width = visible_width(ui);
        let t = theme::get(ui.ctx());
        for row in 0..rows {
            for deck in [2 * row, 2 * row + 1] {
                if let Some(h) = self.decks[deck].size().parallel_wave() {
                    ui.allocate_ui(vec2(row_width, h), |ui| {
                        let wave = theme::card(&t).inner_margin(0).show(ui, |ui| {
                            self.decks[deck].zoom_view(ui, &state.decks[deck], h, &mut self.outbox);
                        });
                        self.decks[deck].wave_drop(ui, &wave.response);
                    });
                    seam(ui);
                }
            }
        }
        let height = ui.available_height();
        let row_heights: Vec<f32> = (0..rows)
            .map(|row| {
                let decks = [2 * row, 2 * row + 1].map(|d| self.decks[d].min_height());
                let crossfader = row + 1 == rows && self.settings.show_crossfader;
                self.mixer.row_height(crossfader, decks[0].max(decks[1]))
            })
            .collect();
        ui.horizontal_top(|ui| {
            for column in 0..3 {
                if column > 0 {
                    seam(ui);
                }
                let width = match column {
                    0 => deck_width,
                    1 => mixer_width,
                    _ => visible_width(ui).max(deck_width),
                };
                ui.allocate_ui_with_layout(
                    vec2(width, height),
                    Layout::top_down(Align::Min),
                    |ui| {
                        if column == 1 {
                            self.mixer.fx_units = self.settings.fx_units;
                            let mut actions = Vec::new();
                            for row in 0..rows {
                                if row > 0 {
                                    seam(ui);
                                }
                                self.mixer.ui(
                                    ui,
                                    &[2 * row, 2 * row + 1],
                                    (
                                        row + 1 == rows && self.settings.show_crossfader,
                                        row_heights[row],
                                    ),
                                    state,
                                    &mut self.outbox,
                                    &mut actions,
                                );
                            }
                            for (action, trigger) in actions {
                                self.execute(action, trigger, state);
                            }
                            return;
                        }
                        for row in 0..rows {
                            let deck = row * 2 + usize::from(column == 2);
                            if row > 0 {
                                seam(ui);
                            }
                            self.decks[deck].compact = rows > 1;
                            self.decks[deck].height = row_heights[row];
                            self.decks[deck].sample_state = state.sample_deck[deck];
                            self.decks[deck].ui(
                                ui,
                                &state.decks[deck],
                                &state.sync,
                                &mut self.outbox,
                            );
                        }
                    },
                );
            }
        });
        self.share_zoom();
    }

    fn share_zoom(&mut self) {
        let current = self.settings.wave_zoom_s;
        if let Some(zoom) = self
            .decks
            .iter()
            .map(crate::DeckView::zoom_s)
            .find(|z| (z - current).abs() > 1e-9)
        {
            self.settings.wave_zoom_s = zoom;
            for deck in &mut self.decks {
                deck.set_zoom(zoom);
            }
        }
    }
}
