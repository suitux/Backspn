use std::sync::Mutex;
use std::time::{Duration, Instant};

use eframe::egui::{self, RichText};

use crate::i18n::tr;
use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Info,
    Warning,
    Error,
}

pub const DEFAULT_SECONDS: [f32; 3] = [5.0, 6.0, 8.0];

#[derive(Debug, Clone, PartialEq)]
struct Toast {
    id: u64,
    kind: Kind,
    text: String,
    born: Instant,
}

#[derive(Debug)]
struct Toasts {
    items: Vec<Toast>,
    next: u64,
    seconds: [f32; 3],
}

const MAX: usize = 4;

impl Toasts {
    fn push(&mut self, kind: Kind, text: String, now: Instant) {
        self.items.retain(|t| t.text != text);
        self.next += 1;
        self.items.push(Toast {
            id: self.next,
            kind,
            text,
            born: now,
        });
        if self.items.len() > MAX {
            self.items.remove(0);
        }
    }

    fn expire(&mut self, now: Instant) {
        let seconds = self.seconds;
        self.items
            .retain(|t| now.duration_since(t.born) < lifespan(seconds, t.kind));
    }

    fn next_expiry(&self, now: Instant) -> Option<Duration> {
        self.items
            .iter()
            .map(|t| (t.born + lifespan(self.seconds, t.kind)).saturating_duration_since(now))
            .min()
    }
}

impl Default for Toasts {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            next: 0,
            seconds: DEFAULT_SECONDS,
        }
    }
}

fn lifespan(seconds: [f32; 3], kind: Kind) -> Duration {
    Duration::from_secs_f32(seconds[kind as usize].max(0.0))
}

static TOASTS: Mutex<Toasts> = Mutex::new(Toasts {
    items: Vec::new(),
    next: 0,
    seconds: DEFAULT_SECONDS,
});

pub fn set_seconds(seconds: [f32; 3]) {
    if let Ok(mut toasts) = TOASTS.lock() {
        toasts.seconds = seconds;
    }
}

fn post(kind: Kind, text: String) {
    if let Ok(mut toasts) = TOASTS.lock() {
        toasts.push(kind, text, Instant::now());
    }
}

pub fn info(text: impl Into<String>) {
    post(Kind::Info, text.into());
}

pub fn warning(text: impl Into<String>) {
    let text = text.into();
    log::warn!("{text}");
    post(Kind::Warning, text);
}

pub fn error(text: impl Into<String>) {
    let text = text.into();
    log::error!("{text}");
    post(Kind::Error, text);
}

#[cfg(test)]
pub fn showing(kind: Kind, test: impl Fn(&str) -> bool) -> bool {
    TOASTS
        .lock()
        .is_ok_and(|t| t.items.iter().any(|t| t.kind == kind && test(&t.text)))
}

const WIDTH: f32 = 340.0;
const SLIDE_S: f32 = 0.22;
const MOVE_S: f32 = 0.18;
const GAP: f32 = 6.0;

fn slide(age_s: f32) -> f32 {
    let p = (age_s / SLIDE_S).clamp(0.0, 1.0);
    (1.0 - p).powi(3)
}

