use std::ops::Range;

pub const SEGMENT: usize = 343_980;
pub const OVERLAP: usize = SEGMENT / 4;
const STRIDE: usize = SEGMENT - OVERLAP;

pub fn segments(total: usize) -> Vec<Range<usize>> {
    let count = total.div_ceil(STRIDE).max(1);
    (0..count)
        .map(|i| {
            let start = i * STRIDE;
            start..(start + SEGMENT).min(total)
        })
        .collect()
}

pub fn weight(i: usize) -> f32 {
    let ramp = |k: usize| k as f32 / (OVERLAP - 1) as f32;
    if i < OVERLAP {
        ramp(i)
    } else if i >= SEGMENT - OVERLAP {
        ramp(SEGMENT - 1 - i)
    } else {
        1.0
    }
}

#[derive(Debug)]
pub struct OverlapAdd {
    channels: usize,
    tracks: Vec<Vec<f32>>,
    weights: Vec<f32>,
}

impl OverlapAdd {
    pub fn new(tracks: usize, channels: usize, frames: usize) -> Self {
        Self {
            channels,
            tracks: vec![vec![0.0; frames * channels]; tracks],
            weights: vec![0.0; frames],
        }
    }

    pub fn add(&mut self, segment: &Range<usize>, output: impl Fn(usize, usize, usize) -> f32) {
        for i in 0..segment.len() {
            let w = weight(i);
            let frame = segment.start + i;
            self.weights[frame] += w;
            for (t, track) in self.tracks.iter_mut().enumerate() {
                for c in 0..self.channels {
                    track[frame * self.channels + c] += output(t, c, i) * w;
                }
            }
        }
    }

    pub fn finish(mut self) -> Vec<Vec<f32>> {
        for track in &mut self.tracks {
            for (frame, w) in self.weights.iter().enumerate() {
                let w = w.max(1e-8);
                for c in 0..self.channels {
                    track[frame * self.channels + c] /= w;
                }
            }
        }
        self.tracks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_overlap_by_a_quarter_and_cover_the_track() {
        assert_eq!(segments(1_000), vec![0..1_000]);
        assert_eq!(segments(0), vec![0..0]);
        let s = segments(3 * SEGMENT);
        assert_eq!(s[0], 0..SEGMENT);
        assert_eq!(s[1].start, SEGMENT - OVERLAP);
        assert_eq!(s.last().unwrap().end, 3 * SEGMENT);
        assert_eq!(s.len(), (3 * SEGMENT).div_ceil(SEGMENT - OVERLAP));
    }

    #[test]
    fn the_window_fades_in_and_out_linearly() {
        assert!(weight(0).abs() < f32::EPSILON);
        assert!((weight(OVERLAP - 1) - 1.0).abs() < 1e-6);
        assert!((weight(SEGMENT / 2) - 1.0).abs() < f32::EPSILON);
        assert!(weight(SEGMENT - 1).abs() < f32::EPSILON);
        let mid = weight(OVERLAP / 2);
        assert!((mid - 0.5).abs() < 1e-3, "{mid}");
    }

    #[test]
    fn an_identity_model_gives_the_input_back() {
        let frames = 2 * SEGMENT + 1_234;
        let input: Vec<f32> = (0..2 * frames).map(|i| ((i % 97) as f32) / 97.0).collect();
        let mut sum = OverlapAdd::new(2, 2, frames);
        for seg in segments(frames) {
            sum.add(&seg, |t, c, i| {
                input[(seg.start + i) * 2 + c] * if t == 0 { 1.0 } else { 0.5 }
            });
        }
        let out = sum.finish();
        for f in 1..frames {
            for c in 0..2 {
                let x = input[f * 2 + c];
                assert!((out[0][f * 2 + c] - x).abs() < 1e-4, "frame {f}");
                assert!((out[1][f * 2 + c] - 0.5 * x).abs() < 1e-4);
            }
        }
    }
}
