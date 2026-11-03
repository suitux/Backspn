use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use backspn_control::display::Frame;
use backspn_engine::DeckState;
use eframe::egui::Color32;

use crate::deck_view::{SIZES, clock};
use crate::theme::Theme;
use crate::waveform::{self, PLAYED_DIM, Peaks, WaveStyle};

mod booth;
mod classic;
mod sample_deck;
mod wide;

pub use booth::partner_of;

pub use sample_deck::{SampleScreen, ScreenCell, SlotOverlay};

pub const WIDTH: usize = 320;
pub const HEIGHT: usize = 240;
/// The larger panel of the bigger controllers.
pub const WIDE: (usize, usize) = (480, 272);

const PAD: i32 = 10;

pub struct ScreenDeck<'a> {
    pub index: usize,
    pub state: &'a DeckState,
    pub name: &'a str,
    pub peaks: Option<&'a Peaks>,
    pub loop_beats: f64,
    pub end_warning: bool,
    pub key: Option<&'a str>,
    pub key_color: Option<Color32>,
    pub synced: bool,
    pub master: bool,
    pub zoom_s: f64,
    pub clock_ms: u64,
    pub stem: Option<StemOverlay>,
    pub sample_deck: Option<SampleScreen>,
    /// Every stem of the track, for the layouts that show them all.
    pub stems: Option<[StemOverlay; 4]>,
    /// The deck this one is mixed against, for the layouts that compare.
    pub partner: Option<Partner<'a>>,
}

/// The other deck a screen compares with.
pub struct Partner<'a> {
    pub index: usize,
    pub state: &'a DeckState,
    pub peaks: Option<&'a Peaks>,
    pub key: Option<backspn_formats::nml::MusicalKey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Layout {
    #[default]
    Standard,
    Classic,
    Waveform,
    BigNumbers,
    Phrase,
    Mix,
    Cues,
    Stems,
    WideDeck,
    WideWaveform,
    WideMix,
}

impl Layout {
    /// The layouts drawn at 320 × 240.
    pub const ALL: [Self; 8] = [
        Self::Standard,
        Self::Classic,
        Self::Waveform,
        Self::BigNumbers,
        Self::Phrase,
        Self::Mix,
        Self::Cues,
        Self::Stems,
    ];
    /// The layouts drawn at 480 × 272.
    pub const WIDE: [Self; 3] = [Self::WideDeck, Self::WideWaveform, Self::WideMix];

    /// The layouts a panel of this size can show: on a wide panel the wide
    /// ones first, then the smaller ones, which sit in its middle.
    pub fn for_panel(size: (usize, usize)) -> Vec<Self> {
        if size == WIDE {
            Self::WIDE.into_iter().chain(Self::ALL).collect()
        } else {
            Self::ALL.to_vec()
        }
    }

    pub fn is_wide(self) -> bool {
        Self::WIDE.contains(&self)
    }

    /// The 320 × 240 layout that stands in for a wide one on a smaller panel.
    pub fn small(self) -> Self {
        match self {
            Self::WideDeck => Self::Standard,
            Self::WideWaveform => Self::Waveform,
            Self::WideMix => Self::Mix,
            other => other,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Standard => "Standard",
            Self::Classic => "Classic",
            Self::Waveform => "Waveform",
            Self::BigNumbers => "Big Numbers",
            Self::Phrase => "Phrase",
            Self::Mix => "Mix",
            Self::Cues => "Cues",
            Self::Stems => "Stems",
            Self::WideDeck => "Wide Deck",
            Self::WideWaveform => "Wide Waveform",
            Self::WideMix => "Wide Mix",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Standard => {
                "Title, artist, BPM, time, loop size and overview, in the app's theme"
            }
            Self::Classic => "Boxes for BPM, key, remaining time and loop, and the track stripe",
            Self::Waveform => "A tall zoomed waveform with the track, time and tempo around it",
            Self::BigNumbers => "Remaining time, BPM and key in large type",
            Self::Phrase => {
                "Beat in the bar, bar in the phrase, and bars to the next cue and to the end"
            }
            Self::Mix => {
                "Both decks' waveforms stacked, with tempo, phase and key against the other deck"
            }
            Self::Cues => "Every hotcue with its time and the bars until it",
            Self::Stems => "Level, mute and filter of the four stems",
            Self::WideDeck => {
                "Waveform, track stripe, tempo, key and loop, and what the controls under the screen do"
            }
            Self::WideWaveform => {
                "A tall waveform with the beat counter, and what the controls under the screen do"
            }
            Self::WideMix => {
                "Both decks lined up, with tempo, phase and key against the other deck"
            }
        }
    }
}

struct Canvas {
    px: Vec<[u8; 3]>,
    width: usize,
    height: usize,
}

impl Canvas {
    fn new(fill: Color32) -> Self {
        Self::sized((WIDTH, HEIGHT), fill)
    }

    fn sized((width, height): (usize, usize), fill: Color32) -> Self {
        Self {
            px: vec![[fill.r(), fill.g(), fill.b()]; width * height],
            width,
            height,
        }
    }

    fn blend(&mut self, (px, py): (i32, i32), color: Color32, alpha: f32) {
        let (w, h) = (self.width as i32, self.height as i32);
        if px < 0 || py < 0 || px >= w || py >= h || alpha <= 0.0 {
            return;
        }
        let pixel = &mut self.px[py as usize * self.width + px as usize];
        let a = alpha.min(1.0);
        for (dst, src) in pixel.iter_mut().zip([color.r(), color.g(), color.b()]) {
            *dst = (f32::from(*dst) * (1.0 - a) + f32::from(src) * a).round() as u8;
        }
    }

    fn rect(&mut self, (left, top): (i32, i32), (width, height): (i32, i32), color: Color32) {
        for row in top.max(0)..(top + height).min(self.height as i32) {
            for col in left.max(0)..(left + width).min(self.width as i32) {
                self.px[row as usize * self.width + col as usize] =
                    [color.r(), color.g(), color.b()];
            }
        }
    }

    fn vline(&mut self, x: i32, y0: i32, y1: i32, c: Color32) {
        self.rect((x, y0.min(y1)), (1, (y1 - y0).abs() + 1), c);
    }

    fn frame(&self) -> Frame {
        Frame {
            width: self.width,
            height: self.height,
            pixels: self
                .px
                .iter()
                .map(|&[r, g, b]| Frame::rgb565(r, g, b))
                .collect(),
        }
    }
}

