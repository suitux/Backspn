use std::sync::Arc;

use realfft::num_complex::Complex;
use realfft::{RealFftPlanner, RealToComplex};

pub fn hann(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / n as f32).cos())
        .collect()
}

pub fn hann_symmetric(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (n - 1) as f32).cos())
        .collect()
}

pub struct Stft {
    fft: Arc<dyn RealToComplex<f32>>,
    window: Vec<f32>,
    input: Vec<f32>,
    output: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
}

impl Stft {
    pub fn hann(len: usize) -> Self {
        Self::new(len, hann(len))
    }

    pub fn new(len: usize, window: Vec<f32>) -> Self {
        assert!(window.len() <= len, "window longer than the FFT");
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(len);
        Self {
            input: fft.make_input_vec(),
            output: fft.make_output_vec(),
            scratch: fft.make_scratch_vec(),
            fft,
            window,
        }
    }

    pub fn spectrum(&mut self, frame: &[f32]) -> &[Complex<f32>] {
        self.input.fill(0.0);
        for ((x, s), w) in self.input.iter_mut().zip(frame).zip(&self.window) {
            *x = s * w;
        }
        self.fft
            .process_with_scratch(&mut self.input, &mut self.output, &mut self.scratch)
            .expect("buffer sizes come from the plan");
        &self.output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_have_the_expected_ends() {
        let periodic = hann(8);
        assert!(periodic[0].abs() < 1e-7 && (periodic[4] - 1.0).abs() < 1e-6);
        let symmetric = hann_symmetric(9);
        assert!(symmetric[0].abs() < 1e-7 && symmetric[8].abs() < 1e-6);
        assert!((symmetric[4] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_tone_peaks_in_its_bin() {
        let len = 1024;
        let mut stft = Stft::hann(len);
        let bin = 37;
        let frame: Vec<f32> = (0..len)
            .map(|i| (2.0 * std::f32::consts::PI * bin as f32 * i as f32 / len as f32).sin())
            .collect();
        let spectrum = stft.spectrum(&frame);
        let peak = (0..spectrum.len())
            .max_by(|&a, &b| spectrum[a].norm().total_cmp(&spectrum[b].norm()))
            .unwrap();
        assert_eq!(peak, bin);
    }

    #[test]
    fn short_windows_are_zero_padded() {
        let mut stft = Stft::new(16, vec![1.0; 4]);
        assert!((stft.spectrum(&[1.0; 4])[0].re - 4.0).abs() < 1e-6);
    }
}
