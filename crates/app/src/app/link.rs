use backspn_engine::{Command, EngineState, MAX_CLOCK_BPM, MIN_CLOCK_BPM, SyncCommand};
use eframe::egui::{self, Color32, Ui, vec2};

use crate::{App, widgets};

const TEMPO_EPSILON: f64 = 1e-4;

pub(crate) fn label(peers: usize) -> String {
    match peers {
        0 => "LINK".into(),
        1 => "1 LINK".into(),
        n => format!("{n} LINKS"),
    }
}

pub(crate) fn peers_notice(peers: usize) -> String {
    if peers == 1 {
        "1 Link Peer".into()
    } else {
        format!("{peers} Link Peers")
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum TempoMove {
    ToClock(f64),
    ToSession(f64),
}

pub(crate) fn tempo_move(
    clock: f64,
    session: f64,
    last_session: f64,
    peers: usize,
) -> Option<TempoMove> {
    if (clock - session).abs() <= TEMPO_EPSILON {
        return None;
    }
    if (session - last_session).abs() > TEMPO_EPSILON && peers > 0 {
        return (MIN_CLOCK_BPM..=MAX_CLOCK_BPM)
            .contains(&session)
            .then_some(TempoMove::ToClock(session));
    }
    Some(TempoMove::ToSession(clock))
}

impl App {
    pub(crate) fn set_link(&mut self, on: bool) {
        self.link.enable(on);
        self.link_tempo = 0.0;
        self.outbox.push(Command::Sync(SyncCommand::SetLink(on)));
        if on {
            self.outbox
                .push(Command::Sync(SyncCommand::SetExternal(false)));
            self.outbox
                .push(Command::Sync(SyncCommand::SetMaster(None)));
            if self.settings.auto_master {
                self.settings.auto_master = false;
                self.send_deck_settings();
            }
        } else {
            self.outbox.push(Command::LinkTimeline(None));
        }
    }

    pub(super) fn tick_link(&mut self, ctx: &egui::Context, state: &EngineState) {
        if !self.link.enabled() {
            self.link_peers = 0;
            return;
        }
        let session = self.link.tempo();
        let peers = self.link.num_peers();
        if peers != self.link_peers {
            self.link_peers = peers;
            crate::toast::warning(peers_notice(peers));
        }
        match tempo_move(state.sync.clock_bpm, session, self.link_tempo, peers) {
            Some(TempoMove::ToClock(bpm)) => {
                self.clock_bpm = bpm;
                self.outbox
                    .push(Command::Sync(SyncCommand::SetClockBpm(bpm)));
            }
            Some(TempoMove::ToSession(bpm)) => {
                self.link.set_tempo(bpm, backspn_link::host_micros());
            }
            None => {}
        }
        self.link_tempo = self.link.tempo();
        self.outbox
            .push(Command::LinkTimeline(Some(self.link.capture().0)));
        ctx.request_repaint_after(std::time::Duration::from_millis(50));
    }

    pub(super) fn link_button(&mut self, ui: &mut Ui, accent: Color32) {
        let on = self.link.enabled();
        let peers = if on { self.link.num_peers() } else { 0 };
        if widgets::toggle(ui, &label(peers), on, accent)
            .on_hover_text(crate::i18n::tr(
                "Sync the master clock with Link: shares tempo and beat phase with other apps \
                 on the network. Shows the phase and the connected peers.",
            ))
            .clicked()
        {
            self.set_link(!on);
        }
        if on {
            let phase = self.link.phase_at_time(backspn_link::host_micros(), 4.0) / 4.0;
            let (rect, _) = ui.allocate_exact_size(
                vec2(ui.available_width().min(60.0), 4.0),
                egui::Sense::hover(),
            );
            let t = crate::theme::get(ui.ctx());
            ui.painter().rect_filled(rect, 1.0, t.well);
            let mut fill = rect;
            fill.set_width(rect.width() * phase as f32);
            ui.painter().rect_filled(fill, 1.0, accent);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_count_the_peers() {
        assert_eq!(label(0), "LINK");
        assert_eq!(label(1), "1 LINK");
        assert_eq!(label(3), "3 LINKS");
    }

    #[test]
    fn the_session_wins_only_when_it_moved_with_peers() {
        assert_eq!(tempo_move(120.0, 120.0, 0.0, 1), None);
        assert_eq!(
            tempo_move(120.0, 128.0, 0.0, 1),
            Some(TempoMove::ToClock(128.0))
        );
        assert_eq!(
            tempo_move(120.0, 128.0, 0.0, 0),
            Some(TempoMove::ToSession(120.0))
        );
        assert_eq!(
            tempo_move(124.0, 128.0, 128.0, 2),
            Some(TempoMove::ToSession(124.0))
        );
        assert_eq!(tempo_move(120.0, 500.0, 0.0, 1), None);
    }
}