struct Fonts {
    sans: FontRef<'static>,
    mono: FontRef<'static>,
}

fn fonts() -> &'static Fonts {
    static FONTS: std::sync::OnceLock<Fonts> = std::sync::OnceLock::new();
    FONTS.get_or_init(|| Fonts {
        sans: FontRef::try_from_slice(include_bytes!("../fonts/Inter-Regular.ttf"))
            .expect("valid font"),
        mono: FontRef::try_from_slice(include_bytes!("../fonts/GeistMono-Medium.ttf"))
            .expect("valid font"),
    })
}

fn text_width(font: &FontRef<'static>, size: f32, text: &str) -> f32 {
    let f = font.as_scaled(PxScale::from(size));
    text.chars().map(|ch| f.h_advance(f.glyph_id(ch))).sum()
}

fn text(
    canvas: &mut Canvas,
    font: &FontRef<'static>,
    size: f32,
    (x, y): (f32, f32),
    max_w: f32,
    color: Color32,
    text: &str,
) -> f32 {
    let scaled = font.as_scaled(PxScale::from(size));
    let mut shown: String = text.to_owned();
    if text_width(font, size, &shown) > max_w {
        let ellipsis = text_width(font, size, "…");
        while !shown.is_empty() && text_width(font, size, &shown) + ellipsis > max_w {
            shown.pop();
        }
        shown.push('…');
    }
    let baseline = y + scaled.ascent();
    let mut pen = x;
    for ch in shown.chars() {
        let id = scaled.glyph_id(ch);
        let glyph = id.with_scale_and_position(PxScale::from(size), ab_glyph::point(pen, baseline));
        pen += scaled.h_advance(id);
        if let Some(outline) = font.outline_glyph(glyph) {
            let bounds = outline.px_bounds();
            outline.draw(|gx, gy, cov| {
                canvas.blend(
                    (
                        bounds.min.x as i32 + gx as i32,
                        bounds.min.y as i32 + gy as i32,
                    ),
                    color,
                    cov,
                );
            });
        }
    }
    pen - x
}

fn text_right(
    canvas: &mut Canvas,
    font: &FontRef<'static>,
    size: f32,
    (right, y): (f32, f32),
    color: Color32,
    s: &str,
) {
    let w = text_width(font, size, s);
    text(canvas, font, size, (right - w, y), w + 1.0, color, s);
}

fn beats_label(beats: f64) -> String {
    SIZES
        .iter()
        .find(|(b, _)| (b - beats).abs() < 1e-9)
        .map_or_else(|| format!("{beats}"), |(_, label)| (*label).to_owned())
}

fn waveform_band(
    canvas: &mut Canvas,
    peaks: &Peaks,
    style: &WaveStyle,
    (from_s, to_s): (f64, f64),
    position_s: f64,
    (x0, x1): (i32, i32),
    (y0, y1): (i32, i32),
) {
    let mid = (y0 + y1) as f32 / 2.0;
    let half = (y1 - y0) as f32 * 0.48;
    let per_px = (to_s - from_s) / f64::from(x1 - x0);
    for x in x0..x1 {
        let t0 = from_s + f64::from(x - x0) * per_px;
        for (top, bottom, c) in waveform::column(peaks, style, t0, t0 + per_px) {
            let c = if t0 < position_s {
                c.gamma_multiply(PLAYED_DIM)
            } else {
                c
            };
            let (a, b) = ((mid - top * half).round(), (mid - bottom * half).round());
            canvas.vline(x, a as i32, b as i32, c);
        }
    }
}

fn x_of((x0, x1): (i32, i32), (from_s, to_s): (f64, f64), t: f64) -> i32 {
    x0 + ((t - from_s) / (to_s - from_s) * f64::from(x1 - x0)).round() as i32
}

/// An empty deck with nothing else to show.
pub fn idle(deck: &ScreenDeck) -> bool {
    !deck.state.loaded && deck.sample_deck.is_none() && deck.stem.is_none()
}

const LOGO_PX: u32 = 128;

fn logo_image() -> Option<&'static image::RgbaImage> {
    static LOGO: std::sync::OnceLock<Option<image::RgbaImage>> = std::sync::OnceLock::new();
    LOGO.get_or_init(|| crate::icon::logo_rgba(LOGO_PX))
        .as_ref()
}

pub const LOGO_EXTENSIONS: [&str; 3] = ["png", "jpg", "jpeg"];

/// The largest size with the picture's proportions that fits the screen.
fn fit((width, height): (u32, u32)) -> (u32, u32) {
    let (max_w, max_h) = (WIDTH as u32, HEIGHT as u32);
    if width == 0 || height == 0 {
        return (0, 0);
    }
    if u64::from(width) * u64::from(max_h) > u64::from(height) * u64::from(max_w) {
        let h = (u64::from(height) * u64::from(max_w) / u64::from(width)).max(1);
        (max_w, h as u32)
    } else {
        let w = (u64::from(width) * u64::from(max_h) / u64::from(height)).max(1);
        (w as u32, max_h)
    }
}

fn load_custom_logo(path: &std::path::Path) -> Option<image::RgbaImage> {
    let picture = image::open(path)
        .map_err(|e| log::warn!("screen logo {}: {e}", path.display()))
        .ok()?
        .into_rgba8();
    let (width, height) = fit(picture.dimensions());
    Some(image::imageops::resize(
        &picture,
        width,
        height,
        image::imageops::FilterType::Lanczos3,
    ))
}

type CustomLogo = Option<(std::path::PathBuf, Option<std::sync::Arc<image::RgbaImage>>)>;

/// Read once per path; a file that cannot be read falls back to our logo.
fn custom_logo(path: &std::path::Path) -> Option<std::sync::Arc<image::RgbaImage>> {
    static CACHE: std::sync::Mutex<CustomLogo> = std::sync::Mutex::new(None);
    let mut cache = CACHE.lock().ok()?;
    if cache.as_ref().is_none_or(|(cached, _)| cached != path) {
        let logo = load_custom_logo(path).map(std::sync::Arc::new);
        *cache = Some((path.to_owned(), logo));
    }
    cache.as_ref().and_then(|(_, logo)| logo.clone())
}

