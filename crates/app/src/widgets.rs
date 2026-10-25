use std::f32::consts::PI;
use std::ops::RangeInclusive;
use std::sync::Arc;

use eframe::egui::{
    Align2, Color32, CornerRadius, FontId, Galley, Pos2, Rect, Response, RichText, Sense, Shape,
    Stroke, StrokeKind, TextStyle, Ui, Vec2, pos2, text::LayoutJob, vec2,
};

use crate::theme::{self, Theme};

const KNOB_SIZE: f32 = 30.0;
pub const KNOB_WIDTH: f32 = KNOB_SIZE + 10.0;
const DRAG_PIXELS: f32 = 160.0;
const START_ANGLE: f32 = 0.75 * PI;
const SWEEP: f32 = 1.5 * PI;

pub const LABEL_TRACKING: f32 = 0.15;
pub const BUTTON_TRACKING: f32 = 0.12;
const BLOCK_PADDING: Vec2 = vec2(8.0, 4.0);

pub fn knob(
    ui: &mut Ui,
    value: &mut f32,
    range: RangeInclusive<f32>,
    default: f32,
    label: &str,
) -> Response {
    knob_with(ui, value, range, default, label, KnobLabel::Full)
}

pub fn knob_compact(
    ui: &mut Ui,
    value: &mut f32,
    range: RangeInclusive<f32>,
    default: f32,
    label: &str,
) -> Response {
    knob_with(ui, value, range, default, label, KnobLabel::Compact)
}

pub fn knob_bare(
    ui: &mut Ui,
    value: &mut f32,
    range: RangeInclusive<f32>,
    default: f32,
    label: &str,
) -> Response {
    knob_with(ui, value, range, default, label, KnobLabel::None)
}

#[derive(Clone, Copy, PartialEq)]
enum KnobLabel {
    Full,
    Compact,
    None,
}

impl KnobLabel {
    fn size(self) -> (f32, f32) {
        match self {
            Self::Full => (9.0, 14.0),
            Self::Compact => (7.5, 10.0),
            Self::None => (0.0, 2.0),
        }
    }
}

fn knob_with(
    ui: &mut Ui,
    value: &mut f32,
    range: RangeInclusive<f32>,
    default: f32,
    label: &str,
    style: KnobLabel,
) -> Response {
    let (min, max) = (*range.start(), *range.end());
    let (label_size, label_height) = style.size();
    let (rect, mut response) = ui.allocate_exact_size(
        vec2(KNOB_WIDTH, KNOB_SIZE + label_height),
        Sense::click_and_drag(),
    );
    let before = *value;
    if response.dragged() {
        let fine = if ui.input(|i| i.modifiers.shift) {
            0.2
        } else {
            1.0
        };
        *value -= response.drag_delta().y / DRAG_PIXELS * (max - min) * fine;
    }
    if response.double_clicked() {
        *value = default;
    }
    *value = value.clamp(min, max);
    if (*value - before).abs() > f32::EPSILON {
        response.mark_changed();
    }

    let t = theme::get(ui.ctx());
    let painter = ui.painter_at(rect.expand(2.0));
    let centre = pos2(rect.center().x, rect.top() + KNOB_SIZE / 2.0 + 1.0);
    let radius = KNOB_SIZE / 2.0 - 1.0;
    let frac = (*value - min) / (max - min);
    let zero = if min < 0.0 && max > 0.0 {
        -min / (max - min)
    } else {
        0.0
    };
    let angle = |f: f32| START_ANGLE + f * SWEEP;
    let active = response.hovered() || response.dragged();
    painter.add(Shape::line(
        arc(centre, radius, angle(0.0), angle(1.0)),
        Stroke::new(3.0, t.border),
    ));
    let killed = *value <= min && min >= 0.0 && default > min;
    let arc_color = if killed { t.danger } else { t.accent };
    if (frac - zero).abs() > 0.002 {
        painter.add(Shape::line(
            arc(centre, radius, angle(zero.min(frac)), angle(zero.max(frac))),
            Stroke::new(3.0, arc_color),
        ));
    }
    let body = radius - 4.5;
    painter.circle_filled(
        centre,
        body,
        if active { t.control_hover } else { t.control },
    );
    painter.circle_stroke(centre, body, Stroke::new(1.0, t.border));
    let dir = vec2(angle(frac).cos(), angle(frac).sin());
    painter.line_segment(
        [centre + dir * (body * 0.25), centre + dir * (body - 1.5)],
        Stroke::new(2.0, t.text),
    );
    if style != KnobLabel::None {
        let galley = painter.layout_job(hud_job(label, label_size, LABEL_TRACKING, t.text_faint));
        let size = galley.size();
        painter.galley(
            pos2(rect.center().x - size.x / 2.0, rect.bottom() - size.y),
            galley,
            t.text_faint,
        );
    }
    response.on_hover_text(format!("{label}: {value:.2} (double-click resets)"))
}

