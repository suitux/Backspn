use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};

#[derive(Debug)]
pub struct InputFeed {
    samples: rtrb::Consumer<f32>,
    channels: usize,
    block: Vec<f32>,
}

const MAX_WAITING_BLOCKS: usize = 8;

impl InputFeed {
    pub fn new(samples: rtrb::Consumer<f32>, channels: usize, max_frames: usize) -> Self {
        Self {
            samples,
            channels: channels.max(1),
            block: vec![0.0; max_frames * channels.max(1)],
        }
    }

    pub fn pull(&mut self, frames: usize) {
        let want = (frames * self.channels).min(self.block.len());
        let waiting = self.samples.slots();
        let excess = waiting.saturating_sub(want * MAX_WAITING_BLOCKS);
        let excess = excess - excess % self.channels;
        if excess > 0
            && let Ok(chunk) = self.samples.read_chunk(excess)
        {
            chunk.commit_all();
        }
        let n = self.samples.slots().min(want);
        let n = n - n % self.channels;
        if let Ok(chunk) = self.samples.read_chunk(n) {
            let (a, b) = chunk.as_slices();
            self.block[..a.len()].copy_from_slice(a);
            self.block[a.len()..a.len() + b.len()].copy_from_slice(b);
            chunk.commit_all();
        }
        self.block[n..want].fill(0.0);
    }

    pub fn sample(&self, i: usize, ch: u16) -> f32 {
        let ch = usize::from(ch);
        if ch >= self.channels {
            return 0.0;
        }
        self.block
            .get(i * self.channels + ch)
            .copied()
            .unwrap_or(0.0)
    }

    pub fn stereo(&self, (l, r): (u16, u16), out: &mut [f32]) {
        for (i, frame) in out.as_chunks_mut::<2>().0.iter_mut().enumerate() {
            frame[0] = self.sample(i, l);
            frame[1] = self.sample(i, r);
        }
    }
}

pub fn open(name: &str, rate: u32, max_frames: usize) -> Option<(cpal::Stream, InputFeed, u16)> {
    let host = cpal::default_host();
    let device = host
        .input_devices()
        .ok()?
        .find(|d| d.description().is_ok_and(|desc| desc.name() == name))?;
    let supported = device
        .supported_input_configs()
        .ok()?
        .filter(|c| c.sample_format() == SampleFormat::F32)
        .find_map(|c| c.try_with_sample_rate(rate))?;
    let channels = supported.channels();
    let config = StreamConfig {
        channels,
        sample_rate: rate,
        buffer_size: cpal::BufferSize::Default,
    };
    let capacity = max_frames * usize::from(channels) * MAX_WAITING_BLOCKS * 4;
    let (mut producer, consumer) = rtrb::RingBuffer::<f32>::new(capacity);
    let stream = device
        .build_input_stream::<f32, _, _>(
            config,
            move |data, _| {
                let n = data.len().min(producer.slots());
                if let Ok(mut chunk) = producer.write_chunk(n) {
                    let (a, b) = chunk.as_mut_slices();
                    a.copy_from_slice(&data[..a.len()]);
                    b.copy_from_slice(&data[a.len()..n]);
                    chunk.commit_all();
                }
            },
            |err| log::warn!("audio input: {err}"),
            None,
        )
        .ok()?;
    stream.play().ok()?;
    Some((
        stream,
        InputFeed::new(consumer, usize::from(channels), max_frames),
        channels,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_reads_what_arrived_and_pads_with_silence() {
        let (mut p, c) = rtrb::RingBuffer::<f32>::new(1024);
        let mut feed = InputFeed::new(c, 4, 64);
        for v in [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0] {
            p.push(v).unwrap();
        }
        feed.pull(3);
        assert_eq!((feed.sample(0, 2), feed.sample(1, 3)), (3.0, 8.0));
        assert_eq!(feed.sample(2, 0), 0.0, "not arrived: silence");
        assert_eq!(feed.sample(0, 9), 0.0, "no such channel");
        let mut out = [0.0; 6];
        feed.stereo((2, 3), &mut out);
        assert_eq!(out[..4], [3.0, 4.0, 7.0, 8.0]);
    }

    #[test]
    fn input_that_waited_too_long_is_dropped() {
        let (mut p, c) = rtrb::RingBuffer::<f32>::new(4096);
        let mut feed = InputFeed::new(c, 1, 16);
        for i in 0..1000 {
            p.push(i as f32).unwrap();
        }
        feed.pull(4);
        assert_eq!(feed.sample(0, 0), (1000 - 32) as f32);
    }
}
