use std::f64::consts::{FRAC_PI_2, LN_2, TAU};

const RAMP_S: f64 = 0.1;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Kind {
    LowShelf,
    Peak,
    WidePeak,
    HighShelf,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Params {
    pub w0: f64,
    pub shape: f64,
    pub db: f64,
}

impl Params {
    pub fn new(hz: f64, shape: f64, db: f64, rate: f64) -> Self {
        let hz = hz.clamp(4.07e-5 * rate, 0.47 * rate);
        Self {
            w0: TAU * hz / rate,
            shape,
            db,
        }
    }
}

pub(crate) fn coefficients(kind: Kind, p: Params) -> [f64; 5] {
    let a = 10f64.powf(p.db / 40.0);
    let (sin, cos) = p.w0.sin_cos();
    let (b, a_) = match kind {
        Kind::Peak | Kind::WidePeak => {
            let x = LN_2 / 2.0 * p.shape * p.w0 / sin;
            let x = if kind == Kind::Peak {
                x.min(FRAC_PI_2)
            } else {
                x
            };
            let alpha = sin * x.sinh();
            (
                [1.0 + alpha * a, -2.0 * cos, 1.0 - alpha * a],
                [1.0 + alpha / a, -2.0 * cos, 1.0 - alpha / a],
            )
        }
        Kind::LowShelf | Kind::HighShelf => {
            let alpha = sin / 2.0 * ((a + 1.0 / a) * (p.shape - 1.0) + 2.0).sqrt();
            let k = 2.0 * a.sqrt() * alpha;
            let (p1, m1) = (a + 1.0, a - 1.0);
            if kind == Kind::LowShelf {
                (
                    [
                        a * (p1 - m1 * cos + k),
                        2.0 * a * (m1 - p1 * cos),
                        a * (p1 - m1 * cos - k),
                    ],
                    [p1 + m1 * cos + k, -2.0 * (m1 + p1 * cos), p1 + m1 * cos - k],
                )
            } else {
                (
                    [
                        a * (p1 + m1 * cos + k),
                        -2.0 * a * (m1 + p1 * cos),
                        a * (p1 + m1 * cos - k),
                    ],
                    [p1 - m1 * cos + k, 2.0 * (m1 - p1 * cos), p1 - m1 * cos - k],
                )
            }
        }
    };
    let a0 = a_[0];
    [b[0] / a0, b[1] / a0, b[2] / a0, a_[1] / a0, a_[2] / a0]
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Band {
    kind: Kind,
    pub current: Params,
    start: Params,
    target: Params,
    t: f64,
    step: f64,
    coefs: [f64; 5],
    state: [[f64; 4]; 2],
}

impl Band {
    pub fn new(kind: Kind, params: Params, rate: f64) -> Self {
        Self {
            kind,
            current: params,
            start: params,
            target: params,
            t: 1.0,
            step: 1.0 / (RAMP_S * rate),
            coefs: coefficients(kind, params),
            state: [[0.0; 4]; 2],
        }
    }

    pub fn set(&mut self, target: Params, immediate: bool) {
        if immediate {
            self.current = target;
            self.coefs = coefficients(self.kind, target);
            self.t = 1.0;
        } else if target != self.target {
            self.start = self.current;
            self.t = 0.0;
        }
        self.target = target;
    }

    #[inline]
    pub fn advance(&mut self) {
        if self.t < 1.0 {
            self.t = (self.t + self.step).min(1.0);
            let lerp = |a: f64, b: f64| a + (b - a) * self.t;
            self.current = Params {
                w0: lerp(self.start.w0, self.target.w0),
                shape: lerp(self.start.shape, self.target.shape),
                db: lerp(self.start.db, self.target.db),
            };
            self.coefs = coefficients(self.kind, self.current);
        }
    }

    #[inline]
    pub fn process(&mut self, ch: usize, x: f64) -> f64 {
        let [b0, b1, b2, a1, a2] = self.coefs;
        let s = &mut self.state[ch];
        let y = b0 * x + b1 * s[0] + b2 * s[1] - a1 * s[2] - a2 * s[3];
        *s = [x, s[0], y, s[2]];
        y
    }

    pub fn reset(&mut self) {
        self.state = [[0.0; 4]; 2];
    }
}

#[cfg(test)]
pub(crate) fn response_db(coefs: [f64; 5], hz: f64, rate: f64) -> f64 {
    let [b0, b1, b2, a1, a2] = coefs;
    let w = TAU * hz / rate;
    let z = |c0: f64, c1: f64, c2: f64| {
        let re = c0 + c1 * w.cos() + c2 * (2.0 * w).cos();
        let im = -c1 * w.sin() - c2 * (2.0 * w).sin();
        re.hypot(im)
    };
    20.0 * (z(b0, b1, b2) / z(1.0, a1, a2)).log10()
}