fn arc(centre: Pos2, radius: f32, from: f32, to: f32) -> Vec<Pos2> {
    let steps = (((to - from) / SWEEP) * 40.0).ceil().max(1.0) as usize;
    (0..=steps)
        .map(|i| {
            let a = from + (to - from) * i as f32 / steps as f32;
            centre + vec2(a.cos(), a.sin()) * radius
        })
        .collect()
}

pub fn fader(ui: &mut Ui, value: &mut f32, height: f32, default: f32) -> Response {
    let width = 26.0;
    let (rect, mut response) = ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag());
    let cap = vec2(width - 4.0, 14.0);
    let travel = height - cap.y;
    let before = *value;
    if response.dragged() {
        let fine = if ui.input(|i| i.modifiers.shift) {
            0.2
        } else {
            1.0
        };
        *value -= response.drag_delta().y / travel * fine;
    }
    if response.double_clicked() {
        *value = default;
    }
    *value = value.clamp(0.0, 1.0);
    if (*value - before).abs() > f32::EPSILON {
        response.mark_changed();
    }

    let t = theme::get(ui.ctx());
    let painter = ui.painter_at(rect.expand(1.0));
    let x = rect.center().x;
    let top = rect.top() + cap.y / 2.0;
    let bottom = rect.bottom() - cap.y / 2.0;
    for i in 0..=10 {
        let y = bottom - (bottom - top) * i as f32 / 10.0;
        let w = if i % 5 == 0 { 9.0 } else { 5.0 };
        painter.line_segment(
            [pos2(rect.left(), y), pos2(rect.left() + w, y)],
            Stroke::new(1.0, t.border),
        );
    }
    let track = Rect::from_center_size(pos2(x, rect.center().y), vec2(4.0, bottom - top));
    painter.rect_filled(track, 0.0, t.well);
    let y = bottom - (bottom - top) * *value;
    let filled = Rect::from_min_max(pos2(track.left(), y), track.right_bottom());
    painter.rect_filled(filled, 0.0, t.accent.gamma_multiply(0.7));
    let cap_rect = Rect::from_center_size(pos2(x, y), cap);
    let fill = if response.hovered() || response.dragged() {
        t.control_hover
    } else {
        t.control
    };
    painter.rect_filled(cap_rect, 0.0, fill);
    painter.rect_stroke(
        cap_rect,
        0.0,
        Stroke::new(1.0, t.border_strong),
        StrokeKind::Inside,
    );
    painter.line_segment(
        [
            pos2(cap_rect.left() + 3.0, y),
            pos2(cap_rect.right() - 3.0, y),
        ],
        Stroke::new(2.0, t.text),
    );
    response.on_hover_text(format!("{:.0} % (double-click resets)", *value * 100.0))
}

