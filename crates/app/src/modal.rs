use eframe::egui::{self, RichText};

use crate::i18n::tr;
use crate::theme::{self, WindowExt};

#[derive(Debug, Clone)]
pub struct Ask<T> {
    title: String,
    text: String,
    buttons: Vec<(String, Option<T>)>,
}

impl<T> Ask<T> {
    pub fn new(title: &str, text: impl Into<String>) -> Self {
        Self {
            title: title.to_owned(),
            text: text.into(),
            buttons: Vec::new(),
        }
    }

    pub fn yes_no(title: &str, text: impl Into<String>, yes: T) -> Self {
        Self::new(title, text).cancel("No").button("Yes", yes)
    }

    #[must_use]
    pub fn button(mut self, label: &str, answer: T) -> Self {
        self.buttons.push((label.to_owned(), Some(answer)));
        self
    }

    #[must_use]
    pub fn cancel(mut self, label: &str) -> Self {
        self.buttons.push((label.to_owned(), None));
        self
    }

    fn answer(self, i: usize) -> Option<T> {
        self.buttons.into_iter().nth(i).and_then(|(_, a)| a)
    }
}

pub fn show<T>(ctx: &egui::Context, slot: &mut Option<Ask<T>>) -> Option<T> {
    let ask = slot.as_ref()?;
    let t = theme::get(ctx);
    let mut clicked = None;
    let mut open = true;
    egui::Window::new(tr(&ask.title).to_owned())
        .id(egui::Id::unique("backspn-modal"))
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .collapsible(false)
        .resizable(false)
        .show_closable(ctx, &mut open, |ui| {
            ui.set_min_width(280.0);
            ui.set_max_width(420.0);
            ui.label(tr(&ask.text));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let first = ask.buttons.iter().position(|(_, a)| a.is_some());
                for (i, (label, _)) in ask.buttons.iter().enumerate() {
                    let text = RichText::new(tr(label));
                    let text = if Some(i) == first {
                        text.color(t.accent_ink)
                    } else {
                        text
                    };
                    if ui.button(text).clicked() {
                        clicked = Some(i);
                    }
                }
            });
        });
    if let Some(i) = clicked {
        return slot.take()?.answer(i);
    }
    if !open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        *slot = None;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_button_gives_its_answer() {
        let ask = || {
            Ask::new("Import", "Which tags?")
                .button("Collection", 1)
                .button("File", 2)
                .cancel("Cancel")
        };
        assert_eq!(ask().answer(0), Some(1));
        assert_eq!(ask().answer(1), Some(2));
        assert_eq!(ask().answer(2), None);
        assert_eq!(Ask::yes_no("Delete", "Sure?", ()).answer(0), None);
        assert_eq!(Ask::yes_no("Delete", "Sure?", ()).answer(1), Some(()));
    }

    fn frame(
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        slot: &mut Option<Ask<()>>,
    ) -> Option<()> {
        let mut answer = None;
        let input = egui::RawInput {
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| answer = show(ui.ctx(), slot));
        output.textures_delta.clear();
        answer
    }

    #[test]
    fn escape_closes_the_question_without_an_answer() {
        let ctx = egui::Context::default();
        let mut slot = None;
        assert_eq!(frame(&ctx, vec![], &mut slot), None);
        slot = Some(Ask::yes_no("Delete", "Sure?", ()));
        assert_eq!(frame(&ctx, vec![], &mut slot), None);
        assert!(slot.is_some(), "it stays until answered");
        let escape = egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        assert_eq!(frame(&ctx, vec![escape], &mut slot), None);
        assert!(slot.is_none());
    }
}
