const CLICK_DIVISOR: u32 = 735;

#[derive(Debug, Clone, PartialEq)]
pub struct Metronome {
    len: usize,
    done: usize,
}

impl Metronome {
    pub fn new(sample_rate: u32) -> Self {
        let len = (sample_rate / CLICK_DIVISOR).max(2) as usize;
        Self { len, done: len }
    }

    pub fn render(&mut self, from: Option<f64>, to: Option<f64>, out: &mut [f32]) {
        let frames = out.len() / 2;
        let mut start = 0;
        if let (Some(a), Some(b)) = (from, to)
            && b > a
            && b - a < 1.0
            && b.floor() > a.floor()
        {
            let at = ((b.floor() - a) / (b - a) * frames as f64) as usize;
            self.continue_click(&mut out[..2 * at.min(frames)]);
            self.done = 0;
            start = at.min(frames);
        }
        self.continue_click(&mut out[2 * start..]);
    }

    fn continue_click(&mut self, out: &mut [f32]) {
        let half = self.len / 2;
        for frame in out.as_chunks_mut::<2>().0 {
            if self.done >= self.len {
                return;
            }
            let v = if self.done < half { 1.0 } else { -1.0 };
            frame[0] += v;
            frame[1] += v;
            self.done += 1;
        }
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    fn left(out: &[f32]) -> Vec<f32> {
        out.iter().step_by(2).copied().collect()
    }

    #[test]
    fn a_click_is_one_square_cycle_on_the_beat() {
        let mut m = Metronome::new(44_100);
        assert_eq!(m.len, 60);
        let mut out = vec![0.0; 2 * 100];
        m.render(Some(3.9), Some(4.3), &mut out);
        let l = left(&out);
        assert!(l[..25].iter().all(|&x| x == 0.0));
        assert!(l[25..55].iter().all(|&x| x == 1.0));
        assert!(l[55..85].iter().all(|&x| x == -1.0));
        assert!(l[85..].iter().all(|&x| x == 0.0));
    }

    #[test]
    fn a_click_continues_in_the_next_block_and_jumps_do_not_click() {
        let mut m = Metronome::new(44_100);
        let mut out = vec![0.0; 2 * 40];
        m.render(Some(0.99), Some(1.01), &mut out);
        let first = left(&out);
        assert_eq!(first[20..].iter().filter(|&&x| x == 1.0).count(), 20);
        let mut next = vec![0.0; 2 * 40];
        m.render(Some(1.01), Some(1.03), &mut next);
        let second = left(&next);
        assert_eq!(second.iter().filter(|&&x| x == 1.0).count(), 10);
        assert_eq!(second.iter().filter(|&&x| x == -1.0).count(), 30);
        let mut quiet = vec![0.0; 2 * 40];
        m.render(Some(5.5), Some(9.2), &mut quiet);
        m.render(Some(9.2), Some(8.9), &mut quiet);
        m.render(None, Some(10.1), &mut quiet);
        assert!(quiet.iter().all(|&x| x == 0.0));
    }
}
