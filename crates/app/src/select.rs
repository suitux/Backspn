//! The app's drop-down select. Every select goes through here so they all
//! share the theme's look: the shown text carries no colour of its own, so it
//! follows the widget state (including the inverted hover).

use eframe::egui::{
    AsIdSalt, ComboBox, InnerResponse, PopupCloseBehavior, Response, RichText, Ui, WidgetText,
};

/// Text size of the small selects on decks, FX units and recorders.
pub const COMPACT_TEXT: f32 = 12.0;

#[must_use]
pub struct Select<'a> {
    combo: ComboBox<'a>,
    text: String,
    size: Option<f32>,
}

/// A select showing `text` (already translated if it needs to be).
pub fn select<'a>(id_salt: impl AsIdSalt, text: impl Into<String>) -> Select<'a> {
    Select {
        combo: ComboBox::from_id_salt(id_salt),
        text: text.into(),
        size: None,
    }
}

impl Select<'_> {
    pub fn width(mut self, width: f32) -> Self {
        self.combo = self.combo.width(width);
        self
    }

    /// Maximum height of the open list.
    pub fn height(mut self, height: f32) -> Self {
        self.combo = self.combo.height(height);
        self
    }

    /// Cuts the shown text to the width instead of growing past it.
    pub fn truncate(mut self) -> Self {
        self.combo = self.combo.truncate();
        self
    }

    /// Keeps the list open on clicks inside it, e.g. into a search field.
    pub fn close_on_click_outside(mut self) -> Self {
        self.combo = self
            .combo
            .close_behavior(PopupCloseBehavior::CloseOnClickOutside);
        self
    }

    pub fn text_size(mut self, size: f32) -> Self {
        self.size = Some(size);
        self
    }

    pub fn compact(self) -> Self {
        self.text_size(COMPACT_TEXT)
    }

    /// Opens the list with its own contents.
    pub fn show<R>(self, ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<Option<R>> {
        let text = shown(self.text, self.size);
        self.combo.selected_text(text).show_ui(ui, add)
    }

    /// Opens a list of `(value, label)` options that sets `value`. The
    /// response is marked changed when a different option is picked.
    pub fn options<T: PartialEq, L: Into<WidgetText>>(
        self,
        ui: &mut Ui,
        value: &mut T,
        items: impl IntoIterator<Item = (T, L)>,
    ) -> Response {
        let mut changed = false;
        let mut response = self
            .show(ui, |ui| {
                for (option, label) in items {
                    changed |= ui.selectable_value(value, option, label).changed();
                }
            })
            .response;
        if changed {
            response.mark_changed();
        }
        response
    }
}

fn shown(text: String, size: Option<f32>) -> RichText {
    let text = RichText::new(text);
    match size {
        Some(size) => text.size(size),
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme;
    use eframe::egui::{self, Color32, Pos2, Rect, Shape, epaint::ClippedShape, vec2};

    fn text_colour(shapes: &[ClippedShape], text: &str) -> Option<Color32> {
        shapes.iter().find_map(|s| match &s.shape {
            Shape::Text(t) if t.galley.text() == text => {
                let section = t.galley.job.sections.first()?.format.color;
                Some(if section == Color32::PLACEHOLDER {
                    t.override_text_color.unwrap_or(t.fallback_color)
                } else {
                    section
                })
            }
            _ => None,
        })
    }

    fn frame(ctx: &egui::Context, pointer: Option<Pos2>) -> (Rect, Vec<ClippedShape>) {
        let mut input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0))),
            ..Default::default()
        };
        if let Some(p) = pointer {
            input.events.push(egui::Event::PointerMoved(p));
        }
        let mut rect = Rect::NOTHING;
        let mut output = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                rect = select("hover-test", "Ride")
                    .compact()
                    .show(ui, |_| {})
                    .response
                    .rect;
            });
        });
        output.textures_delta.clear();
        (rect, output.shapes)
    }

    #[test]
    fn a_hovered_select_shows_its_text_in_the_inverted_colour() {
        for id in theme::ThemeId::ALL {
            let t = id.theme();
            let ctx = egui::Context::default();
            theme::apply(&ctx, t);
            frame(&ctx, None);
            let (rect, _) = frame(&ctx, None);
            frame(&ctx, Some(rect.center()));
            let (_, shapes) = frame(&ctx, Some(rect.center()));
            assert_eq!(
                text_colour(&shapes, "Ride"),
                Some(t.invert_fg),
                "{}",
                id.name()
            );
        }
    }
}
