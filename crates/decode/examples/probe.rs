use std::time::Instant;

fn main() {
    for path in std::env::args().skip(1) {
        let started = Instant::now();
        match backspn_decode::decode_file(&path) {
            Ok(track) => println!(
                "{:>7.3} s  {:>7.1} s audio  {:>6} Hz  {path}",
                started.elapsed().as_secs_f64(),
                track.duration_s(),
                track.sample_rate()
            ),
            Err(e) => println!("ERROR {path}: {e}"),
        }
    }
}
