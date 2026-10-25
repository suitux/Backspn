use std::f32::consts::PI;

pub fn eq_gain(knob: f32) -> f32 {
    let knob = knob.clamp(0.0, 1.0);
    if knob <= 0.5 {
        (knob / 0.5).powi(2)
    } else {
        10f32.powf(6.0 * (knob - 0.5) / 0.5 / 20.0)
    }
}

pub fn fader_gain(position: f32) -> f32 {
    position.clamp(0.0, 1.0).powi(2)
}

pub fn balance_gains(balance: f32) -> [f32; 2] {
    let b = balance.clamp(-1.0, 1.0);
    if b < 0.0 {
        [1.0 - 0.5 * b, 1.0 + b]
    } else {
        [1.0 - b, 1.0 + 0.5 * b]
    }
}

pub fn crossfader_gains(p: f32, curve: f32) -> (f32, f32) {
    let p = p.clamp(0.0, 1.0);
    let zone = 0.5 - curve.clamp(0.0, 1.0) / 2.0;
    let fade = |x: f32| {
        if x <= 0.0 {
            0.0
        } else if x >= zone {
            1.0
        } else {
            0.5 - 0.5 * (PI * x / zone).cos()
        }
    };
    if p < 0.5 {
        (1.0, fade(p))
    } else {
        (fade(1.0 - p), 1.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FilterSetting {
    Off,
    LowPass(f64),
    HighPass(f64),
}

const FILTER_DEADZONE: f32 = 0.02;
const LP_TOP_HZ: f64 = 20_000.0;
const LP_BOTTOM_HZ: f64 = 60.0;
const HP_BOTTOM_HZ: f64 = 20.0;
const HP_TOP_HZ: f64 = 15_000.0;

pub fn filter_setting(knob: f32) -> FilterSetting {
    let knob = knob.clamp(-1.0, 1.0);
    if knob.abs() < FILTER_DEADZONE {
        return FilterSetting::Off;
    }
    let amount = f64::from((knob.abs() - FILTER_DEADZONE) / (1.0 - FILTER_DEADZONE));
    if knob < 0.0 {
        FilterSetting::LowPass(LP_TOP_HZ * (LP_BOTTOM_HZ / LP_TOP_HZ).powf(amount))
    } else {
        FilterSetting::HighPass(HP_BOTTOM_HZ * (HP_TOP_HZ / HP_BOTTOM_HZ).powf(amount))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eq_law() {
        assert_eq!(eq_gain(0.0), 0.0);
        assert!((eq_gain(0.5) - 1.0).abs() < 1e-6);
        assert!((20.0 * eq_gain(1.0).log10() - 6.0).abs() < 1e-4);
        assert!(eq_gain(0.25) < eq_gain(0.4));
    }

    #[test]
    fn fader_law() {
        assert_eq!(fader_gain(0.0), 0.0);
        assert!((20.0 * fader_gain(0.5).log10() + 12.04).abs() < 0.01);
        assert_eq!(fader_gain(1.0), 1.0);
    }

    fn close(a: (f32, f32), b: (f32, f32)) -> bool {
        (a.0 - b.0).abs() < 1e-5 && (a.1 - b.1).abs() < 1e-5
    }

    #[test]
    fn crossfader_curves_match_the_samples() {
        for (p, c, want) in [
            (0.0, 0.0, (1.0, 0.0)),
            (0.25, 0.0, (1.0, 0.5)),
            (0.5, 0.0, (1.0, 1.0)),
            (0.75, 0.0, (0.5, 1.0)),
            (1.0, 0.0, (0.0, 1.0)),
            (0.0, 0.5, (1.0, 0.0)),
            (0.125, 0.5, (1.0, 0.5)),
            (0.25, 0.5, (1.0, 1.0)),
            (1.0, 0.5, (0.0, 1.0)),
            (0.0, 1.0, (1.0, 0.0)),
            (0.01, 1.0, (1.0, 1.0)),
            (1.0, 1.0, (0.0, 1.0)),
        ] {
            let got = crossfader_gains(p, c);
            assert!(close(got, want), "p {p} c {c}: {got:?}");
        }
    }

    #[test]
    fn filter_mapping() {
        assert_eq!(filter_setting(0.0), FilterSetting::Off);
        assert_eq!(filter_setting(-1.0), FilterSetting::LowPass(LP_BOTTOM_HZ));
        assert_eq!(filter_setting(1.0), FilterSetting::HighPass(HP_TOP_HZ));
        let FilterSetting::LowPass(hz) = filter_setting(-0.5) else {
            panic!()
        };
        assert!(hz > 500.0 && hz < 2_000.0, "{hz}");
    }
}
