pub mod key;
mod key_model;
pub mod loudness;
pub mod onset;
pub mod signal;
mod stft;
pub mod tap;
pub mod tempo;

use backspn_decode::Track;

pub use key::{Key, Profile};
pub use loudness::Loudness;
pub use onset::Transient;
pub use tempo::{Beatgrid, BpmRange};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Options {
    pub bpm_range: BpmRange,
    pub key_profile: Profile,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            bpm_range: BpmRange::AUTOMATIC,
            key_profile: Profile::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Analysis {
    pub beatgrid: Option<Beatgrid>,
    pub key: Option<Key>,
    pub loudness: Option<Loudness>,
    pub transients: Vec<Transient>,
}

pub fn analyze(track: &Track, options: &Options) -> Analysis {
    let mono = signal::mono_at_analysis_rate(track);
    let envelopes = onset::envelopes(&mono);
    Analysis {
        beatgrid: tempo::estimate(&envelopes, options.bpm_range),
        key: key::key_of(&mono, options.key_profile),
        loudness: loudness::measure(track),
        transients: onset::transients(&envelopes),
    }
}
