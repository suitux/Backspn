use std::time::{Instant, SystemTime};

use backspn_engine::{DECKS, EffectKind, EngineState};
use eframe::egui::{self, Context, RichText};

use crate::App;
use crate::i18n::tr;
use crate::insights::{self, Report, Usage};
use crate::modal::{self, Ask};
use crate::theme::WindowExt;

const NOTICE_TITLE: &str = "Anonymous Usage Statistics";
const NOTICE_TEXT: &str = "Backspn can send an anonymous summary once a day so its developer \
     knows how it is used: version, system, audio settings, controller models, library size and \
     counts such as mixes made or effects used. It never sends track names, file paths, device \
     names or your IP address. You can see exactly what is sent, and switch it off, in \
     Preferences ▸ Global Settings.";

fn notice() -> Ask<bool> {
    Ask::new(NOTICE_TITLE, NOTICE_TEXT)
        .button("Send", true)
        .button("Don't Send", false)
}

fn ago(since: SystemTime) -> String {
    let hours = since.elapsed().map_or(0, |d| d.as_secs() / 3600);
    match hours {
        0 => tr("Last sent less than an hour ago").to_owned(),
        h if h < 48 => tr("Last sent {n} hours ago").replace("{n}", &h.to_string()),
        h => tr("Last sent {n} days ago").replace("{n}", &(h / 24).to_string()),
    }
}

impl App {
    pub(crate) fn insights_on(&self) -> bool {
        self.persist && insights::allowed() && self.settings.insights_enabled
    }

    pub(crate) fn count(&mut self, f: impl FnOnce(&mut Usage)) {
        if self.insights_on() {
            f(self.insights.usage());
        }
    }

    pub(crate) fn tick_insights(&mut self, state: &EngineState, dt: f64) {
        if !self.insights_on() {
            self.browser.usage = Usage::default();
            self.insights.forget();
            return;
        }
        let from_browser = std::mem::take(&mut self.browser.usage);
        self.insights.usage().add(&from_browser);
        self.observe(state);
        let playing = state.decks.iter().any(|d| d.playing);
        self.insights
            .elapsed(dt, playing, self.recorder.recording());
        let now = Instant::now();
        self.insights.tick(now);
        if self.settings.insights_notice_seen && self.insights.due(now) {
            self.send_insights();
        }
    }

    fn observe(&mut self, state: &EngineState) {
        let i = &mut self.insights;
        let (mut syncs, mut loops) = (0, 0);
        for d in 0..DECKS {
            syncs += u64::from(i.rising("sync", d, state.sync.synced[d]));
            loops += u64::from(i.rising("loop", d, state.decks[d].loop_active));
        }
        let mut effects = Vec::new();
        for (u, unit) in state.fx.units.iter().enumerate() {
            for (s, slot) in unit.slots.iter().enumerate() {
                let on = unit.on && slot.on && slot.kind != EffectKind::None;
                if i.rising("fx", u * unit.slots.len() + s, on) {
                    effects.push(format!("{:?}", slot.kind));
                }
            }
        }
        let u = i.usage();
        u.syncs += syncs;
        u.loops += loops;
        for effect in effects {
            u.fx.add(&effect);
        }
    }

    pub(crate) fn send_insights(&mut self) {
        if !self.insights.is_sending() {
            let report = self.insights_report();
            self.insights.send(&report);
        }
    }

    pub(crate) fn insights_report(&mut self) -> Report {
        if let Some(controls) = &self.controls {
            self.insights.saw_controllers(controls.models());
        }
        let s = &self.settings;
        let audio = self.engine.as_ref().ok().map(|e| {
            let info = e.info();
            insights::Audio {
                sample_rate: info.sample_rate,
                buffer_frames: info.buffer_frames,
                channels: info.channels,
                headphones: info.monitor.is_some(),
            }
        });
        let config = insights::Config {
            decks: s.deck_count,
            fx_units: s.fx_units,
            quantize: s.quantize,
            snap: s.snap,
            auto_gain: s.auto_gain_on(),
            external_mixer: s.external_mixer.on,
            link: self.link.enabled(),
            midi_clock: self.midi_clock.send,
        };
        let locale = match s.language {
            crate::i18n::Language::English => "en",
            crate::i18n::Language::Spanish => "es",
        };
        let library = self
            .browser
            .library()
            .and_then(|lib| lib.stats().ok())
            .map(insights::Library::from);
        Report {
            schema: insights::SCHEMA,
            id: self.insights.id().to_owned(),
            version: crate::support::version().to_owned(),
            os: insights::Os::current(),
            locale,
            audio,
            controllers: self.insights.controllers(),
            library,
            config,
            usage: self.insights.pending().clone(),
        }
    }

    pub(crate) fn insights_status(&self) -> (String, bool) {
        if insights::switched_off() {
            return (tr("Switched off by BACKSPN_INSIGHTS.").to_owned(), false);
        }
        if !insights::allowed() {
            return (
                tr("Development builds never send statistics.").to_owned(),
                false,
            );
        }
        let mut status = if self.insights.is_sending() {
            tr("Sending…").to_owned()
        } else if let Some(at) = self.insights.last_sent() {
            ago(at)
        } else {
            tr("Not sent yet").to_owned()
        };
        if let Some(e) = &self.insights.last_error {
            status = format!("{status} · {}: {e}", tr("Last try failed"));
        }
        (status, true)
    }

    pub(crate) fn preview_insights(&mut self) {
        let report = self.insights_report();
        self.insights_preview = serde_json::to_string_pretty(&report).ok();
    }

    pub(crate) fn insights_windows(&mut self, ctx: &Context) {
        if self.insights_on()
            && !self.settings.insights_notice_seen
            && self.wizard.is_none()
            && self.insights_ask.is_none()
            && self.crash_ask.is_none()
        {
            self.insights_ask = Some(notice());
        }
        let shown = self.insights_ask.is_some();
        match modal::show(ctx, &mut self.insights_ask) {
            Some(send) => {
                self.settings.insights_notice_seen = true;
                self.settings.insights_enabled = send;
            }
            None if shown && self.insights_ask.is_none() => {
                self.settings.insights_notice_seen = true;
            }
            None => {}
        }
        let mut open = self.insights_preview.is_some();
        if let Some(json) = &self.insights_preview {
            egui::Window::new(tr("What Is Sent"))
                .collapsible(false)
                .default_size([460.0, 520.0])
                .show_closable(ctx, &mut open, |ui| {
                    ui.label(
                        RichText::new(tr("Usage counts are what happened since the last report."))
                            .weak(),
                    );
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.label(RichText::new(json).monospace());
                        });
                });
        }
        if !open {
            self.insights_preview = None;
        }
    }

    pub(crate) fn save_insights(&mut self) {
        if self.insights_on() {
            self.insights.save();
        }
    }
}
