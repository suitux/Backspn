use backspn_engine::EngineState;
use eframe::egui::{Context, ViewportCommand};

use crate::App;
use crate::i18n::tr;
use crate::modal::{self, Ask};

const PLAYING: &str = "Caution! Decks are playing.";
const RUNNING: &str = "These tasks are still running:";
const STOPS: &str = "Quitting stops them.";
const QUIT: &str = "Quit?";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Quit {
    #[default]
    Free,
    Asking,
    Confirmed,
}

fn question(state: &EngineState, tasks: &[&str]) -> Option<String> {
    let playing = state.decks.iter().any(|d| d.playing);
    if !playing && tasks.is_empty() {
        return None;
    }
    let mut text = String::new();
    if playing {
        text.push_str(tr(PLAYING));
        text.push_str("\n\n");
    }
    if !tasks.is_empty() {
        text.push_str(tr(RUNNING));
        for task in tasks {
            text.push_str("\n• ");
            text.push_str(tr(task));
        }
        text.push_str("\n\n");
        text.push_str(tr(STOPS));
        text.push_str("\n\n");
    }
    text.push_str(tr(QUIT));
    Some(text)
}

impl App {
    fn running_tasks(&self) -> Vec<&'static str> {
        let mut tasks = self.browser.running_tasks();
        if self.recorder.recording() {
            tasks.push("Recording the mix");
        }
        tasks
    }

    pub(crate) fn guard_quit(&mut self, ctx: &Context, state: &EngineState) {
        if self.quit != Quit::Confirmed
            && ctx.input(|i| i.viewport().close_requested())
            && let Some(text) = question(state, &self.running_tasks())
        {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            if self.quit != Quit::Asking {
                self.quit = Quit::Asking;
                self.quit_ask = Some(Ask::yes_no("Quit", text, ()));
            }
        }
        if self.quit != Quit::Asking {
            return;
        }
        if modal::show(ctx, &mut self.quit_ask).is_some() {
            self.quit = Quit::Confirmed;
            ctx.send_viewport_cmd(ViewportCommand::Close);
        } else if self.quit_ask.is_none() {
            self.quit = Quit::Free;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_playing_deck_asks() {
        let mut state = EngineState::default();
        assert_eq!(question(&state, &[]), None);
        state.preview.playing = true;
        assert_eq!(
            question(&state, &[]),
            None,
            "the preview player does not count"
        );
        state.decks[3].playing = true;
        assert_eq!(
            question(&state, &[]).as_deref(),
            Some("Caution! Decks are playing.\n\nQuit?")
        );
    }

    #[test]
    fn running_tasks_ask_and_are_listed() {
        let mut state = EngineState::default();
        let tasks = ["Analyzing tracks", "Writing tags to files"];
        assert_eq!(
            question(&state, &tasks).as_deref(),
            Some(
                "These tasks are still running:\n• Analyzing tracks\n• Writing tags to files\
                 \n\nQuitting stops them.\n\nQuit?"
            )
        );
        state.decks[0].playing = true;
        let text = question(&state, &tasks[..1]).unwrap_or_default();
        assert!(
            text.starts_with("Caution! Decks are playing.\n\nThese tasks"),
            "{text}"
        );
    }
}
