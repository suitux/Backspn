use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use backspn_control::midi::{ClockHandler, Output};
use backspn_engine::{
    ClockInput, Command, MidiClockCommand, MidiClockFeed, MidiClockSink, midi_clock_input,
    midi_clock_tap,
};

pub type ClockHandlers = Arc<dyn Fn() -> ClockHandler + Send + Sync>;

fn stamp_time(anchor: &mut Option<(u64, Instant)>, stamp: u64, now: Instant) -> Instant {
    if stamp == 0 {
        return now;
    }
    if let Some((s0, t0)) = *anchor
        && stamp >= s0
    {
        let at = t0 + Duration::from_micros(stamp - s0);
        if at <= now {
            return at;
        }
    }
    *anchor = Some((stamp, now));
    now
}

const POLL: Duration = Duration::from_micros(500);

fn send_due(feed: &mut MidiClockFeed, now: Instant, mut send: impl FnMut(u8)) -> Option<Instant> {
    while let Some(m) = feed.peek() {
        if m.at > now {
            return Some(m.at);
        }
        feed.pop();
        send(m.byte);
    }
    None
}

const PORT_CHECK: Duration = Duration::from_secs(2);

fn port_change(open: bool, present: bool) -> Option<bool> {
    match (open, present) {
        (false, true) => Some(true),
        (true, false) => Some(false),
        _ => None,
    }
}

struct Sender {
    port: String,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Sender {
    fn open(port: &str) -> Result<(Self, Command), String> {
        let mut output = Output::open(port)?;
        let (tap, mut feed) = midi_clock_tap();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("midi-clock".into())
            .spawn(move || {
                while !stopping.load(Ordering::Relaxed) {
                    let now = Instant::now();
                    let next = send_due(&mut feed, now, |byte| {
                        if let Err(e) = output.send(&[byte]) {
                            log::warn!("MIDI clock: {e}");
                        }
                    });
                    let wait = next.map_or(POLL, |t| t.saturating_duration_since(now));
                    std::thread::sleep(wait.min(POLL));
                }
            })
            .map_err(|e| e.to_string())?;
        let sender = Self {
            port: port.to_owned(),
            stop,
            thread: Some(thread),
        };
        Ok((sender, Command::MidiClock(MidiClockCommand::Tap(Some(tap)))))
    }
}

impl Drop for Sender {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            drop(thread.join());
        }
    }
}

pub struct MidiClock {
    sender: Option<Sender>,
    pub send: bool,
    pub error: Option<String>,
    input: Arc<Mutex<Option<MidiClockSink>>>,
    handlers: ClockHandlers,
    checked: Instant,
}

impl Default for MidiClock {
    fn default() -> Self {
        let input: Arc<Mutex<Option<MidiClockSink>>> = Arc::default();
        let slot = Arc::clone(&input);
        let handlers: ClockHandlers = Arc::new(move || {
            let slot = Arc::clone(&slot);
            let mut anchor = None;
            Box::new(move |status, spp, stamp| {
                let at = stamp_time(&mut anchor, stamp, Instant::now());
                if let Ok(sink) = slot.lock()
                    && let Some(sink) = &*sink
                {
                    sink.push(ClockInput { at, status, spp });
                }
            })
        });
        Self {
            sender: None,
            send: false,
            error: None,
            input,
            handlers,
            checked: Instant::now(),
        }
    }
}

impl MidiClock {
    pub fn handlers(&self) -> ClockHandlers {
        Arc::clone(&self.handlers)
    }

    fn connect_input(&self, out: &mut Vec<Command>) {
        let (sink, feed) = midi_clock_input();
        if let Ok(mut slot) = self.input.lock() {
            *slot = Some(sink);
        }
        out.push(Command::MidiClock(MidiClockCommand::Input(Some(feed))));
    }

    pub fn configure(
        &mut self,
        enabled: bool,
        port: Option<&str>,
        offset_ms: i32,
        fresh: bool,
        out: &mut Vec<Command>,
    ) {
        if fresh {
            self.connect_input(out);
        }
        if !enabled {
            self.set_send(false, out);
        }
        let port = port.filter(|_| enabled);
        let open = self.sender.as_ref().map(|s| s.port.as_str());
        if open == port && !fresh {
            return;
        }
        if port.is_none() || fresh {
            self.sender = None;
            out.push(Command::MidiClock(MidiClockCommand::Tap(None)));
        }
        Self::set_offset_ms(offset_ms, out);
        let Some(port) = port else {
            return;
        };
        self.sender = None;
        match Sender::open(port) {
            Ok((sender, tap)) => {
                self.sender = Some(sender);
                self.error = None;
                out.push(tap);
            }
            Err(e) => {
                crate::toast::error(format!("Cannot open the MIDI clock output: {e}"));
                self.error = Some(e);
            }
        }
        if fresh && self.send {
            out.push(Command::MidiClock(MidiClockCommand::Send(true)));
        }
    }

