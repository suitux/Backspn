use std::sync::mpsc;
use std::time::{Duration, Instant};

use backspn_control::Input;
use backspn_control::s4mk3::{self, Leds, colour};

fn main() {
    let seconds: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);
    let (tx, rx) = mpsc::channel();
    let device = match s4mk3::Device::start(&s4mk3::S4, tx, || {}) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Cannot open the S4 MK3: {e}");
            std::process::exit(1);
        }
    };
    let decoder = s4mk3::Decoder::default();
    let mut leds = Leds::default();
    let end = Instant::now() + Duration::from_secs(seconds);
    let wait = Instant::now() + Duration::from_secs(5);
    while !device.connected() {
        if Instant::now() > wait {
            eprintln!("No S4 MK3 found.");
            std::process::exit(1);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    println!("Move controls for {seconds} s…");
    while Instant::now() < end {
        let Ok((_, event)) = rx.recv_timeout(Duration::from_millis(100)) else {
            continue;
        };
        let Input::Hid(name) = &event.input else {
            continue;
        };
        match event.pressed {
            Some(down) => {
                println!("{name:32} {}", if down { "down" } else { "up" });
                leds.set(&decoder, name.as_str(), colour::GREEN, down);
                device.send_leds(leds.clone());
            }
            None if s4mk3::is_encoder(name.as_str()) => {
                println!(
                    "{name:32} {:+}",
                    backspn_control::midi::relative_steps(event.raw)
                );
            }
            None => println!("{name:32} {:.3}", event.value),
        }
    }
    device.send_leds(Leds::default());
    std::thread::sleep(Duration::from_millis(100));
}
