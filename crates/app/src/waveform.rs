use std::f32::consts::TAU;

use backspn_decode::Track;
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Ui, pos2};

use serde::{Deserialize, Serialize};

use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum WaveMode {
    #[default]
    ThreeBand,
    Spectrum,
    Mono,
}

impl WaveMode {
    pub const ALL: [Self; 3] = [Self::ThreeBand, Self::Spectrum, Self::Mono];

    pub fn name(self) -> &'static str {
        match self {
            Self::ThreeBand => "3 bands",
            Self::Spectrum => "Spectrum",
            Self::Mono => "Mono",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum WavePalette {
    #[default]
    Tricolor,
    Spectrum,
    Ultraviolet,
    Infrared,
    XRay,
    Rgb,
    Fire,
    Ocean,
    Lime,
    Sunset,
    Neon,
    Violet,
    Aurora,
    Ice,
    Vintage,
    Candy,
    Gold,
    Grey,
    Theme,
    Custom,
}

impl WavePalette {
    pub const ALL: [Self; 20] = [
        Self::Spectrum,
        Self::Ultraviolet,
        Self::Infrared,
        Self::XRay,
        Self::Tricolor,
        Self::Rgb,
        Self::Fire,
        Self::Ocean,
        Self::Lime,
        Self::Sunset,
        Self::Neon,
        Self::Violet,
        Self::Aurora,
        Self::Ice,
        Self::Vintage,
        Self::Candy,
        Self::Gold,
        Self::Grey,
        Self::Theme,
        Self::Custom,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Tricolor => "Tricolor",
            Self::Spectrum => "Spectrum",
            Self::Ultraviolet => "Ultraviolet",
            Self::Infrared => "Infrared",
            Self::XRay => "X-Ray",
            Self::Rgb => "RGB",
            Self::Fire => "Fire",
            Self::Ocean => "Ocean",
            Self::Lime => "Lime",
            Self::Sunset => "Sunset",
            Self::Neon => "Neon",
            Self::Violet => "Violet",
            Self::Aurora => "Aurora",
            Self::Ice => "Ice",
            Self::Vintage => "Vintage",
            Self::Candy => "Candy",
            Self::Gold => "Gold",
            Self::Grey => "Grey",
            Self::Theme => "Theme",
            Self::Custom => "Custom",
        }
    }

    #[allow(clippy::unreadable_literal)]
    pub fn colors(self, theme: &Theme, custom: [[u8; 3]; 3]) -> [Color32; 3] {
        let rgb = |c: u32| Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8);
        let hex = |[l, m, h]: [u32; 3]| [rgb(l), rgb(m), rgb(h)];
        match self {
            Self::Tricolor => hex([0x2D6CFF, 0xFF9A2E, 0xF4F4F4]),
            Self::Spectrum => hex([0xFF3D14, 0xE83CF0, 0x34E3C4]),
            Self::Ultraviolet => hex([0x3A2BD8, 0x9160FF, 0xEAE4FF]),
            Self::Infrared => hex([0xB0000E, 0xFF6A00, 0xFFE34D]),
            Self::XRay => hex([0x3E4652, 0x9AA4B2, 0xFFFFFF]),
            Self::Rgb => hex([0xE5383B, 0x3FD15B, 0x4C8DFF]),
            Self::Fire => hex([0xC81D25, 0xFF7B1C, 0xFFE9A8]),
            Self::Ocean => hex([0x1749C7, 0x18C4D8, 0xE3FBFF]),
            Self::Lime => hex([0x168A3A, 0x9BE22D, 0xF2FFE6]),
            Self::Sunset => hex([0x6A2C9C, 0xF0508C, 0xFFC94A]),
            Self::Neon => hex([0xD62AD0, 0x1FE0F0, 0xFFFFFF]),
            Self::Violet => hex([0x5B2BD9, 0xA68BFF, 0xF1ECFF]),
            Self::Aurora => hex([0x0E8C88, 0x8A5CF6, 0xB8FFE2]),
            Self::Ice => hex([0x123A7A, 0x5AB4F5, 0xF2FAFF]),
            Self::Vintage => hex([0x7A4A26, 0xE0A040, 0xFFF1D2]),
            Self::Candy => hex([0xF0508C, 0x5FE3B5, 0xFFFFFF]),
            Self::Gold => hex([0x9A6B12, 0xE8B83A, 0xFFF4CF]),
            Self::Grey => hex([0x5A6170, 0xA4ABB8, 0xF0F2F5]),
            Self::Theme => theme.wave,
            Self::Custom => custom.map(|[r, g, b]| Color32::from_rgb(r, g, b)),
        }
    }
}

