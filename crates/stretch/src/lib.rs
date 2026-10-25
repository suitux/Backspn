use signalsmith_stretch::Stretch;

pub struct Stretcher {
    inner: Stretch,
    input_latency: usize,
    output_latency: usize,
}

impl std::fmt::Debug for Stretcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Stretcher")
            .field("input_latency", &self.input_latency)
            .field("output_latency", &self.output_latency)
            .finish_non_exhaustive()
    }
}

impl Stretcher {
    pub fn new(sample_rate: u32) -> Self {
        let mut inner = Stretch::preset_default(2, sample_rate);
        let input_latency = inner.input_latency();
        let output_latency = inner.output_latency();
        let warm = vec![0.0f32; 2 * (sample_rate as usize / 2 + input_latency).max(1024)];
        inner.seek(&warm, 1.0);
        let mut scratch = vec![0.0f32; 2 * 1024];
        inner.process(&warm[..2 * 1024], &mut scratch);
        inner.reset();
        inner.seek(&warm, 1.0);
        inner.reset();
        Self {
            inner,
            input_latency,
            output_latency,
        }
    }

    pub fn input_latency(&self) -> usize {
        self.input_latency
    }

    pub fn output_latency(&self) -> usize {
        self.output_latency
    }

    pub fn reset(&mut self) {
        self.inner.reset();
    }

    pub fn prime(&mut self, input: &[f32], rate: f64) {
        self.inner.seek(input, rate);
    }

    pub fn set_transpose(&mut self, semitones: f32) {
        self.inner.set_transpose_factor_semitones(semitones, None);
    }

    pub fn process(&mut self, input: &[f32], output: &mut [f32]) {
        self.inner.process(input, output);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 44_100;

    fn sine(freq: f64, start: usize, frames: usize) -> Vec<f32> {
        (start..start + frames)
            .flat_map(|i| {
                let v = (2.0 * std::f64::consts::PI * freq * i as f64 / f64::from(RATE)).sin()
                    as f32
                    * 0.5;
                [v, v]
            })
            .collect()
    }

    fn frequency(x: &[f32]) -> f64 {
        let left: Vec<f32> = x.iter().step_by(2).copied().collect();
        let crossings: Vec<usize> = (1..left.len())
            .filter(|&i| left[i - 1] < 0.0 && left[i] >= 0.0)
            .collect();
        let span = (crossings[crossings.len() - 1] - crossings[0]) as f64;
        (crossings.len() - 1) as f64 * f64::from(RATE) / span
    }

    #[test]
    fn stretches_tempo_without_changing_pitch() {
        let mut s = Stretcher::new(RATE);
        let block_out = 512;
        let tempo = 1.25;
        let block_in = (f64::from(block_out) * tempo) as usize;
        let mut read = 0;
        s.prime(&sine(440.0, 0, s.input_latency()), tempo);
        read += s.input_latency();
        let mut out = Vec::new();
        let mut buf = vec![0.0; 2 * block_out as usize];
        for _ in 0..200 {
            s.process(&sine(440.0, read, block_in), &mut buf);
            read += block_in;
            out.extend_from_slice(&buf);
        }
        let settled = &out[2 * 8192..];
        let f = frequency(settled);
        assert!((f - 440.0).abs() < 2.0, "pitch drifted to {f} Hz");
        assert_eq!(out.len() / 2, 200 * block_out as usize);
    }

    #[test]
    fn transposes_by_semitones() {
        let mut s = Stretcher::new(RATE);
        s.set_transpose(12.0);
        let block = 512;
        let mut read = 0;
        s.prime(&sine(440.0, 0, s.input_latency()), 1.0);
        read += s.input_latency();
        let mut out = Vec::new();
        let mut buf = vec![0.0; 2 * block];
        for _ in 0..200 {
            s.process(&sine(440.0, read, block), &mut buf);
            read += block;
            out.extend_from_slice(&buf);
        }
        let f = frequency(&out[2 * 8192..]);
        assert!((f - 880.0).abs() < 4.0, "an octave up: {f} Hz");
    }

    #[test]
    fn latencies_are_reasonable() {
        let s = Stretcher::new(RATE);
        assert!(s.input_latency() > 1_000 && s.input_latency() < 6_000);
        assert!(s.output_latency() > 1_000 && s.output_latency() < 6_000);
    }
}