fn draw_image(c: &mut Canvas, picture: &image::RgbaImage, (left, top): (i32, i32)) {
    for (col, row, px) in picture.enumerate_pixels() {
        let [red, green, blue, alpha] = px.0;
        c.blend(
            (left + col as i32, top + row as i32),
            Color32::from_rgb(red, green, blue),
            f32::from(alpha) / 255.0,
        );
    }
}

/// The logo screen: the chosen picture fitted to the screen, or ours with
/// the name under it.
pub fn render_logo(t: &Theme, custom: Option<&std::path::Path>) -> Frame {
    const NAME: &str = "Backspn";
    const NAME_SIZE: f32 = 30.0;
    const GAP: i32 = 10;
    let mut c = Canvas::new(t.base);
    if let Some(picture) = custom.and_then(custom_logo) {
        let (width, height) = picture.dimensions();
        let left = (WIDTH as i32 - width as i32) / 2;
        let top = (HEIGHT as i32 - height as i32) / 2;
        draw_image(&mut c, &picture, (left, top));
        return c.frame();
    }
    let top = (HEIGHT as i32 - LOGO_PX as i32 - GAP - NAME_SIZE as i32) / 2;
    if let Some(logo) = logo_image() {
        draw_image(&mut c, logo, ((WIDTH as i32 - LOGO_PX as i32) / 2, top));
    }
    let f = fonts();
    let width = text_width(&f.sans, NAME_SIZE, NAME);
    let name_top = (top + LOGO_PX as i32 + GAP) as f32;
    text(
        &mut c,
        &f.sans,
        NAME_SIZE,
        ((WIDTH as f32 - width) / 2.0, name_top),
        width + 1.0,
        t.text,
        NAME,
    );
    c.frame()
}

pub fn render(deck: &ScreenDeck, style: &WaveStyle, t: &Theme, layout: Layout) -> Frame {
    if let Some(r) = &deck.sample_deck {
        return sample_deck::render(r).frame();
    }
    let layout = layout.small();
    let mut c = match layout {
        Layout::Standard | Layout::WideDeck | Layout::WideWaveform | Layout::WideMix => {
            render_standard(deck, style, t)
        }
        Layout::Classic => classic::render(deck, style, t),
        Layout::Waveform => render_waveform(deck, style, t),
        Layout::BigNumbers => render_big(deck, style, t),
        Layout::Phrase => booth::render_phrase(deck, style, t),
        Layout::Mix => booth::render_mix(deck, style, t),
        Layout::Cues => booth::render_cues(deck, t),
        Layout::Stems => booth::render_stems(deck, style, t),
    };
    if let Some(stem) = &deck.stem {
        draw_stem(&mut c, stem, t.base);
    }
    c.frame()
}

/// A frame for a panel of `size`: a wide layout fills a wide panel; anything
/// else is drawn at 320 × 240, which the controller centres on a larger panel.
pub fn render_panel(
    deck: &ScreenDeck,
    style: &WaveStyle,
    t: &Theme,
    layout: Layout,
    size: (usize, usize),
) -> Frame {
    if size == WIDE && layout.is_wide() && deck.sample_deck.is_none() {
        return wide::render(deck, style, t, layout).frame();
    }
    render(deck, style, t, layout)
}

#[derive(Debug, Clone, PartialEq)]
pub struct StemOverlay {
    pub name: String,
    pub colour: Color32,
    pub volume: f32,
    pub muted: bool,
    pub filter: f32,
    pub filter_on: bool,
}

fn draw_stem(c: &mut Canvas, stem: &StemOverlay, bg: Color32) {
    let f = fonts();
    let dim = |col: Color32, k: f32| {
        let m = |v: u8| (f32::from(v) * k).round() as u8;
        Color32::from_rgb(m(col.r()), m(col.g()), m(col.b()))
    };
    let mid = dim(stem.colour, 0.7);
    c.rect((0, 0), (WIDTH as i32, 156), bg);
    text(
        c,
        &f.sans,
        22.0,
        (12.0, 5.0),
        294.0,
        Color32::WHITE,
        &stem.name,
    );
    let grey = Color32::from_rgb(0x80, 0x80, 0x80);
    for (left, label, on) in [(6, "Volume", !stem.muted), (162, "Filter", stem.filter_on)] {
        let fill = if on { mid } else { dim(mid, 0.5) };
        c.rect((left, 42), (150, 33), fill);
        let ink = if on { Color32::WHITE } else { grey };
        let w = text_width(&f.sans, 24.0, label);
        text(
            c,
            &f.sans,
            24.0,
            (left as f32 + (150.0 - w) / 2.0, 44.0),
            150.0,
            ink,
            label,
        );
    }
    let dark = Color32::from_rgb(0x28, 0x28, 0x28);
    for (left, value, on, centred) in [
        (6, stem.volume, !stem.muted, false),
        (162, stem.filter, stem.filter_on, true),
    ] {
        c.rect((left, 81), (150, 72), dark);
        let fill = if on { stem.colour } else { mid };
        let at = ((150.0 - 5.0) * value.clamp(0.0, 1.0)).round() as i32;
        let (from, to) = if centred {
            (at.min(72), at.max(72) + 5)
        } else {
            (0, at + 5)
        };
        c.rect((left + from, 81), (to - from, 72), fill);
        if centred {
            c.rect((left + 72, 81), (5, 72), Color32::BLACK);
        }
        let cursor = if on { Color32::WHITE } else { grey };
        c.rect((left + at, 81), (5, 72), cursor);
    }
}

fn title<'a>(deck: &ScreenDeck<'a>) -> &'a str {
    deck.name.split_once(" - ").map_or(deck.name, |(_, t)| t)
}

fn remaining(s: &DeckState) -> String {
    let left = (s.duration_s - s.position_s).max(0.0);
    let m = (left / 60.0).floor();
    format!("-{}:{:04.1}", m as u64, left - m * 60.0)
}

pub fn deck_color(index: usize, t: &Theme) -> Color32 {
    if index >= 2 {
        Color32::from_rgb(0xFF, 0x8A, 0x1E)
    } else {
        t.accent
    }
}

fn chip(
    canvas: &mut Canvas,
    (left, top): (i32, i32),
    label: &str,
    on: bool,
    color: Color32,
    t: &Theme,
) -> i32 {
    let mono = &fonts().mono;
    let w = text_width(mono, 12.0, label).ceil() as i32 + 12;
    canvas.rect((left, top), (w, 18), if on { color } else { t.well });
    let ink = if on { Theme::on(color) } else { t.text_dim };
    text(
        canvas,
        mono,
        12.0,
        (left as f32 + 6.0, top as f32 + 2.0),
        w as f32,
        ink,
        label,
    );
    w
}

