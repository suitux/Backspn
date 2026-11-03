use eframe::egui::{self, Response, Shape, Ui};

use super::Page;
use crate::i18n::tr;

#[derive(Debug, Clone, Default)]
struct Pass {
    words: Vec<String>,
    collected: Option<Vec<String>>,
    target: Option<String>,
}

fn pass_id() -> egui::Id {
    egui::Id::unique("prefs-search-pass")
}

fn with_pass<R>(ui: &Ui, f: impl FnOnce(&mut Pass) -> R) -> R {
    ui.ctx()
        .data_mut(|d| f(d.get_temp_mut_or_default::<Pass>(pass_id())))
}

pub(super) fn matches(text: &str, words: &[String]) -> bool {
    let text = text.to_lowercase();
    !words.is_empty() && words.iter().all(|w| text.contains(w.as_str()))
}

pub(super) fn words(query: &str) -> Vec<String> {
    query.split_whitespace().map(str::to_lowercase).collect()
}

fn setting(ui: &mut Ui, text: &str, add: impl FnOnce(&mut Ui) -> Response) -> Response {
    let behind = ui.painter().add(Shape::Noop);
    let response = add(ui);
    let (lit, chosen) = with_pass(ui, |p| {
        if let Some(seen) = &mut p.collected
            && !text.is_empty()
        {
            seen.push(text.to_owned());
        }
        let chosen = p.target.as_deref() == Some(text);
        if chosen {
            p.target = None;
        }
        (matches(text, &p.words), chosen)
    });
    if lit {
        let t = crate::theme::get(ui.ctx());
        ui.painter().set(
            behind,
            Shape::rect_filled(
                response.rect.expand(2.0),
                0.0,
                t.accent.gamma_multiply(0.35),
            ),
        );
    }
    if chosen {
        response.scroll_to_me(Some(egui::Align::Center));
    }
    response
}

pub(super) fn title(ui: &mut Ui, text: &str) -> Response {
    let text = tr(text);
    setting(ui, text, |ui| ui.label(text))
}

pub(super) fn heading(ui: &mut Ui, text: &str) -> Response {
    let text = tr(text);
    let t = crate::theme::get(ui.ctx());
    setting(ui, text, |ui| {
        crate::widgets::hud_label(ui, text, 11.0, t.text_faint)
    })
}

pub(super) fn check(ui: &mut Ui, value: &mut bool, text: &str) -> Response {
    let text = tr(text);
    setting(ui, text, |ui| crate::widgets::check(ui, value, text))
}

pub(super) fn radio<T: PartialEq>(ui: &mut Ui, current: &mut T, value: T, text: &str) -> Response {
    let text = tr(text);
    setting(ui, text, |ui| {
        crate::widgets::radio(ui, current, value, text)
    })
}

#[derive(Debug, Default)]
pub(super) struct Search {
    pub query: String,
    pub index: Option<Vec<(Page, String)>>,
}

impl Search {
    pub fn results(&self, native: Option<&'static str>) -> Vec<(Page, String)> {
        let words = words(&self.query);
        let pages = Page::shown(native)
            .filter(|p| matches(p.title_for(native), &words))
            .map(|p| (p, p.title_for(native).to_owned()));
        let titles = self
            .index
            .iter()
            .flatten()
            .filter(|(p, t)| matches(t, &words) && (native.is_some() || *p != Page::Native))
            .cloned();
        let mut out: Vec<(Page, String)> = Vec::new();
        for r in pages.chain(titles) {
            if !out.contains(&r) {
                out.push(r);
            }
        }
        out
    }

    pub fn begin(&self, ui: &Ui) {
        let words = words(&self.query);
        with_pass(ui, |p| p.words = words);
    }

    pub fn target(ui: &Ui, title: String) {
        with_pass(ui, |p| p.target = Some(title));
    }

    pub fn collect(ui: &mut Ui, draw: impl FnOnce(&mut Ui)) -> Vec<String> {
        with_pass(ui, |p| p.collected = Some(Vec::new()));
        draw(ui);
        with_pass(ui, |p| p.collected.take()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_word_must_appear() {
        let w = words("JOG  tens");
        assert!(matches("Jogwheel tension (S4 MK3)", &w));
        assert!(!matches("Jog spin sensitivity", &w));
        assert!(!matches("anything", &[]), "an empty query lights nothing");
    }

    #[test]
    fn results_list_pages_then_titles_once() {
        let s = Search {
            query: "mixer".into(),
            index: Some(vec![
                (Page::Mixer, "Crossfader curve".into()),
                (Page::DecksLayout, "Show mixer".into()),
                (Page::DecksLayout, "Show mixer".into()),
            ]),
        };
        assert_eq!(
            s.results(Some("Kontrol S4 MK3")),
            [
                (Page::Mixer, "Mixer".to_owned()),
                (Page::DecksLayout, "Show mixer".to_owned())
            ]
        );
    }

    #[test]
    fn the_hidden_controller_page_is_not_found() {
        let s = Search {
            query: "kontrol".into(),
            index: Some(vec![(Page::Native, "Kontrol jog mode".into())]),
        };
        assert_eq!(
            s.results(Some("Kontrol S4 MK3")),
            [
                (Page::Native, "Kontrol S4 MK3".to_owned()),
                (Page::Native, "Kontrol jog mode".to_owned())
            ]
        );
        assert_eq!(s.results(None), []);
    }
}