pub fn meter(ui: &mut Ui, levels: &[f32], height: f32) -> Response {
    const SEGMENTS: usize = 16;
    let bar_w = 5.0;
    let width = (bar_w + 2.0) * levels.len() as f32 - 2.0;
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let painter = ui.painter_at(rect);
    let t = theme::get(ui.ctx());
    let seg_h = height / SEGMENTS as f32;
    for (i, &level) in levels.iter().enumerate() {
        let db = 20.0 * level.max(1e-6).log10();
        let lit = (((db + 48.0) / 48.0).clamp(0.0, 1.0) * SEGMENTS as f32).ceil() as usize;
        let x0 = rect.left() + (bar_w + 2.0) * i as f32;
        for s in 0..SEGMENTS {
            let y1 = rect.bottom() - seg_h * s as f32;
            let seg = Rect::from_min_max(pos2(x0, y1 - seg_h + 1.0), pos2(x0 + bar_w, y1));
            let color = match s {
                s if s >= SEGMENTS - 1 => t.meter[2],
                s if s >= SEGMENTS - 4 => t.meter[1],
                _ => t.meter[0],
            };
            let fill = if s < lit {
                color
            } else {
                color.gamma_multiply(0.12)
            };
            painter.rect_filled(seg, 0.0, fill);
        }
    }
    response
}

pub fn hud_text(text: &str) -> String {
    text.to_uppercase()
}

pub fn hud_job(text: &str, size: f32, tracking: f32, color: Color32) -> LayoutJob {
    let mut job = LayoutJob::simple_singleline(hud_text(text), FontId::monospace(size), color);
    for section in &mut job.sections {
        section.format.extra_letter_spacing = size * tracking;
    }
    job
}

pub fn hud_label(ui: &mut Ui, text: &str, size: f32, color: Color32) -> Response {
    ui.label(hud_job(text, size, LABEL_TRACKING, color))
}

pub fn caption(ui: &mut Ui, text: &str) {
    let t = theme::get(ui.ctx());
    hud_label(ui, text, 10.0, t.text_faint);
}

pub fn check(ui: &mut Ui, value: &mut bool, text: impl Into<String>) -> Response {
    let text: String = text.into();
    let text = crate::i18n::tr(&text).to_owned();
    let t = theme::get(ui.ctx());
    ui.scope(|ui| {
        mark_in_text_colour(ui, &t);
        ui.checkbox(value, body(text, &t))
    })
    .inner
}

pub fn radio<T: PartialEq>(
    ui: &mut Ui,
    current: &mut T,
    value: T,
    text: impl Into<String>,
) -> Response {
    let text: String = text.into();
    let text = crate::i18n::tr(&text).to_owned();
    let t = theme::get(ui.ctx());
    ui.scope(|ui| {
        mark_in_text_colour(ui, &t);
        ui.radio_value(current, value, body(text, &t))
    })
    .inner
}

fn mark_in_text_colour(ui: &mut Ui, t: &Theme) {
    let w = &mut ui.visuals_mut().widgets;
    w.hovered.fg_stroke.color = t.text;
    w.active.fg_stroke.color = t.text;
}