fn draw_zoomed(
    c: &mut Canvas,
    deck: &ScreenDeck,
    style: &WaveStyle,
    t: &Theme,
    (y0, y1): (i32, i32),
) {
    draw_zoomed_in(
        c,
        deck.state,
        deck.peaks,
        style,
        t,
        deck.zoom_s,
        (0, WIDTH as i32),
        (y0, y1),
    );
}

/// A deck's waveform around its playhead between `xs` and `ys`, with the beat
/// grid, the loop, the hotcues and the playhead in the middle.
#[allow(clippy::too_many_arguments)]
fn draw_zoomed_in(
    c: &mut Canvas,
    s: &DeckState,
    peaks: Option<&Peaks>,
    style: &WaveStyle,
    t: &Theme,
    zoom_s: f64,
    xs: (i32, i32),
    (y0, y1): (i32, i32),
) {
    c.rect((xs.0, y0), (xs.1 - xs.0, y1 - y0), t.well);
    let (Some(peaks), true) = (peaks, s.loaded) else {
        return;
    };
    let window = (s.position_s - zoom_s, s.position_s + zoom_s);
    if let Some(grid) = s.grid {
        for (at, bar) in grid.beats_in(window.0, window.1) {
            let line = if bar { t.text_faint } else { t.border };
            c.vline(x_of(xs, window, at), y0, y1 - 1, line);
        }
    }
    if let (Some(a), Some(b)) = (s.loop_start_s, s.loop_end_s) {
        let tint = if s.loop_active { t.play } else { t.text_dim };
        for xx in x_of(xs, window, a).max(xs.0)..x_of(xs, window, b).min(xs.1) {
            for yy in y0..y1 {
                c.blend((xx, yy), tint, 0.18);
            }
        }
    }
    waveform_band(c, peaks, style, window, s.position_s, xs, (y0 + 2, y1 - 2));
    for (n, h) in s.hotcues.iter().enumerate() {
        if let Some(h) = h {
            let hx = x_of(xs, window, h.start_s);
            c.vline(hx, y0, y1 - 1, t.accent);
            c.rect((hx, y0), (10, 13), t.accent);
            let label = (n + 1).to_string();
            text(
                c,
                &fonts().mono,
                11.0,
                (hx as f32 + 2.0, y0 as f32),
                10.0,
                Theme::on(t.accent),
                &label,
            );
        }
    }
    c.rect((i32::midpoint(xs.0, xs.1) - 1, y0), (2, y1 - y0), t.danger);
}

fn render_waveform(deck: &ScreenDeck, style: &WaveStyle, t: &Theme) -> Canvas {
    let ff = fonts();
    let st = deck.state;
    let mut c = Canvas::new(t.base);
    let color = deck_color(deck.index, t);
    c.rect((0, 0), (4, 24), color);
    let shown = if st.loaded { title(deck) } else { "Empty" };
    let ink = if st.loaded { t.text } else { t.text_dim };
    text(&mut c, &ff.sans, 15.0, (10.0, 3.0), 220.0, ink, shown);
    let time_color = if deck.end_warning { t.danger } else { t.text };
    text_right(
        &mut c,
        &ff.mono,
        15.0,
        (WIDTH as f32 - 6.0, 4.0),
        time_color,
        &remaining(st),
    );
    draw_zoomed(&mut c, deck, style, t, (26, 194));
    draw_band_overview(&mut c, deck, style, t, (198, 212));
    let bpm = st
        .effective_bpm()
        .map_or_else(|| "—".to_owned(), |b| format!("{b:.2}"));
    text(&mut c, &ff.mono, 17.0, (6.0, 217.0), 100.0, t.text, &bpm);
    let pct = (st.tempo - 1.0) * 100.0;
    text(
        &mut c,
        &ff.mono,
        12.0,
        (88.0, 221.0),
        70.0,
        t.text_dim,
        &format!("{pct:+.1}%"),
    );
    if let Some(key) = deck.key {
        text(
            &mut c,
            &ff.mono,
            15.0,
            (152.0, 218.0),
            60.0,
            deck.key_color.unwrap_or(t.text),
            key,
        );
    }
    let label = format!("LOOP {}", beats_label(deck.loop_beats));
    let w = text_width(&ff.mono, 12.0, &label).ceil() as i32 + 12;
    chip(
        &mut c,
        (WIDTH as i32 - 6 - w, 217),
        &label,
        st.loop_active,
        t.play,
        t,
    );
    c
}

fn render_big(deck: &ScreenDeck, style: &WaveStyle, t: &Theme) -> Canvas {
    let ff = fonts();
    let st = deck.state;
    let mut c = Canvas::new(t.base);
    let color = deck_color(deck.index, t);
    c.rect((0, 0), (WIDTH as i32, 4), color);
    let shown = if st.loaded { title(deck) } else { "Empty" };
    let ink = if st.loaded { t.text } else { t.text_dim };
    text(
        &mut c,
        &ff.sans,
        17.0,
        (PAD as f32, 10.0),
        300.0,
        ink,
        shown,
    );
    let time = remaining(st);
    let blink_off = deck.end_warning && !(deck.clock_ms / 500).is_multiple_of(2);
    if !blink_off {
        let time_color = if deck.end_warning { t.danger } else { t.text };
        let w = text_width(&ff.mono, 56.0, &time);
        text(
            &mut c,
            &ff.mono,
            56.0,
            ((WIDTH as f32 - w) / 2.0, 38.0),
            320.0,
            time_color,
            &time,
        );
    }
    let bpm = st
        .effective_bpm()
        .map_or_else(|| "—".to_owned(), |b| format!("{b:.1}"));
    text(
        &mut c,
        &ff.mono,
        34.0,
        (PAD as f32, 112.0),
        170.0,
        t.text,
        &bpm,
    );
    let pct = (st.tempo - 1.0) * 100.0;
    text(
        &mut c,
        &ff.mono,
        13.0,
        (PAD as f32 + 2.0, 152.0),
        150.0,
        t.text_dim,
        &format!("{pct:+.1}% BPM"),
    );
    if let Some(key) = deck.key {
        let right = (WIDTH as i32 - PAD) as f32;
        text_right(
            &mut c,
            &ff.mono,
            34.0,
            (right, 112.0),
            deck.key_color.unwrap_or(t.text),
            key,
        );
    }
    let mut x = PAD;
    for (label, on, col) in [
        ("SYNC", deck.synced, color),
        ("MASTER", deck.master, color),
        ("LOOP", st.loop_active, t.play),
    ] {
        x += chip(&mut c, (x, 174), label, on, col, t) + 6;
    }
    draw_band_overview(&mut c, deck, style, t, (200, 234));
    c
}

