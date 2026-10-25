use backspn_dsp::Smoothed;

use crate::{ChannelFilter, FilterModel};

pub const STEMS: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StemControls {
    pub volume: f32,
    pub mute: bool,
    pub filter_on: bool,
    pub filter: f32,
    pub fx_on: bool,
    pub fx_send: f32,
}

impl Default for StemControls {
    fn default() -> Self {
        Self {
            volume: 1.0,
            mute: false,
            filter_on: false,
            filter: 0.5,
            fx_on: true,
            fx_send: 1.0,
        }
    }
}

impl StemControls {
    pub fn route_gains(&self) -> (f32, f32) {
        if self.mute {
            return (0.0, 0.0);
        }
        if self.fx_on {
            (
                self.volume * self.fx_send,
                self.volume * (1.0 - self.fx_send),
            )
        } else {
            (0.0, self.volume)
        }
    }

    fn filter_knob(&self) -> f32 {
        if self.filter_on {
            2.0 * self.filter.clamp(0.0, 1.0) - 1.0
        } else {
            0.0
        }
    }
}

#[derive(Debug, Clone)]
pub struct StemStrip {
    rate: f64,
    controls: [StemControls; STEMS],
    filters: [ChannelFilter; STEMS],
    insert: [Smoothed; STEMS],
    bypass: [Smoothed; STEMS],
}

impl StemStrip {
    pub fn new(rate: f64) -> Self {
        let (insert, bypass) = StemControls::default().route_gains();
        let mut strip = Self {
            rate,
            controls: [StemControls::default(); STEMS],
            filters: std::array::from_fn(|_| ChannelFilter::new(rate)),
            insert: [Smoothed::new(insert, 0.01, rate); STEMS],
            bypass: [Smoothed::new(bypass, 0.01, rate); STEMS],
        };
        for stem in 0..STEMS {
            strip.apply(stem);
        }
        strip
    }

    pub fn controls(&self) -> [StemControls; STEMS] {
        self.controls
    }

    pub fn set(&mut self, stem: usize, controls: StemControls) {
        if stem < STEMS {
            self.controls[stem] = controls;
            self.apply(stem);
        }
    }

    fn apply(&mut self, stem: usize) {
        let c = self.controls[stem];
        let (insert, bypass) = c.route_gains();
        self.insert[stem].set(insert);
        self.bypass[stem].set(bypass);
        self.filters[stem].set(c.filter_knob());
    }

    pub fn process(
        &mut self,
        stems: &mut [&mut [f32]],
        insert: &mut [f32],
        bypass: &mut [f32],
        (model, resonance): (FilterModel, f32),
    ) {
        insert.fill(0.0);
        bypass.fill(0.0);
        for (k, stem) in stems.iter_mut().enumerate().take(STEMS) {
            self.filters[k].process(stem, model, resonance, self.rate);
            let (frames, _) = stem.as_chunks::<2>();
            let (ins, _) = insert.as_chunks_mut::<2>();
            let (byp, _) = bypass.as_chunks_mut::<2>();
            for ((x, i), b) in frames.iter().zip(ins).zip(byp) {
                let gi = self.insert[k].tick();
                let gb = self.bypass[k].tick();
                for ch in 0..2 {
                    i[ch] += x[ch] * gi;
                    b[ch] += x[ch] * gb;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_are_right() {
        let c = StemControls {
            volume: 0.8,
            fx_on: false,
            ..StemControls::default()
        };
        assert_eq!(c.route_gains(), (0.0, 0.8), "no FX: all past the effects");
        let fx = StemControls {
            fx_on: true,
            fx_send: 0.25,
            ..c
        };
        let (i, b) = fx.route_gains();
        assert!((i - 0.2).abs() < 1e-6 && (b - 0.6).abs() < 1e-6);
        let muted = StemControls { mute: true, ..fx };
        assert_eq!(muted.route_gains(), (0.0, 0.0));
    }

    #[test]
    fn stems_sum_into_the_two_paths() {
        let rate = 1_000.0;
        let mut strip = StemStrip::new(rate);
        strip.set(
            1,
            StemControls {
                fx_on: true,
                fx_send: 0.5,
                ..StemControls::default()
            },
        );
        strip.set(
            3,
            StemControls {
                mute: true,
                ..StemControls::default()
            },
        );
        let frames = 200;
        let mut stems: Vec<Vec<f32>> = (0..STEMS)
            .map(|k| vec![(k + 1) as f32; 2 * frames])
            .collect();
        let mut views: Vec<&mut [f32]> = stems.iter_mut().map(Vec::as_mut_slice).collect();
        let (mut insert, mut bypass) = (vec![0.0; 2 * frames], vec![0.0; 2 * frames]);
        strip.process(
            &mut views,
            &mut insert,
            &mut bypass,
            (FilterModel::Standard, 0.0),
        );
        let last = 2 * frames - 1;
        assert!(
            (insert[last] - (1.0 + 1.0 + 3.0)).abs() < 1e-3,
            "{}",
            insert[last]
        );
        assert!((bypass[last] - 1.0).abs() < 1e-3, "{}", bypass[last]);
    }

    #[test]
    fn an_off_filter_leaves_the_stem_alone() {
        let c = StemControls {
            filter: 0.1,
            ..StemControls::default()
        };
        assert!(c.filter_knob().abs() < f32::EPSILON);
        let on = StemControls {
            filter_on: true,
            ..c
        };
        assert!((on.filter_knob() + 0.8).abs() < 1e-6);
    }
}
