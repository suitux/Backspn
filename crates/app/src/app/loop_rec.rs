use backspn_control::{LoopRecorderPart, Trigger, loop_recorder::list_after};
use backspn_engine::loop_recorder::{
    Command as Lr, LoopCopy, LoopRecorderState, SIZES, Source, State,
};
use backspn_engine::{Command, EngineState, Track};
use backspn_library::clock::LocalTime;
use eframe::egui::{self, Ui, vec2};

use crate::i18n::tr;
use crate::sample_capture;
use crate::sample_view::LoopDrag;
use crate::{App, theme, widgets};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LoopCopyFor {
    Cell(usize, usize, usize),
    Save,
}

pub(super) fn copied_loop(copy: &LoopCopy) -> (Track, f64, String) {
    let samples = copy.frames.iter().flatten().copied().collect();
    (
        Track::from_stereo(copy.rate, samples),
        sample_capture::in_range(copy.bpm),
        sample_capture::name("Loop Recorder", LocalTime::now()),
    )
}

pub(super) fn command(
    part: LoopRecorderPart,
    trigger: Trigger,
    s: &LoopRecorderState,
    external: bool,
) -> Option<Lr> {
    let pressed = trigger == Trigger::Press;
    Some(match part {
        LoopRecorderPart::Record if pressed => {
            Lr::Record(!matches!(s.state, State::Recording | State::Overdubbing))
        }
        LoopRecorderPart::Play if pressed => {
            Lr::Play(!matches!(s.state, State::Playing | State::Overdubbing))
        }
        LoopRecorderPart::Delete if pressed => Lr::Delete,
        LoopRecorderPart::Undo if pressed => Lr::Undo,
        LoopRecorderPart::Size => Lr::Size(list_after(s.size, SIZES.len(), trigger)?),
        LoopRecorderPart::Source if !external => {
            Lr::Source(Source::ALL[list_after(s.source.index(), Source::ALL.len(), trigger)?])
        }
        LoopRecorderPart::DryWet => Lr::DryWet(match trigger {
            Trigger::Set(v) => v,
            Trigger::Delta(d) => s.dry_wet + d / 64.0,
            Trigger::Reset => 0.5,
            _ => return None,
        }),
        _ => return None,
    })
}

fn beats(index: usize) -> String {
    format!("{}", SIZES[index])
}

impl App {
    pub(crate) fn send_loop_recorder_settings(&mut self) {
        self.outbox.push(Command::LoopRecorder(Lr::Decay(
            self.settings.loop_recorder_decay / 100.0,
        )));
        self.outbox.push(Command::LoopRecorder(Lr::Latency(
            self.settings.loop_recorder_latency_ms,
        )));
    }

    pub(crate) fn request_loop_copy(&mut self, frames: usize, target: LoopCopyFor) {
        let copy = LoopCopy {
            frames: Vec::with_capacity(frames),
            ..LoopCopy::default()
        };
        self.outbox.push(Command::LoopRecorderCopy(Box::new(copy)));
        self.loop_capture = Some(target);
    }

    pub(crate) fn finish_loop_capture(&mut self, copy: &LoopCopy) {
        match self.loop_capture.take() {
            Some(LoopCopyFor::Cell(deck, slot, cell)) => {
                self.finish_cell_capture((deck, slot, cell), copy);
            }
            Some(LoopCopyFor::Save) => self.save_loop(copy),
            None => {}
        }
    }

    fn save_loop(&mut self, copy: &LoopCopy) {
        if copy.frames.is_empty() {
            crate::toast::error("The loop recorder's loop could not be saved");
            return;
        }
        let (track, bpm, name) = copied_loop(copy);
        self.save_loop_file(&track, bpm, &name, Some(0.0));
    }

    pub(crate) fn save_loop_from(&mut self, from: LoopDrag) {
        match from {
            LoopDrag::Recorder => {
                let s = self.engine_state().loop_recorder;
                if s.has_loop() {
                    self.request_loop_copy(s.frames, LoopCopyFor::Save);
                } else {
                    crate::toast::error("The loop recorder has nothing to save yet");
                }
            }
            LoopDrag::Deck(deck) => self.save_deck_loop(deck),
        }
    }

