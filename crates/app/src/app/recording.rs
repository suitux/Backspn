use eframe::egui::{self, RichText, Ui};

use crate::i18n::tr;
use crate::recorder::Options;
use crate::{App, theme, widgets};

impl App {
    fn recorder_options(&self) -> Options {
        Options {
            dir: self.settings.recordings_dir.clone(),
            prefix: self.settings.recording_prefix.clone(),
            split_mb: self.settings.recording_split_mb,
            format: self.settings.recording_format,
        }
    }

    pub(crate) fn toggle_recording(&mut self) {
        if self.recorder.recording() {
            self.finish_recording();
            return;
        }
        let Ok(engine) = &self.engine else {
            return;
        };
        let rate = engine.info().sample_rate;
        match self.recorder.start(rate, &self.recorder_options()) {
            Ok(tap) => {
                self.outbox.push(tap);
                self.count(|u| u.recordings += 1);
            }
            Err(e) => {
                crate::toast::error(format!("Cannot record: {e}"));
                self.recorder.set_error(format!("Cannot record: {e}"));
            }
        }
    }

    pub(crate) fn finish_recording(&mut self) {
        if self.recorder.recording() {
            let untap = self.recorder.stop();
            self.outbox.push(untap);
        }
        self.poll_recorder();
    }

    pub(crate) fn poll_recorder(&mut self) {
        let files = self.recorder.take_finished();
        if !files.is_empty() {
            self.browser.add_recordings(&files);
        }
    }

    pub(crate) fn set_recording_gain(&mut self, knob: f32) {
        self.settings.recording_gain = knob.clamp(0.0, 1.0);
        self.recorder.set_gain(self.settings.recording_gain);
    }

    pub(crate) fn load_last_recording(&mut self, deck: usize) {
        if let Some(path) = self.recorder.last.clone() {
            self.open_file(deck, path);
        }
    }

    pub(crate) fn recorder_bar(&mut self, ui: &mut Ui) {
        let t = theme::get(ui.ctx());
        let status = self.recorder.status();
        let recording = self.recorder.recording();
        if recording {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
        ui.add_enabled_ui(recording, |ui| {
            if widgets::toggle(ui, "CUT", false, t.accent)
                .on_hover_text(tr("Close this file and go on in a new one"))
                .clicked()
            {
                self.recorder.cut();
            }
        });
        let mut gain = self.settings.recording_gain;
        ui.spacing_mut().slider_width = 60.0;
        if ui
            .add(egui::Slider::new(&mut gain, 0.0..=1.0).show_value(false))
            .on_hover_text(format!(
                "Recording level: {:+.1} dB",
                20.0 * crate::recorder::gain(gain).log10()
            ))
            .changed()
        {
            self.set_recording_gain(gain);
        }
        let peak = status.peak[0].max(status.peak[1]);
        let level = if peak >= 1.0 {
            RichText::new("●").color(t.danger)
        } else if peak > 1e-4 {
            RichText::new("●").color(t.accent)
        } else {
            RichText::new("●").color(t.text_dim)
        };
        ui.label(level)
            .on_hover_text(tr("Recording peak (red: clipping)"));
        let secs = status.seconds as u64;
        let time = format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60);
        ui.label(RichText::new(time).monospace().color(if recording {
            t.text
        } else {
            t.text_dim
        }));
        let hint = status.file.as_ref().map_or_else(
            || "Record the mix".to_owned(),
            |f| format!("Recording to {}", f.display()),
        );
        if widgets::toggle(ui, "● REC", recording, t.danger)
            .on_hover_text(hint)
            .clicked()
        {
            self.toggle_recording();
        }
        if let Some(e) = status.error {
            ui.label(RichText::new(e).color(t.danger));
        }
    }
}