fn style_id() -> egui::Id {
    egui::Id::unique("backspn-wave-style")
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaveStyle {
    pub mode: WaveMode,
    pub colors: [Color32; 3],
    pub direct: bool,
}

impl WaveStyle {
    pub fn get(ctx: &egui::Context) -> Self {
        ctx.data(|d| d.get_temp(style_id()))
            .unwrap_or_else(|| Self {
                mode: WaveMode::default(),
                colors: WavePalette::default().colors(&crate::theme::get(ctx), [[0; 3]; 3]),
                direct: false,
            })
    }

    pub fn set(self, ctx: &egui::Context) {
        ctx.data_mut(|d| d.insert_temp(style_id(), self));
    }
}

const LOW_HZ: f32 = 200.0;
const HIGH_HZ: f32 = 2_500.0;

pub const OVERVIEW_COLUMNS: usize = 400;

pub const OVERVIEW_STRIDE: usize = 7;
const OLD_OVERVIEW_STRIDE: usize = 4;

pub const FRAMES_PER_BIN: usize = 256;

#[derive(Debug, Clone, Default)]
pub struct Peaks {
    bins: Vec<(f32, f32)>,
    bands: Vec<[f32; 3]>,
    band_max: Vec<[f32; 3]>,
    bin_s: f64,
    pub density: crate::density::Density,
}

impl Peaks {
    pub fn compute(track: &Track) -> Self {
        let rate = track.sample_rate() as f32;
        let coeff = |hz: f32| 1.0 - (-TAU * hz / rate).exp();
        let (a_low, a_high) = (coeff(LOW_HZ), coeff(HIGH_HZ));
        let (mut low1, mut low2, mut high1, mut high2) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        let mut bins = Vec::new();
        let mut bands = Vec::new();
        let mut band_max = Vec::new();
        for chunk in track.samples().chunks(FRAMES_PER_BIN * 2) {
            let (mut lo, mut hi) = (0.0f32, 0.0f32);
            let mut energy = [0.0f32; 3];
            let mut peak = [0.0f32; 3];
            let frames = chunk.as_chunks::<2>().0;
            for &[l, r] in frames {
                let s = l.midpoint(r);
                lo = lo.min(s);
                hi = hi.max(s);
                low1 += a_low * (s - low1);
                low2 += a_low * (low1 - low2);
                high1 += a_high * (s - high1);
                high2 += a_high * (high1 - high2);
                let split = [low2.abs(), (high2 - low2).abs(), (s - high2).abs()];
                for b in 0..3 {
                    energy[b] += split[b];
                    peak[b] = peak[b].max(split[b]);
                }
            }
            let n = frames.len().max(1) as f32;
            bins.push((lo, hi));
            bands.push(energy.map(|e| e / n));
            band_max.push(peak);
        }
        let bin_s = FRAMES_PER_BIN as f64 / f64::from(track.sample_rate());
        Self {
            bins,
            bands,
            band_max,
            bin_s,
            density: crate::density::Density::compute(track),
        }
    }

    pub fn overview(&self, columns: usize) -> Vec<u8> {
        let duration = self.bins.len() as f64 * self.bin_s;
        if duration <= 0.0 {
            return Vec::new();
        }
        let level = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        let step = duration / columns as f64;
        (0..columns)
            .flat_map(|c| {
                let (t0, t1) = (c as f64 * step, (c + 1) as f64 * step);
                let (lo, hi) = self.range(t0, t1);
                let [l, m, h] = self.band_peaks(t0, t1);
                let [ml, mm, mh] = self.density.mean_bands(t0, t1);
                [
                    level(hi.max(-lo)),
                    level(l),
                    level(m),
                    level(h),
                    level(ml),
                    level(mm),
                    level(mh),
                ]
            })
            .collect()
    }

    fn bin_range(&self, from_s: f64, to_s: f64) -> Option<std::ops::Range<usize>> {
        if self.bins.is_empty() || to_s <= 0.0 {
            return None;
        }
        let start = ((from_s.max(0.0) / self.bin_s) as usize).min(self.bins.len());
        let end = ((to_s / self.bin_s).ceil() as usize)
            .max(1)
            .clamp(start, self.bins.len());
        Some(start..end)
    }

    fn bands(&self, from_s: f64, to_s: f64) -> [f32; 3] {
        self.bin_range(from_s, to_s).map_or([0.0; 3], |r| {
            self.bands[r].iter().fold([0.0f32; 3], |acc, b| {
                [acc[0].max(b[0]), acc[1].max(b[1]), acc[2].max(b[2])]
            })
        })
    }

    pub fn band_peaks(&self, from_s: f64, to_s: f64) -> [f32; 3] {
        self.bin_range(from_s, to_s).map_or([0.0; 3], |r| {
            self.band_max[r].iter().fold([0.0f32; 3], |acc, b| {
                [acc[0].max(b[0]), acc[1].max(b[1]), acc[2].max(b[2])]
            })
        })
    }

    fn range(&self, from_s: f64, to_s: f64) -> (f32, f32) {
        self.bin_range(from_s, to_s).map_or((0.0, 0.0), |r| {
            self.bins[r]
                .iter()
                .fold((0.0, 0.0), |(lo, hi), &(a, b)| (lo.min(a), hi.max(b)))
        })
    }
}

pub fn spectral_color(colors: [Color32; 3], [low, mid, high]: [f32; 3]) -> Color32 {
    let split = low + mid + 0.01;
    let weights = [low / split, mid / split, high * high];
    let mut rgb = [0.0f32; 3];
    for (w, c) in weights.iter().zip(colors) {
        for (channel, v) in rgb.iter_mut().zip([c.r(), c.g(), c.b()]) {
            *channel += w * f32::from(v);
        }
    }
    let [r, g, b] = rgb.map(|v| v.min(255.0) as u8);
    Color32::from_rgb(r, g, b)
}

pub const PLAYED_DIM: f32 = 0.45;
const BAND_SCALE: [f32; 3] = [1.0, 0.8, 0.6];

pub fn column(peaks: &Peaks, style: &WaveStyle, t0: f64, t1: f64) -> Vec<(f32, f32, Color32)> {
    let (lo, hi) = peaks.range(t0, t1);
    layers(
        style,
        peaks.band_peaks(t0, t1),
        peaks.bands(t0, t1),
        (lo, hi),
    )
}

fn layers(
    style: &WaveStyle,
    [low, mids, high]: [f32; 3],
    mean_bands: [f32; 3],
    (lo, hi): (f32, f32),
) -> Vec<(f32, f32, Color32)> {
    match style.mode {
        WaveMode::ThreeBand => {
            let mut layers: [(f32, Color32); 3] = std::array::from_fn(|b| {
                let level = [low, mids, high][b];
                ((level * BAND_SCALE[b]).min(1.0), style.colors[b])
            });
            layers.sort_by(|a, b| b.0.total_cmp(&a.0));
            layers
                .into_iter()
                .filter(|(h, _)| *h > 0.002)
                .map(|(h, c)| (h, -h, c))
                .collect()
        }
        WaveMode::Spectrum | WaveMode::Mono => {
            if hi <= lo {
                return Vec::new();
            }
            let c = if style.mode == WaveMode::Mono {
                style.colors[1]
            } else {
                let colour = spectral_color(style.colors, mean_bands);
                colour.gamma_multiply((hi.max(-lo) * 1.2).clamp(0.3, 1.0))
            };
            vec![(hi, lo, c)]
        }
    }
}

pub fn paint(
    painter: &egui::Painter,
    rect: Rect,
    peaks: &Peaks,
    (from_s, to_s): (f64, f64),
    position_s: f64,
    style: &WaveStyle,
) {
    let width = rect.width().max(1.0);
    let seconds_per_px = (to_s - from_s) / f64::from(width);
    let mid = rect.center().y;
    let half = rect.height() * 0.48;
    let mut x = rect.left();
    while x < rect.right() {
        let t0 = from_s + f64::from(x - rect.left()) * seconds_per_px;
        let t1 = t0 + seconds_per_px;
        for (top, bottom, c) in column(peaks, style, t0, t1) {
            let c = if t0 < position_s {
                c.gamma_multiply(PLAYED_DIM)
            } else {
                c
            };
            painter.line_segment(
                [pos2(x, mid - top * half), pos2(x, mid - bottom * half)],
                Stroke::new(1.0, c),
            );
        }
        x += 1.0;
    }
}

pub fn paint_overview(
    painter: &egui::Painter,
    rect: Rect,
    overview: &[u8],
    played: Option<f32>,
    style: &WaveStyle,
) {
    let columns = overview.len() / overview_stride(overview);
    if columns == 0 {
        return;
    }
    let mid = rect.center().y;
    let half = rect.height() * 0.48;
    let width = rect.width().max(1.0);
    let mut x = rect.left();
    while x < rect.right() {
        let f = (x - rect.left()) / width;
        let c = ((f * columns as f32) as usize).min(columns - 1);
        for (top, bottom, colour) in overview_column(overview, c, style) {
            let colour = if played.is_some_and(|p| f < p) {
                colour.gamma_multiply(PLAYED_DIM)
            } else {
                colour
            };
            painter.line_segment(
                [pos2(x, mid - top * half), pos2(x, mid - bottom * half)],
                Stroke::new(1.0, colour),
            );
        }
        x += 1.0;
    }
}

fn overview_stride(overview: &[u8]) -> usize {
    if overview.len() == OVERVIEW_COLUMNS * OLD_OVERVIEW_STRIDE {
        OLD_OVERVIEW_STRIDE
    } else {
        OVERVIEW_STRIDE
    }
}

fn overview_column(overview: &[u8], c: usize, style: &WaveStyle) -> Vec<(f32, f32, Color32)> {
    let stride = overview_stride(overview);
    let at = |i: usize| f32::from(overview[stride * c + i]) / 255.0;
    let amp = at(0);
    let peaks = [at(1), at(2), at(3)];
    if stride == OLD_OVERVIEW_STRIDE {
        return layers(style, peaks, peaks, (-amp, amp));
    }
    let means = [at(4), at(5), at(6)];
    if style.mode != WaveMode::Spectrum {
        return layers(style, peaks, means, (-amp, amp));
    }
    if amp <= 0.0 {
        return Vec::new();
    }
    vec![(
        amp,
        -amp,
        crate::density::band_colour(means, style.colors, style.direct),
    )]
}

pub fn density_image(
    ui: &Ui,
    rect: Rect,
    peaks: &Peaks,
    window: (f64, f64),
    style: &WaveStyle,
) -> egui::ColorImage {
    let width = (rect.width() * ui.ctx().pixels_per_point())
        .round()
        .max(1.0) as usize;
    peaks
        .density
        .image(window, width, style.colors, style.direct)
}

pub fn paint_texture(painter: &egui::Painter, rect: Rect, texture: &egui::TextureHandle) {
    painter.image(
        texture.id(),
        rect,
        Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );
}

pub fn dim_played(painter: &egui::Painter, rect: Rect, window: (f64, f64), position_s: f64) {
    let x = x_of(rect, window, position_s).clamp(rect.left(), rect.right());
    if x > rect.left() {
        painter.rect_filled(
            Rect::from_min_max(rect.min, egui::pos2(x, rect.bottom())),
            0.0,
            egui::Color32::from_black_alpha(115),
        );
    }
}

pub fn x_of(rect: Rect, (from_s, to_s): (f64, f64), t_s: f64) -> f32 {
    rect.left() + ((t_s - from_s) / (to_s - from_s)) as f32 * rect.width()
}

pub fn time_at(rect: Rect, (from_s, to_s): (f64, f64), pos: Pos2) -> f64 {
    let frac = f64::from((pos.x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0);
    from_s + frac * (to_s - from_s)
}

pub fn marker(painter: &egui::Painter, rect: Rect, x: f32, color: Color32, width: f32) {
    if x >= rect.left() && x <= rect.right() {
        painter.line_segment(
            [pos2(x, rect.top()), pos2(x, rect.bottom())],
            Stroke::new(width, color),
        );
    }
}

pub fn cue_mark(painter: &egui::Painter, rect: Rect, window: (f64, f64), t_s: f64, color: Color32) {
    let x = x_of(rect, window, t_s);
    if x < rect.left() || x > rect.right() {
        return;
    }
    marker(painter, rect, x, color, 1.0);
    painter.rect_filled(
        Rect::from_min_size(pos2(x, rect.top()), egui::vec2(5.0, 5.0)),
        0.0,
        color,
    );
}

pub fn loop_region(
    painter: &egui::Painter,
    rect: Rect,
    window: (f64, f64),
    (start_s, end_s): (f64, f64),
    active: bool,
) {
    if let Some(span) = loop_span(rect, window, (start_s, end_s)) {
        let alpha = if active { 60 } else { 25 };
        painter.rect_filled(
            span,
            0.0,
            Color32::from_rgba_unmultiplied(90, 220, 120, alpha),
        );
    }
}

pub fn loop_span(rect: Rect, window: (f64, f64), (start_s, end_s): (f64, f64)) -> Option<Rect> {
    let x0 = x_of(rect, window, start_s).max(rect.left());
    let x1 = x_of(rect, window, end_s).min(rect.right());
    (x1 > x0).then(|| Rect::from_min_max(pos2(x0, rect.top()), pos2(x1, rect.bottom())))
}

pub fn hotcue(
    painter: &egui::Painter,
    rect: Rect,
    window: (f64, f64),
    t_s: f64,
    number: usize,
    color: Color32,
) {
    let x = x_of(rect, window, t_s);
    if x < rect.left() || x > rect.right() {
        return;
    }
    marker(painter, rect, x, color, 1.5);
    let tag = Rect::from_min_size(pos2(x, rect.top()), egui::vec2(11.0, 11.0));
    painter.rect_filled(tag, 0.0, color);
    painter.text(
        tag.center(),
        egui::Align2::CENTER_CENTER,
        number.to_string(),
        egui::FontId::monospace(9.0),
        Color32::BLACK,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_loop_span_is_cut_to_what_shows() {
        let rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 10.0));
        let span = loop_span(rect, (0.0, 10.0), (2.0, 4.0)).unwrap();
        assert_eq!((span.left(), span.right()), (20.0, 40.0));
        let cut = loop_span(rect, (0.0, 10.0), (8.0, 14.0)).unwrap();
        assert_eq!((cut.left(), cut.right()), (80.0, 100.0));
        assert!(loop_span(rect, (0.0, 10.0), (12.0, 14.0)).is_none());
    }

    #[test]
    fn overview_follows_the_track() {
        let rate = 44_100usize;
        let samples: Vec<f32> = (0..2 * rate)
            .flat_map(|i| {
                let s = if i < rate {
                    0.0
                } else {
                    (TAU * 60.0 * i as f32 / rate as f32).sin() * 0.8
                };
                [s, s]
            })
            .collect();
        let peaks = Peaks::compute(&Track::from_stereo(rate as u32, samples));
        let o = peaks.overview(10);
        assert_eq!(o.len(), 10 * OVERVIEW_STRIDE);
        assert_eq!(o[0], 0, "silent start");
        let end = OVERVIEW_STRIDE * 8;
        assert!(o[end] > 150, "loud end: {}", o[end]);
        assert!(o[end + 1] > o[end + 3], "low band over high");
        assert!(
            o[end + 4] <= o[end + 1],
            "a band's mean is not over its peak"
        );

        for direct in [true, false] {
            let style = WaveStyle {
                mode: WaveMode::Spectrum,
                colors: [
                    Color32::from_rgb(200, 40, 40),
                    Color32::from_rgb(40, 200, 40),
                    Color32::from_rgb(40, 40, 200),
                ],
                direct,
            };
            let step = peaks.bins.len() as f64 * peaks.bin_s / 10.0;
            for c in 8..10 {
                let window = (c as f64 * step, (c + 1) as f64 * step);
                let image = peaks.density.image(window, 1, style.colors, style.direct);
                let deck = image
                    .pixels
                    .iter()
                    .max_by_key(|p| p.a())
                    .map(|p| {
                        let [r, g, b, _] = p.to_srgba_unmultiplied();
                        [r, g, b]
                    })
                    .unwrap();
                let small = overview_column(&o, c, &style)[0].2;
                let small = [small.r(), small.g(), small.b()];
                assert!(
                    deck.iter().zip(small).all(|(x, y)| x.abs_diff(y) <= 24),
                    "column {c}, direct {direct}: deck {deck:?}, list {small:?}"
                );
            }
        }
        assert_eq!(Peaks::default().overview(10), Vec::<u8>::new());
    }

    #[test]
    fn bands_follow_the_spectrum() {
        let rate = 44_100;
        let tone = |hz: f32| -> Vec<f32> {
            (0..rate)
                .flat_map(|i| {
                    let s = (TAU * hz * i as f32 / rate as f32).sin() * 0.5;
                    [s, s]
                })
                .collect()
        };
        let low = Peaks::compute(&Track::from_stereo(rate as u32, tone(60.0)));
        let high = Peaks::compute(&Track::from_stereo(rate as u32, tone(8_000.0)));
        let b = low.bands(0.2, 0.8);
        assert!(b[0] > b[1] && b[0] > b[2], "60 Hz is low: {b:?}");
        let b = high.bands(0.2, 0.8);
        assert!(b[2] > b[0] && b[2] > b[1], "8 kHz is high: {b:?}");
        let p = low.band_peaks(0.2, 0.8);
        assert!(
            p[0] > 0.4 && p[1] < 0.6 * p[0] && p[2] < 0.05,
            "60 Hz peaks: {p:?}"
        );
        let p = high.band_peaks(0.2, 0.8);
        assert!(p[2] > 0.3 && p[0] < 0.05, "8 kHz peaks: {p:?}");
        let t = crate::theme::DARK;
        let low = spectral_color(t.wave, [1.0, 0.0, 0.0]);
        let diff = |a: Color32, b: Color32| {
            (0..3)
                .map(|i| a.to_array()[i].abs_diff(b.to_array()[i]))
                .max()
                .unwrap()
        };
        assert!(diff(low, t.wave[0]) <= 3, "{low:?}");
        assert_eq!(spectral_color(t.wave, [0.0, 0.0, 1.0]), t.wave[2]);
        let half = spectral_color(t.wave, [0.5, 0.5, 0.0]);
        assert!(diff(half, t.wave[0].lerp_to_gamma(t.wave[1], 0.5)) <= 40);
    }

    #[test]
    fn palettes_give_three_colours_and_custom_uses_settings() {
        let t = crate::theme::DARK;
        let custom = [[1, 2, 3], [4, 5, 6], [7, 8, 9]];
        let mut seen = std::collections::HashSet::new();
        for p in WavePalette::ALL {
            let c = p.colors(&t, custom);
            assert!(c[0] != c[1] && c[1] != c[2], "{}", p.name());
            if p != WavePalette::Theme && p != WavePalette::Custom {
                assert!(seen.insert(c.map(|c| c.to_array())), "{} repeats", p.name());
            }
        }
        assert_eq!(
            WavePalette::Custom.colors(&t, custom)[2],
            Color32::from_rgb(7, 8, 9)
        );
        assert_eq!(WavePalette::Theme.colors(&t, custom), t.wave);
    }

    #[test]
    fn envelope_of_known_signal() {
        let mut samples = vec![0.0; FRAMES_PER_BIN * 2 * 3];
        samples[0] = 0.8;
        samples[FRAMES_PER_BIN * 2 * 2 + 1] = -0.6;
        let peaks = Peaks::compute(&Track::from_stereo(256, samples));
        assert_eq!(peaks.bins.len(), 3);
        assert_eq!(peaks.range(0.0, 1.0), (0.0, 0.4));
        assert_eq!(peaks.range(1.0, 2.0), (0.0, 0.0));
        assert_eq!(peaks.range(0.0, 3.0), (-0.3, 0.4));
        assert_eq!(peaks.range(5.0, 6.0), (0.0, 0.0));
        assert_eq!(peaks.range(-2.0, -1.0), (0.0, 0.0));
    }
}
