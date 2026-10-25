use super::*;

const RATE: u32 = 48_000;

fn tone(freq: f64, frames: usize, amp: f32) -> Vec<f32> {
    (0..frames)
        .flat_map(|i| {
            let v =
                (2.0 * std::f64::consts::PI * freq * i as f64 / f64::from(RATE)).sin() as f32 * amp;
            [v, v]
        })
        .collect()
}

fn rms_left(x: &[f32]) -> f32 {
    let left: Vec<f32> = x.iter().step_by(2).copied().collect();
    (left.iter().map(|v| v * v).sum::<f32>() / left.len() as f32).sqrt()
}

fn run(mixer: &mut Mixer, sources: &[Vec<f32>], blocks: usize) -> (Vec<f32>, Vec<f32>) {
    let n = 512;
    let mut master = vec![0.0; 2 * n];
    let mut cue = vec![0.0; 2 * n];
    for b in 0..blocks {
        let mut bufs: Vec<Vec<f32>> = sources
            .iter()
            .map(|s| s[(b * 2 * n) % (s.len() - 2 * n)..][..2 * n].to_vec())
            .collect();
        let mut refs: Vec<&mut [f32]> = bufs.iter_mut().map(Vec::as_mut_slice).collect();
        mixer.process(&mut refs, &mut master, &mut cue);
    }
    (master, cue)
}

fn quiet_mixer() -> Mixer {
    let mut m = Mixer::new(RATE);
    m.handle(Command::MasterVolume(1.0));
    m.handle(Command::Limiter(false));
    m.handle(Command::Headroom(0.0));
    m
}

#[test]
fn unity_path_passes_signal() {
    let mut m = quiet_mixer();
    let (master, _) = run(&mut m, &[tone(1_000.0, 48_000, 0.5)], 20);
    assert!((rms_left(&master) - 0.5 / 2f32.sqrt()).abs() < 0.01);
}

#[test]
fn low_kill_removes_bass_only() {
    let mut m = quiet_mixer();
    m.handle(Command::Eq(0, Band::Low, 0.0));
    let (bass, _) = run(&mut m, &[tone(60.0, 48_000, 0.5)], 20);
    assert!(rms_left(&bass) < 0.02, "{}", rms_left(&bass));
    let (mid, _) = run(&mut m, &[tone(1_000.0, 48_000, 0.5)], 20);
    assert!(rms_left(&mid) > 0.33);
}

#[test]
fn filter_sweeps_cut_highs_and_lows() {
    let mut m = quiet_mixer();
    m.handle(Command::Filter(0, -0.8));
    let (out, _) = run(&mut m, &[tone(5_000.0, 48_000, 0.5)], 30);
    assert!(
        rms_left(&out) < 0.02,
        "low-pass left highs: {}",
        rms_left(&out)
    );
    m.handle(Command::Filter(0, 0.8));
    let (out, _) = run(&mut m, &[tone(60.0, 48_000, 0.5)], 30);
    assert!(
        rms_left(&out) < 0.02,
        "high-pass left lows: {}",
        rms_left(&out)
    );
}

#[test]
fn crossfader_and_assign() {
    let mut m = quiet_mixer();
    m.handle(Command::Assign(0, Assign::A));
    m.handle(Command::Assign(1, Assign::B));
    m.handle(Command::Crossfader(0.0));
    let sources = [tone(440.0, 48_000, 0.5), tone(1_000.0, 48_000, 0.0)];
    let (out, _) = run(&mut m, &sources, 20);
    assert!(rms_left(&out) > 0.34);
    m.handle(Command::Crossfader(1.0));
    let (out, _) = run(&mut m, &sources, 20);
    assert!(rms_left(&out) < 0.001);
}

#[test]
fn cue_bus_is_pre_fader() {
    let mut m = quiet_mixer();
    m.handle(Command::Fader(0, 0.0));
    m.handle(Command::Cue(0, true));
    m.handle(Command::CueVolume(1.0));
    let (master, cue) = run(&mut m, &[tone(440.0, 48_000, 0.5)], 20);
    assert!(rms_left(&master) < 0.001);
    assert!(rms_left(&cue) > 0.34);
    m.handle(Command::CueMix(1.0));
    let (_, cue) = run(&mut m, &[tone(440.0, 48_000, 0.5)], 20);
    assert!(
        rms_left(&cue) < 0.001,
        "cue mix fully on master, master silent"
    );
}

