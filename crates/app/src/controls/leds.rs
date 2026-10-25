use backspn_control::feedback::Feedback;
use backspn_control::output::{self, OutMapping};
use std::time::{Duration, Instant};

use backspn_control::{Action, midi, session};

use super::Controls;

pub(super) struct Led {
    pub(super) port: midi::Output,
    sent: output::Sender,
    keep_alive: session::KeepAlive,
}

impl Controls {
    fn port_messages(&self, name: &str) -> Option<&session::PortMessages> {
        self.set.port_messages.iter().find(|m| m.port == name)
    }

    pub(super) fn connect_output(&mut self, name: &str) {
        if !midi::output_ports().iter().any(|p| p == name) {
            log::info!("MIDI output {name:?} not present yet");
            return;
        }
        match midi::Output::open(name) {
            Ok(mut port) => {
                for message in self
                    .port_messages(name)
                    .map(|m| m.on_connect())
                    .unwrap_or_default()
                {
                    if let Err(e) = port.send(message) {
                        log::warn!("MIDI output {name}: {e}");
                    }
                }
                self.leds.push(Led {
                    port,
                    sent: output::Sender::default(),
                    keep_alive: session::KeepAlive::new(Instant::now()),
                });
            }
            Err(e) => self.message = Some(e),
        }
    }

    fn close_output(&self, led: &mut Led) {
        let Some(m) = self.port_messages(&led.port.name) else {
            return;
        };
        let name = led.port.name.clone();
        let clear: Vec<Vec<u8>> = self
            .set
            .outputs
            .iter()
            .filter(|o| m.clear_leds && backspn_control::on_port(o.port.as_deref(), Some(&name)))
            .filter_map(|o| output::message(&o.output, 0))
            .map(|c| c.to_vec())
            .collect();
        let quit = (!m.quit.is_empty()).then(|| m.quit.clone());
        for message in clear.into_iter().chain(quit) {
            if let Err(e) = led.port.send(&message) {
                log::warn!("MIDI output {}: {e}", led.port.name);
                break;
            }
        }
    }

    pub(super) fn close_outputs(&mut self) {
        let mut leds = std::mem::take(&mut self.leds);
        for led in &mut leds {
            self.close_output(led);
        }
    }

    pub(super) fn keep_alive_due(&self) -> Option<Duration> {
        let now = Instant::now();
        self.leds
            .iter()
            .filter(|l| {
                self.port_messages(&l.port.name)
                    .is_some_and(|m| !m.keep_alive.is_empty())
            })
            .map(|l| l.keep_alive.next(now))
            .min()
    }

    pub(super) fn watch_outputs(&mut self) {
        let open: Vec<String> = self.leds.iter().map(|l| l.port.name.clone()).collect();
        let (add, gone) = super::port_changes(&self.set.midi_outputs, &open, &midi::output_ports());
        self.leds.retain(|l| !gone.contains(&l.port.name));
        if !add.is_empty() {
            for name in add {
                self.connect_output(&name);
            }
            self.resend_leds();
        }
    }

    pub(super) fn send_leds(&mut self, feedback: &Feedback) {
        let now = Instant::now();
        let lit = self.pulses.lit(now);
        for led in &mut self.leds {
            let keep_alive = self
                .set
                .port_messages
                .iter()
                .find(|m| m.port == led.port.name)
                .map(|m| m.keep_alive.as_slice())
                .filter(|m| !m.is_empty());
            let modifiers = self
                .sources
                .get(&led.port.name)
                .map(|s| s.modifiers)
                .unwrap_or_default();
            let outputs: Vec<OutMapping> = self
                .set
                .outputs
                .iter()
                .filter(|o| backspn_control::on_port(o.port.as_deref(), Some(&led.port.name)))
                .map(|o| {
                    let mut o = o.clone();
                    if o.focus {
                        o.action = o
                            .action
                            .on_deck(self.set.output_target_deck(&o, feedback.focus));
                        o.focus = false;
                    }
                    o
                })
                .collect();
            for message in led.sent.update(&outputs, feedback, &modifiers, &lit, false) {
                if let Err(e) = led.port.send(&message) {
                    log::warn!("LED output {}: {e}", led.port.name);
                    break;
                }
                led.keep_alive.sent(now);
            }
            if let Some(message) = keep_alive.filter(|_| led.keep_alive.due(now))
                && let Err(e) = led.port.send(message)
            {
                log::warn!("MIDI output {}: {e}", led.port.name);
            }
        }
    }

    pub fn fired(&mut self, action: Action) {
        if output::pulses(action) {
            self.pulses.fire(action, Instant::now());
        }
    }

    pub fn output_due(&self) -> Option<Duration> {
        let pulse = self.pulses.next_change(Instant::now());
        [pulse, self.keep_alive_due()].into_iter().flatten().min()
    }

    pub(super) fn resend_leds(&mut self) {
        for led in &mut self.leds {
            led.sent.reset();
        }
    }

    pub(super) fn import_outputs(&mut self, outputs: Vec<OutMapping>, port: &str) {
        if outputs.is_empty() {
            return;
        }
        for o in outputs {
            self.set
                .outputs
                .retain(|m| !(m.output == o.output && m.action == o.action));
            self.set.outputs.push(o);
        }
        if !port.is_empty()
            && !self.set.midi_outputs.iter().any(|n| n == port)
            && midi::output_ports().iter().any(|n| n == port)
        {
            self.set.midi_outputs.push(port.to_owned());
            self.connect_output(port);
        }
        self.resend_leds();
    }

    pub(super) fn close_output_named(&mut self, name: &str) {
        if let Some(i) = self.leds.iter().position(|l| l.port.name == name) {
            let mut led = self.leds.remove(i);
            self.close_output(&mut led);
        }
    }
}

impl Drop for Controls {
    fn drop(&mut self) {
        self.close_outputs();
    }
}
