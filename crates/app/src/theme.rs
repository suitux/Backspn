#![allow(clippy::unreadable_literal)]

use std::sync::Arc;

use backspn_engine::CueKind;
use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Id, Margin, Shadow,
    Stroke, TextStyle, ThemePreference, Visuals, style::HandleShape, vec2,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ThemeId {
    #[default]
    #[serde(alias = "Midnight", alias = "Graphite")]
    Dark,
    #[serde(alias = "Daylight")]
    Light,
    Darcula,
    Solar,
    Purple,
    Ocean,
}

impl ThemeId {
    pub const ALL: [Self; 6] = [
        Self::Dark,
        Self::Light,
        Self::Darcula,
        Self::Solar,
        Self::Purple,
        Self::Ocean,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Dark => "Dark",
            Self::Light => "Light",
            Self::Darcula => "Darcula",
            Self::Solar => "Solar",
            Self::Purple => "Purple",
            Self::Ocean => "Ocean",
        }
    }

    pub fn theme(self) -> Theme {
        match self {
            Self::Dark => DARK,
            Self::Light => LIGHT,
            Self::Darcula => DARCULA,
            Self::Solar => SOLAR,
            Self::Purple => PURPLE,
            Self::Ocean => OCEAN,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    pub dark: bool,
    pub base: Color32,
    pub surface: Color32,
    pub well: Color32,
    pub control: Color32,
    pub control_hover: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub text_faint: Color32,
    pub accent: Color32,
    pub accent_ink: Color32,
    pub invert: Color32,
    pub invert_fg: Color32,
    pub decks: [Color32; 4],
    pub decks_ink: [Color32; 4],
    pub play: Color32,
    pub cue: Color32,
    pub sync: Color32,
    pub master: Color32,
    pub danger: Color32,
    pub meter: [Color32; 3],
    pub wave: [Color32; 3],
    pub playhead: Color32,
    pub beat: Color32,
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

const fn mix(from: u32, to: u32, percent: i32) -> Color32 {
    const fn channel(from: u32, to: u32, shift: u32, percent: i32) -> u8 {
        let a = ((from >> shift) & 0xFF) as i32;
        let b = ((to >> shift) & 0xFF) as i32;
        (a + (b - a) * percent / 100) as u8
    }
    Color32::from_rgb(
        channel(from, to, 16, percent),
        channel(from, to, 8, percent),
        channel(from, to, 0, percent),
    )
}

const DARK_BG: u32 = 0x121316;
const DARK_INK: u32 = 0xF2F1EE;
const LIGHT_BG: u32 = 0xF2F1EE;
const LIGHT_INK: u32 = 0x101014;
const ACCENT: u32 = 0x2D81FF;
const DECKS: [Color32; 4] = [rgb(0x3568C9), rgb(0xD97700), rgb(0x9D4CFF), rgb(0x00A873)];

pub const DARK: Theme = Theme {
    dark: true,
    base: rgb(DARK_BG),
    surface: rgb(DARK_BG),
    well: rgb(0x060607),
    control: rgb(0x101014),
    control_hover: mix(DARK_BG, DARK_INK, 10),
    border: mix(DARK_BG, DARK_INK, 15),
    border_strong: mix(DARK_BG, DARK_INK, 40),
    text: rgb(DARK_INK),
    text_dim: mix(DARK_BG, DARK_INK, 60),
    text_faint: mix(DARK_BG, DARK_INK, 38),
    accent: rgb(ACCENT),
    accent_ink: rgb(0x74AAFF),
    invert: rgb(0xFFFFFF),
    invert_fg: rgb(0x0B0B0C),
    decks: DECKS,
    decks_ink: [rgb(0x7EA3EE), rgb(0xFFB347), rgb(0xC08FFF), rgb(0x2FD39C)],
    play: rgb(0x00A873),
    cue: rgb(0xD97700),
    sync: rgb(ACCENT),
    master: rgb(0xE0B020),
    danger: rgb(0xFF6259),
    meter: [rgb(0x00A873), rgb(0xD97700), rgb(0xFF6259)],
    wave: [rgb(0x3568C9), rgb(0x2FD39C), rgb(DARK_INK)],
    playhead: rgb(0xFFFFFF),
    beat: mix(DARK_BG, DARK_INK, 15),
};

pub const LIGHT: Theme = Theme {
    dark: false,
    base: rgb(LIGHT_BG),
    surface: rgb(LIGHT_BG),
    well: rgb(0xE9E8E4),
    control: rgb(0xFAF9F7),
    control_hover: mix(LIGHT_BG, LIGHT_INK, 8),
    border: mix(LIGHT_BG, LIGHT_INK, 16),
    border_strong: mix(LIGHT_BG, LIGHT_INK, 40),
    text: rgb(LIGHT_INK),
    text_dim: mix(LIGHT_BG, LIGHT_INK, 62),
    text_faint: mix(LIGHT_BG, LIGHT_INK, 42),
    accent: rgb(ACCENT),
    accent_ink: rgb(0x1659C4),
    invert: rgb(LIGHT_INK),
    invert_fg: rgb(LIGHT_BG),
    decks: DECKS,
    decks_ink: [rgb(0x2A55A8), rgb(0xB35F00), rgb(0x7A2FE0), rgb(0x007A54)],
    play: rgb(0x00A873),
    cue: rgb(0xD97700),
    sync: rgb(ACCENT),
    master: rgb(0x9A6B00),
    danger: rgb(0xB3261E),
    meter: [rgb(0x00A873), rgb(0xD97700), rgb(0xB3261E)],
    wave: [rgb(0x3568C9), rgb(0x00A873), rgb(LIGHT_INK)],
    playhead: rgb(LIGHT_INK),
    beat: mix(LIGHT_BG, LIGHT_INK, 16),
};

const DARCULA_BG: u32 = 0x2B2B2B;
const DARCULA_INK: u32 = 0xA9B7C6;
const DARCULA_BLUE: u32 = 0x6897BB;

pub const DARCULA: Theme = Theme {
    dark: true,
    base: rgb(DARCULA_BG),
    surface: rgb(DARCULA_BG),
    well: rgb(0x1E1F22),
    control: rgb(0x313335),
    control_hover: mix(DARCULA_BG, DARCULA_INK, 10),
    border: mix(DARCULA_BG, DARCULA_INK, 15),
    border_strong: mix(DARCULA_BG, DARCULA_INK, 40),
    text: rgb(DARCULA_INK),
    text_dim: mix(DARCULA_BG, DARCULA_INK, 60),
    text_faint: mix(DARCULA_BG, DARCULA_INK, 42),
    accent: rgb(DARCULA_BLUE),
    accent_ink: rgb(0x8FB6D8),
    invert: rgb(0xFFFFFF),
    invert_fg: rgb(DARCULA_BG),
    decks: [rgb(0x4A6F9B), rgb(0xCC7832), rgb(0x9876AA), rgb(0x6A8759)],
    decks_ink: [rgb(0x8FB6D8), rgb(0xE09A5A), rgb(0xB89CCB), rgb(0x8FAE7A)],
    play: rgb(0x59A869),
    cue: rgb(0xCC7832),
    sync: rgb(DARCULA_BLUE),
    master: rgb(0xFFC66D),
    danger: rgb(0xFF6B68),
    meter: [rgb(0x59A869), rgb(0xCC7832), rgb(0xFF6B68)],
    wave: [rgb(DARCULA_BLUE), rgb(0x6A8759), rgb(DARCULA_INK)],
    playhead: rgb(0xFFFFFF),
    beat: mix(DARCULA_BG, DARCULA_INK, 15),
};

const SOLAR_BG: u32 = 0x002B36;
const SOLAR_INK: u32 = 0x93A1A1;
const SOLAR_BLUE: u32 = 0x268BD2;

pub const SOLAR: Theme = Theme {
    dark: true,
    base: rgb(SOLAR_BG),
    surface: rgb(SOLAR_BG),
    well: rgb(0x00212B),
    control: rgb(0x073642),
    control_hover: mix(SOLAR_BG, SOLAR_INK, 10),
    border: mix(SOLAR_BG, SOLAR_INK, 15),
    border_strong: mix(SOLAR_BG, SOLAR_INK, 40),
    text: rgb(SOLAR_INK),
    text_dim: mix(SOLAR_BG, SOLAR_INK, 60),
    text_faint: mix(SOLAR_BG, SOLAR_INK, 42),
    accent: rgb(SOLAR_BLUE),
    accent_ink: rgb(0x5FA8E0),
    invert: rgb(0xEEE8D5),
    invert_fg: rgb(SOLAR_BG),
    decks: [rgb(SOLAR_BLUE), rgb(0xCB4B16), rgb(0x6C71C4), rgb(0x859900)],
    decks_ink: [rgb(0x5FA8E0), rgb(0xE07A4A), rgb(0x9398D8), rgb(0xA3B82A)],
    play: rgb(0x859900),
    cue: rgb(0xCB4B16),
    sync: rgb(SOLAR_BLUE),
    master: rgb(0xB58900),
    danger: rgb(0xDC322F),
    meter: [rgb(0x859900), rgb(0xB58900), rgb(0xDC322F)],
    wave: [rgb(SOLAR_BLUE), rgb(0x2AA198), rgb(SOLAR_INK)],
    playhead: rgb(0xFDF6E3),
    beat: mix(SOLAR_BG, SOLAR_INK, 15),
};

const PURPLE_BG: u32 = 0x16121F;
const PURPLE_INK: u32 = 0xEFEAF7;
const PURPLE_ACCENT: u32 = 0x9D5CFF;

pub const PURPLE: Theme = Theme {
    dark: true,
    base: rgb(PURPLE_BG),
    surface: rgb(PURPLE_BG),
    well: rgb(0x0C0A12),
    control: rgb(0x1D1828),
    control_hover: mix(PURPLE_BG, PURPLE_INK, 10),
    border: mix(PURPLE_BG, PURPLE_INK, 15),
    border_strong: mix(PURPLE_BG, PURPLE_INK, 40),
    text: rgb(PURPLE_INK),
    text_dim: mix(PURPLE_BG, PURPLE_INK, 60),
    text_faint: mix(PURPLE_BG, PURPLE_INK, 38),
    accent: rgb(PURPLE_ACCENT),
    accent_ink: rgb(0xC4A1FF),
    invert: rgb(0xFFFFFF),
    invert_fg: rgb(PURPLE_BG),
    decks: [rgb(0x5B6BE0), rgb(0xE0609A), rgb(0xB04CFF), rgb(0x2EC4B6)],
    decks_ink: [rgb(0x9AA4FF), rgb(0xFF8CBE), rgb(0xCFA0FF), rgb(0x5EE6D8)],
    play: rgb(0x1FC27A),
    cue: rgb(0xFF8A3D),
    sync: rgb(PURPLE_ACCENT),
    master: rgb(0xF2C94C),
    danger: rgb(0xFF5C7A),
    meter: [rgb(0x1FC27A), rgb(0xFF8A3D), rgb(0xFF5C7A)],
    wave: [rgb(0x7A5CFF), rgb(0x2EC4B6), rgb(PURPLE_INK)],
    playhead: rgb(0xFFFFFF),
    beat: mix(PURPLE_BG, PURPLE_INK, 15),
};

const OCEAN_BG: u32 = 0x2E3440;
const OCEAN_INK: u32 = 0xECEFF4;
const OCEAN_ACCENT: u32 = 0x88C0D0;

pub const OCEAN: Theme = Theme {
    dark: true,
    base: rgb(OCEAN_BG),
    surface: rgb(OCEAN_BG),
    well: rgb(0x242933),
    control: rgb(0x3B4252),
    control_hover: mix(OCEAN_BG, OCEAN_INK, 10),
    border: mix(OCEAN_BG, OCEAN_INK, 15),
    border_strong: mix(OCEAN_BG, OCEAN_INK, 40),
    text: rgb(OCEAN_INK),
    text_dim: mix(OCEAN_BG, OCEAN_INK, 60),
    text_faint: mix(OCEAN_BG, OCEAN_INK, 38),
    accent: rgb(OCEAN_ACCENT),
    accent_ink: rgb(OCEAN_ACCENT),
    invert: rgb(OCEAN_INK),
    invert_fg: rgb(OCEAN_BG),
    decks: [rgb(0x5E81AC), rgb(0xD08770), rgb(0xB48EAD), rgb(0xA3BE8C)],
    decks_ink: [rgb(0x81A1C1), rgb(0xD08770), rgb(0xB48EAD), rgb(0xA3BE8C)],
    play: rgb(0xA3BE8C),
    cue: rgb(0xD08770),
    sync: rgb(OCEAN_ACCENT),
    master: rgb(0xEBCB8B),
    danger: rgb(0xBF616A),
    meter: [rgb(0xA3BE8C), rgb(0xEBCB8B), rgb(0xBF616A)],
    wave: [rgb(0x5E81AC), rgb(0x8FBCBB), rgb(OCEAN_INK)],
    playhead: rgb(OCEAN_INK),
    beat: mix(OCEAN_BG, OCEAN_INK, 15),
};

impl Theme {
    pub fn hotcue(&self, kind: CueKind) -> Color32 {
        match kind {
            CueKind::Cue | CueKind::Other(_) => self.sync,
            CueKind::FadeIn | CueKind::FadeOut => self.cue,
            CueKind::Load => self.master,
            CueKind::Grid => self.playhead,
            CueKind::Loop => self.play,
        }
    }

    pub fn deck(&self, deck: usize) -> Color32 {
        self.decks[deck % 4]
    }

    pub fn deck_ink(&self, deck: usize) -> Color32 {
        self.decks_ink[deck % 4]
    }

    pub fn on(fill: Color32) -> Color32 {
        if luma(fill) > 150.0 {
            Color32::from_rgb(12, 14, 18)
        } else {
            Color32::WHITE
        }
    }

    pub fn pressed(&self) -> Color32 {
        self.invert.lerp_to_gamma(self.base, 0.2)
    }

    pub fn faded(&self, color: Color32, pressed: bool) -> Color32 {
        color.lerp_to_gamma(self.base, if pressed { 0.2 } else { 0.1 })
    }
}

fn luma(c: Color32) -> f32 {
    let [r, g, b, _] = c.to_array();
    0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b)
}

fn theme_id() -> Id {
    Id::unique("backspn-theme")
}

pub fn get(ctx: &egui::Context) -> Theme {
    ctx.data(|d| d.get_temp(theme_id())).unwrap_or(DARK)
}

pub const SANS: &str = "inter";
pub const MONO: &str = "geist-mono";

pub fn fonts() -> FontDefinitions {
    let mut defs = FontDefinitions::default();
    for (name, bytes, family) in [
        (
            SANS,
            &include_bytes!("../fonts/Inter-Regular.ttf")[..],
            FontFamily::Proportional,
        ),
        (
            MONO,
            &include_bytes!("../fonts/GeistMono-Medium.ttf")[..],
            FontFamily::Monospace,
        ),
    ] {
        defs.font_data
            .insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
        defs.families
            .entry(family)
            .or_default()
            .insert(0, name.to_owned());
    }
    defs
}

pub fn set_tooltips(ctx: &egui::Context, on: bool) {
    ctx.all_styles_mut(|style| {
        style.interaction.tooltip_delay = if on { TOOLTIP_DELAY_S } else { f32::INFINITY };
    });
}

const TOOLTIP_DELAY_S: f32 = 0.5;

pub fn apply(ctx: &egui::Context, theme: Theme) {
    ctx.data_mut(|d| d.insert_temp(theme_id(), theme));
    ctx.set_theme(if theme.dark {
        ThemePreference::Dark
    } else {
        ThemePreference::Light
    });
    ctx.all_styles_mut(|style| {
        style.visuals = visuals(&theme);
        let s = &mut style.spacing;
        s.item_spacing = vec2(6.0, 6.0);
        s.button_padding = vec2(10.0, 4.0);
        s.interact_size.y = 24.0;
        s.window_margin = Margin::same(14);
        s.menu_margin = Margin::same(6);
        s.slider_rail_height = 4.0;
        style.text_styles = [
            (
                TextStyle::Heading,
                FontId::new(18.0, FontFamily::Proportional),
            ),
            (TextStyle::Body, FontId::new(13.5, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(11.5, FontFamily::Monospace)),
            (TextStyle::Small, FontId::new(10.5, FontFamily::Monospace)),
            (
                TextStyle::Monospace,
                FontId::new(12.5, FontFamily::Monospace),
            ),
        ]
        .into();
    });
}

fn visuals(t: &Theme) -> Visuals {
    let mut v = if t.dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    v.panel_fill = t.base;
    v.window_fill = t.surface;
    v.window_stroke = Stroke::new(1.0, t.border_strong);
    v.window_corner_radius = CornerRadius::ZERO;
    v.menu_corner_radius = CornerRadius::ZERO;
    v.window_shadow = Shadow::NONE;
    v.popup_shadow = Shadow::NONE;
    v.extreme_bg_color = t.well;
    v.text_edit_bg_color = Some(t.well);
    v.faint_bg_color = t.control;
    v.code_bg_color = t.well;
    v.hyperlink_color = t.accent_ink;
    v.warn_fg_color = t.cue;
    v.error_fg_color = t.danger;
    v.selection.bg_fill = t.accent.gamma_multiply(if t.dark { 0.45 } else { 0.3 });
    v.selection.stroke = Stroke::new(1.0, t.text);
    v.slider_trailing_fill = true;
    v.handle_shape = HandleShape::Rect { aspect_ratio: 0.5 };

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = t.surface;
    w.noninteractive.weak_bg_fill = t.surface;
    w.noninteractive.bg_stroke = Stroke::new(1.0, t.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, t.text);
    w.noninteractive.corner_radius = CornerRadius::ZERO;
    for (state, fill, outline, text) in [
        (&mut w.inactive, t.control, t.border, t.text),
        (&mut w.hovered, t.invert, t.invert, t.invert_fg),
        (&mut w.active, t.control_hover, t.border_strong, t.text),
        (&mut w.open, t.control, t.border_strong, t.text),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.bg_stroke = Stroke::new(1.0, outline);
        state.fg_stroke = Stroke::new(1.0, text);
        state.corner_radius = CornerRadius::ZERO;
        state.expansion = 0.0;
    }
    w.hovered.bg_fill = t.control_hover;
    v.interact_cursor = Some(egui::CursorIcon::PointingHand);
    v
}

pub fn accent_hover<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let accent = get(ui.ctx()).accent;
    ui.scope(|ui| {
        ui.visuals_mut().widgets.hovered.fg_stroke.color = accent;
        add(ui)
    })
    .inner
}

pub fn collapsing<R>(
    ui: &mut egui::Ui,
    header: egui::CollapsingHeader,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::CollapsingResponse<R> {
    let t = get(ui.ctx());
    let hovered = ui.visuals().widgets.hovered;
    ui.visuals_mut().widgets.hovered.fg_stroke.color = t.accent;
    let response = header.show(ui, |ui| {
        ui.visuals_mut().widgets.hovered = hovered;
        body(ui)
    });
    ui.visuals_mut().widgets.hovered = hovered;
    response
}

pub const CARD_CHROME: f32 = 22.0;

pub const BORDER: f32 = 1.0;

pub fn card(t: &Theme) -> egui::Frame {
    egui::Frame::new()
        .fill(t.surface)
        .stroke(Stroke::new(BORDER, t.border))
        .inner_margin(Margin::same(10))
}

pub trait WindowExt {
    fn show_closable<R>(
        self,
        ctx: &egui::Context,
        open: &mut bool,
        add_contents: impl FnOnce(&mut egui::Ui) -> R,
    ) -> Option<egui::InnerResponse<Option<R>>>;
}

impl WindowExt for egui::Window<'_> {
    fn show_closable<R>(
        self,
        ctx: &egui::Context,
        open: &mut bool,
        add_contents: impl FnOnce(&mut egui::Ui) -> R,
    ) -> Option<egui::InnerResponse<Option<R>>> {
        let shown = self.show(ctx, add_contents)?;
        let window = &shown.response;
        let rect = close_rect(ctx, window.rect);
        let mut ui = egui::Ui::new(
            ctx.clone(),
            window.id.with("close"),
            egui::UiBuilder::new()
                .layer_id(window.layer_id)
                .max_rect(rect),
        );
        if crate::widgets::close_button(&mut ui, rect).clicked() {
            *open = false;
        }
        Some(shown)
    }
}

fn close_rect(ctx: &egui::Context, window: egui::Rect) -> egui::Rect {
    let style = ctx.global_style();
    let line = style.visuals.window_stroke.width;
    let margin = style.spacing.window_margin;
    let side = ctx.fonts_mut(|f| f.row_height(&egui::TextStyle::Heading.resolve(&style)));
    egui::Rect::from_min_size(
        egui::pos2(
            window.right() - line - f32::from(margin.right) - side,
            window.top() + line + f32::from(margin.top),
        ),
        vec2(side, side),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_collapsing_header_body_and_what_follows_keep_the_inverted_hover() {
        let ctx = egui::Context::default();
        let t = ThemeId::default().theme();
        apply(&ctx, t);
        let (mut body, mut after) = (None, None);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let header = egui::CollapsingHeader::new("Sets").default_open(true);
                collapsing(ui, header, |ui| {
                    body = Some(ui.visuals().widgets.hovered.fg_stroke.color);
                });
                after = Some(ui.visuals().widgets.hovered.fg_stroke.color);
            });
        });
        output.textures_delta.clear();
        assert_eq!(body, Some(t.invert_fg));
        assert_eq!(after, Some(t.invert_fg));
    }

    fn button_rect(ctx: &egui::Context, pointer: egui::Pos2) -> egui::Rect {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(400.0, 200.0),
            )),
            events: vec![egui::Event::PointerMoved(pointer)],
            ..Default::default()
        };
        let mut rect = egui::Rect::NOTHING;
        let mut output = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                rect = ui.button("RELOOP").rect;
            });
        });
        output.textures_delta.clear();
        rect
    }

    #[test]
    fn mix_composites_in_gamma_space() {
        assert_eq!(
            mix(0x000000, 0xFFFFFF, 50),
            Color32::from_rgb(127, 127, 127)
        );
        assert_eq!(mix(0x121316, 0xF2F1EE, 0), rgb(0x121316));
        assert_eq!(mix(0x121316, 0xF2F1EE, 100), rgb(0xF2F1EE));
    }

    #[test]
    fn the_close_button_closes_the_window() {
        let ctx = egui::Context::default();
        apply(&ctx, DARK);
        let mut open = true;
        let mut window = egui::Rect::NOTHING;
        let frame = |events: Vec<egui::Event>, open: &mut bool, window: &mut egui::Rect| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    vec2(800.0, 600.0),
                )),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                if let Some(shown) = egui::Window::new("Preferences")
                    .fixed_pos(egui::pos2(100.0, 100.0))
                    .show_closable(ui.ctx(), open, |ui| ui.label("x"))
                {
                    *window = shown.response.rect;
                }
            });
            output.textures_delta.clear();
        };
        frame(vec![], &mut open, &mut window);
        let center = close_rect(&ctx, window).center();
        assert!(window.contains(center));
        let click = |pressed| egui::Event::PointerButton {
            pos: center,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(
            vec![egui::Event::PointerMoved(center)],
            &mut open,
            &mut window,
        );
        frame(vec![click(true)], &mut open, &mut window);
        assert!(open);
        frame(vec![click(false)], &mut open, &mut window);
        assert!(!open);
    }

    #[test]
    fn hover_inverts_the_control() {
        for id in ThemeId::ALL {
            let t = id.theme();
            let w = visuals(&t).widgets;
            assert_eq!(w.hovered.weak_bg_fill, t.invert, "{}", id.name());
            assert_eq!(w.hovered.bg_fill, t.control_hover, "{}", id.name());
            assert_eq!(w.hovered.fg_stroke.color, t.invert_fg, "{}", id.name());
            assert_ne!(
                w.active.weak_bg_fill,
                w.hovered.weak_bg_fill,
                "{}",
                id.name()
            );
            assert_eq!(w.inactive.bg_fill, t.control, "{}", id.name());
            assert_eq!(visuals(&t).strong_text_color(), t.text, "{}", id.name());
            assert_eq!(w.open.weak_bg_fill, t.control, "{}", id.name());
            for state in [
                &w.noninteractive,
                &w.inactive,
                &w.hovered,
                &w.active,
                &w.open,
            ] {
                assert_eq!(state.corner_radius, CornerRadius::ZERO, "{}", id.name());
                assert_eq!(state.bg_stroke.width, 1.0, "{}", id.name());
                assert_eq!(state.expansion, 0.0, "{}", id.name());
            }
            let v = visuals(&t);
            assert_eq!(v.window_shadow, Shadow::NONE);
            assert_eq!(v.popup_shadow, Shadow::NONE);
            assert_eq!(v.window_corner_radius, CornerRadius::ZERO);
            assert_eq!(v.menu_corner_radius, CornerRadius::ZERO);
            assert_eq!(v.interact_cursor, Some(egui::CursorIcon::PointingHand));
        }
    }

    #[test]
    fn hovering_a_button_does_not_resize_it() {
        for id in ThemeId::ALL {
            let ctx = egui::Context::default();
            apply(&ctx, id.theme());
            let away = button_rect(&ctx, egui::pos2(390.0, 190.0));
            let _ = button_rect(&ctx, away.center());
            let over = button_rect(&ctx, away.center());
            assert_eq!(away, over, "{}", id.name());
        }
    }

    #[test]
    fn every_theme_has_readable_text() {
        for id in ThemeId::ALL {
            let t = id.theme();
            let contrast = |a: Color32, b: Color32| (luma(a) - luma(b)).abs();
            assert!(contrast(t.text, t.surface) > 120.0, "{}", id.name());
            assert!(contrast(t.text_dim, t.surface) > 50.0, "{}", id.name());
            assert!(contrast(t.text_faint, t.surface) > 40.0, "{}", id.name());
            assert!(contrast(t.invert_fg, t.invert) > 120.0, "{}", id.name());
            assert!(contrast(t.accent_ink, t.surface) > 60.0, "{}", id.name());
            for deck in 0..4 {
                assert!(
                    contrast(t.deck_ink(deck), t.surface) > 60.0,
                    "{}",
                    id.name()
                );
            }
            assert_eq!(t.dark, contrast(t.text, Color32::BLACK) > 120.0);
            assert_eq!(t.base, t.surface, "cards share the background colour");
        }
    }

    #[test]
    fn label_colour_follows_fill() {
        assert_eq!(
            Theme::on(Color32::from_rgb(250, 250, 250)),
            Color32::from_rgb(12, 14, 18)
        );
        assert_eq!(Theme::on(DARK.base), Color32::WHITE);
        assert_eq!(Theme::on(LIGHT.invert), Color32::WHITE);
    }

    #[test]
    fn theme_ids_round_trip_and_old_names_still_load() {
        for id in ThemeId::ALL {
            let json = serde_json::to_string(&id).unwrap();
            assert_eq!(serde_json::from_str::<ThemeId>(&json).unwrap(), id);
        }
        for (old, new) in [
            ("Midnight", ThemeId::Dark),
            ("Graphite", ThemeId::Dark),
            ("Daylight", ThemeId::Light),
        ] {
            let json = format!("\"{old}\"");
            assert_eq!(serde_json::from_str::<ThemeId>(&json).unwrap(), new);
        }
    }

    #[test]
    fn bundled_fonts_come_first_and_parse() {
        let defs = fonts();
        assert_eq!(defs.families[&FontFamily::Proportional][0], SANS);
        assert_eq!(defs.families[&FontFamily::Monospace][0], MONO);
        for name in [SANS, MONO] {
            let data = &defs.font_data[name];
            assert!(
                ab_glyph::FontRef::try_from_slice(data.bytes()).is_ok(),
                "{name}"
            );
        }
        assert!(defs.families[&FontFamily::Proportional].len() > 1);
    }
}