#[test]
fn limiter_prevents_clipping() {
    let mut m = Mixer::new(RATE);
    m.handle(Command::MasterVolume(1.0));
    m.handle(Command::Headroom(0.0));
    let loud = tone(100.0, 48_000, 0.9);
    let (out, _) = run(&mut m, &[loud.clone(), loud], 20);
    let peak = out.iter().fold(0.0f32, |p, v| p.max(v.abs()));
    assert!(peak <= 1.0);
    assert!(m.state().limiter_db < -3.0);
}

#[test]
fn meters_track_and_decay() {
    let mut m = quiet_mixer();
    run(&mut m, &[tone(440.0, 48_000, 0.5)], 5);
    let s = m.state();
    assert!((s.channel_peaks[0] - 0.5).abs() < 0.05);
    assert!(s.master_peak[0] > 0.4);
    run(&mut m, &[vec![0.0; 2 * 48_000]], 94);
    assert!(m.state().channel_peaks[0] < 0.07, "≈ 20 dB/s decay");
}

#[test]
fn ignores_out_of_range_channels() {
    let mut m = Mixer::new(RATE);
    m.handle(Command::Fader(9, 0.0));
    assert_eq!(m.state(), MixerState::default());
}

#[test]
fn club_filter_model_sweeps_and_bypasses_at_the_centre() {
    let mut m = quiet_mixer();
    m.handle(Command::FilterModel(FilterModel::Club));
    assert_eq!(m.state().filter_model, FilterModel::Club);
    let treble = tone(8_000.0, 48_000, 0.5);
    let (dry, _) = run(&mut m, std::slice::from_ref(&treble), 40);
    m.handle(Command::Filter(0, -0.8));
    let (filtered, _) = run(&mut m, std::slice::from_ref(&treble), 40);
    assert!(rms_left(&filtered) < 0.1 * rms_left(&dry));
    m.handle(Command::Filter(0, 0.0));
    let (centre, _) = run(&mut m, std::slice::from_ref(&treble), 40);
    assert!((rms_left(&centre) - rms_left(&dry)).abs() < 0.02 * rms_left(&dry));
}

#[test]
fn auto_gain_multiplies_the_trim() {
    let mut m = quiet_mixer();
    let src = tone(1_000.0, 48_000, 0.1);
    let (unity, _) = run(&mut m, std::slice::from_ref(&src), 40);
    m.handle(Command::Gain(0, -6.0));
    m.handle(Command::AutoGain(0, 20.0));
    assert_eq!(m.state().channels[0].auto_gain_db, MAX_AUTO_GAIN_DB);
    let (boosted, _) = run(&mut m, std::slice::from_ref(&src), 40);
    let db = 20.0 * (rms_left(&boosted) / rms_left(&unity)).log10();
    assert!((db - 6.0).abs() < 0.2, "{db}");
}

#[test]
fn balance_and_mute() {
    assert_eq!(balance_gains(0.0), [1.0, 1.0]);
    assert_eq!(balance_gains(-1.0), [1.5, 0.0]);
    assert_eq!(balance_gains(0.5), [0.5, 1.25]);
    let mut m = quiet_mixer();
    let src = tone(1_000.0, 48_000, 0.1);
    m.handle(Command::Balance(0, 1.0));
    let (master, _) = run(&mut m, std::slice::from_ref(&src), 40);
    let right: Vec<f32> = master.iter().skip(1).step_by(2).copied().collect();
    assert!(rms_left(&master) < 1e-4);
    assert!(right.iter().any(|v| v.abs() > 0.1));
    m.handle(Command::Mute(0, true));
    let (master, _) = run(&mut m, std::slice::from_ref(&src), 40);
    assert!(master.iter().all(|v| v.abs() < 1e-4));
    assert!(m.state().channels[0].mute);
}

#[test]
fn mono_master_sums_both_sides() {
    let mut m = quiet_mixer();
    m.handle(Command::Balance(0, 1.0));
    m.handle(Command::MasterMono(true));
    let (master, _) = run(&mut m, &[tone(1_000.0, 48_000, 0.1)], 40);
    for frame in master.chunks(2) {
        assert!((frame[0] - frame[1]).abs() < 1e-6);
    }
    assert!(rms_left(&master) > 0.01);
}