    fn save_deck_loop(&mut self, deck: usize) {
        let state = self.engine_state();
        let Some(deck_state) = state.decks.get(deck) else {
            return;
        };
        let Some((start, end)) = sample_capture::active_loop(deck_state) else {
            crate::toast::error("The deck has no active loop to save");
            return;
        };
        let Some((audio, name)) = self.decks[deck].audio() else {
            return;
        };
        let track = sample_capture::cut(&audio, start, end);
        let bpm = sample_capture::bpm(deck_state, start);
        let anchor = sample_capture::grid_anchor(deck_state.grid.as_ref(), start);
        let name = sample_capture::name(&name, LocalTime::now());
        self.save_loop_file(&track, bpm, &name, anchor);
    }

    fn save_loop_file(&mut self, track: &Track, bpm: f64, name: &str, grid_anchor_s: Option<f64>) {
        let path = match sample_capture::write_wav(&self.settings.samples_dir, name, track) {
            Ok(path) => path,
            Err(e) => {
                crate::toast::error(format!("Cannot write the loop: {e}"));
                return;
            }
        };
        if self
            .browser
            .add_sample(&path, bpm, track.duration_s(), grid_anchor_s)
            .is_some()
        {
            crate::toast::info(format!("Loop saved to Loops: {name}"));
        }
    }

    fn loop_recorder_press(&mut self, part: LoopRecorderPart, s: &LoopRecorderState) {
        if let Some(c) = command(part, Trigger::Press, s, self.settings.external_mixer.on) {
            self.outbox.push(Command::LoopRecorder(c));
        }
    }

