use std::time::Instant;

use backspn_analysis::{BpmRange, Profile, key, loudness, onset, signal, tempo};

fn main() {
    let path = std::env::args().nth(1).expect("usage: profile FILE");
    let mut t = Instant::now();
    let mut lap = |label: &str| {
        println!("{label:<12} {:>7.3} s", t.elapsed().as_secs_f64());
        t = Instant::now();
    };
    let track = backspn_decode::decode_file(&path).expect("decodable file");
    lap("decode");
    let mono = signal::mono_at_analysis_rate(&track);
    lap("resample");
    let env = onset::envelopes(&mono);
    lap("envelopes");
    let transients = onset::transients(&env);
    lap("transients");
    let grid = tempo::estimate(&env, BpmRange::AUTOMATIC);
    lap("tempo");
    let k = key::estimate(&key::chroma(&mono), Profile::Temperley);
    lap("key");
    let l = loudness::measure(&track);
    lap("loudness");
    println!("{} transients, {grid:?}, {k:?}, {l:?}", transients.len());
}