#[test]
fn headroom_lowers_the_master() {
    let mut m = quiet_mixer();
    let src = tone(1_000.0, 48_000, 0.5);
    let (unity, _) = run(&mut m, std::slice::from_ref(&src), 20);
    m.handle(Command::Headroom(DEFAULT_HEADROOM_DB));
    let (lower, _) = run(&mut m, std::slice::from_ref(&src), 20);
    let db = 20.0 * (rms_left(&lower) / rms_left(&unity)).log10();
    assert!((db - DEFAULT_HEADROOM_DB).abs() < 0.05, "{db}");
}

#[test]
fn headroom_lowers_the_cue_bus_too() {
    let mut m = quiet_mixer();
    m.handle(Command::Fader(0, 0.0));
    m.handle(Command::Cue(0, true));
    m.handle(Command::CueVolume(1.0));
    let src = tone(440.0, 48_000, 0.5);
    let (_, unity) = run(&mut m, std::slice::from_ref(&src), 20);
    m.handle(Command::Headroom(-6.0));
    let (_, lower) = run(&mut m, std::slice::from_ref(&src), 20);
    let db = 20.0 * (rms_left(&lower) / rms_left(&unity)).log10();
    assert!((db + 6.0).abs() < 0.05, "{db}");
}

#[test]
fn four_band_eq_cuts_the_mid_high_band() {
    let mut m = quiet_mixer();
    m.handle(Command::EqModel(EqModel::FourBand));
    let src = tone(1_850.0, 48_000, 0.3);
    let (flat, _) = run(&mut m, std::slice::from_ref(&src), 40);
    m.handle(Command::Eq(0, Band::Mid, 0.0));
    let (cut, _) = run(&mut m, std::slice::from_ref(&src), 40);
    let db = 20.0 * (rms_left(&cut) / rms_left(&flat)).log10();
    assert!(db < -20.0, "{db}");
    assert_eq!(m.state().channels[0].eq[Band::Mid as usize], 0.0);
}

#[test]
fn wide_isolator_kills_the_low_band() {
    let mut m = quiet_mixer();
    m.handle(Command::EqModel(EqModel::WideIsolator));
    let src = tone(60.0, 48_000, 0.3);
    let (flat, _) = run(&mut m, std::slice::from_ref(&src), 40);
    m.handle(Command::Eq(0, Band::Low, 0.0));
    let (cut, _) = run(&mut m, std::slice::from_ref(&src), 40);
    let db = 20.0 * (rms_left(&cut) / rms_left(&flat)).log10();
    assert!(db < -30.0, "{db}");
    assert!(
        (rms_left(&flat) - 0.3 / 2f32.sqrt()).abs() < 0.02,
        "centred is flat"
    );
}

#[test]
fn ladder_filter_model_low_passes() {
    let mut m = quiet_mixer();
    m.handle(Command::FilterModel(FilterModel::Ladder));
    let src = tone(3_000.0, 48_000, 0.3);
    let (open, _) = run(&mut m, std::slice::from_ref(&src), 40);
    m.handle(Command::Filter(0, -1.0));
    let (closed, _) = run(&mut m, std::slice::from_ref(&src), 40);
    let db = 20.0 * (rms_left(&closed) / rms_left(&open)).log10();
    assert!(db < -40.0, "{db}");
}

#[test]
fn external_outputs_carry_each_channel_times_master_and_headroom() {
    let mut m = quiet_mixer();
    m.handle(Command::Headroom(-6.0));
    let mut a = vec![0.5f32; 2 * 512];
    let mut b = vec![-2.0f32; 2 * 512];
    for _ in 0..50 {
        a.fill(0.5);
        b.fill(-2.0);
        m.finish_external(&mut [&mut a, &mut b]);
    }
    let g = 10f32.powf(-6.0 / 20.0);
    assert!((a[0] - 0.5 * g).abs() < 1e-4, "{}", a[0]);
    assert!((b[0] + 1.0).abs() < 1e-6);
    assert!((m.state().master_peak[0] - 2.0 * g).abs() < 1e-3);
}