    fn loop_recorder_buttons(&mut self, ui: &mut Ui, s: &LoopRecorderState, t: &theme::Theme) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let recording = matches!(s.state, State::Recording | State::Overdubbing);
            if widgets::toggle(ui, "REC", recording, t.danger)
                .on_hover_text(tr("Record a loop, then overdub on it"))
                .clicked()
            {
                self.loop_recorder_press(LoopRecorderPart::Record, s);
            }
            let playing = matches!(s.state, State::Playing | State::Overdubbing);
            if widgets::toggle(ui, "PLAY", playing, t.play)
                .on_hover_text(tr("Play the loop, in time with the master clock"))
                .clicked()
            {
                self.loop_recorder_press(LoopRecorderPart::Play, s);
            }
            let undo = if s.undo == 2 { "REDO" } else { "UNDO" };
            if widgets::toggle(ui, undo, s.undo != 0, t.accent)
                .on_hover_text(tr("Undo the last overdub, or restore it"))
                .clicked()
            {
                self.loop_recorder_press(LoopRecorderPart::Undo, s);
            }
            if widgets::toggle(ui, "DEL", false, t.accent)
                .on_hover_text(tr("Delete the loop"))
                .clicked()
            {
                self.loop_recorder_press(LoopRecorderPart::Delete, s);
            }
            let saving = self.loop_capture == Some(LoopCopyFor::Save);
            ui.add_enabled_ui(s.has_loop() && !saving, |ui| {
                if widgets::toggle(ui, "SAVE", false, t.accent)
                    .on_hover_text(tr("Save the loop to the Loops list of the browser"))
                    .clicked()
                {
                    self.request_loop_copy(s.frames, LoopCopyFor::Save);
                }
            });
        });
    }

    pub(super) fn loop_recorder_panel(&mut self, ui: &mut Ui, state: &EngineState) -> f32 {
        let s = state.loop_recorder;
        let t = theme::get(ui.ctx());
        theme::card(&t)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    widgets::caption(ui, "LOOP RECORDER");
                    let mut size = s.size;
                    ui.add_enabled_ui(s.state == State::Empty, |ui| {
                        egui::ComboBox::from_id_salt("loop-recorder-size")
                            .width(48.0)
                            .selected_text(egui::RichText::new(beats(size)).size(12.0))
                            .show_ui(ui, |ui| {
                                for i in 0..SIZES.len() {
                                    ui.selectable_value(&mut size, i, beats(i));
                                }
                            })
                    })
                    .response
                    .on_hover_text(tr("Loop length in beats (only while empty)"));
                    if size != s.size {
                        self.outbox.push(Command::LoopRecorder(Lr::Size(size)));
                    }
                    let mut source = s.source;
                    ui.add_enabled_ui(!self.settings.external_mixer.on, |ui| {
                        egui::ComboBox::from_id_salt("loop-recorder-source")
                            .width(56.0)
                            .selected_text(egui::RichText::new(source.label()).size(12.0))
                            .show_ui(ui, |ui| {
                                for value in Source::ALL {
                                    ui.selectable_value(&mut source, value, value.label());
                                }
                            })
                    })
                    .response
                    .on_hover_text(tr(
                        "What it records: master, cue, external input or microphone",
                    ));
                    if source != s.source {
                        self.outbox.push(Command::LoopRecorder(Lr::Source(source)));
                    }
                    let mut dry_wet = s.dry_wet;
                    if widgets::knob_compact(ui, &mut dry_wet, 0.0..=1.0, 0.5, "DRY/WET").changed()
                    {
                        self.outbox.push(Command::LoopRecorder(Lr::DryWet(dry_wet)));
                    }
                });
                self.loop_recorder_buttons(ui, &s, &t);
                let (rect, bar) = ui.allocate_exact_size(
                    vec2(ui.available_width(), 8.0),
                    egui::Sense::click_and_drag(),
                );
                bar.on_hover_text(tr(
                    "Drag onto a sample deck cell to copy the loop there, or onto Loops to save it",
                ))
                .dnd_set_drag_payload(crate::sample_view::LoopDrag::Recorder);
                let rect = rect.shrink2(vec2(0.0, 2.0));
                ui.painter().rect_filled(rect, 1.0, t.well);
                if s.state != State::Empty {
                    let mut fill = rect;
                    fill.set_width(rect.width() * s.position);
                    ui.painter().rect_filled(fill, 1.0, t.accent);
                }
                let natural = ui.min_rect().height();
                self.fx.natural_heights[0] = natural;
                widgets::pad_to_height(ui, self.fx.natural_heights[1]);
            })
            .response
            .rect
            .height()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_play_toggle_from_the_state() {
        let mut s = LoopRecorderState::default();
        assert_eq!(
            command(LoopRecorderPart::Record, Trigger::Press, &s, false),
            Some(Lr::Record(true))
        );
        s.state = State::Overdubbing;
        assert_eq!(
            command(LoopRecorderPart::Record, Trigger::Press, &s, false),
            Some(Lr::Record(false))
        );
        assert_eq!(
            command(LoopRecorderPart::Play, Trigger::Press, &s, false),
            Some(Lr::Play(false))
        );
        assert_eq!(
            command(LoopRecorderPart::Record, Trigger::Release, &s, false),
            None
        );
    }

    #[test]
    fn lists_cycle_and_dry_wet_follows_knobs_and_encoders() {
        let s = LoopRecorderState {
            size: 3,
            source: Source::Aux,
            dry_wet: 0.5,
            ..LoopRecorderState::default()
        };
        assert_eq!(
            command(LoopRecorderPart::Size, Trigger::Press, &s, false),
            Some(Lr::Size(0))
        );
        assert_eq!(
            command(LoopRecorderPart::Source, Trigger::Press, &s, false),
            Some(Lr::Source(Source::Main))
        );
        assert_eq!(
            command(LoopRecorderPart::DryWet, Trigger::Set(0.2), &s, false),
            Some(Lr::DryWet(0.2))
        );
        assert_eq!(
            command(LoopRecorderPart::DryWet, Trigger::Delta(32.0), &s, false),
            Some(Lr::DryWet(1.0))
        );
        assert_eq!(
            command(LoopRecorderPart::State, Trigger::Press, &s, false),
            None
        );
        assert_eq!(
            command(LoopRecorderPart::Source, Trigger::Press, &s, true),
            None
        );
    }
}
