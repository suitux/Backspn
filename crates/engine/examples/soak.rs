use std::sync::Arc;
use std::time::{Duration, Instant};

use backspn_engine::{Command, Config, DeckCommand, Engine};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut seconds = 60u64;
    let mut config = Config::default();
    let mut file = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--seconds" => seconds = args.next().ok_or("--seconds needs a value")?.parse()?,
            "--buffer" => {
                config.buffer_frames = Some(args.next().ok_or("--buffer needs a value")?.parse()?);
            }
            _ => file = Some(arg),
        }
    }

    let mut engine = Engine::start(&config)?;
    let info = engine.info().clone();
    println!(
        "device {:?}: {} Hz, {} ch, buffer {:?} frames ({:.2} ms)",
        info.device,
        info.sample_rate,
        info.channels,
        info.buffer_frames,
        info.buffer_latency().unwrap_or_default().as_secs_f64() * 1e3
    );

    if let Some(path) = &file {
        let started = Instant::now();
        let track = backspn_decode::decode_file(path)?;
        println!(
            "decoded {path} in {:.2} s: {:.1} s at {} Hz",
            started.elapsed().as_secs_f64(),
            track.duration_s(),
            track.sample_rate()
        );
        let controller = engine.controller();
        controller
            .send(Command::Deck(0, DeckCommand::Load(Arc::new(track))))
            .map_err(|_| "queue full")?;
        controller
            .send(Command::Deck(0, DeckCommand::Play))
            .map_err(|_| "queue full")?;
    }

    let started = Instant::now();
    let deadline = started + Duration::from_secs(seconds);
    let mut next_report = started + Duration::from_secs(10);
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
        let controller = engine.controller();
        controller.collect_garbage();
        if file.is_some() && !controller.state().decks[0].playing {
            controller
                .send(Command::Deck(0, DeckCommand::Play))
                .map_err(|_| "queue full")?;
        }
        if Instant::now() >= next_report {
            report(&mut engine, started.elapsed());
            next_report += Duration::from_secs(10);
        }
    }
    let stats = report(&mut engine, started.elapsed());
    if stats.xruns + stats.overruns > 0 {
        std::process::exit(1);
    }
    Ok(())
}

fn report(engine: &mut Engine, elapsed: Duration) -> backspn_engine::StatsSnapshot {
    let s = engine.controller().stats();
    println!(
        "{:>5.0} s  callbacks {:>8}  xruns {}  overruns {}  max callback {:.3} ms  load {:.1} %  output latency {:.2} ms",
        elapsed.as_secs_f64(),
        s.callbacks,
        s.xruns,
        s.overruns,
        s.max_callback.as_secs_f64() * 1e3,
        s.load_permille as f64 / 10.0,
        s.output_latency.as_secs_f64() * 1e3
    );
    s
}