fn body(text: impl Into<String>, t: &Theme) -> RichText {
    RichText::new(text)
        .text_style(TextStyle::Body)
        .color(t.text)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub fill: Color32,
    pub text: Color32,
    pub outline: Color32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hover {
    Invert,
    Fade,
}

pub fn hovered_look(look: Look, hover: Hover, pressed: bool, t: &Theme) -> Look {
    match hover {
        Hover::Invert => {
            let fill = if pressed { t.pressed() } else { t.invert };
            Look {
                fill,
                text: t.invert_fg,
                outline: fill,
            }
        }
        Hover::Fade => Look {
            fill: t.faded(look.fill, pressed),
            text: look.text,
            outline: t.faded(look.outline, pressed),
        },
    }
}

fn block(
    ui: &mut Ui,
    galley: Arc<Galley>,
    size: Vec2,
    look: Look,
    hover: Hover,
    sense: Sense,
) -> Response {
    let size = block_size(galley.size(), size);
    let (rect, response) = ui.allocate_exact_size(size, sense);
    let t = theme::get(ui.ctx());
    let look = if response.hovered() && response.enabled() {
        hovered_look(look, hover, response.is_pointer_button_down_on(), &t)
    } else {
        look
    };
    let painter = ui.painter();
    painter.rect(
        rect,
        CornerRadius::ZERO,
        look.fill,
        Stroke::new(1.0, look.outline),
        StrokeKind::Inside,
    );
    let pos = rect.center() - galley.size() / 2.0;
    painter.galley(pos, galley, look.text);
    response
}

pub fn close_button(ui: &mut Ui, rect: Rect) -> Response {
    let response = ui.interact(rect, ui.scope_id().with("close"), Sense::click());
    let t = theme::get(ui.ctx());
    let rest = Look {
        fill: t.control,
        text: t.text,
        outline: t.border,
    };
    let look = if response.hovered() {
        hovered_look(
            rest,
            Hover::Invert,
            response.is_pointer_button_down_on(),
            &t,
        )
    } else {
        rest
    };
    let painter = ui.painter();
    painter.rect(
        rect,
        CornerRadius::ZERO,
        look.fill,
        Stroke::new(1.0, look.outline),
        StrokeKind::Inside,
    );
    let cross = rect.shrink(rect.width() * 0.3);
    let stroke = Stroke::new(1.5, look.text);
    painter.line_segment([cross.left_top(), cross.right_bottom()], stroke);
    painter.line_segment([cross.right_top(), cross.left_bottom()], stroke);
    response
}

fn block_size(text_size: Vec2, wanted: Vec2) -> Vec2 {
    let natural = text_size + 2.0 * BLOCK_PADDING;
    vec2(
        if wanted.x > 0.0 { wanted.x } else { natural.x },
        natural.y.max(wanted.y),
    )
}

fn block_galley(ui: &Ui, text: &str, size: f32) -> Arc<Galley> {
    ui.painter()
        .layout_job(hud_job(text, size, BUTTON_TRACKING, Color32::PLACEHOLDER))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lit {
    Off,
    Half,
    On,
}

pub fn toggle(ui: &mut Ui, text: &str, on: bool, color: Color32) -> Response {
    toggle_lit(ui, text, if on { Lit::On } else { Lit::Off }, color)
}

pub fn toggle_lit(ui: &mut Ui, text: &str, lit: Lit, color: Color32) -> Response {
    let t = theme::get(ui.ctx());
    let (look, hover) = match lit {
        Lit::On => (
            Look {
                fill: color,
                text: Theme::on(color),
                outline: color,
            },
            Hover::Fade,
        ),
        Lit::Half => (
            Look {
                fill: t.control,
                text: color,
                outline: color,
            },
            Hover::Fade,
        ),
        Lit::Off => (
            Look {
                fill: t.control,
                text: t.text_dim,
                outline: t.border,
            },
            Hover::Invert,
        ),
    };
    let galley = block_galley(ui, text, 11.0);
    block(ui, galley, vec2(0.0, 22.0), look, hover, Sense::click())
}

pub fn hover_outline(ui: &Ui, response: &Response) {
    if response.hovered() && response.enabled() {
        let t = theme::get(ui.ctx());
        ui.painter().rect_stroke(
            response.rect,
            0.0,
            Stroke::new(1.0, t.invert),
            StrokeKind::Inside,
        );
    }
}

pub fn cell_fill(on: bool, hovered: bool, color: Color32, t: &Theme) -> Color32 {
    if on {
        color
    } else if hovered {
        t.text_faint.lerp_to_gamma(t.base, 0.5)
    } else {
        t.control_hover
    }
}

const CELL: Vec2 = vec2(19.0, 17.0);
pub const TOGGLE_GRID_WIDTH: f32 = 2.0 * CELL.x;

pub fn toggle_grid(ui: &mut Ui, states: &[bool], color: Color32, hint: &str) -> Option<usize> {
    let t = theme::get(ui.ctx());
    let rows = states.len().div_ceil(2);
    let (rect, area) =
        ui.allocate_exact_size(vec2(CELL.x * 2.0, CELL.y * rows as f32), Sense::hover());
    let painter = ui.painter_at(rect.expand(1.0));
    painter.rect_filled(rect, 0.0, t.control);
    let mut clicked = None;
    for (n, &on) in states.iter().enumerate() {
        let cell = Rect::from_min_size(
            rect.min + vec2(CELL.x * (n % 2) as f32, CELL.y * (n / 2) as f32),
            CELL,
        );
        let response = ui
            .interact(cell, area.id.with(("grid", n)), Sense::click())
            .on_hover_text(format!("{hint} {}", n + 1));
        let fill = cell_fill(on, response.hovered(), color, &t);
        let ink = if on { Theme::on(color) } else { t.text_dim };
        painter.rect_filled(cell.shrink(0.5), 0.0, fill);
        painter.text(
            cell.center(),
            Align2::CENTER_CENTER,
            (n + 1).to_string(),
            FontId::monospace(11.0),
            ink,
        );
        if response.clicked() {
            clicked = Some(n);
        }
    }
    let line = Stroke::new(1.0, t.border);
    painter.line_segment(
        [
            pos2(rect.center().x, rect.top()),
            pos2(rect.center().x, rect.bottom()),
        ],
        line,
    );
    for r in 1..rows {
        let y = rect.top() + CELL.y * r as f32;
        painter.line_segment([pos2(rect.left(), y), pos2(rect.right(), y)], line);
    }
    painter.rect_stroke(rect, 0.0, line, StrokeKind::Inside);
    clicked
}

pub fn step_row(
    ui: &mut Ui,
    steps: u64,
    cells: usize,
    current: Option<usize>,
    color: Color32,
    width: f32,
) -> Option<usize> {
    const BEATS: usize = 4;
    const BEAT_GAP: f32 = 4.0;
    let t = theme::get(ui.ctx());
    let cells = cells.clamp(BEATS, 64);
    let per_beat = cells / BEATS;
    let cell_w = ((width - (BEATS - 1) as f32 * BEAT_GAP) / cells as f32).max(2.0);
    let height = 18.0;
    let (rect, area) = ui.allocate_exact_size(
        vec2(
            cell_w * cells as f32 + (BEATS - 1) as f32 * BEAT_GAP,
            height,
        ),
        Sense::hover(),
    );
    let painter = ui.painter_at(rect.expand(1.0));
    let mut clicked = None;
    let margin = if cell_w > 6.0 { 1.0 } else { 0.25 };
    for i in 0..cells {
        let x = rect.left() + cell_w * i as f32 + BEAT_GAP * (i / per_beat) as f32;
        let cell = Rect::from_min_size(pos2(x, rect.top()), vec2(cell_w, height)).shrink(margin);
        let response = ui
            .interact(cell, area.id.with(("step", i)), Sense::click())
            .on_hover_text(format!("Step {}", i + 1));
        let on = steps & (1 << i) != 0;
        let fill = cell_fill(on, response.hovered(), color, &t);
        painter.rect_filled(cell, 0.0, fill);
        if current == Some(i) {
            painter.rect_stroke(cell, 0.0, Stroke::new(1.5, t.text), StrokeKind::Outside);
        }
        if response.clicked() {
            clicked = Some(i);
        }
    }
    clicked
}

pub fn dot(ui: &mut Ui, on: bool, color: Color32, hint: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::click());
    let t = theme::get(ui.ctx());
    let light = Rect::from_center_size(rect.center(), vec2(9.0, 9.0));
    let painter = ui.painter();
    if on {
        painter.rect_filled(light, 0.0, color);
    } else {
        painter.rect_filled(light, 0.0, t.control);
        painter.rect_stroke(light, 0.0, Stroke::new(1.0, t.border), StrokeKind::Inside);
    }
    if response.hovered() {
        painter.rect_stroke(
            light.expand(2.5),
            0.0,
            Stroke::new(1.0, t.invert),
            StrokeKind::Inside,
        );
    }
    response.on_hover_text(hint)
}

pub fn transport(
    ui: &mut Ui,
    text: &str,
    active: bool,
    color: Color32,
    size: Vec2,
    sense: Sense,
) -> Response {
    let t = theme::get(ui.ctx());
    let look = if active {
        Look {
            fill: color,
            text: Theme::on(color),
            outline: color,
        }
    } else {
        Look {
            fill: t.control,
            text: color,
            outline: color,
        }
    };
    let galley = block_galley(ui, text, 13.0);
    block(ui, galley, size, look, Hover::Fade, sense)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Play,
    Pause,
}

pub fn transport_icon(
    ui: &mut Ui,
    icon: Icon,
    active: bool,
    color: Color32,
    size: Vec2,
) -> Response {
    let response = transport(ui, "", active, color, size, Sense::click());
    let ink = if active { Theme::on(color) } else { color };
    let c = response.rect.center();
    let painter = ui.painter();
    match icon {
        Icon::Play => {
            let h = 7.0;
            painter.add(Shape::convex_polygon(
                vec![
                    pos2(c.x - h * 0.7, c.y - h),
                    pos2(c.x + h, c.y),
                    pos2(c.x - h * 0.7, c.y + h),
                ],
                ink,
                Stroke::NONE,
            ));
        }
        Icon::Pause => {
            for dx in [-4.0, 4.0] {
                painter.rect_filled(
                    Rect::from_center_size(pos2(c.x + dx, c.y), vec2(4.0, 14.0)),
                    0.0,
                    ink,
                );
            }
        }
    }
    response
}

pub fn pad(ui: &mut Ui, text: &str, fill: Option<Color32>, size: Vec2) -> Response {
    let t = theme::get(ui.ctx());
    let (look, hover) = match fill {
        Some(fill) => (
            Look {
                fill,
                text: Theme::on(fill),
                outline: fill,
            },
            Hover::Fade,
        ),
        None => (
            Look {
                fill: t.control,
                text: t.text_dim,
                outline: t.border,
            },
            Hover::Invert,
        ),
    };
    let galley = block_galley(ui, text, 11.0);
    block(ui, galley, size, look, hover, Sense::click())
}

pub fn chip(ui: &mut Ui, text: &str, color: Color32) -> Response {
    let t = theme::get(ui.ctx());
    let mut job = LayoutJob::simple_singleline(text.to_owned(), FontId::monospace(10.5), color);
    for section in &mut job.sections {
        section.format.extra_letter_spacing = 10.5 * 0.1;
    }
    let galley = ui.painter().layout_job(job);
    let size = galley.size() + vec2(10.0, 4.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, color.lerp_to_gamma(t.base, 0.84));
    painter.galley(rect.center() - galley.size() / 2.0, galley, color);
    response
}

pub fn divider(ui: &mut Ui, height: f32) {
    let t = theme::get(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(9.0, height), Sense::hover());
    ui.painter().line_segment(
        [rect.center_top(), rect.center_bottom()],
        Stroke::new(1.0, t.border),
    );
}

pub fn pad_to_height(ui: &mut Ui, height: f32) {
    let min = ui.min_rect();
    let missing = height - min.height();
    if missing > 0.0 {
        let pad = Rect::from_min_max(min.left_bottom(), pos2(min.left(), min.bottom() + missing));
        ui.allocate_rect(pad, Sense::hover());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui;

    #[test]
    fn a_hovered_check_paints_its_mark_in_the_text_colour() {
        for id in theme::ThemeId::ALL {
            let t = id.theme();
            let ctx = egui::Context::default();
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                mark_in_text_colour(ui, &t);
                let w = &ui.visuals().widgets;
                assert_eq!(w.hovered.fg_stroke.color, t.text, "{}", id.name());
                assert_eq!(w.active.fg_stroke.color, t.text, "{}", id.name());
            });
            output.textures_delta.clear();
            assert_ne!(t.text, t.invert_fg, "{}", id.name());
        }
    }

    #[test]
    fn padding_a_box_makes_it_exactly_that_tall() {
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0))),
            ..Default::default()
        };
        let mut heights = (0.0, 0.0);
        let mut output = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let padded = egui::Frame::NONE
                    .show(ui, |ui| {
                        ui.label("short");
                        pad_to_height(ui, 100.0);
                    })
                    .response
                    .rect;
                let tall = egui::Frame::NONE
                    .show(ui, |ui| {
                        ui.label("tall");
                        pad_to_height(ui, 10.0);
                    })
                    .response
                    .rect;
                heights = (padded.height(), tall.height());
            });
        });
        output.textures_delta.clear();
        assert!((heights.0 - 100.0).abs() < 0.5, "padded: {}", heights.0);
        assert!(
            heights.1 < 30.0,
            "no padding when already taller: {}",
            heights.1
        );
    }

    #[test]
    fn labels_are_tracked_capitals() {
        let job = hud_job("sync", 11.0, LABEL_TRACKING, Color32::WHITE);
        assert_eq!(job.text, "SYNC");
        assert_eq!(job.sections.len(), 1);
        let format = &job.sections[0].format;
        assert!((format.extra_letter_spacing - 1.65).abs() < 1e-4);
        assert_eq!(format.font_id, FontId::monospace(11.0));
    }

    #[test]
    fn a_fixed_width_block_never_grows_with_its_text() {
        let text = vec2(40.0, 14.0);
        assert_eq!(block_size(text, vec2(30.0, 30.0)), vec2(30.0, 30.0));
        assert_eq!(block_size(text, vec2(0.0, 22.0)), vec2(56.0, 22.0));
        assert_eq!(block_size(text, vec2(0.0, 0.0)), vec2(56.0, 22.0));
    }

    #[test]
    fn grid_cells_tint_and_never_invert() {
        for t in [theme::DARK, theme::LIGHT] {
            let off = cell_fill(false, false, t.accent, &t);
            let over = cell_fill(false, true, t.accent, &t);
            assert_eq!(cell_fill(true, true, t.accent, &t), t.accent);
            assert_ne!(off, t.base, "an off cell shows against the background");
            assert_ne!(over, off);
            assert_ne!(over, t.invert, "hover tints, it does not invert");
            let luma = |c: Color32| u32::from(c.r()) + u32::from(c.g()) + u32::from(c.b());
            let towards_ink = |c: Color32| luma(c).abs_diff(luma(t.base));
            assert!(towards_ink(over) > towards_ink(off));
            assert!(towards_ink(over) < towards_ink(t.text));
        }
    }

    #[test]
    fn neutral_controls_invert_and_coloured_ones_fade() {
        let t = theme::DARK;
        let rest = Look {
            fill: t.control,
            text: t.text_dim,
            outline: t.border,
        };
        let over = hovered_look(rest, Hover::Invert, false, &t);
        assert_eq!(over.fill, t.invert);
        assert_eq!(over.text, t.invert_fg);
        let pressed = hovered_look(rest, Hover::Invert, true, &t);
        assert_ne!(pressed.fill, over.fill, "pressed fades the inverse");
        assert_eq!(pressed.text, t.invert_fg);

        let lit = Look {
            fill: t.play,
            text: Theme::on(t.play),
            outline: t.play,
        };
        let over = hovered_look(lit, Hover::Fade, false, &t);
        assert_eq!(over.text, lit.text, "the text keeps its colour");
        assert_ne!(over.fill, lit.fill);
        assert_eq!(over.fill, t.faded(t.play, false));
    }
}
