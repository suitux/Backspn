#[derive(Debug)]
pub struct RecordTap {
    producer: rtrb::Producer<f32>,
}

#[derive(Debug)]
pub struct RecordFeed {
    consumer: rtrb::Consumer<f32>,
}

pub fn record_tap(rate: u32, seconds: f64) -> (RecordTap, RecordFeed) {
    let samples = (f64::from(rate) * seconds) as usize * 2;
    let (producer, consumer) = rtrb::RingBuffer::new(samples.max(2));
    (RecordTap { producer }, RecordFeed { consumer })
}

impl RecordTap {
    pub fn push(&mut self, block: &[f32]) -> bool {
        match self.producer.write_chunk_uninit(block.len()) {
            Ok(chunk) => {
                chunk.fill_from_iter(block.iter().copied());
                true
            }
            Err(_) => false,
        }
    }
}

impl RecordFeed {
    pub fn drain(&mut self, out: &mut Vec<f32>) {
        let n = self.consumer.slots();
        if let Ok(chunk) = self.consumer.read_chunk(n) {
            let (a, b) = chunk.as_slices();
            out.extend_from_slice(a);
            out.extend_from_slice(b);
            chunk.commit_all();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_pass_whole_or_not_at_all() {
        let (mut tap, mut feed) = record_tap(4, 1.0);
        assert!(tap.push(&[1.0, 2.0, 3.0, 4.0]));
        assert!(!tap.push(&[5.0; 6]), "no room for a whole block");
        assert!(tap.push(&[5.0, 6.0]));
        let mut out = Vec::new();
        feed.drain(&mut out);
        assert_eq!(out, [1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    }
}
