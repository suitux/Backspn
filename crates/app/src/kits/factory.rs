use super::synth::{Style, Voice};
use super::{DEFAULT_KNOBS, Kit, KitSound, Steps};

pub struct FactoryKit {
    pub id: &'static str,
    pub name: &'static str,
    pub style: Style,
}

pub const KITS: [FactoryKit; 2] = [
    FactoryKit {
        id: "analog-kit",
        name: "Analog Kit",
        style: Style::Analog,
    },
    FactoryKit {
        id: "club-kit",
        name: "Club Kit",
        style: Style::Club,
    },
];

pub fn file_name(voice: Voice) -> &'static str {
    match voice {
        Voice::Kick => "kick.wav",
        Voice::Snare => "snare.wav",
        Voice::Clap => "clap.wav",
        Voice::ClosedHat => "closed-hat.wav",
        Voice::OpenHat => "open-hat.wav",
        Voice::Ride => "ride.wav",
        Voice::Rim => "rim.wav",
        Voice::Tom => "tom.wav",
    }
}

fn sound_name(voice: Voice) -> &'static str {
    match voice {
        Voice::Kick => "Kick",
        Voice::Snare => "Snare",
        Voice::Clap => "Clap",
        Voice::ClosedHat => "Closed Hat",
        Voice::OpenHat => "Open Hat",
        Voice::Ride => "Ride",
        Voice::Rim => "Rim",
        Voice::Tom => "Tom",
    }
}

fn level_db(voice: Voice) -> f32 {
    match voice {
        Voice::Kick => 0.0,
        Voice::Snare => -2.0,
        Voice::Clap | Voice::Tom => -3.0,
        Voice::Rim => -6.0,
        Voice::ClosedHat => -8.0,
        Voice::OpenHat => -9.0,
        Voice::Ride => -10.0,
    }
}

/// P4 to P8: the sound alone on an even grid that halves each time, from
/// quarters to sixty-fourths, so stepping up tightens it into a roll.
const ROLL: [&str; 5] = [
    "x...x...x...x...",
    "x.x.x.x.x.x.x.x.",
    "xxxxxxxxxxxxxxxx",
    "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
    "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
];

/// P1 to P3: driving one-bar grooves.
fn grooves(voice: Voice) -> [&'static str; 3] {
    match voice {
        Voice::Kick => ["x...x...x...x.x.", "x..x..x.x..x..x.", "x.x.x..xx.x.x.xx"],
        Voice::Snare => ["....x.......x..x", "....x.x.....x.xx", "....x..x.x..xxxx"],
        Voice::Clap => ["....x.......xx..", "....x.x.....x.x.", "..x.x..x..x.x.xx"],
        Voice::ClosedHat => ["x.xxx.xxx.xxx.xx", "x.x.x.xxx.x.x.xx", "xxx.xxx.xxx.xxxx"],
        Voice::OpenHat => ["..x...x...x...x.", "..x...x...x...xx", "..x..xx...x..xxx"],
        Voice::Ride => ["x.x.x.x.x.x.x.x.", "x.xxx.xxx.xxx.xx", "x.x.x.xxx.x.x.xx"],
        Voice::Rim => ["...x..x....x..x.", "x..x..x...x..x..", "..x.xx.x..x.xx.x"],
        Voice::Tom => ["..........x.x.x.", "x..x..x.....xxxx", "......x.x.x.xxxx"],
    }
}

fn patterns(voice: Voice) -> [&'static str; 8] {
    let [p1, p2, p3] = grooves(voice);
    let [p4, p5, p6, p7, p8] = ROLL;
    [p1, p2, p3, p4, p5, p6, p7, p8]
}

impl FactoryKit {
    pub fn kit(&self) -> Kit {
        Kit {
            name: self.name.to_owned(),
            author: "Backspn".to_owned(),
            knobs: DEFAULT_KNOBS,
            sounds: Voice::ALL
                .iter()
                .map(|&voice| KitSound {
                    name: sound_name(voice).to_owned(),
                    file: file_name(voice).to_owned(),
                    level_db: level_db(voice),
                    patterns: patterns(voice).map(|p| Steps::parse(p).unwrap_or_default()),
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pattern_parses() {
        for voice in Voice::ALL {
            for p in patterns(voice) {
                assert!(Steps::parse(p).is_some(), "{p}");
            }
        }
    }

    #[test]
    fn upper_patterns_double_the_hits_on_an_even_grid() {
        let hits = ROLL.map(|p| Steps::parse(p).map_or(0, |s| s.bits.count_ones()));
        assert_eq!(hits, [4, 8, 16, 32, 64]);
        for p in ROLL {
            let steps = Steps::parse(p).unwrap_or_default();
            let gap = steps.cells() / steps.bits.count_ones() as usize;
            assert!(
                (0..steps.cells()).all(|i| steps.on(i) == (i % gap == 0)),
                "{p}"
            );
        }
    }

    #[test]
    fn kits_have_distinct_ids_and_files() {
        assert_ne!(KITS[0].id, KITS[1].id);
        let mut files: Vec<_> = Voice::ALL.iter().map(|&v| file_name(v)).collect();
        files.sort_unstable();
        files.dedup();
        assert_eq!(files.len(), Voice::ALL.len());
    }
}