pub fn show(ctx: &egui::Context, bottom: f32) {
    let Ok(mut toasts) = TOASTS.lock() else {
        return;
    };
    let now = Instant::now();
    toasts.expire(now);
    if toasts.items.is_empty() {
        return;
    }
    let t = theme::get(ctx);
    let screen = ctx.content_rect();
    let mut floor = screen.bottom() - bottom - 8.0;
    let mut closed = None;
    let mut sliding = false;
    for toast in toasts.items.iter().rev() {
        let id = egui::Id::unique("backspn-toast").with(toast.id);
        let height = ctx.memory(|m| m.area_rect(id)).map_or(52.0, |r| r.height());
        let top = ctx.animate_value_with_time(id.with("top"), floor - height, MOVE_S);
        floor -= height + GAP;
        let off = slide(now.duration_since(toast.born).as_secs_f32());
        sliding |= off > 0.0;
        let x = screen.right() - 12.0 - WIDTH + off * (WIDTH + 24.0);
        let (accent, title) = match toast.kind {
            Kind::Info => (t.accent, None),
            Kind::Warning => (t.cue, Some(tr("Warning"))),
            Kind::Error => (t.danger, Some(tr("Error"))),
        };
        egui::Area::new(id)
            .order(egui::Order::Foreground)
            .constrain(false)
            .fixed_pos(egui::pos2(x, top))
            .show(ctx, |ui| {
                let frame = egui::Frame::popup(ui.style())
                    .fill(t.surface)
                    .stroke(egui::Stroke::new(1.0, accent))
                    .inner_margin(egui::Margin {
                        left: 12,
                        right: 24,
                        top: 8,
                        bottom: 8,
                    });
                let rect = frame
                    .show(ui, |ui| {
                        ui.set_width(WIDTH - 36.0);
                        if let Some(title) = title {
                            ui.label(RichText::new(title).strong().color(accent));
                        }
                        ui.label(tr(&toast.text));
                    })
                    .response
                    .rect;
                let corner = egui::Rect::from_min_size(
                    egui::pos2(rect.right() - 18.0, rect.top() + 4.0),
                    egui::vec2(14.0, 14.0),
                );
                let close =
                    egui::Button::new(RichText::new("×").size(12.0).color(t.text_dim)).frame(false);
                if ui.put(corner, close).on_hover_text(tr("Close")).clicked() {
                    closed = Some(toast.id);
                }
            });
    }
    if let Some(id) = closed {
        toasts.items.retain(|t| t.id != id);
    }
    if sliding {
        ctx.request_repaint();
    } else if let Some(wait) = toasts.next_expiry(now) {
        ctx.request_repaint_after(wait);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_toast_slides_in_from_the_right() {
        assert_eq!(slide(0.0), 1.0);
        assert!(slide(0.05) > 0.3 && slide(0.05) < 1.0);
        assert!(slide(0.15) < slide(0.05));
        assert_eq!(slide(SLIDE_S), 0.0);
        assert_eq!(slide(5.0), 0.0);
    }

    #[test]
    fn toasts_go_by_themselves_errors_later() {
        let t0 = Instant::now();
        let mut toasts = Toasts::default();
        toasts.push(Kind::Info, "Saved".into(), t0);
        toasts.push(Kind::Error, "Cannot save".into(), t0);
        assert_eq!(toasts.next_expiry(t0), Some(Duration::from_secs(5)));
        toasts.expire(t0 + Duration::from_secs(6));
        assert_eq!(toasts.items.len(), 1);
        assert_eq!(toasts.items[0].kind, Kind::Error);
        toasts.expire(t0 + Duration::from_secs(8));
        assert_eq!(toasts.items, []);
    }

    #[test]
    fn each_kind_stays_as_long_as_the_settings_say() {
        let t0 = Instant::now();
        let mut toasts = Toasts {
            seconds: [1.0, 2.0, 3.0],
            ..Toasts::default()
        };
        toasts.push(Kind::Info, "i".into(), t0);
        toasts.push(Kind::Warning, "w".into(), t0);
        toasts.push(Kind::Error, "e".into(), t0);
        toasts.expire(t0 + Duration::from_millis(1500));
        let kinds: Vec<Kind> = toasts.items.iter().map(|t| t.kind).collect();
        assert_eq!(kinds, [Kind::Warning, Kind::Error]);
        toasts.expire(t0 + Duration::from_millis(2500));
        assert_eq!(toasts.items.len(), 1);
    }

    #[test]
    fn the_same_text_starts_over_and_the_oldest_make_room() {
        let t0 = Instant::now();
        let later = t0 + Duration::from_secs(3);
        let mut toasts = Toasts::default();
        toasts.push(Kind::Info, "A".into(), t0);
        toasts.push(Kind::Info, "B".into(), t0);
        toasts.push(Kind::Info, "A".into(), later);
        let texts: Vec<&str> = toasts.items.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(texts, ["B", "A"]);
        assert_eq!(toasts.items[1].born, later);
        for text in ["C", "D", "E"] {
            toasts.push(Kind::Info, text.into(), later);
        }
        let texts: Vec<&str> = toasts.items.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(texts, ["A", "C", "D", "E"]);
    }
}
