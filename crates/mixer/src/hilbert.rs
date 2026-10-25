const RE: [f32; 3] = [0.332_671_66, 0.779_184_17, 0.946_964_2];
const IM: [f32; 3] = [0.599_935, 0.885_985_7, 0.984_576_5];
const SECTIONS: usize = 7;
const LANES: usize = 4;
const MAX_DELAY: usize = 8;

#[derive(Debug, Clone)]
pub struct Hilbert {
    coefs: [[f32; LANES]; SECTIONS],
    state: [[[f32; LANES]; MAX_DELAY]; SECTIONS],
    pos: usize,
    delay: usize,
}

impl Hilbert {
    pub fn new(a: f32, multiplier: usize) -> Self {
        let warp = |c: f32| (c + a) / (a * c + 1.0);
        let chain =
            |c: &[f32; 3], extra: f32| [c[0], c[1], c[2], -c[0], -c[1], -c[2], extra].map(warp);
        let (re, im) = (chain(&RE, 0.0), chain(&IM, 0.0));
        Self {
            coefs: std::array::from_fn(|s| [re[s], im[s], re[s], im[s]]),
            state: [[[0.0; LANES]; MAX_DELAY]; SECTIONS],
            pos: 0,
            delay: multiplier.clamp(1, MAX_DELAY),
        }
    }

    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> [f32; LANES] {
        let read = (self.pos + MAX_DELAY - self.delay) % MAX_DELAY;
        let mut x = [l, l, r, r];
        let mut re = [0.0; LANES];
        for (s, (coefs, state)) in self.coefs.iter().zip(&mut self.state).enumerate() {
            let old = state[read];
            let mut new = [0.0; LANES];
            for lane in 0..LANES {
                let v = coefs[lane] * (x[lane] - old[lane]);
                new[lane] = x[lane] + v;
                x[lane] = old[lane] + v;
            }
            state[self.pos] = new;
            if s == SECTIONS - 2 {
                re = x;
            }
        }
        self.pos = (self.pos + 1) % MAX_DELAY;
        [re[0], x[1], re[2], x[3]]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope(hz: f32, rate: f32) -> (f32, f32) {
        let mut h = Hilbert::new(-0.89, 1);
        let (mut lo, mut hi) = (f32::MAX, 0.0f32);
        for n in 0..20_000 {
            let x = 0.5 * (std::f32::consts::TAU * hz * n as f32 / rate).sin();
            let [re, im, ..] = h.process(x, x);
            if n > 10_000 {
                let e = re.hypot(im);
                lo = lo.min(e);
                hi = hi.max(e);
            }
        }
        (lo, hi)
    }

    #[test]
    fn the_envelope_is_flat_across_the_band() {
        for hz in [40.0, 200.0, 1_000.0, 5_000.0] {
            let (lo, hi) = envelope(hz, 48_000.0);
            assert!(lo > 0.48 && hi < 0.52, "{hz} Hz: {lo}…{hi}");
        }
    }
}