    pub fn watch(&mut self, enabled: bool, port: Option<&str>, out: &mut Vec<Command>) {
        let Some(port) = port.filter(|_| enabled) else {
            return;
        };
        if self.checked.elapsed() < PORT_CHECK {
            return;
        }
        self.checked = Instant::now();
        let present = backspn_control::midi::output_ports()
            .iter()
            .any(|p| p == port);
        match port_change(self.sender.is_some(), present) {
            Some(true) => match Sender::open(port) {
                Ok((sender, tap)) => {
                    log::info!("MIDI clock output back: {port}");
                    self.sender = Some(sender);
                    self.error = None;
                    out.push(tap);
                }
                Err(e) => self.error = Some(e),
            },
            Some(false) => {
                log::warn!("MIDI clock output gone: {port}");
                self.sender = None;
                self.error = Some(format!("{port} unplugged, waiting for it"));
                out.push(Command::MidiClock(MidiClockCommand::Tap(None)));
            }
            None => {}
        }
    }

    pub fn set_send(&mut self, on: bool, out: &mut Vec<Command>) {
        if self.send != on {
            self.send = on;
            out.push(Command::MidiClock(MidiClockCommand::Send(on)));
        }
    }

    pub fn set_offset_ms(ms: i32, out: &mut Vec<Command>) {
        out.push(Command::MidiClock(MidiClockCommand::OffsetMs(f64::from(
            ms,
        ))));
    }

    pub fn resync(out: &mut Vec<Command>) {
        out.push(Command::MidiClock(MidiClockCommand::Resync));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use backspn_engine::ClockMessage;

    #[test]
    fn only_due_messages_go_and_the_next_time_comes_back() {
        let (mut tap, mut feed) = midi_clock_tap();
        let now = Instant::now();
        let later = now + Duration::from_millis(5);
        for (at, byte) in [(now, 0xFA), (now, 0xF8), (later, 0xF8)] {
            assert!(tap.push(ClockMessage { at, byte }));
        }
        let mut sent = Vec::new();
        assert_eq!(send_due(&mut feed, now, |b| sent.push(b)), Some(later));
        assert_eq!(sent, [0xFA, 0xF8]);
        assert_eq!(send_due(&mut feed, later, |b| sent.push(b)), None);
        assert_eq!(sent, [0xFA, 0xF8, 0xF8]);
    }

    #[test]
    fn the_port_opens_when_back_and_closes_when_gone() {
        assert_eq!(port_change(false, true), Some(true));
        assert_eq!(port_change(true, false), Some(false));
        assert_eq!(port_change(true, true), None);
        assert_eq!(port_change(false, false), None);
    }

    #[test]
    fn disabling_turns_start_off_and_drops_the_tap() {
        let mut clock = MidiClock::default();
        let mut out = Vec::new();
        clock.set_send(true, &mut out);
        out.clear();
        clock.configure(false, Some("any"), 0, false, &mut out);
        assert!(!clock.send);
        let names: Vec<String> = out.iter().map(|c| format!("{c:?}")).collect();
        assert_eq!(names, ["MidiClock(Send(false))"]);
    }

    #[test]
    fn stamps_keep_their_spacing_from_the_first() {
        let mut anchor = None;
        let t0 = Instant::now();
        assert_eq!(stamp_time(&mut anchor, 1_000, t0), t0);
        let late = t0 + Duration::from_millis(3);
        assert_eq!(
            stamp_time(&mut anchor, 2_000, late),
            t0 + Duration::from_millis(1)
        );
        assert_eq!(stamp_time(&mut anchor, 0, late), late);
        assert_eq!(stamp_time(&mut anchor, 500, late), late);
        assert_eq!(anchor, Some((500, late)));
    }

    #[test]
    fn the_inputs_reach_the_engines_end() {
        let clock = MidiClock::default();
        let mut out = Vec::new();
        clock.connect_input(&mut out);
        let Some(Command::MidiClock(MidiClockCommand::Input(Some(mut feed)))) = out.pop() else {
            panic!("no feed");
        };
        let mut handler = clock.handlers()();
        handler(0xFA, 0, 0);
        assert_eq!(feed.pop().map(|m| m.status), Some(0xFA));
    }

    #[test]
    fn a_missing_port_is_reported() {
        let mut clock = MidiClock::default();
        let mut out = Vec::new();
        clock.configure(true, Some("no such port"), 0, false, &mut out);
        assert!(clock.error.is_some());
        assert!(clock.sender.is_none());
    }
}
