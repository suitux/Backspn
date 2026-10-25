pub const RANGE: u16 = 4096;
pub const MARGIN: u16 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Poti {
    pub min: u16,
    pub centre: u16,
    pub max: u16,
    pub has_centre: bool,
}

impl Poti {
    pub fn new(has_centre: bool) -> Self {
        Self {
            min: MARGIN,
            centre: RANGE / 2,
            max: RANGE - MARGIN,
            has_centre,
        }
    }

    #[must_use]
    pub fn validated(self) -> Self {
        let (m, tol) = (i32::from(MARGIN), i32::from(RANGE) / 10);
        let (min, centre, max) = (
            i32::from(self.min),
            i32::from(self.centre),
            i32::from(self.max),
        );
        let ok = if self.has_centre {
            min < centre - tol - 2 * m && max > centre + tol + 2 * m
        } else {
            min + m < max
        };
        if ok { self } else { Self::new(self.has_centre) }
    }

    pub fn value(&self, raw: u16) -> f32 {
        let m = f32::from(MARGIN);
        let (min, centre, max) = (
            f32::from(self.min),
            f32::from(self.centre),
            f32::from(self.max),
        );
        let raw = f32::from(raw).clamp(min + m, (max - m).max(min + m));
        let v = if !self.has_centre {
            (raw - min - m) / (max - min - 2.0 * m)
        } else if (raw - centre).abs() <= 2.0 * m {
            0.5
        } else if raw < centre {
            (raw - min - m) / (2.0 * (centre - min - 3.0 * m))
        } else {
            0.5 + (raw - centre - 2.0 * m) / (2.0 * (max - centre - 3.0 * m))
        };
        if v.is_finite() {
            v.clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

#[derive(Debug)]
pub struct StoredCalibration {
    pub reports: &'static [u8],
    pub product_id: u16,
    pub first_entry: usize,
    pub with_centre: &'static [usize],
    pub faders: &'static [usize],
    pub no_centre: &'static [usize],
    pub first_analog: usize,
}

pub const REPORT_DATA: usize = 32;

impl StoredCalibration {
    fn count(&self) -> usize {
        self.with_centre
            .iter()
            .chain(self.faders)
            .max()
            .map_or(0, |&m| m + 1)
    }

    pub fn defaults(&self) -> Vec<Poti> {
        (0..self.count())
            .map(|i| Poti::new(!self.no_centre.contains(&i)))
            .collect()
    }

    pub fn parse(&self, data: &[u8]) -> Option<Vec<Poti>> {
        if data.len() < self.reports.len() * REPORT_DATA
            || data[..2] == [0xFF, 0xFF]
            || data[1] != 0
            || u16::from_le_bytes([data[2], data[3]]) != self.product_id
        {
            return None;
        }
        let word = |at: usize| u16::from_le_bytes([data[at], data[at + 1]]);
        let pick = |at: usize, own: u16| {
            let v = word(at);
            if v < RANGE { v } else { own }
        };
        let mut potis = self.defaults();
        let mut at = self.first_entry;
        for &i in self.with_centre {
            let p = &mut potis[i];
            p.min = pick(at, p.min);
            p.centre = pick(at + 2, p.centre);
            p.max = pick(at + 4, p.max);
            at += 6;
        }
        for &i in self.faders {
            let p = &mut potis[i];
            p.min = pick(at, p.min);
            p.max = pick(at + 2, p.max);
            at += 4;
        }
        Some(potis.into_iter().map(Poti::validated).collect())
    }

    pub fn analog_byte(&self, index: usize) -> usize {
        self.first_analog + 2 * index
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fader_runs_between_its_margins() {
        let p = Poti {
            min: 100,
            centre: 0,
            max: 4000,
            has_centre: false,
        };
        assert_eq!(p.value(0), 0.0);
        assert_eq!(p.value(116), 0.0);
        assert_eq!(p.value(3984), 1.0);
        assert!((p.value(2050) - 0.5).abs() < 1e-3);
    }

    #[test]
    fn a_knob_rests_at_its_centre() {
        let p = Poti {
            min: 50,
            centre: 2000,
            max: 4050,
            has_centre: true,
        };
        assert_eq!(p.value(2000 + 32), 0.5);
        assert_eq!(p.value(2000 - 32), 0.5);
        assert_eq!(p.value(66), 0.0);
        assert_eq!(p.value(4034), 1.0);
        assert!((p.value(2000 + 33) - 0.5).abs() < 1e-3);
        assert!(p.value(1000) < 0.5 && p.value(3000) > 0.5);
    }

    #[test]
    fn bad_values_fall_back_to_the_defaults() {
        let knob = Poti {
            min: 1900,
            centre: 2000,
            max: 4000,
            has_centre: true,
        };
        assert_eq!(knob.validated(), Poti::new(true));
        let fader = Poti {
            min: 100,
            centre: 0,
            max: 110,
            has_centre: false,
        };
        assert_eq!(fader.validated(), Poti::new(false));
        let good = Poti {
            min: 100,
            centre: 0,
            max: 4000,
            has_centre: false,
        };
        assert_eq!(good.validated(), good);
    }

    const TEST: StoredCalibration = StoredCalibration {
        reports: &[0xD0, 0xD1],
        product_id: 0x1210,
        first_entry: 0x12,
        with_centre: &[0, 1],
        faders: &[2],
        no_centre: &[0],
        first_analog: 0x01,
    };

    fn data() -> Vec<u8> {
        let mut d = vec![0u8; 64];
        d[1..4].copy_from_slice(&[0x00, 0x10, 0x12]);
        let put =
            |d: &mut Vec<u8>, at: usize, v: u16| d[at..at + 2].copy_from_slice(&v.to_le_bytes());
        put(&mut d, 0x12, 40);
        put(&mut d, 0x14, 0xFFFF);
        put(&mut d, 0x16, 4050);
        put(&mut d, 0x18, 60);
        put(&mut d, 0x1A, 2100);
        put(&mut d, 0x1C, 4020);
        put(&mut d, 0x1E, 200);
        put(&mut d, 0x20, 3900);
        d
    }

    #[test]
    fn stored_values_fill_the_potis() {
        let potis = TEST.parse(&data()).expect("valid");
        assert_eq!(
            potis[0],
            Poti {
                min: 40,
                centre: 2048,
                max: 4050,
                has_centre: false
            }
        );
        assert_eq!(
            (potis[1].min, potis[1].centre, potis[1].max),
            (60, 2100, 4020)
        );
        assert!(potis[1].has_centre);
        assert_eq!((potis[2].min, potis[2].max), (200, 3900));
        assert_eq!(TEST.analog_byte(2), 0x05);
    }

    #[test]
    fn empty_or_foreign_memory_is_not_used() {
        let mut d = data();
        d[0] = 0xFF;
        d[1] = 0xFF;
        assert_eq!(TEST.parse(&d), None, "empty");
        let mut d = data();
        d[3] = 0x13;
        assert_eq!(TEST.parse(&d), None, "another model");
        assert_eq!(TEST.parse(&data()[..40]), None, "short");
        assert_eq!(TEST.defaults().len(), 3);
    }
}