fn draw_band_overview(
    c: &mut Canvas,
    deck: &ScreenDeck,
    style: &WaveStyle,
    t: &Theme,
    ys: (i32, i32),
) {
    draw_band_overview_in(c, deck, style, t, (PAD, WIDTH as i32 - PAD), ys);
}

/// The whole track between `xs` and `ys`, the played part dimmed, with the position.
fn draw_band_overview_in(
    c: &mut Canvas,
    deck: &ScreenDeck,
    style: &WaveStyle,
    t: &Theme,
    xs: (i32, i32),
    (y0, y1): (i32, i32),
) {
    let s = deck.state;
    c.rect((xs.0, y0), (xs.1 - xs.0, y1 - y0), t.well);
    if let (Some(peaks), true) = (deck.peaks, s.loaded && s.duration_s > 0.0) {
        let whole = (0.0, s.duration_s);
        waveform_band(c, peaks, style, whole, s.position_s, xs, (y0 + 1, y1 - 1));
        let px = x_of(xs, whole, s.position_s).clamp(xs.0, xs.1 - 2);
        c.rect((px, y0), (2, y1 - y0), t.danger);
    }
}

fn render_standard(deck: &ScreenDeck, style: &WaveStyle, t: &Theme) -> Canvas {
    let mut c = Canvas::new(t.base);
    draw_names(&mut c, deck, t);
    c.rect((0, 62), (WIDTH as i32, 1), t.border);
    draw_readouts(&mut c, deck, t);
    draw_loop(&mut c, deck, t);
    c.rect((0, 176), (WIDTH as i32, 1), t.border);
    draw_overview(&mut c, deck, style, t);
    c
}

fn draw_names(c: &mut Canvas, deck: &ScreenDeck, t: &Theme) {
    let f = fonts();
    let width = (WIDTH as i32 - 2 * PAD) as f32;
    let (artist, title) = deck
        .name
        .split_once(" - ")
        .map_or(("", deck.name), |(a, t)| (a, t));
    if deck.state.loaded {
        text(c, &f.sans, 22.0, (PAD as f32, 8.0), width, t.text, title);
        text(
            c,
            &f.sans,
            15.0,
            (PAD as f32, 36.0),
            width,
            t.text_dim,
            artist,
        );
    } else {
        text(
            c,
            &f.sans,
            22.0,
            (PAD as f32, 8.0),
            width,
            t.text_dim,
            "Empty",
        );
    }
}

fn draw_readouts(c: &mut Canvas, deck: &ScreenDeck, t: &Theme) {
    let f = fonts();
    let s = deck.state;
    let bpm = s
        .effective_bpm()
        .map_or_else(|| "—".to_owned(), |b| format!("{b:.2}"));
    text(c, &f.mono, 34.0, (PAD as f32, 70.0), 180.0, t.text, &bpm);
    let pct = (s.tempo - 1.0) * 100.0;
    text(
        c,
        &f.mono,
        14.0,
        (PAD as f32 + 2.0, 110.0),
        120.0,
        t.text_dim,
        &format!("{pct:+.1}% BPM"),
    );
    let right = (WIDTH as i32 - PAD) as f32;
    let time = if s.loaded {
        clock(s.position_s)
    } else {
        "—".into()
    };
    let color = if deck.end_warning { t.danger } else { t.text };
    text_right(c, &f.mono, 34.0, (right, 70.0), color, &time);
    text_right(c, &f.mono, 14.0, (right - 2.0, 110.0), t.text_faint, "TIME");
}

fn draw_loop(c: &mut Canvas, deck: &ScreenDeck, t: &Theme) {
    let mono = &fonts().mono;
    let (top, height) = (136, 30);
    text(
        c,
        mono,
        20.0,
        (PAD as f32 + 2.0, top as f32 + 3.0),
        80.0,
        t.text_faint,
        "LOOP",
    );
    let fill = if deck.state.loop_active {
        t.play
    } else {
        t.accent
    };
    let label = beats_label(deck.loop_beats);
    let width = (text_width(mono, 15.0, &label).ceil() as i32 + 24).max(56);
    let size = 22.0;
    let label_w = text_width(mono, size, &label);
    let left = PAD + 60;
    c.rect((left, top), (width, height), fill);
    text(
        c,
        mono,
        size,
        (
            left as f32 + (width as f32 - label_w) / 2.0,
            top as f32 + (height as f32 - size * 1.2) / 2.0,
        ),
        width as f32,
        Theme::on(fill),
        &label,
    );
}

fn draw_overview(c: &mut Canvas, deck: &ScreenDeck, style: &WaveStyle, t: &Theme) {
    let s = deck.state;
    let (oy0, oy1) = (186, 232);
    let xs = (PAD, WIDTH as i32 - PAD);
    c.rect((xs.0, oy0), (xs.1 - xs.0, oy1 - oy0), t.well);
    if let (Some(peaks), true) = (deck.peaks, s.loaded && s.duration_s > 0.0) {
        let whole = (0.0, s.duration_s);
        if let (Some(a), Some(b)) = (s.loop_start_s, s.loop_end_s) {
            let (xa, xb) = (x_of(xs, whole, a), x_of(xs, whole, b));
            let tint = if s.loop_active { t.play } else { t.text_dim };
            for xx in xa.max(xs.0)..xb.min(xs.1) {
                for yy in oy0..oy1 {
                    c.blend((xx, yy), tint, 0.18);
                }
            }
        }
        waveform_band(c, peaks, style, whole, s.position_s, xs, (oy0 + 2, oy1 - 2));
        let px = x_of(xs, whole, s.position_s).clamp(xs.0, xs.1 - 2);
        c.rect((px, oy0), (2, oy1 - oy0), t.danger);
    }
}

