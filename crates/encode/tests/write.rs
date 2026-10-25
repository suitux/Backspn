use std::f32::consts::TAU;
use std::path::PathBuf;

use backspn_encode::{RATE, STEMS, generated_stems, write};
use backspn_formats::bstem;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("backspn-encode-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

fn stems(rate: u32) -> Vec<Vec<f32>> {
    let frames = 2 * rate as usize;
    (0..STEMS)
        .map(|k| {
            let hz = 110.0 * (k + 1) as f32;
            (0..frames)
                .flat_map(|i| {
                    let x = 0.2 * (TAU * hz * i as f32 / rate as f32).sin();
                    [x, x]
                })
                .collect()
        })
        .collect()
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}

fn correlation(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    dot / (rms(a) * rms(b) * a.len() as f32)
}

#[test]
fn written_stems_read_back_in_line_with_the_source() {
    let path = scratch("sines.bstem");
    let sources = stems(RATE);
    let mut steps = Vec::new();
    write(
        &path,
        RATE,
        std::array::from_fn(|k| sources[k].as_slice()),
        &mut |f| steps.push(f),
    )
    .unwrap();
    assert_eq!(steps.last(), Some(&1.0));

    let header = bstem::header(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(header.stems, generated_stems());
    let decoded = backspn_decode::decode_stems(&path).unwrap();
    assert_eq!(decoded.len(), STEMS);
    for (stem, source) in decoded.iter().zip(&sources) {
        assert_eq!(stem.sample_rate(), RATE);
        assert_eq!(stem.samples().len(), source.len());
        let c = correlation(stem.samples(), source);
        assert!(c > 0.95, "in line with the source: {c}");
    }
    let _ = std::fs::remove_file(&path);
}

#[test]
fn other_rates_are_stored_at_48_khz_with_the_same_duration() {
    let path = scratch("cd.bstem");
    let sources = stems(44_100);
    write(
        &path,
        44_100,
        std::array::from_fn(|k| sources[k].as_slice()),
        &mut |_| {},
    )
    .unwrap();
    let decoded = backspn_decode::decode_stems(&path).unwrap();
    let stem = &decoded[0];
    assert_eq!(stem.sample_rate(), RATE);
    assert!(
        (stem.duration_s() - 2.0).abs() < 1e-3,
        "{}",
        stem.duration_s()
    );
    assert!((rms(stem.samples()) / rms(&sources[0]) - 1.0).abs() < 0.1);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn stems_of_different_lengths_are_refused() {
    let a = [0.0; 8];
    let b = [0.0; 6];
    assert!(write(&scratch("bad.bstem"), RATE, [&a, &a, &a, &b], &mut |_| {}).is_err());
}
