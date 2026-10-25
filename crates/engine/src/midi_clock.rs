use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockMessage {
    pub at: Instant,
    pub byte: u8,
}

#[derive(Debug)]
pub struct MidiClockTap {
    producer: rtrb::Producer<ClockMessage>,
}

#[derive(Debug)]
pub struct MidiClockFeed {
    consumer: rtrb::Consumer<ClockMessage>,
}

const CAPACITY: usize = 256;

pub fn midi_clock_tap() -> (MidiClockTap, MidiClockFeed) {
    let (producer, consumer) = rtrb::RingBuffer::new(CAPACITY);
    (MidiClockTap { producer }, MidiClockFeed { consumer })
}

impl MidiClockTap {
    pub fn push(&mut self, message: ClockMessage) -> bool {
        self.producer.push(message).is_ok()
    }
}

impl MidiClockFeed {
    pub fn pop(&mut self) -> Option<ClockMessage> {
        self.consumer.pop().ok()
    }

    pub fn peek(&self) -> Option<ClockMessage> {
        self.consumer.peek().ok().copied()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockInput {
    pub at: Instant,
    pub status: u8,
    pub spp: u16,
}

#[derive(Debug, Clone)]
pub struct MidiClockSink {
    producer: std::sync::Arc<std::sync::Mutex<rtrb::Producer<ClockInput>>>,
}

#[derive(Debug)]
pub struct MidiClockInFeed {
    consumer: rtrb::Consumer<ClockInput>,
}

pub fn midi_clock_input() -> (MidiClockSink, MidiClockInFeed) {
    let (producer, consumer) = rtrb::RingBuffer::new(CAPACITY);
    let producer = std::sync::Arc::new(std::sync::Mutex::new(producer));
    (MidiClockSink { producer }, MidiClockInFeed { consumer })
}

impl MidiClockSink {
    pub fn push(&self, input: ClockInput) {
        if let Ok(mut producer) = self.producer.lock() {
            let _ = producer.push(input);
        }
    }
}

impl MidiClockInFeed {
    pub fn pop(&mut self) -> Option<ClockInput> {
        self.consumer.pop().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sink_reaches_the_one_feed() {
        let (sink, mut feed) = midi_clock_input();
        let other = sink.clone();
        let at = Instant::now();
        sink.push(ClockInput {
            at,
            status: 0xFA,
            spp: 0,
        });
        other.push(ClockInput {
            at,
            status: 0xF8,
            spp: 0,
        });
        assert_eq!(feed.pop().map(|m| m.status), Some(0xFA));
        assert_eq!(feed.pop().map(|m| m.status), Some(0xF8));
        assert_eq!(feed.pop(), None);
    }

    #[test]
    fn messages_come_out_in_order_and_a_full_ring_drops() {
        let (mut tap, mut feed) = midi_clock_tap();
        let at = Instant::now();
        for _ in 0..CAPACITY {
            assert!(tap.push(ClockMessage { at, byte: 0xF8 }));
        }
        assert!(!tap.push(ClockMessage { at, byte: 0xFC }));
        assert_eq!(feed.peek().map(|m| m.byte), Some(0xF8));
        let mut n = 0;
        while feed.pop().is_some() {
            n += 1;
        }
        assert_eq!(n, CAPACITY);
    }
}