pub fn rgb_bytes(frame: &Frame) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(frame.pixels.len() * 3);
    for &px in &frame.pixels {
        let (r, g, b) = (px >> 11, (px >> 5) & 0x3f, px & 0x1f);
        bytes.extend([(r << 3) as u8, (g << 2) as u8, (b << 3) as u8]);
    }
    bytes
}

pub fn write_ppm(path: &std::path::Path, frame: &Frame) -> std::io::Result<()> {
    let mut bytes = format!("P6\n{WIDTH} {HEIGHT}\n255\n").into_bytes();
    bytes.extend(rgb_bytes(frame));
    std::fs::write(path, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::DARK;

    fn pixel(frame: &Frame, x: usize, y: usize) -> u16 {
        frame.pixels[y * WIDTH + x]
    }

    fn rgb565(c: Color32) -> u16 {
        Frame::rgb565(c.r(), c.g(), c.b())
    }

    fn style() -> WaveStyle {
        WaveStyle {
            mode: waveform::WaveMode::ThreeBand,
            colors: [Color32::RED, Color32::GREEN, Color32::BLUE],
            direct: false,
        }
    }

    fn deck(state: &DeckState, loop_beats: f64) -> ScreenDeck<'_> {
        ScreenDeck {
            index: 0,
            state,
            name: "",
            peaks: None,
            loop_beats,
            end_warning: false,
            key: None,
            key_color: None,
            synced: false,
            master: false,
            zoom_s: 4.0,
            clock_ms: 0,
            stem: None,
            sample_deck: None,
            stems: None,
            partner: None,
        }
    }

    fn sample_screen() -> SampleScreen {
        let cell = |number, colour, looped, playing| ScreenCell {
            number,
            name: format!("Sample {number}"),
            colour,
            looped,
            playing,
        };
        let red = Some(Color32::from_rgb(200, 0, 0));
        SampleScreen {
            title: "My Set".into(),
            bpm: 124.0,
            quantize: "1".into(),
            quantize_on: true,
            beat: "12.3.2".into(),
            loop_beats: 4.0,
            loop_active: false,
            recording: false,
            cells: [
                cell(1, red, true, true),
                cell(2, red, false, false),
                cell(3, None, false, false),
                cell(4, red, true, false),
            ],
            slot: None,
        }
    }

    #[test]
    fn a_sample_deck_shows_its_cells_and_a_held_slot() {
        let state = DeckState::default();
        let mut r = sample_screen();
        let d = ScreenDeck {
            sample_deck: Some(r.clone()),
            ..deck(&state, 4.0)
        };
        for layout in Layout::ALL {
            let frame = render(&d, &style(), &DARK, layout);
            assert_eq!(
                pixel(&frame, 8, 161),
                Frame::rgb565(200, 0, 0),
                "{layout:?}"
            );
            assert_eq!(pixel(&frame, 86, 161), Frame::rgb565(120, 0, 0));
            assert_eq!(pixel(&frame, 164, 161), Frame::rgb565(0x55, 0x55, 0x55));
            assert_eq!(pixel(&frame, 300, 10), Frame::rgb565(0, 0, 0));
        }
        r.recording = true;
        let rec = ScreenDeck {
            sample_deck: Some(r.clone()),
            ..deck(&state, 4.0)
        };
        let frame = render(&rec, &style(), &DARK, Layout::Standard);
        assert_eq!(pixel(&frame, 300, 10), Frame::rgb565(0xFF, 0x00, 0x50));
        r.slot = Some(SlotOverlay {
            browsing: false,
            volume: 1.0,
            muted: false,
            filter: 0.5,
            filter_on: false,
            around: [None, Some(r.cells[0].clone()), None],
        });
        let held = ScreenDeck {
            sample_deck: Some(r.clone()),
            ..deck(&state, 4.0)
        };
        let frame = render(&held, &style(), &DARK, Layout::Standard);
        assert_eq!(pixel(&frame, 10, 45), Frame::rgb565(120, 0, 0));
        assert_eq!(
            pixel(&frame, 170, 45),
            Frame::rgb565(60, 0, 0),
            "filter off"
        );
        if let Some(slot) = &mut r.slot {
            slot.browsing = true;
        }
        let browsing = ScreenDeck {
            sample_deck: Some(r),
            ..deck(&state, 4.0)
        };
        let frame = render(&browsing, &style(), &DARK, Layout::Standard);
        assert_eq!(pixel(&frame, 300, 90), Frame::rgb565(0x46, 0x46, 0x46));
    }

    #[test]
    fn every_layout_fills_the_screen_and_classic_warns_in_red() {
        let state = DeckState::default();
        let d = deck(&state, 4.0);
        for layout in Layout::ALL {
            let frame = render(&d, &style(), &DARK, layout);
            assert_eq!(frame.pixels.len(), WIDTH * HEIGHT, "{layout:?}");
        }
        let classic = render(&d, &style(), &DARK, Layout::Classic);
        let well = rgb565(DARK.well);
        assert_eq!(pixel(&classic, 10, 85), well);
        let warn = ScreenDeck {
            end_warning: true,
            ..deck(&state, 4.0)
        };
        let red = rgb565(DARK.danger);
        assert_eq!(
            pixel(&render(&warn, &style(), &DARK, Layout::Classic), 10, 85),
            red
        );
        let held = ScreenDeck {
            stem: Some(StemOverlay {
                name: "Drums".into(),
                colour: Color32::from_rgb(200, 100, 0),
                volume: 1.0,
                muted: true,
                filter: 0.9,
                filter_on: false,
            }),
            sample_deck: None,
            ..d
        };
        for layout in Layout::ALL {
            let frame = render(&held, &style(), &DARK, layout);
            assert_eq!(
                pixel(&frame, 10, 70),
                Frame::rgb565(70, 35, 0),
                "{layout:?}"
            );
            assert_eq!(pixel(&frame, 162 + 74, 120), Frame::rgb565(0, 0, 0));
        }
    }

    #[test]
    fn screen_snapshots() {
        let Some(dir) = std::env::var_os("BACKSPN_SCREEN_SNAPSHOT") else {
            return;
        };
        let rate = 8000;
        let samples: Vec<f32> = (0..rate * 60 * 2)
            .map(|i| (i as f32 * 0.05).sin() * ((i / 4000) % 7) as f32 / 7.0)
            .collect();
        let track = backspn_decode::Track::from_stereo(rate as u32, samples);
        let peaks = Peaks::compute(&track);
        let mut state = DeckState {
            loaded: true,
            duration_s: 60.0,
            position_s: 21.3,
            tempo: 1.02,
            keylock: true,
            loop_active: true,
            ..DeckState::default()
        };
        state.hotcues[0] = Some(backspn_engine::HotcuePoint {
            start_s: 5.0,
            kind: backspn_engine::CueKind::Cue,
            loop_s: None,
        });
        let d = ScreenDeck {
            name: "Some Artist - A Rather Long Track Title",
            peaks: Some(&peaks),
            key: Some("8A"),
            key_color: Some(Color32::from_rgb(0x40, 0xC0, 0xFF)),
            synced: true,
            loop_beats: 8.0,
            ..deck(&state, 8.0)
        };
        for layout in Layout::ALL {
            let path = std::path::Path::new(&dir).join(format!("{layout:?}.ppm"));
            write_ppm(&path, &render(&d, &style(), &DARK, layout)).unwrap();
        }
        let mut r = sample_screen();
        let sample_deck = |r: &SampleScreen| ScreenDeck {
            sample_deck: Some(r.clone()),
            ..deck(&state, 8.0)
        };
        let path = std::path::Path::new(&dir).join("SampleDeck.ppm");
        write_ppm(
            &path,
            &render(&sample_deck(&r), &style(), &DARK, Layout::Standard),
        )
        .unwrap();
        r.slot = Some(SlotOverlay {
            browsing: false,
            volume: 0.7,
            muted: false,
            filter: 0.3,
            filter_on: true,
            around: [None, Some(r.cells[0].clone()), Some(r.cells[1].clone())],
        });
        let path = std::path::Path::new(&dir).join("SampleSlot.ppm");
        write_ppm(
            &path,
            &render(&sample_deck(&r), &style(), &DARK, Layout::Standard),
        )
        .unwrap();
        if let Some(slot) = &mut r.slot {
            slot.browsing = true;
        }
        let path = std::path::Path::new(&dir).join("SampleBrowse.ppm");
        write_ppm(
            &path,
            &render(&sample_deck(&r), &style(), &DARK, Layout::Standard),
        )
        .unwrap();
    }

    fn booth_state() -> DeckState {
        let mut state = DeckState {
            loaded: true,
            playing: true,
            duration_s: 60.0,
            position_s: 21.3,
            tempo: 1.02,
            keylock: true,
            loop_active: true,
            loop_start_s: Some(20.0),
            loop_end_s: Some(23.9),
            grid: Some(backspn_engine::Grid {
                bpm: 123.0,
                anchor_s: 0.1,
                ..backspn_engine::Grid::FIXED
            }),
            ..DeckState::default()
        };
        let cue = |start_s, loop_s| {
            Some(backspn_engine::HotcuePoint {
                start_s,
                kind: backspn_engine::CueKind::Cue,
                loop_s,
            })
        };
        state.hotcues[0] = cue(5.0, None);
        state.hotcues[1] = cue(28.0, None);
        state.hotcues[3] = cue(40.0, Some(3.9));
        state
    }

    fn booth_stems() -> [StemOverlay; 4] {
        let stem = |name: &str, (r, g, b), volume, muted, filter_on| StemOverlay {
            name: name.to_owned(),
            colour: Color32::from_rgb(r, g, b),
            volume,
            muted,
            filter: 0.3,
            filter_on,
        };
        [
            stem("Drums", (0xE8, 0x5D, 0x3A), 1.0, false, false),
            stem("Bass", (0xE8, 0xB8, 0x3A), 0.8, false, true),
            stem("Other", (0x3A, 0xB8, 0xE8), 0.6, false, false),
            stem("Vocals", (0xB0, 0x5D, 0xE8), 0.0, true, false),
        ]
    }

    fn every_size() -> [(usize, usize); 2] {
        [(WIDTH, HEIGHT), WIDE]
    }

    #[test]
    fn every_layout_draws_empty_loaded_and_compared_decks_at_every_size() {
        let empty = DeckState::default();
        let loaded = booth_state();
        let other = DeckState {
            position_s: 30.0,
            ..booth_state()
        };
        for size in every_size() {
            for layout in Layout::for_panel(size) {
                for state in [&empty, &loaded] {
                    for with_extras in [false, true] {
                        let d = ScreenDeck {
                            key: Some("8A"),
                            stems: with_extras.then(booth_stems),
                            partner: with_extras.then_some(Partner {
                                index: 1,
                                state: &other,
                                peaks: None,
                                key: backspn_formats::nml::MusicalKey::parse("9A"),
                            }),
                            ..deck(state, 4.0)
                        };
                        let frame = render_panel(&d, &style(), &DARK, layout, size);
                        let expected = if layout.is_wide() && size == WIDE {
                            WIDE
                        } else {
                            (WIDTH, HEIGHT)
                        };
                        assert_eq!((frame.width, frame.height), expected, "{layout:?}");
                        assert_eq!(frame.pixels.len(), expected.0 * expected.1);
                    }
                }
            }
        }
    }

    #[test]
    fn the_small_panel_lists_only_its_layouts_and_wide_ones_fall_back() {
        assert!(
            Layout::for_panel((WIDTH, HEIGHT))
                .iter()
                .all(|l| !l.is_wide())
        );
        let wide = Layout::for_panel(WIDE);
        assert_eq!(&wide[..3], &Layout::WIDE, "the wide ones first");
        assert_eq!(wide.len(), Layout::WIDE.len() + Layout::ALL.len());
        let state = booth_state();
        let d = deck(&state, 4.0);
        assert_eq!(
            render(&d, &style(), &DARK, Layout::WideDeck).pixels,
            render(&d, &style(), &DARK, Layout::Standard).pixels,
            "a wide layout on a small panel draws its small stand-in"
        );
        assert_eq!(Layout::WideMix.small(), Layout::Mix);
    }

    #[test]
    fn the_wide_deck_names_the_stems_under_the_screen() {
        let state = booth_state();
        let d = ScreenDeck {
            stems: Some(booth_stems()),
            ..deck(&state, 4.0)
        };
        let frame = render_panel(&d, &style(), &DARK, Layout::WideDeck, WIDE);
        let px = |x: usize, y: usize| frame.pixels[y * WIDE.0 + x];
        assert_eq!(
            px(4, 240),
            rgb565(Color32::from_rgb(0xE8, 0x5D, 0x3A)),
            "drums tab"
        );
        assert_ne!(
            px(364, 240),
            rgb565(Color32::from_rgb(0xB0, 0x5D, 0xE8)),
            "muted vocals dimmed"
        );
        assert_eq!(px(240, 100), rgb565(DARK.well), "the waveform's well");
    }

    #[test]
    fn the_phrase_screen_lights_the_beat_of_the_bar() {
        let state = DeckState {
            position_s: 0.1 + 2.0 * 60.0 / 123.0 + 0.01,
            ..booth_state()
        };
        let frame = render(&deck(&state, 4.0), &style(), &DARK, Layout::Phrase);
        let lit = rgb565(deck_color(0, &DARK));
        let third = 10 + 2 * ((300 - 18) / 4 + 6) + 5;
        assert_eq!(pixel(&frame, third, 50), lit, "beat 3 of the bar");
        assert_eq!(pixel(&frame, 15, 50), rgb565(DARK.well));
    }

    #[test]
    fn layout_previews() {
        let Some(dir) = std::env::var_os("BACKSPN_SCREEN_SNAPSHOT") else {
            return;
        };
        let rate = 8000;
        let samples: Vec<f32> = (0..rate * 60 * 2)
            .map(|i| (i as f32 * 0.05).sin() * ((i / 4000) % 7) as f32 / 7.0)
            .collect();
        let track = backspn_decode::Track::from_stereo(rate as u32, samples);
        let peaks = Peaks::compute(&track);
        let state = booth_state();
        let other = DeckState {
            position_s: 33.4,
            tempo: 1.0,
            ..booth_state()
        };
        let d = ScreenDeck {
            name: "Some Artist - A Rather Long Track Title",
            peaks: Some(&peaks),
            key: Some("8A"),
            key_color: Some(Color32::from_rgb(0x40, 0xC0, 0xFF)),
            synced: true,
            loop_beats: 8.0,
            stems: Some(booth_stems()),
            partner: Some(Partner {
                index: 1,
                state: &other,
                peaks: Some(&peaks),
                key: backspn_formats::nml::MusicalKey::parse("9A"),
            }),
            ..deck(&state, 8.0)
        };
        for size in every_size() {
            for layout in Layout::for_panel(size) {
                let frame = render_panel(&d, &style(), &DARK, layout, size);
                let image = image::RgbImage::from_raw(
                    frame.width as u32,
                    frame.height as u32,
                    rgb_bytes(&frame),
                )
                .unwrap();
                let name = format!("{layout:?}-{}x{}.png", size.0, size.1);
                image.save(std::path::Path::new(&dir).join(name)).unwrap();
            }
        }
    }

    #[test]
    fn rgb_bytes_widen_the_565_pixels() {
        let frame = Frame {
            width: 2,
            height: 1,
            pixels: vec![Frame::rgb565(255, 0, 0), Frame::rgb565(0, 255, 0)].into_boxed_slice(),
        };
        assert_eq!(rgb_bytes(&frame), [248, 0, 0, 0, 252, 0]);
    }

    #[test]
    fn empty_deck_is_the_theme_background_with_an_empty_well() {
        let state = DeckState::default();
        let deck = deck(&state, 4.0);
        let frame = render(&deck, &style(), &DARK, Layout::Standard);
        assert_eq!(frame.width, WIDTH);
        assert_eq!(pixel(&frame, 1, 1), rgb565(DARK.base));
        assert_eq!(pixel(&frame, 160, 62), rgb565(DARK.border), "separator");
        assert_eq!(pixel(&frame, 160, 210), rgb565(DARK.well), "no waveform");
        assert_eq!(pixel(&frame, 80, 150), rgb565(DARK.accent), "loop pad");
    }

    #[test]
    fn active_loop_lights_the_pad_green() {
        let state = DeckState {
            loop_active: true,
            ..DeckState::default()
        };
        let deck = deck(&state, 0.25);
        let frame = render(&deck, &style(), &DARK, Layout::Standard);
        assert_eq!(pixel(&frame, 80, 150), rgb565(DARK.play));
    }

    #[test]
    fn only_an_empty_deck_is_idle() {
        let empty = DeckState::default();
        assert!(idle(&deck(&empty, 4.0)));
        let loaded = DeckState {
            loaded: true,
            ..DeckState::default()
        };
        assert!(!idle(&deck(&loaded, 4.0)));
    }

    #[test]
    fn logo_screen_is_centred_on_the_theme_background() {
        let frame = render_logo(&DARK, None);
        assert_eq!((frame.width, frame.height), (WIDTH, HEIGHT));
        assert_eq!(pixel(&frame, 1, 1), rgb565(DARK.base));
        assert_ne!(
            pixel(&frame, WIDTH / 2, HEIGHT / 2 - 20),
            rgb565(DARK.base),
            "logo"
        );
    }

    #[test]
    fn pictures_fit_the_screen_keeping_their_proportions() {
        assert_eq!(fit((320, 240)), (320, 240));
        assert_eq!(fit((64, 48)), (320, 240), "small pictures grow");
        assert_eq!(fit((1000, 100)), (320, 32), "wide");
        assert_eq!(fit((100, 1000)), (24, 240), "tall");
        assert_eq!(fit((0, 10)), (0, 0));
    }

    #[test]
    fn a_custom_logo_fills_the_screen_and_a_missing_one_falls_back() {
        let dir = std::env::temp_dir().join(format!("backspn-logo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("logo.png");
        image::RgbaImage::from_pixel(32, 24, image::Rgba([255, 0, 0, 255]))
            .save(&path)
            .unwrap();
        let red = Frame::rgb565(255, 0, 0);
        let frame = render_logo(&DARK, Some(&path));
        assert_eq!(pixel(&frame, 0, 0), red);
        assert_eq!(pixel(&frame, WIDTH - 1, HEIGHT - 1), red);
        let missing = render_logo(&DARK, Some(&dir.join("missing.png")));
        assert_eq!(missing.pixels, render_logo(&DARK, None).pixels);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn loop_labels_match_the_size_buttons() {
        assert_eq!(beats_label(0.25), "1/4");
        assert_eq!(beats_label(1.0 / 32.0), "1/32");
        assert_eq!(beats_label(16.0), "16");
        assert_eq!(beats_label(3.0), "3");
    }
}
